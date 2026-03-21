use serde::{Deserialize, Serialize};

use spiders_shared::ids::{OutputId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputState {
    pub id: OutputId,
    pub name: String,
    pub logical_x: i32,
    pub logical_y: i32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub enabled: bool,
    pub focused_workspace_id: Option<WorkspaceId>,
}
