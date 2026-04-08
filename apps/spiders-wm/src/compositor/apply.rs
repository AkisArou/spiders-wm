use smithay::desktop::Window;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER, Serial};
use smithay::wayland::shell::wlr_layer::KeyboardInteractivity;
use tracing::debug;

use crate::actions::focus::FocusUpdate;
use crate::backend::BackendState;
use crate::state::SpidersWm;
use spiders_core::WindowId;

impl SpidersWm {
    pub(crate) fn request_backend_redraw(&mut self) {
        match self.backend.as_mut() {
            Some(BackendState::Winit(backend)) => backend.window().request_redraw(),
            Some(BackendState::Tty(_)) => self.schedule_tty_redraw(),
            None => {}
        }
    }

    pub(crate) fn apply_focus_update(&mut self, focus_update: FocusUpdate) {
        let summary = format!("{focus_update:?}");
        self.debug_protocol_event("focus-update", None, || summary);
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
        let started_at = std::time::Instant::now();
        let focus_summary = focused_window_id.as_ref().map(ToString::to_string);
        self.debug_protocol_event("apply-modeled-focus", focus_summary.as_deref(), || {
            format!("serial={serial:?}")
        });
        let focused_surface =
            focused_window_id.clone().and_then(|window_id| self.surface_for_window_id(window_id));
        let backend_focus_surface = self
            .preserved_layer_focus_surface_for_modeled_focus()
            .or(focused_surface.clone());
        self.apply_backend_focus(backend_focus_surface, serial);
        self.apply_window_activation(focused_surface.as_ref());
        debug!(
            focused_window = ?focused_window_id,
            elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
            "wm applied modeled focus"
        );
        self.request_backend_redraw();
    }

    pub fn set_focus(&mut self, surface: Option<WlSurface>, serial: Serial) {
        if surface.as_ref().is_some_and(|surface| self.is_layer_surface(surface)) {
            self.layer_shell_focus_surface = surface.clone();
            self.apply_backend_focus(surface, serial);
            self.apply_window_activation(self.modeled_focused_surface().as_ref());
            self.request_backend_redraw();
            return;
        }

        self.layer_shell_focus_surface = None;
        let focused_window_id = self.resolve_focus_window_id(surface.as_ref());
        let requested_window_id = focused_window_id.as_ref().map(ToString::to_string);
        self.debug_protocol_event("set-focus-request", requested_window_id.as_deref(), || {
            format!("serial={serial:?}")
        });
        let focused_window_id = self.update_modeled_focus(focused_window_id);
        self.apply_modeled_focus(focused_window_id, serial);
    }

    pub(crate) fn map_window_element(&mut self, window: Window, location: Point<i32, Logical>) {
        self.space.map_element(window, location, false);
        self.request_backend_redraw();
    }

    pub(crate) fn raise_window_element(&mut self, window: &Window) {
        self.space.raise_element(window, true);
        self.request_backend_redraw();
    }

    pub(crate) fn unmap_window_element(&mut self, window: &Window) {
        self.space.unmap_elem(window);
        self.request_backend_redraw();
    }

    fn resolve_focus_window_id(&self, surface: Option<&WlSurface>) -> Option<WindowId> {
        surface.and_then(|surface| self.window_id_for_surface(surface))
    }

    pub(crate) fn modeled_focused_surface(&self) -> Option<WlSurface> {
        self.model
            .focused_window_id()
            .cloned()
            .and_then(|window_id| self.surface_for_window_id(window_id))
    }

    fn preserved_layer_focus_surface_for_modeled_focus(&mut self) -> Option<WlSurface> {
        let preserved = self.focused_layer_surface().and_then(|layer_surface| {
            should_preserve_layer_focus_for_modeled_focus(Some(
                layer_surface.cached_state().keyboard_interactivity,
            ))
            .then(|| layer_surface.wl_surface().clone())
        });

        if preserved.is_none() {
            self.layer_shell_focus_surface = None;
        }

        preserved
    }

    pub(crate) fn refresh_keyboard_focus(&mut self, serial: Serial) {
        if let Some(layer_surface) = self.exclusive_keyboard_focus_layer_surface() {
            self.layer_shell_focus_surface = Some(layer_surface.wl_surface().clone());
            self.apply_backend_focus(self.layer_shell_focus_surface.clone(), serial);
            self.apply_window_activation(self.modeled_focused_surface().as_ref());
            self.request_backend_redraw();
            return;
        }

        if let Some(layer_surface) = self.focused_layer_surface()
            && !self.should_restore_layer_focus(Some(layer_surface.cached_state().keyboard_interactivity))
        {
            self.apply_backend_focus(self.layer_shell_focus_surface.clone(), serial);
            self.apply_window_activation(self.modeled_focused_surface().as_ref());
            self.request_backend_redraw();
            return;
        }

        self.layer_shell_focus_surface = None;
        let modeled_focus = self.modeled_focused_surface();
        self.apply_backend_focus(modeled_focus.clone(), serial);
        self.apply_window_activation(modeled_focus.as_ref());
        self.request_backend_redraw();
    }

    fn update_modeled_focus(&mut self, focused_window_id: Option<WindowId>) -> Option<WindowId> {
        let seat_name = self.active_backend_seat_name().to_string();
        let (focused_window_id, events) = {
            let mut runtime = self.runtime();
            let focused_window_id = runtime
                .request_focus_window_selection(seat_name.as_str(), focused_window_id)
                .focused_window_id;
            (focused_window_id, runtime.take_events())
        };
        self.broadcast_runtime_events(events);
        focused_window_id
    }

    pub(crate) fn apply_backend_focus(&mut self, surface: Option<WlSurface>, serial: Serial) {
        let backend_focus_window_id = self.resolve_focus_window_id(surface.as_ref());
        let backend_focus_window_id = backend_focus_window_id.as_ref().map(ToString::to_string);
        self.debug_protocol_event(
            "apply-backend-focus",
            backend_focus_window_id.as_deref(),
            || format!("serial={serial:?}"),
        );
        self.focused_surface = surface.clone();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, surface, serial);
        }
    }

    pub(crate) fn apply_window_activation(&self, focused_surface: Option<&WlSurface>) {
        let focused_window_id =
            self.resolve_focus_window_id(focused_surface).as_ref().map(ToString::to_string);
        self.debug_protocol_event("apply-window-activation", focused_window_id.as_deref(), || {
            format!("managed_windows={}", self.managed_windows().len())
        });
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

fn should_preserve_layer_focus_for_modeled_focus(
    keyboard_interactivity: Option<KeyboardInteractivity>,
) -> bool {
    matches!(keyboard_interactivity, Some(KeyboardInteractivity::Exclusive))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modeled_focus_preserves_exclusive_layer_focus() {
        assert!(should_preserve_layer_focus_for_modeled_focus(Some(
            KeyboardInteractivity::Exclusive,
        )));
    }

    #[test]
    fn modeled_focus_clears_on_demand_layer_focus() {
        assert!(!should_preserve_layer_focus_for_modeled_focus(Some(
            KeyboardInteractivity::OnDemand,
        )));
    }

    #[test]
    fn modeled_focus_clears_non_interactive_layer_focus() {
        assert!(!should_preserve_layer_focus_for_modeled_focus(Some(
            KeyboardInteractivity::None,
        )));
        assert!(!should_preserve_layer_focus_for_modeled_focus(None));
    }
}
