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

pub fn set_floating_rect(
    wm: &mut WmState,
    window_id: WindowId,
    rect: Rectangle<i32, Logical>,
) -> bool {
    let Some(window_state) = wm.windows.get_mut(&window_id) else {
        return false;
    };

    if window_state.mode() == (WindowMode::Floating { rect }) {
        return false;
    }

    window_state.set_mode(WindowMode::Floating { rect });
    true
}

#[cfg(test)]
mod tests {
    use smithay::utils::Rectangle;

    use super::set_floating_rect;
    use crate::model::{ManagedWindowState, WindowId, WindowMode, WmState, WorkspaceId};

    #[test]
    fn set_floating_rect_is_noop_when_rect_is_unchanged() {
        let mut wm = WmState::default();
        let window_id = WindowId::from("w1");
        let rect = Rectangle::new((10, 20).into(), (300, 200).into());
        wm.windows.insert(
            window_id.clone(),
            ManagedWindowState {
                id: window_id.clone(),
                workspace: WorkspaceId::from("ws-1"),
                output: None,
                mode: WindowMode::Floating { rect },
                mapped: true,
                app_id: None,
                title: None,
            },
        );

        assert!(!set_floating_rect(&mut wm, window_id, rect));
    }

    #[test]
    fn set_floating_rect_updates_when_rect_changes() {
        let mut wm = WmState::default();
        let window_id = WindowId::from("w1");
        let rect = Rectangle::new((10, 20).into(), (300, 200).into());
        let updated = Rectangle::new((10, 20).into(), (320, 220).into());
        wm.windows.insert(
            window_id.clone(),
            ManagedWindowState {
                id: window_id.clone(),
                workspace: WorkspaceId::from("ws-1"),
                output: None,
                mode: WindowMode::Floating { rect },
                mapped: true,
                app_id: None,
                title: None,
            },
        );

        assert!(set_floating_rect(&mut wm, window_id.clone(), updated));
        assert_eq!(
            wm.windows.get(&window_id).unwrap().mode(),
            WindowMode::Floating { rect: updated }
        );
    }
}

pub fn default_floating_rect() -> Rectangle<i32, Logical> {
    Rectangle::new(Point::from((80, 80)), Size::from((960, 640)))
}
