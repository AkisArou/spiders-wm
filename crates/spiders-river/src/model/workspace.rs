use serde::{Deserialize, Serialize};

use spiders_shared::ids::WorkspaceId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub id: WorkspaceId,
    pub name: String,
    pub tag_mask: u32,
}
