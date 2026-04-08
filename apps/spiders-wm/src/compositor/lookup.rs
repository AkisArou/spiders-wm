use smithay::desktop::LayerSurface;
use smithay::desktop::layer_map_for_output;
use smithay::desktop::{Window, WindowSurfaceType};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle};
use smithay::wayland::compositor::get_parent;

use crate::state::{ManagedWindow, SpidersWm};
use spiders_core::wm::WindowGeometry;
use spiders_core::{WindowId, WorkspaceId};

impl SpidersWm {
    pub fn primary_output(&self) -> Option<&Output> {
        self.space.outputs().next()
    }

    pub fn primary_output_cloned(&self) -> Option<Output> {
        self.primary_output().cloned()
    }

    pub fn output_for_surface(&self, surface: &WlSurface) -> Option<Output> {
        let window = self.window_for_root_surface(surface)?;
        let geometry = self.element_geometry(&window)?;

        self.space
            .outputs()
            .find_map(|output| {
                self.space.output_geometry(output).and_then(|output_geometry| {
                    rects_intersect(output_geometry, geometry).then(|| output.clone())
                })
            })
            .or_else(|| self.primary_output_cloned())
    }

    pub fn managed_window_count(&self) -> usize {
        self.managed_windows.len()
    }

    pub fn managed_windows(&self) -> &[ManagedWindow] {
        &self.managed_windows
    }

    pub fn managed_window_ids(&self) -> Vec<WindowId> {
        self.managed_windows.iter().map(|record| record.id.clone()).collect()
    }

    pub fn insert_managed_window(&mut self, window_id: WindowId, window: Window) {
        self.managed_windows.push(ManagedWindow {
            id: window_id,
            window,
            mapped: false,
            frame_sync: Default::default(),
        });
    }

    pub fn remove_managed_window_at(&mut self, index: usize) -> ManagedWindow {
        self.managed_windows.remove(index)
    }

    pub fn visible_managed_window_ids(&self) -> Vec<WindowId> {
        self.visible_managed_window_positions()
            .into_iter()
            .filter_map(|index| {
                self.managed_window_at(index).and_then(|record| {
                    (!self.window_is_closing(&record.id)).then_some(record.id.clone())
                })
            })
            .collect()
    }

    pub fn managed_window_at(&self, index: usize) -> Option<&ManagedWindow> {
        self.managed_windows.get(index)
    }

    pub fn managed_window_for_id(&self, window_id: &WindowId) -> Option<&ManagedWindow> {
        self.managed_windows.iter().find(|record| &record.id == window_id)
    }

    pub fn managed_window_mut_for_id(
        &mut self,
        window_id: &WindowId,
    ) -> Option<&mut ManagedWindow> {
        self.managed_windows.iter_mut().find(|record| &record.id == window_id)
    }

    pub fn swap_managed_window_positions(&mut self, first_index: usize, second_index: usize) {
        self.managed_windows.swap(first_index, second_index);
    }

    pub fn current_output_cloned(&self) -> Option<Output> {
        self.primary_output_cloned()
    }

    pub fn current_workspace_id(&self) -> Option<&WorkspaceId> {
        self.model.current_workspace_id.as_ref()
    }

    pub fn window_workspace_id(&self, window_id: &WindowId) -> Option<WorkspaceId> {
        self.model.windows.get(window_id).and_then(|window| window.workspace_id.clone())
    }

    pub fn window_is_on_current_workspace(&self, window_id: &WindowId) -> bool {
        self.model.window_is_on_current_workspace(window_id.clone())
    }

    pub fn window_is_closing(&self, window_id: &WindowId) -> bool {
        self.model.windows.get(window_id).is_some_and(|window| window.closing)
    }

    pub fn window_is_floating(&self, window_id: &WindowId) -> bool {
        self.model.windows.get(window_id).is_some_and(|window| window.floating)
    }

    pub fn window_floating_geometry(&self, window_id: &WindowId) -> Option<WindowGeometry> {
        self.model.floating_geometry(window_id)
    }

    pub fn current_output_geometry(&self) -> Option<Rectangle<i32, Logical>> {
        let output = self.primary_output()?;
        self.space.output_geometry(output)
    }

