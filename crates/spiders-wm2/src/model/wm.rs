use std::collections::{HashMap, HashSet};

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
        let visible_workspaces = outputs
            .values()
            .filter_map(|output| output.current_workspace.clone())
            .collect::<HashSet<_>>();
        let visible_workspaces = if visible_workspaces.is_empty() {
            HashSet::from([self.active_workspace.clone()])
        } else {
            visible_workspaces
        };
        let visible_window_ids = self
            .workspaces
            .values()
            .filter(|workspace| visible_workspaces.contains(&workspace.id))
            .flat_map(|workspace| workspace.windows.iter().cloned())
            .collect();

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
                        visible_workspaces.contains(&workspace.id),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::WmState;
    use crate::model::{
        ManagedWindowState, OutputId, OutputNode, WindowId, WorkspaceId, WorkspaceState,
    };

    #[test]
    fn snapshot_marks_windows_visible_on_all_current_output_workspaces() {
        let mut wm = WmState::default();
        wm.active_workspace = WorkspaceId::from("ws-2");
        wm.focused_output = Some(OutputId::from("out-1"));
        wm.workspaces.insert(
            WorkspaceId::from("ws-2"),
            WorkspaceState {
                id: WorkspaceId::from("ws-2"),
                name: "ws-2".into(),
                output: Some(OutputId::from("out-1")),
                windows: vec![WindowId::from("w2")],
            },
        );
        wm.workspaces.insert(
            WorkspaceId::from("ws-3"),
            WorkspaceState {
                id: WorkspaceId::from("ws-3"),
                name: "ws-3".into(),
                output: Some(OutputId::from("out-2")),
                windows: vec![WindowId::from("w3")],
            },
        );
        wm.windows.insert(
            WindowId::from("w2"),
            ManagedWindowState::tiled(
                WindowId::from("w2"),
                WorkspaceId::from("ws-2"),
                Some(OutputId::from("out-1")),
            ),
        );
        wm.windows.insert(
            WindowId::from("w3"),
            ManagedWindowState::tiled(
                WindowId::from("w3"),
                WorkspaceId::from("ws-3"),
                Some(OutputId::from("out-2")),
            ),
        );

        let outputs = HashMap::from([
            (
                OutputId::from("out-1"),
                OutputNode {
                    id: OutputId::from("out-1"),
                    name: "one".into(),
                    enabled: true,
                    current_workspace: Some(WorkspaceId::from("ws-2")),
                    logical_size: (1280, 720),
                },
            ),
            (
                OutputId::from("out-2"),
                OutputNode {
                    id: OutputId::from("out-2"),
                    name: "two".into(),
                    enabled: true,
                    current_workspace: Some(WorkspaceId::from("ws-3")),
                    logical_size: (1280, 720),
                },
            ),
        ]);

        let snapshot = wm.snapshot(&outputs, &spiders_config::model::Config::default());

        let mut visible_window_ids = snapshot.visible_window_ids.clone();
        visible_window_ids.sort();
        assert_eq!(
            visible_window_ids,
            vec![WindowId::from("w2"), WindowId::from("w3")]
        );
        assert!(
            snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.id == WorkspaceId::from("ws-2"))
                .unwrap()
                .visible
        );
        assert!(
            snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.id == WorkspaceId::from("ws-3"))
                .unwrap()
                .visible
        );
    }
}
