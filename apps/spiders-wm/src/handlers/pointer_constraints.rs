use smithay::input::pointer::PointerHandle;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point};
use smithay::wayland::compositor::get_parent;
use smithay::wayland::pointer_constraints::{PointerConstraintsHandler, with_pointer_constraint};

use crate::state::SpidersWm;

impl PointerConstraintsHandler for SpidersWm {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        self.maybe_activate_pointer_constraint(surface, pointer);
    }

    fn cursor_position_hint(
        &mut self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
        location: Point<f64, Logical>,
    ) {
        let is_constraint_active = with_pointer_constraint(surface, pointer, |constraint| {
            constraint.is_some_and(|c| c.is_active())
        });
        if !is_constraint_active {
            return;
        }

        let Some(origin) = self.surface_origin(surface) else {
            return;
        };

        pointer.set_location(origin + location);
        self.pointer_location = origin + location;
        self.request_backend_redraw();
    }
}

impl SpidersWm {
    pub(crate) fn maybe_activate_pointer_constraint(
        &self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
    ) {
        let root = root_surface(surface);
        let focused_root = pointer.current_focus().as_ref().map(root_surface);
        if focused_root.as_ref() != Some(&root) {
            self.debug_protocol_event("pointer-constraint-skip-focus", None, || {
                format!(
                    "focused_root_matches={} pointer_location={:?}",
                    focused_root.as_ref() == Some(&root),
                    pointer.current_location()
                )
            });
            return;
        }

        let Some(origin) = self.surface_origin(&root) else {
            self.debug_protocol_event("pointer-constraint-skip-origin", None, || {
                format!("pointer_location={:?}", pointer.current_location())
            });
            return;
        };

        with_pointer_constraint(&root, pointer, |constraint| {
            let Some(constraint) = constraint else {
                self.debug_protocol_event("pointer-constraint-skip-missing", None, || {
                    format!("pointer_location={:?} origin={origin:?}", pointer.current_location())
                });
                return;
            };
            if constraint.is_active() {
                self.debug_protocol_event("pointer-constraint-already-active", None, || {
                    format!("pointer_location={:?} origin={origin:?}", pointer.current_location())
                });
                return;
            }

            if let Some(region) = constraint.region() {
                let pos_within_surface = pointer.current_location() - origin;
                if !region.contains(pos_within_surface.to_i32_round()) {
                    let rounded = pos_within_surface.to_i32_round::<i32>();
                    self.debug_protocol_event("pointer-constraint-skip-region", None, || {
                        format!(
                            "pointer_location={:?} origin={origin:?} pos_within_surface={pos_within_surface:?} rounded={:?}",
                            pointer.current_location(),
                            rounded
                        )
                    });
                    return;
                }
            }

            self.debug_protocol_event("pointer-constraint-activate", None, || {
                format!("pointer_location={:?} origin={origin:?}", pointer.current_location())
            });
            constraint.activate();
        });
    }

    pub(crate) fn surface_origin(&self, surface: &WlSurface) -> Option<Point<f64, Logical>> {
        let root = root_surface(surface);
        if let Some(window) = self.window_for_root_surface(&root) {
            return self.element_location(&window).map(|location| location.to_f64());
        }

        let layer_surface = self.layer_surface_for_surface(&root)?;
        let output = self.output_for_surface(&root)?;
        let map = smithay::desktop::layer_map_for_output(&output);
        map.layer_geometry(&layer_surface).map(|geometry| geometry.loc.to_f64())
    }
}

fn root_surface(surface: &WlSurface) -> WlSurface {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    root
}

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Point};

    #[test]
    fn pointer_constraint_activation_requires_focus() {
        assert!(!should_activate_constraint(false, true));
        assert!(should_activate_constraint(true, true));
    }

    #[test]
    fn pointer_constraint_activation_uses_root_surface_focus() {
        assert!(should_activate_constraint(true, true));
    }

    #[test]
    fn pointer_constraint_activation_requires_region_membership() {
        assert!(!should_activate_constraint(true, false));
    }

    fn should_activate_constraint(surface_is_focused: bool, pointer_is_in_region: bool) -> bool {
        surface_is_focused && pointer_is_in_region
    }

    #[test]
    fn cursor_hint_target_adds_surface_origin() {
        let origin = Point::<f64, Logical>::from((100.0, 50.0));
        let hint = Point::<f64, Logical>::from((12.0, 8.0));

        assert_eq!(origin + hint, Point::from((112.0, 58.0)));
    }
}