    pub fn output_union_geometry(&self) -> Option<Rectangle<i32, Logical>> {
        let mut outputs = self.space.outputs();
        let first = outputs.next().and_then(|output| self.space.output_geometry(output))?;
        Some(outputs.fold(first, |union, output| {
            self.space
                .output_geometry(output)
                .map(|geometry| union.merge(geometry))
                .unwrap_or(union)
        }))
    }

    pub fn output_for_window_id(&self, window_id: &WindowId) -> Option<Output> {
        let surface = self.surface_for_window_id(window_id.clone())?;
        self.output_for_surface(&surface)
    }

    pub fn output_geometry_for(&self, output: &Output) -> Option<Rectangle<i32, Logical>> {
        self.space.output_geometry(output)
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        if let Some((surface, point)) = self.layer_surface_under(pos) {
            return Some((surface, point));
        }
        self.space.element_under(pos).and_then(|(window, location)| {
            window
                .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(surface, point)| (surface, (point + location).to_f64()))
        })
    }

    pub fn window_under(&self, pos: Point<f64, Logical>) -> Option<Window> {
        self.space.element_under(pos).map(|(window, _)| window.clone())
    }

    pub fn layer_surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        for output in self.space.output_under(pos) {
            let map = layer_map_for_output(output);
            for layer in map.layers().rev() {
                let Some(geometry) = map.layer_geometry(layer) else {
                    continue;
                };
                if let Some((surface, point)) =
                    layer.surface_under(pos - geometry.loc.to_f64(), WindowSurfaceType::ALL)
                {
                    return Some((surface, (point + geometry.loc).to_f64()));
                }
            }
        }
        None
    }

    pub fn is_layer_surface(&self, surface: &WlSurface) -> bool {
        self.layer_surface_for_surface(surface).is_some()
    }

    pub fn exclusive_keyboard_focus_layer_surface(&self) -> Option<LayerSurface> {
        let focused_output = self
            .focused_layer_surface()
            .and_then(|layer| {
                self.space
                    .outputs()
                    .find(|output| {
                        let map = layer_map_for_output(output);
                        map.layer_for_surface(layer.wl_surface(), WindowSurfaceType::TOPLEVEL)
                            .is_some()
                    })
                    .cloned()
            })
            .or_else(|| {
                self.layer_shell_focus_surface.as_ref().and_then(|surface| {
                    self.space
                        .outputs()
                        .find(|output| {
                            let map = layer_map_for_output(output);
                            map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).is_some()
                        })
                        .cloned()
                })
            })
            .or_else(|| {
                self.focused_surface.as_ref().and_then(|surface| self.output_for_surface(surface))
            })
            .or_else(|| self.current_output_cloned());

        let focused_output = focused_output?;
        let map = layer_map_for_output(&focused_output);

        map.layers()
            .rev()
            .find(|layer| {
                layer.cached_state().keyboard_interactivity
                    == smithay::wayland::shell::wlr_layer::KeyboardInteractivity::Exclusive
            })
            .cloned()
    }

    pub fn focused_layer_surface(&self) -> Option<LayerSurface> {
        self.layer_shell_focus_surface
            .as_ref()
            .and_then(|surface| self.layer_surface_for_surface(surface))
    }

    pub fn should_restore_layer_focus(
        &self,
        focused_layer_keyboard_interactivity: Option<
            smithay::wayland::shell::wlr_layer::KeyboardInteractivity,
        >,
    ) -> bool {
        if self.exclusive_keyboard_focus_layer_surface().is_some() {
            return false;
        }

        !matches!(
            focused_layer_keyboard_interactivity,
            Some(
                smithay::wayland::shell::wlr_layer::KeyboardInteractivity::Exclusive
                    | smithay::wayland::shell::wlr_layer::KeyboardInteractivity::OnDemand
            )
        )
    }

    pub fn layer_surface_for_surface(&self, surface: &WlSurface) -> Option<LayerSurface> {
        let root_surface = root_surface(surface);

        self.space.outputs().find_map(|output| {
            let map = layer_map_for_output(output);
            map.layer_for_surface(&root_surface, WindowSurfaceType::TOPLEVEL).cloned()
        })
    }

    pub fn window_for_root_surface(&self, root: &WlSurface) -> Option<Window> {
        self.space.elements().find_map(|window| {
            window
                .toplevel()
                .is_some_and(|toplevel| toplevel.wl_surface() == root)
                .then(|| window.clone())
        })
    }

    pub fn element_location(&self, window: &Window) -> Option<Point<i32, Logical>> {
        self.space.element_location(window)
    }

    pub fn element_geometry(&self, window: &Window) -> Option<Rectangle<i32, Logical>> {
        self.space.element_geometry(window)
    }

    pub fn window_id_for_surface(&self, surface: &WlSurface) -> Option<WindowId> {
        self.managed_window_for_surface(surface).map(|record| record.id.clone())
    }

    pub fn managed_window_for_surface(&self, surface: &WlSurface) -> Option<&ManagedWindow> {
        self.managed_windows.iter().find(|record| {
            record.window.toplevel().is_some_and(|toplevel| toplevel.wl_surface() == surface)
        })
    }

    pub fn managed_window_mut_for_surface(
        &mut self,
        surface: &WlSurface,
    ) -> Option<&mut ManagedWindow> {
        self.managed_windows.iter_mut().find(|record| {
            record.window.toplevel().is_some_and(|toplevel| toplevel.wl_surface() == surface)
        })
    }

    pub fn managed_window_position_for_surface(&self, surface: &WlSurface) -> Option<usize> {
        self.managed_windows.iter().position(|record| {
            record.window.toplevel().is_some_and(|toplevel| toplevel.wl_surface() == surface)
        })
    }

    pub fn surface_for_window_id(&self, window_id: WindowId) -> Option<WlSurface> {
        self.managed_window_for_id(&window_id).and_then(|record| {
            record.window.toplevel().map(|toplevel| toplevel.wl_surface().clone())
        })
    }

    pub fn window_id_under(&self, pos: Point<f64, Logical>) -> Option<WindowId> {
        self.window_under(pos)
            .and_then(|window| window.toplevel().map(|toplevel| toplevel.wl_surface().clone()))
            .and_then(|surface| self.window_id_for_surface(&surface))
    }

    pub fn visible_managed_window_positions(&self) -> Vec<usize> {
        self.managed_windows()
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                (record.mapped && self.model.window_is_layout_eligible(&record.id)).then_some(index)
            })
            .collect()
    }
}

