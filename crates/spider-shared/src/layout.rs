use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutNodeType {
    Workspace,
    Group,
    Window,
    Slot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutNode {
    pub node_type: LayoutNodeType,
    pub id: Option<String>,
    pub class: Vec<String>,
    pub children: Vec<LayoutNode>,
}
