use spiders_core::runtime::layout_context::{
    LayoutEvaluationContext, LayoutMonitorContext, LayoutStateContext, LayoutWindowContext,
    LayoutWorkspaceContext,
};
use spiders_core::{LayoutSpace, OutputId, WindowId, WorkspaceId};

use crate::PreviewSessionState;

pub fn build_preview_layout_context(
    state: &PreviewSessionState,
    selected_layout_name: Option<String>,
    monitor_name: impl Into<String>,
    width: u32,
    height: u32,
) -> LayoutEvaluationContext {
    let workspace_id = WorkspaceId::from(state.active_workspace_name.as_str());
    let output_id = OutputId::from("preview-output");
    let visible_windows = state
        .windows
        .iter()
        .filter(|window| window.workspace_name == state.active_workspace_name)
        .collect::<Vec<_>>();

    LayoutEvaluationContext {
        monitor: LayoutMonitorContext { name: monitor_name.into(), width, height, scale: Some(1) },
        workspace: LayoutWorkspaceContext {
            name: state.active_workspace_name.clone(),
            workspaces: state.workspace_names.clone(),
            window_count: visible_windows.len(),
        },
        windows: visible_windows
            .iter()
            .map(|window| LayoutWindowContext {
                id: WindowId::from(window.id.as_str()),
                app_id: window.app_id.clone(),
                title: window.title.clone(),
                class: window.class.clone(),
                instance: window.instance.clone(),
                role: window.role.clone(),
                shell: window.shell.clone(),
                window_type: window.window_type.clone(),
                floating: window.floating,
                fullscreen: window.fullscreen,
                focused: window.focused,
            })
            .collect(),
        state: Some(LayoutStateContext {
            focused_window_id: state
                .windows
                .iter()
                .find(|window| window.focused)
                .map(|window| WindowId::from(window.id.as_str())),
            current_output_id: Some(output_id),
            current_workspace_id: Some(workspace_id.clone()),
            visible_window_ids: visible_windows
                .iter()
                .map(|window| WindowId::from(window.id.as_str()))
                .collect(),
            workspace_names: state.workspace_names.clone(),
            selected_layout_name: selected_layout_name.clone(),
            layout_adjustments: state.layout_adjustments.clone(),
        }),
        workspace_id,
        output: None,
        selected_layout_name,
        space: LayoutSpace { width: width as f32, height: height as f32 },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use spiders_core::resize::LayoutAdjustmentState;

    use super::*;
    use crate::PreviewSessionWindow;

    #[test]
    fn preview_layout_context_uses_active_workspace_and_adjustments() {
        let context = build_preview_layout_context(
            &PreviewSessionState {
                active_workspace_name: "2".to_string(),
                workspace_names: vec!["1".to_string(), "2".to_string()],
                windows: vec![
                    PreviewSessionWindow {
                        id: "win-1".to_string(),
                        app_id: Some("foot".to_string()),
                        title: Some("Terminal".to_string()),
                        class: Some("foot".to_string()),
                        instance: Some("foot".to_string()),
                        role: None,
                        shell: Some("xdg_toplevel".to_string()),
                        window_type: None,
                        floating: false,
                        fullscreen: false,
                        focused: false,
                        workspace_name: "1".to_string(),
                    },
                    PreviewSessionWindow {
                        id: "win-2".to_string(),
                        app_id: Some("foot".to_string()),
                        title: Some("Terminal 2".to_string()),
                        class: Some("foot".to_string()),
                        instance: Some("foot".to_string()),
                        role: None,
                        shell: Some("xdg_toplevel".to_string()),
                        window_type: None,
                        floating: false,
                        fullscreen: false,
                        focused: true,
                        workspace_name: "2".to_string(),
                    },
                ],
                remembered_focus_by_scope: BTreeMap::new(),
                layout_adjustments: LayoutAdjustmentState {
                    split_weights_by_node_id: BTreeMap::from([("frame".to_string(), vec![12, 8])]),
                },
            },
            Some("master-stack".to_string()),
            "DP-1",
            3440,
            1440,
        );

        assert_eq!(context.workspace.name, "2");
        assert_eq!(context.workspace.window_count, 1);
        assert_eq!(context.windows.len(), 1);
        assert_eq!(context.windows[0].id.as_str(), "win-2");
        assert_eq!(
            context
                .state
                .as_ref()
                .and_then(|state| state.layout_adjustments.split_weights_by_node_id.get("frame")),
            Some(&vec![12, 8])
        );
    }
}
