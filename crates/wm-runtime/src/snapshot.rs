use serde::{Deserialize, Serialize};
use spiders_core::{LayoutRect, WindowId};
use spiders_scene::ComputedStyle;
use std::collections::BTreeMap;

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
    pub layout_style: Option<ComputedStyle>,
    #[serde(default)]
    pub titlebar_style: Option<ComputedStyle>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub data: BTreeMap<String, String>,
    #[serde(default)]
    pub children: Vec<PreviewSnapshotNode>,
}
