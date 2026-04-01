use smithay::desktop::Window;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use tracing::{debug, info};

use crate::actions::focus::FocusUpdate;
use crate::frame_sync;
use crate::model::window_id;
use crate::runtime::{RuntimeCommand, RuntimeResult};
use crate::state::{ManagedWindow, SpidersWm};

impl SpidersWm {
    pub fn close_focused_window(&mut self) {
        let closing_window_id = match self
            .runtime()
            .execute(RuntimeCommand::RequestCloseFocusedWindowSelection)
        {
            RuntimeResult::CloseSelection(selection) => selection.closing_window_id,
            _ => None,
        };
        info!(closing_window = ?closing_window_id, "wm2 close focused window request");
        let Some(focused_surface) =
            closing_window_id.and_then(|window_id| self.surface_for_window_id(window_id))
        else {
            return;
        };

        self.capture_close_snapshot(&focused_surface);

        if let Some(record) = self.managed_window_for_surface(&focused_surface) {
            if let Some(toplevel) = record.window.toplevel() {
                toplevel.send_close();
            }
        }
    }

    pub fn toggle_focused_window_floating(&mut self) {
        let toggled_window_id = match self
            .runtime()
            .execute(RuntimeCommand::ToggleFocusedWindowFloating)
        {
            RuntimeResult::Window(toggled_window_id) => toggled_window_id,
            _ => None,
        };
        if toggled_window_id.is_none() {
            return;
        }

        self.schedule_relayout();
        if let Some(window_id) = toggled_window_id {
            let floating = self.window_is_floating(&window_id);
            self.emit_window_floating_change(window_id, floating);
        }
    }

    pub fn toggle_focused_window_fullscreen(&mut self) {
        let toggled_window_id = match self
            .runtime()
            .execute(RuntimeCommand::ToggleFocusedWindowFullscreen)
        {
            RuntimeResult::Window(toggled_window_id) => toggled_window_id,
            _ => None,
        };
        if toggled_window_id.is_none() {
            return;
        }

        self.schedule_relayout();
        if let Some(window_id) = toggled_window_id {
            let fullscreen = self.window_is_fullscreen(&window_id);
            self.emit_window_fullscreen_change(window_id, fullscreen);
        }
    }

    pub fn add_window(&mut self, window: Window) {
        let window_id = window_id(self.next_window_id);
        self.next_window_id += 1;
        let _ = self.runtime().execute(RuntimeCommand::PlaceNewWindow {
            window_id: window_id.clone(),
        });

        self.insert_managed_window(window_id.clone(), window);

        if let Some(toplevel) = self
            .managed_window_for_id(&window_id)
            .and_then(|record| record.window.toplevel().cloned())
        {
            crate::frame_sync::install_window_pre_commit_hook(&toplevel);
        }

        info!(window = %window_id.0, total_windows = self.managed_window_count(), "wm2 added window");
        self.log_managed_window_state("after add window");
    }

    pub fn handle_window_unmap(&mut self, surface: &WlSurface) {
        self.capture_close_snapshot(surface);

        let window_update = if let Some(record) = self.find_window_mut(surface) {
            if !record.mapped {
                None
            } else {
                record.mapped = false;
                let snapshot = record.frame_sync.begin_unmap();
                Some((record.id.clone(), record.window.clone(), snapshot))
            }
        } else {
            None
        };

        let Some((window_id, window, snapshot)) = window_update else {
            return;
        };
        let location = self
            .element_location(&window)
            .map(|location| location - window.geometry().loc);

        let focus_update = match self.runtime().execute(RuntimeCommand::UnmapWindow {
            window_id: window_id.clone(),
        }) {
            RuntimeResult::FocusUpdate(focus_update) => focus_update,
            _ => FocusUpdate::Unchanged,
        };
        let closing = self.window_is_closing(&window_id);

        info!(
            window = %window_id.0,
            closing,
            focus_update = ?focus_update,
            "wm2 close start"
        );

        self.unmap_window_element(&window);

        self.log_managed_window_state("after close start");

        self.apply_focus_update(focus_update);
        let relayout_transaction = self.start_relayout();

        if let Some(result) =
            self.frame_sync
                .push_closing_overlay(snapshot, location, relayout_transaction)
        {
            debug!(
                window = %window_id.0,
                location = ?location,
                geometry_loc = ?window.geometry().loc,
                carried_overlays = result.carried_overlays,
                transaction = result.transaction_debug_id,
                "wm2 added closing snapshot overlay"
            );
        }
    }

