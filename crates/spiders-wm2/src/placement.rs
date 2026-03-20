use std::collections::HashSet;

use smithay::utils::{Logical, Rectangle};
use spiders_shared::wm::StateSnapshot;

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
    window_id: &WindowId,
) -> Option<Rectangle<i32, Logical>> {
    let window = app.wm.windows.get(window_id)?;
    let output_rect = live_output_rect_for_window(app, window_id);
    let tiled_rect = app
        .layout
        .desired_tiled_rect(window_id)
        .unwrap_or_else(default_tiled_rect);
    window.rect(output_rect, tiled_rect)
}

pub fn presented_window_rect(
    app: &AppState,
    committed_snapshot: Option<&StateSnapshot>,
    window_id: &WindowId,
) -> Option<Rectangle<i32, Logical>> {
    committed_snapshot
        .and_then(|snapshot| committed_window_rect(app, Some(snapshot), window_id))
        .or_else(|| desired_window_rect(app, window_id))
}

pub fn committed_window_rect(
    app: &AppState,
    committed_snapshot: Option<&StateSnapshot>,
    window_id: &WindowId,
) -> Option<Rectangle<i32, Logical>> {
    let tiled_rect = app
        .layout
        .committed_tiled_rect(window_id)
        .unwrap_or_else(default_tiled_rect);

    let (mode, output_rect) = if let Some(snapshot) = committed_snapshot {
        let mode = snapshot
            .windows
            .iter()
            .find(|window| &window.id == window_id)
            .map(|window| window.mode.into())?;
        (mode, snapshot_output_rect_for_window(snapshot, window_id))
    } else {
        let mode = app.wm.windows.get(window_id).map(|window| window.mode())?;
        (mode, live_output_rect_for_window(app, window_id))
    };

    rect_for_mode(mode, output_rect, tiled_rect)
}

pub fn window_is_visible(
    app: &AppState,
    committed_snapshot: Option<&StateSnapshot>,
    visible: &HashSet<WindowId>,
    window_id: &WindowId,
) -> bool {
    if let Some(fullscreen_window) = committed_snapshot
        .and_then(focused_fullscreen_window_in_snapshot)
        .or_else(|| focused_fullscreen_window(app))
    {
        return visible.contains(window_id) && fullscreen_window == *window_id;
    }

    visible.contains(window_id)
}

fn focused_fullscreen_window_in_snapshot(snapshot: &StateSnapshot) -> Option<WindowId> {
    let focused_window_id = snapshot.focused_window_id.as_ref()?;
    let focused_window = snapshot
        .windows
        .iter()
        .find(|window| &window.id == focused_window_id)?;

    focused_window
        .is_fullscreen()
        .then(|| focused_window_id.clone())
}

fn default_tiled_rect() -> Rectangle<i32, Logical> {
    Rectangle::new((0, 0).into(), (960, 640).into())
}

fn live_output_rect_for_window(
    app: &AppState,
    window_id: &WindowId,
) -> Option<Rectangle<i32, Logical>> {
    let output_id = app.wm.windows.get(window_id)?.output.as_ref()?;
    let output = app.topology.outputs.get(output_id)?;

    Some(Rectangle::new(
        (0, 0).into(),
        (output.logical_size.0 as i32, output.logical_size.1 as i32).into(),
    ))
}

fn snapshot_output_rect_for_window(
    snapshot: &StateSnapshot,
    window_id: &WindowId,
) -> Option<Rectangle<i32, Logical>> {
    let output_id = snapshot
        .windows
        .iter()
        .find(|window| &window.id == window_id)?
        .output_id
        .as_ref()?;
    let output = snapshot
        .outputs
        .iter()
        .find(|output| &output.id == output_id)?;

    Some(Rectangle::new(
        (output.logical_x, output.logical_y).into(),
        (output.logical_width as i32, output.logical_height as i32).into(),
    ))
}

