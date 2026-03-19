use std::collections::HashSet;

use smithay::utils::{Logical, Rectangle};

use crate::{
    app::AppState,
    model::{WindowId, WindowMode},
};

pub fn focused_fullscreen_window(app: &AppState) -> Option<WindowId> {
    app.wm.focused_window.clone().filter(|window_id| {
        matches!(
            app.wm.windows.get(window_id).map(|window| window.mode()),
            Some(WindowMode::Fullscreen)
        )
    })
}

pub fn desired_window_rect(
    app: &AppState,
    output_rect: Option<Rectangle<i32, Logical>>,
    window_id: &WindowId,
) -> Option<Rectangle<i32, Logical>> {
    let window = app.wm.windows.get(window_id)?;
    let tiled_rect = app
        .layout
        .desired_tiled_rect(window_id)
        .unwrap_or_else(default_tiled_rect);
    window.rect(output_rect, tiled_rect)
}

pub fn committed_window_rect(
    app: &AppState,
    output_rect: Option<Rectangle<i32, Logical>>,
    window_id: &WindowId,
) -> Option<Rectangle<i32, Logical>> {
    let window = app.wm.windows.get(window_id)?;
    let tiled_rect = app
        .layout
        .committed_tiled_rect(window_id)
        .unwrap_or_else(default_tiled_rect);
    window.rect(output_rect, tiled_rect)
}

pub fn window_is_visible(
    app: &AppState,
    visible: &HashSet<WindowId>,
    window_id: &WindowId,
) -> bool {
    if let Some(fullscreen_window) = focused_fullscreen_window(app) {
        return visible.contains(window_id) && fullscreen_window == *window_id;
    }

    visible.contains(window_id)
}

fn default_tiled_rect() -> Rectangle<i32, Logical> {
    Rectangle::new((0, 0).into(), (960, 640).into())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use smithay::utils::Rectangle;

    use super::{committed_window_rect, focused_fullscreen_window, window_is_visible};
    use crate::{
        app::AppState,
        model::{ManagedWindowState, WindowId, WindowMode, WorkspaceId},
    };

    #[test]
    fn fullscreen_window_is_the_only_visible_window() {
        let mut app = AppState::default();
        let workspace = app.wm.active_workspace.clone();
        let fullscreen = WindowId::from("1");
        let other = WindowId::from("2");

        app.wm.focused_window = Some(fullscreen.clone());
        app.wm.workspaces.get_mut(&workspace).unwrap().windows =
            vec![fullscreen.clone(), other.clone()];
        app.wm.windows.insert(
            fullscreen.clone(),
            ManagedWindowState {
                id: fullscreen.clone(),
                workspace: workspace.clone(),
                output: None,
                mode: WindowMode::Fullscreen,
                mapped: true,
                app_id: None,
                title: None,
            },
        );
        app.wm.windows.insert(
            other.clone(),
            ManagedWindowState::tiled(other.clone(), workspace, None),
        );

        let visible = HashSet::from([fullscreen.clone(), other.clone()]);

        assert_eq!(focused_fullscreen_window(&app), Some(fullscreen.clone()));
        assert!(window_is_visible(&app, &visible, &fullscreen));
        assert!(!window_is_visible(&app, &visible, &other));
    }

    #[test]
    fn floating_window_uses_its_stored_rect() {
        let mut app = AppState::default();
        let workspace = app.wm.active_workspace.clone();
        let window_id = WindowId::from("5");
        let rect = Rectangle::new((10, 20).into(), (300, 200).into());

        app.wm.windows.insert(
            window_id.clone(),
            ManagedWindowState {
                id: window_id.clone(),
                workspace,
                output: None,
                mode: WindowMode::Floating { rect },
                mapped: true,
                app_id: None,
                title: None,
            },
        );

        assert_eq!(committed_window_rect(&app, None, &window_id), Some(rect));
    }

    #[test]
    fn tiled_window_uses_default_tiled_rect() {
        let mut app = AppState::default();
        let workspace = WorkspaceId::from("1");
        let window_id = WindowId::from("9");

        app.wm.windows.insert(
            window_id.clone(),
            ManagedWindowState::tiled(window_id.clone(), workspace, None),
        );

        assert_eq!(
            committed_window_rect(&app, None, &window_id),
            Some(Rectangle::new((0, 0).into(), (960, 640).into()))
        );
    }
}
