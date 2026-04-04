use spiders_core::WindowId;
use spiders_core::types::WindowMode;
use spiders_core::{OutputId, WorkspaceId};
pub use spiders_titlebar_core::{AppearancePlan, DecorationMode, TitlebarPlan};

use crate::protocol::river_window_management_v1::river_window_v1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManageWindowPlan {
    pub window_id: WindowId,
    pub width: i32,
    pub height: i32,
    pub tiled_edges: river_window_v1::Edges,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderWindowPlan {
    pub window_id: WindowId,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearTiledStatePlan {
    pub window_id: WindowId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffscreenWindowPlan {
    pub window_id: WindowId,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BorderPlan {
    pub window_id: WindowId,
    pub width: i32,
    pub edges: river_window_v1::Edges,
    pub red: u32,
    pub green: u32,
    pub blue: u32,
    pub alpha: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowModePlan {
    pub window_id: WindowId,
    pub mode: WindowMode,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusPlan {
    FocusWindow { window_id: WindowId },
    ClearFocus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseWindowPlan {
    pub window_id: WindowId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizeWindowPlan {
    pub window_id: WindowId,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointerRenderPlan {
    pub window_id: WindowId,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveWindowToTopPlan {
    pub window_id: WindowId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivateWorkspacePlan {
    pub workspace_id: WorkspaceId,
    pub focus: FocusPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveFocusedWindowToWorkspacePlan {
    pub window_id: WindowId,
    pub workspace_id: WorkspaceId,
    pub focus: FocusPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveWindowInWorkspacePlan {
    pub window_id: WindowId,
    pub target_window_id: WindowId,
    pub focus: FocusPlan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandPlan {
    Spawn { command: String },
    ActivateWorkspace(ActivateWorkspacePlan),
    MoveFocusedWindowToWorkspace(MoveFocusedWindowToWorkspacePlan),
    MoveWindowInWorkspace(MoveWindowInWorkspacePlan),
    SetWindowMode(WindowModePlan),
    FocusOutput { output_id: OutputId },
    FocusWindow { stack: MoveWindowToTopPlan, focus: FocusPlan },
    CloseFocusedWindow,
    FocusDirection { stack: MoveWindowToTopPlan, focus: FocusPlan },
    Noop,
}
