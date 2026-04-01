use serde::{Deserialize, Serialize};

use crate::snapshot::{OutputSnapshot, StateSnapshot, WindowSnapshot, WorkspaceSnapshot};
use crate::types::LayoutRef;
use spiders_tree::{LayoutRect, OutputId, WindowId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QueryRequest {
    State,
    FocusedWindow,
    CurrentOutput,
    CurrentWorkspace,
    MonitorList,
    WorkspaceNames,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "kebab-case")]
pub enum QueryResponse {
    State(StateSnapshot),
    FocusedWindow(Option<WindowSnapshot>),
    CurrentOutput(Option<OutputSnapshot>),
    CurrentWorkspace(Option<WorkspaceSnapshot>),
    MonitorList(Vec<OutputSnapshot>),
    WorkspaceNames(Vec<String>),
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
    WindowWorkspaceChange {
        window_id: WindowId,
        workspaces: Vec<String>,
    },
    WindowFloatingChange {
        window_id: WindowId,
        floating: bool,
    },
    WindowGeometryChange {
        window_id: WindowId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        floating_rect: Option<LayoutRect>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_id: Option<OutputId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<WorkspaceId>,
    },
    WindowFullscreenChange {
        window_id: WindowId,
        fullscreen: bool,
    },
    WorkspaceChange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<WorkspaceId>,
        active_workspaces: Vec<String>,
    },
    LayoutChange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<WorkspaceId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout: Option<LayoutRef>,
    },
    ConfigReloaded,
}
