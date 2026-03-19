use std::collections::HashMap;

use smithay::{
    desktop::Window,
    reexports::wayland_server::{backend::ObjectId, protocol::wl_surface::WlSurface, Resource},
    utils::Logical,
};

use crate::model::WindowId;

#[derive(Debug, Default)]
pub struct SmithayBindings {
    next_window_id: u64,
    surface_to_window: HashMap<ObjectId, WindowId>,
    window_to_surface: HashMap<WindowId, WlSurface>,
    window_to_element: HashMap<WindowId, Window>,
    window_to_last_configure_size: HashMap<WindowId, (i32, i32)>,
}

impl SmithayBindings {
    pub fn alloc_window_id(&mut self) -> WindowId {
        self.next_window_id += 1;
        WindowId::from(self.next_window_id.to_string())
    }

    pub fn bind_surface(&mut self, surface: WlSurface, window_id: WindowId) {
        self.surface_to_window
            .insert(surface.id(), window_id.clone());
        self.window_to_surface.insert(window_id, surface);
    }

    pub fn bind_window_element(&mut self, window_id: WindowId, window: Window) {
        self.window_to_element.insert(window_id, window);
    }

    pub fn unbind_window(&mut self, window_id: &WindowId) -> bool {
        let Some(surface) = self.window_to_surface.remove(window_id) else {
            self.window_to_element.remove(window_id);
            self.window_to_last_configure_size.remove(window_id);
            return false;
        };

        self.surface_to_window.remove(&surface.id());
        self.window_to_element.remove(window_id);
        self.window_to_last_configure_size.remove(window_id);
        true
    }

    pub fn window_for_surface(&self, surface_id: &ObjectId) -> Option<WindowId> {
        self.surface_to_window.get(surface_id).cloned()
    }

    pub fn surface_for_window(&self, window_id: &WindowId) -> Option<WlSurface> {
        self.window_to_surface.get(window_id).cloned()
    }

    pub fn element_for_window(&self, window_id: &WindowId) -> Option<Window> {
        self.window_to_element.get(window_id).cloned()
    }

    pub fn known_windows(&self) -> Vec<WindowId> {
        self.window_to_element.keys().cloned().collect()
    }

    pub fn last_configure_size(&self, window_id: &WindowId) -> Option<(i32, i32)> {
        self.window_to_last_configure_size.get(window_id).copied()
    }

    pub fn record_configure_size(
        &mut self,
        window_id: &WindowId,
        size: smithay::utils::Size<i32, Logical>,
    ) {
        self.window_to_last_configure_size
            .insert(window_id.clone(), (size.w, size.h));
    }
}
