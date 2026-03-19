use crate::model::{WindowId, WindowMode, WmState};

use smithay::utils::{Logical, Point, Rectangle, Size};

pub fn toggle_floating_focused_window(wm: &mut WmState) {
    let Some(window_id) = wm.focused_window.clone() else {
        return;
    };

    let Some(window_state) = wm.windows.get_mut(&window_id) else {
        return;
    };

    window_state.set_mode(match window_state.mode() {
        WindowMode::Floating { .. } => WindowMode::Tiled,
        WindowMode::Tiled | WindowMode::Fullscreen => WindowMode::Floating {
            rect: default_floating_rect(),
        },
    });
}

pub fn set_floating_rect(wm: &mut WmState, window_id: WindowId, rect: Rectangle<i32, Logical>) {
    let Some(window_state) = wm.windows.get_mut(&window_id) else {
        return;
    };

    window_state.set_mode(WindowMode::Floating { rect });
}

pub fn default_floating_rect() -> Rectangle<i32, Logical> {
    Rectangle::new(Point::from((80, 80)), Size::from((960, 640)))
}
