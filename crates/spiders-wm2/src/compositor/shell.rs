use smithay::desktop::{PopupKind, Window};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Rectangle, SERIAL_COUNTER, Serial};
use smithay::wayland::shell::xdg::PopupSurface;

use crate::actions::focus::{self, FocusUpdate};
use crate::actions::window;
use crate::actions::workspace;
use crate::frame_sync::{Transaction, WindowFrameSyncState};
use crate::model::WindowId;
use crate::state::{ManagedWindow, SpidersWm};

impl SpidersWm {
    pub fn close_focused_window(&mut self) {
        let Some(focused_surface) = self.focused_surface.as_ref() else {
            return;
        };

        let _ = window::mark_focused_window_closing(&mut self.model);

        if let Some(record) = self
            .managed_windows
            .iter()
            .find(|record| record.wl_surface() == *focused_surface)
        {
            if let Some(toplevel) = record.window.toplevel() {
                toplevel.send_close();
            }
        }
    }

    pub fn set_focus(&mut self, surface: Option<WlSurface>, serial: Serial) {
        self.focused_surface = surface.clone();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, surface, serial);
        }

        let focused_window_id = self
            .focused_surface
            .as_ref()
            .and_then(|focused| {
                self.managed_windows
                    .iter()
                    .find(|record| record.wl_surface() == *focused)
                    .map(|record| record.id)
            });
        focus::set_focused_window(&mut self.model, focused_window_id);

        for record in &self.managed_windows {
            let active = self
                .focused_surface
                .as_ref()
                .is_some_and(|focused| record.wl_surface() == *focused);
            record.window.set_activated(active);
            if let Some(toplevel) = record.window.toplevel() {
                let _ = toplevel.send_pending_configure();
            }
        }
    }

    pub fn add_window(&mut self, window: Window) {
        let window_id = WindowId(self.next_window_id);
        self.next_window_id += 1;
        workspace::place_new_window(&mut self.model, window_id);

        self.managed_windows.push(ManagedWindow {
            id: window_id,
            window,
            mapped: false,
            frame_sync: WindowFrameSyncState::default(),
        });
    }

    pub fn handle_window_close(&mut self, surface: &WlSurface) {
        let Some(position) = self
            .managed_windows
            .iter()
            .position(|record| record.wl_surface() == *surface)
        else {
            return;
        };

        let record = self.managed_windows.remove(position);
        let focus_update = focus::remove_window(&mut self.model, record.id);
        let transaction = Transaction::new();
        let monitor = transaction.monitor();

        if record.mapped {
            if let (Some(snapshot), Some(element_location)) = (
                record.frame_sync.snapshot_owned(),
                self.space.element_location(&record.window),
            ) {
                self.frame_sync.push_closing_window(snapshot.into_closing_window(
                    element_location,
                    record.window.geometry().loc,
                    monitor,
                ));
            }
            self.space.unmap_elem(&record.window);
        }

        if let FocusUpdate::Set(next_focus_window_id) = focus_update {
            let next_focus = next_focus_window_id.and_then(|window_id| {
                self.managed_windows
                    .iter()
                    .find(|candidate| candidate.id == window_id)
                    .map(ManagedWindow::wl_surface)
            });
            self.set_focus(next_focus, SERIAL_COUNTER.next_serial());
        }

        self.schedule_relayout_with_transaction(Some(transaction));
    }

    pub fn find_window_mut(&mut self, surface: &WlSurface) -> Option<&mut ManagedWindow> {
        self.managed_windows
            .iter_mut()
            .find(|record| record.wl_surface() == *surface)
    }

    pub fn is_known_window_mapped(&self, surface: &WlSurface) -> bool {
        self.managed_windows
            .iter()
            .find(|record| record.wl_surface() == *surface)
            .is_some_and(|record| record.mapped)
    }

    pub fn handle_window_commit(&mut self, surface: &WlSurface) {
        let mut mapped_window_id = None;
        let window_update = if let Some(record) = self.find_window_mut(surface) {
            let update = record.frame_sync.consume_commit_update(record.mapped);
            if !record.mapped && update.pending_location.is_some() {
                record.mapped = true;
                record.frame_sync.mark_snapshot_dirty();
                mapped_window_id = Some(record.id);
            }

            Some((record.id, record.window.clone(), update.pending_location, update.first_map))
        } else {
            None
        };

        if let Some(window_id) = mapped_window_id {
            self.model.set_window_mapped(window_id, true);
        }

        if let Some((window_id, window, pending_location, first_map)) = window_update {
            window.on_commit();

            if first_map {
                self.schedule_relayout();
                self.set_focus(Some(surface.clone()), SERIAL_COUNTER.next_serial());

                let mut mapped_window_id = None;
                if let Some(record) = self.find_window_mut(surface) {
                    let pending_location = record.frame_sync.take_pending_location();
                    if pending_location.is_some() {
                        record.mapped = true;
                        record.frame_sync.mark_snapshot_dirty();
                        mapped_window_id = Some(record.id);
                    }

                    if let Some(location) = pending_location {
                        self.space.map_element(window, location, false);
                    }
                }

                if let Some(window_id) = mapped_window_id {
                    self.model.set_window_mapped(window_id, true);
                }

                return;
            }

            let location = pending_location.or_else(|| {
                self.find_window_mut(surface)
                    .and_then(|record| record.frame_sync.pending_location())
            });

            if let Some(location) = location {
                self.space.map_element(window, location, false);
                self.model.set_window_mapped(window_id, true);
            }
        }
    }

    pub fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = smithay::desktop::find_popup_root_surface(&PopupKind::Xdg(popup.clone()))
        else {
            return;
        };
        let Some(window) = self.space.elements().find(|window| {
            window
                .toplevel()
                .is_some_and(|toplevel| toplevel.wl_surface() == &root)
        }) else {
            return;
        };

        let output = self
            .space
            .outputs()
            .next()
            .expect("output missing for popup");
        let output_geo = self
            .space
            .output_geometry(output)
            .expect("output geometry missing");
        let window_geo = self
            .space
            .element_geometry(window)
            .unwrap_or(Rectangle::new((0, 0).into(), (0, 0).into()));

        let mut target = output_geo;
        target.loc -= smithay::desktop::get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}