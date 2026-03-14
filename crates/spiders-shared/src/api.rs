use serde::{Deserialize, Serialize};

use crate::ids::{OutputId, WindowId, WorkspaceId};
use crate::layout::LayoutRect;
use crate::wm::{LayoutRef, OutputSnapshot, StateSnapshot, WindowSnapshot, WorkspaceSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FocusDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutCycleDirection {
    Next,
    Previous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum WmAction {
    Spawn {
        command: String,
    },
    ReloadConfig,
    SetLayout {
        name: String,
    },
    CycleLayout {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        direction: Option<LayoutCycleDirection>,
    },
    ViewTag {
        tag: String,
    },
    ToggleViewTag {
        tag: String,
    },
    ActivateWorkspace {
        workspace_id: WorkspaceId,
    },
    AssignWorkspace {
        workspace_id: WorkspaceId,
        output_id: OutputId,
    },
    ToggleFloating,
    ToggleFullscreen,
    FocusWindow {
        window_id: WindowId,
    },
    SetFloatingWindowGeometry {
        window_id: WindowId,
        rect: LayoutRect,
    },
    FocusDirection {
        direction: FocusDirection,
    },
    CloseFocusedWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QueryRequest {
    State,
    FocusedWindow,
    CurrentOutput,
    CurrentWorkspace,
    MonitorList,
    TagNames,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "kebab-case")]
pub enum QueryResponse {
    State(StateSnapshot),
    FocusedWindow(Option<WindowSnapshot>),
    CurrentOutput(Option<OutputSnapshot>),
    CurrentWorkspace(Option<WorkspaceSnapshot>),
    MonitorList(Vec<OutputSnapshot>),
    TagNames(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum CompositorEvent {
    FocusChange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        focused_window_id: Option<WindowId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current_output_id: Option<OutputId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current_workspace_id: Option<WorkspaceId>,
    },
    WindowCreated {
        window: WindowSnapshot,
    },
    WindowDestroyed {
        window_id: WindowId,
    },
    WindowTagChange {
        window_id: WindowId,
        tags: Vec<String>,
    },
    WindowFloatingChange {
        window_id: WindowId,
        floating: bool,
    },
    WindowGeometryChange {
        window_id: WindowId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        floating_rect: Option<LayoutRect>,
    },
    WindowFullscreenChange {
        window_id: WindowId,
        fullscreen: bool,
    },
    TagChange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<WorkspaceId>,
        active_tags: Vec<String>,
    },
    LayoutChange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<WorkspaceId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout: Option<LayoutRef>,
    },
    ConfigReloaded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_serializes_with_stable_tag() {
        let json = serde_json::to_value(WmAction::ToggleFullscreen).unwrap();

        assert_eq!(json["type"], "toggle-fullscreen");
    }
}