fn root_surface(surface: &WlSurface) -> WlSurface {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    root
}

fn rects_intersect(left: Rectangle<i32, Logical>, right: Rectangle<i32, Logical>) -> bool {
    let left_x2 = left.loc.x + left.size.w;
    let left_y2 = left.loc.y + left.size.h;
    let right_x2 = right.loc.x + right.size.w;
    let right_y2 = right.loc.y + right.size.h;

    left.loc.x < right_x2 && right.loc.x < left_x2 && left.loc.y < right_y2 && right.loc.y < left_y2
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::wayland::shell::wlr_layer::KeyboardInteractivity;

    fn should_restore_layer_focus(
        focused_layer_keyboard_interactivity: Option<KeyboardInteractivity>,
        has_exclusive_layer: bool,
    ) -> bool {
        if has_exclusive_layer {
            return false;
        }

        !matches!(
            focused_layer_keyboard_interactivity,
            Some(KeyboardInteractivity::Exclusive | KeyboardInteractivity::OnDemand)
        )
    }

    #[test]
    fn rects_intersect_detects_overlap() {
        let left = Rectangle::new((0, 0).into(), (100, 100).into());
        let right = Rectangle::new((90, 10).into(), (50, 50).into());

        assert!(rects_intersect(left, right));
    }

    #[test]
    fn rects_intersect_rejects_touching_edges_without_overlap() {
        let left = Rectangle::new((0, 0).into(), (100, 100).into());
        let right = Rectangle::new((100, 0).into(), (50, 50).into());

        assert!(!rects_intersect(left, right));
    }

    #[test]
    fn layer_focus_restore_keeps_explicit_on_demand_focus_without_exclusive_override() {
        assert!(!should_restore_layer_focus(Some(KeyboardInteractivity::OnDemand), false));
    }

    #[test]
    fn layer_focus_restore_prefers_new_exclusive_layer() {
        assert!(!should_restore_layer_focus(Some(KeyboardInteractivity::OnDemand), true));
    }

    #[test]
    fn layer_focus_restore_keeps_exclusive_focus() {
        assert!(!should_restore_layer_focus(Some(KeyboardInteractivity::Exclusive), false));
    }

    #[test]
    fn layer_focus_restore_drops_non_interactive_layer_focus() {
        assert!(should_restore_layer_focus(Some(KeyboardInteractivity::None), false));
        assert!(should_restore_layer_focus(None, false));
    }
}
