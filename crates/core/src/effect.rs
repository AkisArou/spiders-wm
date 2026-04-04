use serde::{Deserialize, Serialize};

use crate::command::FocusDirection;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceTarget {
    Named(String),
    Next,
    Previous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceAssignment {
    Move(u8),
    Toggle(u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FocusTarget {
    Next,
    Previous,
    Direction(FocusDirection),
    Window(crate::WindowId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowToggle {
    Floating,
    Fullscreen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum WmHostEffect {
    SpawnCommand { command: String },
    RequestQuit,
    ActivateWorkspace { target: WorkspaceTarget },
    AssignFocusedWindowToWorkspace { assignment: WorkspaceAssignment },
    SpawnTerminal,
    FocusWindow { target: FocusTarget },
    CloseFocusedWindow,
    ReloadConfig,
    ToggleFocusedWindow { toggle: WindowToggle },
    SwapFocusedWindow { direction: FocusDirection },
    SetLayout { name: String },
    CycleLayout { direction: Option<crate::command::LayoutCycleDirection> },
}