fn rect_for_mode(
    mode: WindowMode,
    output_rect: Option<Rectangle<i32, Logical>>,
    tiled_rect: Rectangle<i32, Logical>,
) -> Option<Rectangle<i32, Logical>> {
    match mode {
        WindowMode::Tiled => Some(tiled_rect),
        WindowMode::Floating { rect } => Some(rect),
        WindowMode::Fullscreen => output_rect,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use smithay::utils::Rectangle;
    use spiders_shared::wm::{
        OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowMode as SharedWindowMode,
        WindowSnapshot,
    };

    use super::{
        committed_window_rect, focused_fullscreen_window, focused_fullscreen_window_in_snapshot,
        presented_window_rect, window_is_visible,
    };
    use crate::{
        app::AppState,
        model::{ManagedWindowState, OutputId, OutputNode, WindowId, WindowMode, WorkspaceId},
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
        assert!(window_is_visible(&app, None, &visible, &fullscreen));
        assert!(!window_is_visible(&app, None, &visible, &other));
    }

    #[test]
    fn committed_snapshot_visibility_blocks_desired_visibility_during_pending_transaction() {
        let app = AppState::default();
        let committed = committed_snapshot(vec![WindowSnapshot {
            id: WindowId::from("1"),
            shell: ShellKind::XdgToplevel,
            app_id: None,
            title: None,
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            mode: SharedWindowMode::Tiled,
            focused: true,
            urgent: false,
            output_id: None,
            workspace_id: Some(WorkspaceId::from("1")),
            workspaces: vec![],
        }]);
        let visible = HashSet::from([WindowId::from("1")]);

        assert!(window_is_visible(
            &app,
            Some(&committed),
            &visible,
            &WindowId::from("1")
        ));
        assert!(!window_is_visible(
            &app,
            Some(&committed),
            &visible,
            &WindowId::from("2")
        ));
    }

    #[test]
    fn committed_snapshot_fullscreen_controls_pending_visibility() {
        let app = AppState::default();
        let committed = committed_snapshot(vec![
            WindowSnapshot {
                id: WindowId::from("1"),
                shell: ShellKind::XdgToplevel,
                app_id: None,
                title: None,
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: true,
                mode: SharedWindowMode::Fullscreen,
                focused: true,
                urgent: false,
                output_id: None,
                workspace_id: Some(WorkspaceId::from("1")),
                workspaces: vec![],
            },
            WindowSnapshot {
                id: WindowId::from("2"),
                shell: ShellKind::XdgToplevel,
                app_id: None,
                title: None,
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: true,
                mode: SharedWindowMode::Tiled,
                focused: false,
                urgent: false,
                output_id: None,
                workspace_id: Some(WorkspaceId::from("1")),
                workspaces: vec![],
            },
        ]);
        let visible = HashSet::from([WindowId::from("1"), WindowId::from("2")]);

        assert_eq!(
            focused_fullscreen_window_in_snapshot(&committed),
            Some(WindowId::from("1"))
        );
        assert!(window_is_visible(
            &app,
            Some(&committed),
            &visible,
            &WindowId::from("1")
        ));
        assert!(!window_is_visible(
            &app,
            Some(&committed),
            &visible,
            &WindowId::from("2")
        ));
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

    #[test]
    fn committed_window_rect_prefers_committed_snapshot_mode_over_live_desired_mode() {
        let mut app = AppState::default();
        let workspace = WorkspaceId::from("1");
        let window_id = WindowId::from("9");
        let committed_rect = Rectangle::new((0, 0).into(), (500, 300).into());

        app.wm.windows.insert(
            window_id.clone(),
            ManagedWindowState {
                id: window_id.clone(),
                workspace: workspace.clone(),
                output: Some(OutputId::from("out-1")),
                mode: WindowMode::Fullscreen,
                mapped: true,
                app_id: None,
                title: None,
            },
        );
        app.topology.outputs.insert(
            OutputId::from("out-1"),
            crate::model::OutputNode {
                id: OutputId::from("out-1"),
                name: "winit".into(),
                enabled: true,
                current_workspace: Some(workspace.clone()),
                logical_size: (1920, 1080),
            },
        );
        app.layout
            .committed_tiled_window_rects
            .insert(window_id.clone(), committed_rect);

        let committed = committed_snapshot(vec![WindowSnapshot {
            id: window_id.clone(),
            shell: ShellKind::XdgToplevel,
            app_id: None,
            title: None,
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            mode: SharedWindowMode::Tiled,
            focused: true,
            urgent: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(workspace),
            workspaces: vec![],
        }]);

        assert_eq!(
            committed_window_rect(&app, Some(&committed), &window_id),
            Some(committed_rect)
        );
    }

    #[test]
    fn presented_window_rect_prefers_committed_geometry_while_pending() {
        let mut app = AppState::default();
        let workspace = WorkspaceId::from("1");
        let window_id = WindowId::from("3");
        let desired_rect = Rectangle::new((300, 200).into(), (800, 600).into());
        let committed_rect = Rectangle::new((20, 10).into(), (500, 400).into());

        app.wm.windows.insert(
            window_id.clone(),
            ManagedWindowState::tiled(window_id.clone(), workspace.clone(), None),
        );
        app.layout
            .desired_tiled_window_rects
            .insert(window_id.clone(), desired_rect);
        app.layout
            .committed_tiled_window_rects
            .insert(window_id.clone(), committed_rect);

        let committed = committed_snapshot(vec![WindowSnapshot {
            id: window_id.clone(),
            shell: ShellKind::XdgToplevel,
            app_id: None,
            title: None,
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            mode: SharedWindowMode::Tiled,
            focused: true,
            urgent: false,
            output_id: None,
            workspace_id: Some(workspace),
            workspaces: vec![],
        }]);

        assert_eq!(
            presented_window_rect(&app, Some(&committed), &window_id),
            Some(committed_rect)
        );
    }

    #[test]
    fn presented_window_rect_uses_desired_geometry_for_new_uncommitted_window() {
        let mut app = AppState::default();
        let workspace = WorkspaceId::from("1");
        let window_id = WindowId::from("new");
        let desired_rect = Rectangle::new((300, 200).into(), (800, 600).into());

        app.wm.windows.insert(
            window_id.clone(),
            ManagedWindowState::tiled(window_id.clone(), workspace, None),
        );
        app.layout
            .desired_tiled_window_rects
            .insert(window_id.clone(), desired_rect);

        let committed = committed_snapshot(vec![WindowSnapshot {
            id: WindowId::from("old"),
            shell: ShellKind::XdgToplevel,
            app_id: None,
            title: None,
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            mode: SharedWindowMode::Tiled,
            focused: true,
            urgent: false,
            output_id: None,
            workspace_id: Some(WorkspaceId::from("1")),
            workspaces: vec![],
        }]);

        assert_eq!(
            presented_window_rect(&app, Some(&committed), &window_id),
            Some(desired_rect)
        );
    }

    #[test]
    fn presented_window_rect_falls_back_to_desired_without_committed_scene() {
        let mut app = AppState::default();
        let workspace = WorkspaceId::from("1");
        let window_id = WindowId::from("4");
        let desired_rect = Rectangle::new((300, 200).into(), (800, 600).into());

        app.wm.windows.insert(
            window_id.clone(),
            ManagedWindowState::tiled(window_id.clone(), workspace, None),
        );
        app.layout
            .desired_tiled_window_rects
            .insert(window_id.clone(), desired_rect);

        assert_eq!(
            presented_window_rect(&app, None, &window_id),
            Some(desired_rect)
        );
    }

    #[test]
    fn committed_window_rect_uses_committed_snapshot_after_live_window_removal() {
        let mut app = AppState::default();
        let workspace = WorkspaceId::from("1");
        let window_id = WindowId::from("7");
        let committed_rect = Rectangle::new((20, 40).into(), (640, 360).into());

        app.layout
            .committed_tiled_window_rects
            .insert(window_id.clone(), committed_rect);

        let committed = committed_snapshot(vec![WindowSnapshot {
            id: window_id.clone(),
            shell: ShellKind::XdgToplevel,
            app_id: None,
            title: None,
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            mode: SharedWindowMode::Tiled,
            focused: false,
            urgent: false,
            output_id: None,
            workspace_id: Some(workspace),
            workspaces: vec![],
        }]);

        assert_eq!(
            committed_window_rect(&app, Some(&committed), &window_id),
            Some(committed_rect)
        );
    }

    #[test]
    fn committed_window_rect_does_not_fall_back_to_live_window_when_snapshot_omits_it() {
        let mut app = AppState::default();
        let workspace = WorkspaceId::from("1");
        let window_id = WindowId::from("late");

        app.wm.windows.insert(
            window_id.clone(),
            ManagedWindowState::tiled(window_id.clone(), workspace, None),
        );

        let committed = committed_snapshot(vec![WindowSnapshot {
            id: WindowId::from("existing"),
            shell: ShellKind::XdgToplevel,
            app_id: None,
            title: None,
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            mode: SharedWindowMode::Tiled,
            focused: true,
            urgent: false,
            output_id: None,
            workspace_id: Some(WorkspaceId::from("1")),
            workspaces: vec![],
        }]);

        assert_eq!(
            committed_window_rect(&app, Some(&committed), &window_id),
            None
        );
    }

    #[test]
    fn desired_window_rect_uses_window_output_size_for_fullscreen_windows() {
        let mut app = AppState::default();
        let workspace = WorkspaceId::from("1");
        let window_id = WindowId::from("fullscreen");

        app.wm.windows.insert(
            window_id.clone(),
            ManagedWindowState {
                id: window_id.clone(),
                workspace: workspace.clone(),
                output: Some(OutputId::from("out-2")),
                mode: WindowMode::Fullscreen,
                mapped: true,
                app_id: None,
                title: None,
            },
        );
        app.topology.outputs.insert(
            OutputId::from("out-2"),
            OutputNode {
                id: OutputId::from("out-2"),
                name: "two".into(),
                enabled: true,
                current_workspace: Some(workspace),
                logical_size: (1600, 900),
            },
        );

        assert_eq!(
            super::desired_window_rect(&app, &window_id),
            Some(Rectangle::new((0, 0).into(), (1600, 900).into()))
        );
    }

    #[test]
    fn presented_window_rect_uses_committed_output_size_for_fullscreen_windows() {
        let mut app = AppState::default();
        let workspace = WorkspaceId::from("1");
        let window_id = WindowId::from("fullscreen");

        app.wm.windows.insert(
            window_id.clone(),
            ManagedWindowState {
                id: window_id.clone(),
                workspace: workspace.clone(),
                output: Some(OutputId::from("out-2")),
                mode: WindowMode::Fullscreen,
                mapped: true,
                app_id: None,
                title: None,
            },
        );
        app.topology.outputs.insert(
            OutputId::from("out-2"),
            OutputNode {
                id: OutputId::from("out-2"),
                name: "two".into(),
                enabled: true,
                current_workspace: Some(workspace.clone()),
                logical_size: (1920, 1080),
            },
        );

        let committed = StateSnapshot {
            focused_window_id: Some(window_id.clone()),
            current_output_id: Some(OutputId::from("out-2")),
            current_workspace_id: Some(workspace.clone()),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "two".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 1280,
                logical_height: 720,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(workspace.clone()),
            }],
            workspaces: vec![],
            windows: vec![WindowSnapshot {
                id: window_id.clone(),
                shell: ShellKind::XdgToplevel,
                app_id: None,
                title: None,
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: true,
                mode: SharedWindowMode::Fullscreen,
                focused: true,
                urgent: false,
                output_id: Some(OutputId::from("out-2")),
                workspace_id: Some(workspace),
                workspaces: vec![],
            }],
            visible_window_ids: vec![window_id.clone()],
            workspace_names: vec!["1".into()],
        };

        assert_eq!(
            presented_window_rect(&app, Some(&committed), &window_id),
            Some(Rectangle::new((0, 0).into(), (1280, 720).into()))
        );
    }

    fn committed_snapshot(windows: Vec<WindowSnapshot>) -> StateSnapshot {
        StateSnapshot {
            focused_window_id: windows
                .iter()
                .find(|window| window.focused)
                .map(|window| window.id.clone()),
            current_output_id: Some("out-1".into()),
            current_workspace_id: Some("1".into()),
            outputs: vec![OutputSnapshot {
                id: "out-1".into(),
                name: "winit".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 1280,
                logical_height: 720,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some("1".into()),
            }],
            workspaces: vec![],
            windows,
            visible_window_ids: vec![],
            workspace_names: vec!["1".into()],
        }
    }
}
