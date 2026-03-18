use std::collections::HashMap;

use smithay::reexports::wayland_server::backend::ObjectId;

use crate::state::WindowId;

#[derive(Debug, Default)]
pub struct SmithayBindings {
    next_window_id: u64,
    surface_to_window: HashMap<ObjectId, WindowId>,
}

impl SmithayBindings {
    pub fn alloc_window_id(&mut self) -> WindowId {
        self.next_window_id += 1;
        WindowId(self.next_window_id)
    }

    pub fn bind_surface(&mut self, surface_id: ObjectId, window_id: WindowId) {
        self.surface_to_window.insert(surface_id, window_id);
    }

    pub fn window_for_surface(&self, surface_id: &ObjectId) -> Option<WindowId> {
        self.surface_to_window.get(surface_id).copied()
    }

    pub fn unbind_surface(&mut self, surface_id: &ObjectId) -> Option<WindowId> {
        self.surface_to_window.remove(surface_id)
    }
}
