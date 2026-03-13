use serde::{Deserialize, Serialize};

use crate::ids::{OutputId, WindowId, WorkspaceId};
use crate::layout::LayoutSpace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellKind {
    XdgToplevel,
    X11,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputTransform {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutRef {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedLayout {
    pub name: String,
    pub module: String,
    pub stylesheet: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutEvaluationContext {
    pub state: StateSnapshot,
    pub workspace: WorkspaceSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_layout: Option<SelectedLayout>,
    pub space: LayoutSpace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSnapshot {
    pub id: WindowId,
    pub shell: ShellKind,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub class: Option<String>,
    pub instance: Option<String>,
    pub role: Option<String>,
    pub window_type: Option<String>,
    pub mapped: bool,
    pub floating: bool,
    pub fullscreen: bool,
    pub focused: bool,
    pub urgent: bool,
    pub output_id: Option<OutputId>,
    pub workspace_id: Option<WorkspaceId>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub id: WorkspaceId,
    pub name: String,
    pub output_id: Option<OutputId>,
    pub active_tags: Vec<String>,
    pub focused: bool,
    pub visible: bool,
    pub effective_layout: Option<LayoutRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSnapshot {
    pub id: OutputId,
    pub name: String,
    pub logical_width: u32,
    pub logical_height: u32,
    pub scale: u32,
    pub transform: OutputTransform,
    pub enabled: bool,
    pub current_workspace_id: Option<WorkspaceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub focused_window_id: Option<WindowId>,
    pub current_output_id: Option<OutputId>,
    pub current_workspace_id: Option<WorkspaceId>,
    pub outputs: Vec<OutputSnapshot>,
    pub workspaces: Vec<WorkspaceSnapshot>,
    pub windows: Vec<WindowSnapshot>,
    pub visible_window_ids: Vec<WindowId>,
    pub tag_names: Vec<String>,
}

impl StateSnapshot {
    pub fn current_workspace(&self) -> Option<&WorkspaceSnapshot> {
        self.current_workspace_id.as_ref().and_then(|workspace_id| {
            self.workspaces
                .iter()
                .find(|workspace| &workspace.id == workspace_id)
        })
    }

    pub fn current_output(&self) -> Option<&OutputSnapshot> {
        self.current_output_id
            .as_ref()
            .and_then(|output_id| self.outputs.iter().find(|output| &output.id == output_id))
    }

    pub fn workspace_by_id(&self, workspace_id: &WorkspaceId) -> Option<&WorkspaceSnapshot> {
        self.workspaces
            .iter()
            .find(|workspace| &workspace.id == workspace_id)
    }

    pub fn output_by_id(&self, output_id: &OutputId) -> Option<&OutputSnapshot> {
        self.outputs.iter().find(|output| &output.id == output_id)
    }

    pub fn layout_space_for_workspace(&self, workspace: &WorkspaceSnapshot) -> LayoutSpace {
        let output = workspace
            .output_id
            .as_ref()
            .and_then(|output_id| self.output_by_id(output_id));

        LayoutSpace {
            width: output
                .map(|output| output.logical_width as f32)
                .unwrap_or_default(),
            height: output
                .map(|output| output.logical_height as f32)
                .unwrap_or_default(),
        }
    }

    pub fn layout_context(
        &self,
        workspace: &WorkspaceSnapshot,
        selected_layout: Option<SelectedLayout>,
    ) -> LayoutEvaluationContext {
        let output = workspace
            .output_id
            .as_ref()
            .and_then(|output_id| self.output_by_id(output_id))
            .cloned();

        LayoutEvaluationContext {
            state: self.clone(),
            workspace: workspace.clone(),
            output,
            selected_layout,
            space: self.layout_space_for_workspace(workspace),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_snapshot_resolves_current_workspace_output_and_space() {
        let state = StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_width: 1920,
                logical_height: 1080,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![WorkspaceSnapshot {
                id: WorkspaceId::from("ws-1"),
                name: "1".into(),
                output_id: Some(OutputId::from("out-1")),
                active_tags: vec!["1".into()],
                focused: true,
                visible: true,
                effective_layout: Some(LayoutRef {
                    name: "master-stack".into(),
                }),
            }],
            windows: vec![],
            visible_window_ids: vec![],
            tag_names: vec!["1".into()],
        };

        let workspace = state.current_workspace().unwrap();
        let output = state.current_output().unwrap();
        let space = state.layout_space_for_workspace(workspace);

        assert_eq!(workspace.id, WorkspaceId::from("ws-1"));
        assert_eq!(output.id, OutputId::from("out-1"));
        assert_eq!(space.width, 1920.0);
        assert_eq!(space.height, 1080.0);
    }

    #[test]
    fn state_snapshot_builds_layout_evaluation_context() {
        let state = StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_width: 1920,
                logical_height: 1080,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![WorkspaceSnapshot {
                id: WorkspaceId::from("ws-1"),
                name: "1".into(),
                output_id: Some(OutputId::from("out-1")),
                active_tags: vec!["1".into()],
                focused: true,
                visible: true,
                effective_layout: Some(LayoutRef {
                    name: "master-stack".into(),
                }),
            }],
            windows: vec![],
            visible_window_ids: vec![],
            tag_names: vec!["1".into()],
        };
        let workspace = state.current_workspace().unwrap();

        let context = state.layout_context(
            workspace,
            Some(SelectedLayout {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
            }),
        );

        assert_eq!(context.workspace.id, WorkspaceId::from("ws-1"));
        assert_eq!(context.output.unwrap().id, OutputId::from("out-1"));
        assert_eq!(context.space.width, 1920.0);
    }
}
