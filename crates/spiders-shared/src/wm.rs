use serde::{Deserialize, Serialize};

use crate::ids::{OutputId, WindowId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellKind {
    XdgToplevel,
    X11,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputTransform {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutRef {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSnapshot {
    pub id: WindowId,
    pub shell: ShellKind,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub class: Option<String>,
    pub instance: Option<String>,
    pub role: Option<String>,
    pub window_type: Option<String>,
    pub mapped: bool,
    pub floating: bool,
    pub fullscreen: bool,
    pub focused: bool,
    pub urgent: bool,
    pub output_id: Option<OutputId>,
    pub workspace_id: Option<WorkspaceId>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub id: WorkspaceId,
    pub name: String,
    pub output_id: Option<OutputId>,
    pub active_tags: Vec<String>,
    pub focused: bool,
    pub visible: bool,
    pub effective_layout: Option<LayoutRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSnapshot {
    pub id: OutputId,
    pub name: String,
    pub logical_width: u32,
    pub logical_height: u32,
    pub scale: u32,
    pub transform: OutputTransform,
    pub enabled: bool,
    pub current_workspace_id: Option<WorkspaceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub focused_window_id: Option<WindowId>,
    pub current_output_id: Option<OutputId>,
    pub current_workspace_id: Option<WorkspaceId>,
    pub outputs: Vec<OutputSnapshot>,
    pub workspaces: Vec<WorkspaceSnapshot>,
    pub windows: Vec<WindowSnapshot>,
    pub visible_window_ids: Vec<WindowId>,
    pub tag_names: Vec<String>,
}
