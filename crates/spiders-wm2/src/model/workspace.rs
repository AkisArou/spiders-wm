use spiders_config::model::Config;
use spiders_shared::wm::LayoutRef;
use spiders_shared::wm::WorkspaceSnapshot;

use crate::model::{OutputId, WindowId, WorkspaceId};

#[derive(Debug, Clone)]
pub struct WorkspaceState {
    pub id: WorkspaceId,
    pub name: String,
    pub output: Option<OutputId>,
    pub windows: Vec<WindowId>,
}

impl WorkspaceState {
    pub fn snapshot(
        &self,
        focused: bool,
        visible: bool,
        config: &Config,
        output_name: Option<&str>,
    ) -> WorkspaceSnapshot {
        let effective_layout = output_name
            .and_then(|output_name| config.layout_selection.per_monitor.get(output_name))
            .or_else(|| {
                config
                    .workspaces
                    .iter()
                    .position(|name| name == &self.name)
                    .and_then(|index| config.layout_selection.per_workspace.get(index))
            })
            .or_else(|| {
                self.name.parse::<usize>().ok().and_then(|index| {
                    config
                        .layout_selection
                        .per_workspace
                        .get(index.saturating_sub(1))
                })
            })
            .or(config.layout_selection.default.as_ref())
            .map(|name| LayoutRef { name: name.clone() });

        WorkspaceSnapshot {
            id: self.id.clone(),
            name: self.name.clone(),
            output_id: self.output.clone(),
            active_workspaces: vec![self.name.clone()],
            focused,
            visible,
            effective_layout,
        }
    }
}
