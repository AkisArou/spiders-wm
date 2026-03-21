use std::collections::{HashMap, VecDeque};

use smithay::{
    desktop::Window,
    reexports::wayland_server::{backend::ObjectId, protocol::wl_surface::WlSurface, Resource},
    utils::{HookId, Logical, Serial},
    wayland::shell::xdg::ToplevelConfigure,
};

use crate::model::WindowId;

#[derive(Debug, Default)]
pub struct SmithayBindings {
    next_window_id: u64,
    surface_to_window: HashMap<ObjectId, WindowId>,
    window_to_surface: HashMap<WindowId, WlSurface>,
    window_to_element: HashMap<WindowId, Window>,
    window_to_last_configure_size: HashMap<WindowId, (i32, i32)>,
    window_to_commit_hook: HashMap<WindowId, HookId>,
    window_to_pending_commit_serials: HashMap<WindowId, VecDeque<Serial>>,
    window_to_acked_toplevel_configure: HashMap<WindowId, ToplevelConfigure>,
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

    pub fn bind_commit_hook(&mut self, window_id: WindowId, hook_id: HookId) {
        self.window_to_commit_hook.insert(window_id, hook_id);
    }

    pub fn unbind_window(&mut self, window_id: &WindowId) -> bool {
        let Some(surface) = self.window_to_surface.remove(window_id) else {
            self.window_to_element.remove(window_id);
            self.window_to_last_configure_size.remove(window_id);
            self.window_to_commit_hook.remove(window_id);
            self.window_to_pending_commit_serials.remove(window_id);
            self.window_to_acked_toplevel_configure.remove(window_id);
            return false;
        };

        self.surface_to_window.remove(&surface.id());
        self.window_to_element.remove(window_id);
        self.window_to_last_configure_size.remove(window_id);
        self.window_to_commit_hook.remove(window_id);
        self.window_to_pending_commit_serials.remove(window_id);
        self.window_to_acked_toplevel_configure.remove(window_id);
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

    pub fn record_pending_commit_serial(&mut self, window_id: &WindowId, serial: Serial) {
        self.window_to_pending_commit_serials
            .entry(window_id.clone())
            .or_default()
            .push_back(serial);
    }

    pub fn record_acked_toplevel_configure(
        &mut self,
        window_id: &WindowId,
        configure: ToplevelConfigure,
    ) {
        self.window_to_acked_toplevel_configure
            .insert(window_id.clone(), configure);
    }

    pub fn pending_commit_serials(&self, window_id: &WindowId) -> Vec<Serial> {
        self.window_to_pending_commit_serials
            .get(window_id)
            .map(|queue| queue.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn take_pending_commit_serials_through(
        &mut self,
        window_id: &WindowId,
        commit_serial: Serial,
    ) -> Vec<Serial> {
        let Some(queue) = self.window_to_pending_commit_serials.get_mut(window_id) else {
            return vec![];
        };

        let mut completed = Vec::new();
        while let Some(serial) = queue.front().copied() {
            if commit_serial.is_no_older_than(&serial) {
                queue.pop_front();
                completed.push(serial);
            } else {
                break;
            }
        }

        if queue.is_empty() {
            self.window_to_pending_commit_serials.remove(window_id);
        }

        completed
    }

    pub fn commit_hook(&self, window_id: &WindowId) -> Option<HookId> {
        self.window_to_commit_hook.get(window_id).cloned()
    }
}
