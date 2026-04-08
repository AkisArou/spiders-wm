use smithay::desktop::{
    LayerSurface, PopupKind, WindowSurfaceType, get_popup_toplevel_coords, layer_map_for_output,
};
use smithay::utils::{Logical, Point, Rectangle, Size};
use smithay::wayland::shell::xdg::PopupSurface;

use crate::state::SpidersWm;

impl SpidersWm {
    pub fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = smithay::desktop::find_popup_root_surface(&PopupKind::Xdg(popup.clone()))
        else {
            return;
        };

        if let Some(window) = self.window_for_root_surface(&root) {
            self.unconstrain_window_popup(popup, &window);
            return;
        }

        if let Some((layer_surface, output)) = self.space.outputs().find_map(|output| {
            let map = layer_map_for_output(output);
            let layer_surface = map.layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)?;
            Some((layer_surface.clone(), output.clone()))
        }) {
            self.unconstrain_layer_popup(popup, &layer_surface, &output);
        }
    }

    fn unconstrain_window_popup(&self, popup: &PopupSurface, window: &smithay::desktop::Window) {
        let toplevel = window.toplevel().expect("window missing toplevel");
        let output_geo = self
            .output_for_surface(toplevel.wl_surface())
            .and_then(|output| self.output_geometry_for(&output))
            .or_else(|| self.current_output_geometry())
            .expect("output geometry missing");
        let window_geo =
            self.element_geometry(window).unwrap_or(Rectangle::new((0, 0).into(), (0, 0).into()));

        let target = popup_target_for_window(
            output_geo,
            window_geo,
            get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone())),
        );

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }

    fn unconstrain_layer_popup(
        &self,
        popup: &PopupSurface,
        layer_surface: &LayerSurface,
        output: &smithay::output::Output,
    ) {
        let output_geo = self.output_geometry_for(output).expect("output geometry missing");
        let map = layer_map_for_output(output);
        let Some(layer_geo) = map.layer_geometry(layer_surface) else {
            return;
        };

        let target = popup_target_for_layer(
            output_geo.size,
            layer_geo.loc,
            get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone())),
        );

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

fn popup_target_for_window(
    output_geo: Rectangle<i32, Logical>,
    window_geo: Rectangle<i32, Logical>,
    popup_toplevel_coords: Point<i32, Logical>,
) -> Rectangle<i32, Logical> {
    let mut target = output_geo;
    target.loc -= popup_toplevel_coords;
    target.loc -= window_geo.loc;
    target
}

fn popup_target_for_layer(
    output_size: Size<i32, Logical>,
    layer_location: Point<i32, Logical>,
    popup_toplevel_coords: Point<i32, Logical>,
) -> Rectangle<i32, Logical> {
    let mut target = Rectangle::from_size(output_size);
    target.loc -= layer_location;
    target.loc -= popup_toplevel_coords;
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_target_for_window_is_relative_to_parent_window() {
        let output_geo = Rectangle::new((0, 0).into(), (1280, 800).into());
        let window_geo = Rectangle::new((100, 200).into(), (500, 400).into());
        let popup_toplevel_coords = Point::from((20, 30));

        let target = popup_target_for_window(output_geo, window_geo, popup_toplevel_coords);

        assert_eq!(target.loc, Point::from((-120, -230)));
        assert_eq!(target.size, Size::from((1280, 800)));
    }

    #[test]
    fn popup_target_for_layer_is_relative_to_layer_surface() {
        let output_size = Size::from((1280, 800));
        let layer_location = Point::from((0, 32));
        let popup_toplevel_coords = Point::from((10, 4));

        let target = popup_target_for_layer(output_size, layer_location, popup_toplevel_coords);

        assert_eq!(target.loc, Point::from((-10, -36)));
        assert_eq!(target.size, Size::from((1280, 800)));
    }
}
