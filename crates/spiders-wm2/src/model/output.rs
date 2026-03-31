use super::{OutputId, WorkspaceId};

/// Output metadata tracked by the compositor model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutputModel {
    pub id: OutputId,
    pub name: String,
    pub logical_x: i32,
    pub logical_y: i32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub enabled: bool,
    pub focused_workspace_id: Option<WorkspaceId>,
}
