use serde::{Deserialize, Serialize};

use crate::snapshot::OutputSnapshot;
use crate::snapshot::WindowSnapshot;
use crate::types::LayoutRef;
use crate::{LayoutRect, OutputId, WindowId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum WmEvent {
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
    WindowIdentityChange {
        window: WindowSnapshot,
    },
    WindowFloatingChange {
        window_id: WindowId,
        floating: bool,
    },
    WindowMappedChange {
        window_id: WindowId,
        mapped: bool,
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
    OutputChange {
        output: OutputSnapshot,
    },
    ConfigReloaded,
}
