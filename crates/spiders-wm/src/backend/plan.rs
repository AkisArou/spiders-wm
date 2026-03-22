use spiders_tree::WindowId;
use spiders_tree::{OutputId, WorkspaceId};
use spiders_scene::{ColorValue, FontWeightValue, TextAlignValue};
use spiders_shared::types::WindowMode;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationMode {
    ClientSide,
    CompositorTitlebar,
    NoTitlebar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppearancePlan {
    pub window_id: WindowId,
    pub decoration_mode: DecorationMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitlebarPlan {
    pub window_id: WindowId,
    pub height: i32,
    pub background: ColorValue,
    pub border_bottom_width: i32,
    pub border_bottom_color: ColorValue,
    pub title: String,
    pub text_color: ColorValue,
    pub text_align: TextAlignValue,
    pub font_family: Option<String>,
    pub font_size: i32,
    pub font_weight: FontWeightValue,
    pub letter_spacing: i32,
    pub box_shadow: Option<String>,
    pub padding_top: i32,
    pub padding_right: i32,
    pub padding_bottom: i32,
    pub padding_left: i32,
    pub corner_radius_top_left: i32,
    pub corner_radius_top_right: i32,
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
    Spawn {
        command: String,
    },
    ActivateWorkspace(ActivateWorkspacePlan),
    MoveFocusedWindowToWorkspace(MoveFocusedWindowToWorkspacePlan),
    MoveWindowInWorkspace(MoveWindowInWorkspacePlan),
    SetWindowMode(WindowModePlan),
    FocusOutput {
        output_id: OutputId,
    },
    FocusWindow {
        stack: MoveWindowToTopPlan,
        focus: FocusPlan,
    },
    CloseFocusedWindow,
    FocusDirection {
        stack: MoveWindowToTopPlan,
        focus: FocusPlan,
    },
    Noop,
}
