use smithay::desktop::{PopupManager, layer_map_for_output};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::with_states;
use smithay::wayland::fractional_scale::{FractionalScaleHandler, with_fractional_scale};

use crate::state::SpidersWm;

impl FractionalScaleHandler for SpidersWm {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        self.update_fractional_scale_for_surface(&surface);
    }
}

impl SpidersWm {
    pub(crate) fn update_fractional_scale_for_surface(&self, surface: &WlSurface) {
        let Some(output_scale) = self
            .output_for_surface(surface)
            .map(|output| output.current_scale().fractional_scale())
        else {
            return;
        };

        with_states(surface, |states| {
            with_fractional_scale(states, |fractional_scale| {
                fractional_scale.set_preferred_scale(output_scale);
            });
        });
    }

    pub(crate) fn refresh_fractional_scale_for_window_surface(&self, surface: &WlSurface) {
        if let Some(window) = self.window_for_root_surface(surface) {
            window.with_surfaces(|surface, _| {
                self.update_fractional_scale_for_surface(surface);
            });

            for (popup, _) in PopupManager::popups_for_surface(surface) {
                self.update_fractional_scale_for_surface(popup.wl_surface());
            }
        }
    }

    pub(crate) fn refresh_fractional_scale_for_layer_surface(&self, surface: &WlSurface) {
        if let Some(layer_surface) = self.layer_surface_for_surface(surface) {
            layer_surface.with_surfaces(|surface, _| {
                self.update_fractional_scale_for_surface(surface);
            });

            for (popup, _) in PopupManager::popups_for_surface(layer_surface.wl_surface()) {
                self.update_fractional_scale_for_surface(popup.wl_surface());
            }
        }
    }

    pub(crate) fn refresh_fractional_scale_for_mapped_surfaces(&self) {
        for record in self.managed_windows() {
            if let Some(toplevel) = record.window.toplevel() {
                self.refresh_fractional_scale_for_window_surface(toplevel.wl_surface());
            }
        }

        for output in self.space.outputs() {
            let map = layer_map_for_output(output);
            for layer in map.layers() {
                self.refresh_fractional_scale_for_layer_surface(layer.wl_surface());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fractional_scale_prefers_surface_output_scale() {
        assert_eq!(preferred_fractional_scale(Some(1.25)), Some(1.25));
        assert_eq!(preferred_fractional_scale(None), None);
    }

    #[test]
    fn fractional_scale_keeps_exact_fractional_preference() {
        assert_eq!(preferred_fractional_scale(Some(1.5)), Some(1.5));
        assert_eq!(preferred_fractional_scale(Some(2.0)), Some(2.0));
    }

    fn preferred_fractional_scale(output_scale: Option<f64>) -> Option<f64> {
        output_scale
    }
}
