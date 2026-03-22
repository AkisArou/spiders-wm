use serde::{Deserialize, Serialize};

use spiders_tree::{OutputId, WindowId, WorkspaceId};
use spiders_tree::LayoutRect;
use spiders_shared::wm::WindowMode;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    pub id: WindowId,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub class: Option<String>,
    pub instance: Option<String>,
    pub role: Option<String>,
    pub window_type: Option<String>,
    pub identifier: Option<String>,
    pub unreliable_pid: Option<u32>,
    pub output_id: Option<OutputId>,
    pub workspace_ids: Vec<WorkspaceId>,
    pub is_new: bool,
    pub closed: bool,
    pub mapped: bool,
    pub mode: WindowMode,
    pub focused: bool,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub last_floating_rect: Option<LayoutRect>,
}
