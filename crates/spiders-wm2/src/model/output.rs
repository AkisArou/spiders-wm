use spiders_shared::wm::{OutputSnapshot, OutputTransform};

use crate::model::{OutputId, WorkspaceId};

#[derive(Debug, Clone)]
pub struct OutputNode {
    pub id: OutputId,
    pub name: String,
    pub enabled: bool,
    pub current_workspace: Option<WorkspaceId>,
    pub logical_size: (u32, u32),
}

impl OutputNode {
    pub fn snapshot(&self) -> OutputSnapshot {
        OutputSnapshot {
            id: self.id.clone(),
            name: self.name.clone(),
            logical_x: 0,
            logical_y: 0,
            logical_width: self.logical_size.0,
            logical_height: self.logical_size.1,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: self.enabled,
            current_workspace_id: self.current_workspace.clone(),
        }
    }
}
