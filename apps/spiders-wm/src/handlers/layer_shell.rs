use smithay::backend::renderer::utils::with_renderer_surface_state;
use smithay::desktop::{LayerSurface, PopupKind, WindowSurfaceType, layer_map_for_output};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::compositor::{get_parent, with_states};
use smithay::wayland::shell::wlr_layer::{
    KeyboardInteractivity, LayerSurfaceData, WlrLayerShellHandler, WlrLayerShellState,
};
use smithay::wayland::shell::xdg::PopupSurface;

use crate::state::SpidersWm;

impl WlrLayerShellHandler for SpidersWm {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: smithay::wayland::shell::wlr_layer::LayerSurface,
        wl_output: Option<WlOutput>,
        _layer: smithay::wayland::shell::wlr_layer::Layer,
        namespace: String,
    ) {
        let output = wl_output
            .as_ref()
            .and_then(Output::from_resource)
            .or_else(|| self.primary_output_cloned());
        let Some(output) = output else {
            surface.send_close();
            return;
        };

        let mut map = layer_map_for_output(&output);
        let _ = map.map_layer(&LayerSurface::new(surface, namespace));
        drop(map);
        self.request_backend_redraw();
    }

    fn layer_destroyed(&mut self, surface: smithay::wayland::shell::wlr_layer::LayerSurface) {
        let focused_surface = (self.layer_shell_focus_surface.as_ref()
            == Some(surface.wl_surface()))
        .then(|| surface.wl_surface().clone());
        for output in self.space.outputs().cloned().collect::<Vec<_>>() {
            let mut map = layer_map_for_output(&output);
            let layer = map.layers().find(|layer| layer.layer_surface() == &surface).cloned();
            if let Some(layer) = layer {
                map.unmap_layer(&layer);
            }
        }
        if focused_surface.is_some() {
            self.refresh_keyboard_focus(smithay::utils::SERIAL_COUNTER.next_serial());
        }
        self.request_backend_redraw();
    }

    fn new_popup(
        &mut self,
        _parent: smithay::wayland::shell::wlr_layer::LayerSurface,
        popup: PopupSurface,
    ) {
        self.unconstrain_popup(&popup);
        let _ = self.popups.track_popup(PopupKind::Xdg(popup));
    }
}

impl SpidersWm {
    pub(crate) fn layer_shell_handle_commit(&mut self, surface: &WlSurface) -> bool {
        let mut root_surface = surface.clone();
        while let Some(parent) = get_parent(&root_surface) {
            root_surface = parent;
        }

        let output = self
            .space
            .outputs()
            .find(|output| {
                let map = layer_map_for_output(output);
                map.layer_for_surface(&root_surface, WindowSurfaceType::TOPLEVEL).is_some()
            })
            .cloned();
        let Some(output) = output else {
            return false;
        };

        if surface != &root_surface {
            self.request_backend_redraw();
            return true;
        }

        let mut map = layer_map_for_output(&output);
        let previous_non_exclusive_zone = map.non_exclusive_zone();
        map.arrange();
        let non_exclusive_zone_changed =
            did_non_exclusive_zone_change(previous_non_exclusive_zone, map.non_exclusive_zone());

        let layer = map.layer_for_surface(&root_surface, WindowSurfaceType::TOPLEVEL).cloned();
        let Some(layer) = layer else {
            return false;
        };

        let is_mapped =
            with_renderer_surface_state(&root_surface, |state| state.buffer().is_some())
                .unwrap_or(false);

        let initial_configure_sent = with_states(&root_surface, |states| {
            states
                .data_map
                .get::<LayerSurfaceData>()
                .and_then(|data| data.lock().ok())
                .map(|attributes| attributes.initial_configure_sent)
                .unwrap_or(false)
        });

        if !initial_configure_sent {
            layer.layer_surface().send_configure();
        } else {
            let _ = layer.layer_surface().send_pending_configure();
        }

        let keyboard_interactivity = layer.cached_state().keyboard_interactivity;
        self.refresh_fractional_scale_for_layer_surface(&root_surface);
        if is_mapped && matches!(keyboard_interactivity, KeyboardInteractivity::Exclusive) {
            self.set_focus(
                Some(root_surface.clone()),
                smithay::utils::SERIAL_COUNTER.next_serial(),
            );
        } else if self
            .layer_shell_focus_surface
            .as_ref()
            .is_some_and(|focused| self.layer_surface_for_surface(focused).as_ref() == Some(&layer))
            && (!is_mapped || self.should_restore_layer_focus(Some(keyboard_interactivity)))
        {
            self.refresh_keyboard_focus(smithay::utils::SERIAL_COUNTER.next_serial());
        }

        drop(map);
        if non_exclusive_zone_changed {
            self.queue_relayout();
        }
        self.request_backend_redraw();
        true
    }
}

fn did_non_exclusive_zone_change(
    previous: Rectangle<i32, Logical>,
    next: Rectangle<i32, Logical>,
) -> bool {
    previous != next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_exclusive_zone_change_detects_new_exclusive_bar() {
        let previous = Rectangle::new((0, 0).into(), (1280, 800).into());
        let next = Rectangle::new((0, 32).into(), (1280, 768).into());

        assert!(did_non_exclusive_zone_change(previous, next));
    }

    #[test]
    fn non_exclusive_zone_change_ignores_stable_zone() {
        let zone = Rectangle::new((0, 32).into(), (1280, 768).into());

        assert!(!did_non_exclusive_zone_change(zone, zone));
    }
}
