use std::collections::HashMap;

use spiders_config::model::Config;
use spiders_shared::wm::StateSnapshot;

use crate::model::{
    ManagedWindowState, OutputId, OutputNode, WindowId, WorkspaceId, WorkspaceState,
};

#[derive(Debug)]
pub struct WmState {
    pub active_workspace: WorkspaceId,
    pub focused_window: Option<WindowId>,
    pub focused_output: Option<OutputId>,
    pub workspaces: HashMap<WorkspaceId, WorkspaceState>,
    pub windows: HashMap<WindowId, ManagedWindowState>,
}

impl Default for WmState {
    fn default() -> Self {
        let ws = WorkspaceId::from("1");

        let mut workspaces = HashMap::new();

        workspaces.insert(
            ws.clone(),
            WorkspaceState {
                id: ws.clone(),
                name: "1".into(),
                output: None,
                windows: Vec::new(),
            },
        );

        Self {
            active_workspace: ws,
            focused_window: None,
            focused_output: None,
            workspaces,
            windows: HashMap::new(),
        }
    }
}

impl WmState {
    pub fn snapshot(
        &self,
        outputs: &HashMap<OutputId, OutputNode>,
        config: &Config,
    ) -> StateSnapshot {
        let active_workspace = self.workspaces.get(&self.active_workspace);
        let visible_window_ids = active_workspace
            .map(|workspace| workspace.windows.clone())
            .unwrap_or_default();

        let mut workspace_names = self
            .workspaces
            .values()
            .map(|workspace| workspace.name.clone())
            .collect::<Vec<_>>();
        workspace_names.sort();

        StateSnapshot {
            focused_window_id: self.focused_window.clone(),
            current_output_id: self.focused_output.clone(),
            current_workspace_id: Some(self.active_workspace.clone()),
            outputs: outputs.values().map(OutputNode::snapshot).collect(),
            workspaces: self
                .workspaces
                .values()
                .map(|workspace| {
                    workspace.snapshot(
                        workspace.id == self.active_workspace,
                        workspace.id == self.active_workspace,
                        config,
                        workspace
                            .output
                            .as_ref()
                            .and_then(|output_id| outputs.get(output_id))
                            .map(|output| output.name.as_str()),
                    )
                })
                .collect(),
            windows: self
                .windows
                .values()
                .map(|window| window.snapshot(self.focused_window.as_ref() == Some(&window.id)))
                .collect(),
            visible_window_ids,
            workspace_names,
        }
    }
}
