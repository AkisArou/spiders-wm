use smithay::desktop::WindowSurfaceType;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point};

use crate::model::WindowId;
use crate::state::{ManagedWindow, SpidersWm};

impl SpidersWm {
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(surface, point)| (surface, (point + location).to_f64()))
            })
    }

    pub fn window_id_for_surface(&self, surface: &WlSurface) -> Option<WindowId> {
        self.managed_window_for_surface(surface)
            .map(|record| record.id.clone())
    }

    pub fn managed_window_for_surface(&self, surface: &WlSurface) -> Option<&ManagedWindow> {
        self.managed_windows.iter().find(|record| {
            record
                .window
                .toplevel()
                .is_some_and(|toplevel| toplevel.wl_surface() == surface)
        })
    }

    pub fn managed_window_mut_for_surface(
        &mut self,
        surface: &WlSurface,
    ) -> Option<&mut ManagedWindow> {
        self.managed_windows.iter_mut().find(|record| {
            record
                .window
                .toplevel()
                .is_some_and(|toplevel| toplevel.wl_surface() == surface)
        })
    }

    pub fn managed_window_position_for_surface(&self, surface: &WlSurface) -> Option<usize> {
        self.managed_windows.iter().position(|record| {
            record
                .window
                .toplevel()
                .is_some_and(|toplevel| toplevel.wl_surface() == surface)
        })
    }

    pub fn surface_for_window_id(&self, window_id: WindowId) -> Option<WlSurface> {
        self.managed_windows
            .iter()
            .find(|record| record.id == window_id)
            .and_then(|record| {
                record
                    .window
                    .toplevel()
                    .map(|toplevel| toplevel.wl_surface().clone())
            })
    }

    pub fn window_id_under(&self, pos: Point<f64, Logical>) -> Option<WindowId> {
        self.space
            .element_under(pos)
            .and_then(|(window, _)| {
                window
                    .toplevel()
                    .map(|toplevel| toplevel.wl_surface().clone())
            })
            .and_then(|surface| self.window_id_for_surface(&surface))
    }

    pub fn visible_managed_window_positions(&self) -> Vec<usize> {
        self.managed_windows
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                self.model
                    .window_is_layout_eligible(&record.id)
                    .then_some(index)
            })
            .collect()
    }
}
