use smithay::{
    reexports::wayland_server::Resource,
    utils::{Logical, Point},
};

use crate::{
    actions,
    model::{FloatingDragState, FloatingResizeState, PointerInteraction, WindowMode},
    runtime::SpidersWm2,
};

impl SpidersWm2 {
    pub fn begin_floating_drag(
        &mut self,
        surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        pointer_location: Point<f64, Logical>,
    ) {
        let Some(window_id) = self.app.bindings.window_for_surface(&surface.id()) else {
            return;
        };

        let Some(WindowMode::Floating { rect }) = self
            .app
            .wm
            .windows
            .get(&window_id)
            .map(|window| window.mode())
        else {
            return;
        };

        self.runtime.pointer_interaction = Some(PointerInteraction::Move(FloatingDragState {
            window_id,
            pointer_offset: pointer_location - rect.loc.to_f64(),
        }));
    }

    pub fn update_floating_drag(&mut self, pointer_location: Point<f64, Logical>) {
        let Some(PointerInteraction::Move(ref drag)) = self.runtime.pointer_interaction else {
            return;
        };

        let Some(WindowMode::Floating { rect }) = self
            .app
            .wm
            .windows
            .get(&drag.window_id)
            .map(|window| window.mode())
        else {
            self.runtime.pointer_interaction = None;
            return;
        };

        let new_loc = pointer_location - drag.pointer_offset;
        let updated_rect = smithay::utils::Rectangle::new(new_loc.to_i32_round(), rect.size);

        actions::set_floating_rect(&mut self.app.wm, drag.window_id.clone(), updated_rect);
        self.refresh_active_workspace();
    }

    pub fn end_floating_drag(&mut self) {
        if matches!(
            self.runtime.pointer_interaction,
            Some(PointerInteraction::Move(_))
        ) {
            self.runtime.pointer_interaction = None;
        }
    }

    pub fn begin_floating_resize(
        &mut self,
        surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        pointer_location: Point<f64, Logical>,
    ) {
        let Some(window_id) = self.app.bindings.window_for_surface(&surface.id()) else {
            return;
        };

        let Some(WindowMode::Floating { rect }) = self
            .app
            .wm
            .windows
            .get(&window_id)
            .map(|window| window.mode())
        else {
            return;
        };

        self.runtime.pointer_interaction = Some(PointerInteraction::Resize(FloatingResizeState {
            window_id,
            pointer_origin: pointer_location,
            initial_rect: rect,
        }));
    }

    pub fn update_floating_resize(&mut self, pointer_location: Point<f64, Logical>) {
        let Some(PointerInteraction::Resize(ref resize)) = self.runtime.pointer_interaction else {
            return;
        };

        let Some(WindowMode::Floating { .. }) = self
            .app
            .wm
            .windows
            .get(&resize.window_id)
            .map(|window| window.mode())
        else {
            self.runtime.pointer_interaction = None;
            return;
        };

        let delta = pointer_location - resize.pointer_origin;
        let width = (resize.initial_rect.size.w as f64 + delta.x).max(160.0) as i32;
        let height = (resize.initial_rect.size.h as f64 + delta.y).max(120.0) as i32;

        let updated_rect = smithay::utils::Rectangle::new(
            resize.initial_rect.loc,
            smithay::utils::Size::from((width, height)),
        );

        actions::set_floating_rect(&mut self.app.wm, resize.window_id.clone(), updated_rect);
        self.refresh_active_workspace();
    }

    pub fn end_floating_resize(&mut self) {
        if matches!(
            self.runtime.pointer_interaction,
            Some(PointerInteraction::Resize(_))
        ) {
            self.runtime.pointer_interaction = None;
        }
    }
}
