use smithay::desktop::Window;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER, Serial};

use crate::actions::focus::FocusUpdate;
use crate::state::SpidersWm;
use spiders_core::WindowId;

impl SpidersWm {
    pub(crate) fn apply_focus_update(&mut self, focus_update: FocusUpdate) {
        if let FocusUpdate::Set(next_focus_window_id) = focus_update {
            self.apply_modeled_focus(next_focus_window_id, SERIAL_COUNTER.next_serial());
        }
    }

    pub(crate) fn apply_workspace_selection(
        &mut self,
        focused_window_id: Option<WindowId>,
        serial: Serial,
    ) {
        self.schedule_relayout();
        self.apply_modeled_focus(focused_window_id, serial);
    }

    pub(crate) fn apply_modeled_focus(
        &mut self,
        focused_window_id: Option<WindowId>,
        serial: Serial,
    ) {
        let focused_surface =
            focused_window_id.and_then(|window_id| self.surface_for_window_id(window_id));
        self.apply_backend_focus(focused_surface.clone(), serial);
        self.apply_window_activation(focused_surface.as_ref());
        self.refresh_titlebar_snapshot_and_overlays();
        if let Some(backend) = self.backend.as_ref() {
            backend.window().request_redraw();
        }
    }

    pub(crate) fn set_focus_with_new_serial(&mut self, surface: Option<WlSurface>) {
        self.set_focus(surface, SERIAL_COUNTER.next_serial());
    }

    pub fn set_focus(&mut self, surface: Option<WlSurface>, serial: Serial) {
        let focused_window_id = self.resolve_focus_window_id(surface.as_ref());
        let focused_window_id = self.update_modeled_focus(focused_window_id);
        self.apply_modeled_focus(focused_window_id, serial);
    }

    pub(crate) fn map_window_element(&mut self, window: Window, location: Point<i32, Logical>) {
        self.space.map_element(window, location, false);
    }

    pub(crate) fn raise_window_element(&mut self, window: &Window) {
        self.space.raise_element(window, true);
    }

    pub(crate) fn unmap_window_element(&mut self, window: &Window) {
        self.space.unmap_elem(window);
    }

    fn resolve_focus_window_id(&self, surface: Option<&WlSurface>) -> Option<WindowId> {
        surface.and_then(|surface| self.window_id_for_surface(surface))
    }

    fn update_modeled_focus(&mut self, focused_window_id: Option<WindowId>) -> Option<WindowId> {
        let (focused_window_id, events) = {
            let mut runtime = self.runtime();
            let focused_window_id = runtime
                .request_focus_window_selection("winit", focused_window_id)
                .focused_window_id;
            (focused_window_id, runtime.take_events())
        };
        self.broadcast_runtime_events(events);
        focused_window_id
    }

    pub(crate) fn apply_backend_focus(&mut self, surface: Option<WlSurface>, serial: Serial) {
        self.focused_surface = surface.clone();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, surface, serial);
        }
    }

    pub(crate) fn apply_window_activation(&self, focused_surface: Option<&WlSurface>) {
        for record in self.managed_windows() {
            let active = focused_surface.is_some_and(|focused| {
                record.window.toplevel().is_some_and(|toplevel| toplevel.wl_surface() == focused)
            });
            record.window.set_activated(active);
            if let Some(toplevel) = record.window.toplevel() {
                let _ = toplevel.send_pending_configure();
            }
        }
    }
}
