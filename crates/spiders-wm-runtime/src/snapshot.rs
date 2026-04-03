use serde::{Deserialize, Serialize};
use spiders_core::{LayoutRect, WindowId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PreviewSnapshotClasses {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSnapshotNode {
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "class", alias = "className")]
    pub class_name: Option<PreviewSnapshotClasses>,
    #[serde(default)]
    pub rect: Option<LayoutRect>,
    #[serde(default, rename = "window_id", alias = "windowId")]
    pub window_id: Option<WindowId>,
    #[serde(default)]
    pub axis: Option<String>,
    #[serde(default)]
    pub reverse: bool,
    #[serde(default)]
    pub children: Vec<PreviewSnapshotNode>,
}
