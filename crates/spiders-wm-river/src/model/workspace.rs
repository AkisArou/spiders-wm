use serde::{Deserialize, Serialize};

use spiders_core::WorkspaceId;
use spiders_core::types::LayoutRef;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub id: WorkspaceId,
    pub name: String,
    pub tag_mask: u32,
    pub effective_layout: Option<LayoutRef>,
}