    pub fn finalize_window_destroy(&mut self, surface: &WlSurface) {
        let Some(position) = self.managed_window_position_for_surface(surface) else {
            return;
        };

        let record = self.remove_managed_window_at(position);
        let window_id = record.id.clone();
        let mut focus_update = FocusUpdate::Unchanged;

        if record.mapped {
            self.unmap_window_element(&record.window);
            focus_update = match self.runtime().execute(RuntimeCommand::UnmapWindow {
                window_id: window_id.clone(),
            }) {
                RuntimeResult::FocusUpdate(focus_update) => focus_update,
                _ => FocusUpdate::Unchanged,
            };
        }

        let remove_update = match self.runtime().execute(RuntimeCommand::RemoveWindow {
            window_id: window_id.clone(),
        }) {
            RuntimeResult::FocusUpdate(focus_update) => focus_update,
            _ => FocusUpdate::Unchanged,
        };

        if matches!(focus_update, FocusUpdate::Unchanged) {
            focus_update = remove_update;
        }

        info!(
            window = %window_id.0,
            was_mapped = record.mapped,
            focus_update = ?focus_update,
            "wm2 finalized window destroy"
        );

        self.log_managed_window_state("after window destroy");

        self.apply_focus_update(focus_update);
        self.schedule_relayout();
    }

    pub(crate) fn capture_close_snapshot(&mut self, surface: &WlSurface) {
        let output = self.current_output_cloned();
        let Some(output) = output else {
            return;
        };

        let scale = output.current_scale().fractional_scale().into();
        let snapshot = match self.backend.as_mut() {
            Some(backend) => {
                frame_sync::capture_close_snapshot(backend.renderer(), surface, scale, 1.0)
            }
            None => None,
        };

        if let Some(snapshot) = snapshot
            && let Some(record) = self.find_window_mut(surface)
        {
            record.frame_sync.store_close_snapshot(snapshot);
            debug!(window = %record.id.0, "wm2 captured close snapshot");
        }
    }

    pub fn find_window_mut(&mut self, surface: &WlSurface) -> Option<&mut ManagedWindow> {
        self.managed_window_mut_for_surface(surface)
    }

    pub fn is_known_window_mapped(&self, surface: &WlSurface) -> bool {
        self.managed_window_for_surface(surface)
            .is_some_and(|record| record.mapped)
    }

    pub fn handle_window_commit(&mut self, surface: &WlSurface) {
        let window_update = if let Some(record) = self.find_window_mut(surface) {
            let first_map = !record.mapped;
            if first_map {
                record.mapped = true;
            }

            let ready_layout = record.frame_sync.take_ready_layout();

            debug!(
                window = %record.id.0,
                mapped = record.mapped,
                first_map,
                ready_layout = ?ready_layout,
                pending_configures = record.frame_sync.has_pending_configures(),
                "wm2 handle window commit"
            );
            Some((
                record.id.clone(),
                record.window.clone(),
                first_map,
                ready_layout,
            ))
        } else {
            None
        };

        if let Some((window_id, window, first_map, ready_layout)) = window_update {
            window.on_commit();

            let layout = ready_layout.or_else(|| {
                first_map
                    .then(|| self.planned_layout_for_surface(surface))
                    .flatten()
            });

            if let Some((location, size)) = layout {
                self.map_window_element(window.clone(), location);
                debug!(
                    window = %window_id.0,
                    location = ?location,
                    size = ?size,
                    "wm2 mapped window after commit"
                );
            }

            if first_map {
                info!(window = %window_id.0, "wm2 first map commit");
                let _ = self.runtime().execute(RuntimeCommand::SyncWindowMapped {
                    window_id,
                    mapped: true,
                });
                self.schedule_relayout();
                self.set_focus_with_new_serial(Some(surface.clone()));
            }
        }
    }
}
