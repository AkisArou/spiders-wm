use smithay::desktop::{Window, WindowSurfaceType};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle};

use crate::state::{ManagedWindow, SpidersWm};
use spiders_core::wm::WindowGeometry;
use spiders_core::{WindowId, WorkspaceId};

impl SpidersWm {
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
            .filter_map(|index| self.managed_window_at(index).map(|record| record.id.clone()))
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
        self.space.outputs().next().cloned()
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
        let output = self.space.outputs().next()?;
        self.space.output_geometry(output)
    }

    pub fn output_geometry_for(&self, output: &Output) -> Option<Rectangle<i32, Logical>> {
        self.space.output_geometry(output)
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space.element_under(pos).and_then(|(window, location)| {
            window
                .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(surface, point)| (surface, (point + location).to_f64()))
        })
    }

    pub fn window_under(&self, pos: Point<f64, Logical>) -> Option<Window> {
        self.space.element_under(pos).map(|(window, _)| window.clone())
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
                self.model.window_is_layout_eligible(&record.id).then_some(index)
            })
            .collect()
    }
}
