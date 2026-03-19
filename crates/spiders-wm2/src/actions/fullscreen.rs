use crate::model::{WindowMode, WmState};

pub fn toggle_fullscreen_focused_window(wm: &mut WmState) {
    let Some(window_id) = wm.focused_window.clone() else {
        return;
    };

    let Some(window_state) = wm.windows.get_mut(&window_id) else {
        return;
    };

    window_state.set_mode(match window_state.mode() {
        WindowMode::Fullscreen => WindowMode::Tiled,
        WindowMode::Tiled | WindowMode::Floating { .. } => WindowMode::Fullscreen,
    });
}
