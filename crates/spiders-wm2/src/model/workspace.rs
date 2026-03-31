use super::{OutputId, WorkspaceId};

/// Persistent workspace state independent of backend-specific objects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceModel {
    pub id: WorkspaceId,
    pub name: String,
    pub output_id: Option<OutputId>,
    pub focused: bool,
    pub visible: bool,
}
