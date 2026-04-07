use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use spiders_core::LayoutRect;
use spiders_core::query::state_snapshot_for_model;
use spiders_ipc::{DebugDumpKind, DebugResponse};
use spiders_scene::LayoutSnapshotNode;
use tracing::{debug, warn};

use crate::state::SpidersWm;

#[derive(Debug, Clone, Serialize)]
pub struct DebugSceneNode {
    pub kind: &'static str,
    pub id: Option<String>,
    pub name: Option<String>,
    pub rect: LayoutRect,
    pub window_id: Option<String>,
    pub children: Vec<DebugSceneNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugFrameSyncDump {
    pub closing_overlay_count: usize,
    pub managed_windows: Vec<DebugManagedWindowFrameSync>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugManagedWindowFrameSync {
    pub window_id: String,
    pub mapped: bool,
    pub closing: bool,
    pub has_close_snapshot: bool,
    pub has_pending_configures: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugSeatsDump {
    pub focused_window_id: Option<String>,
    pub seats: Vec<DebugSeatState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugSeatState {
    pub seat_id: String,
    pub focused_window_id: Option<String>,
    pub hovered_window_id: Option<String>,
    pub interacted_window_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugProfile {
    Off,
    Minimal,
    Protocol,
    Render,
    Full,
}

impl DebugProfile {
    pub fn from_env() -> Self {
        match std::env::var("SPIDERS_WM_DEBUG_PROFILE").ok().as_deref().map(str::trim) {
            Some("minimal") => Self::Minimal,
            Some("protocol") => Self::Protocol,
            Some("render") => Self::Render,
            Some("full") => Self::Full,
            _ => Self::Off,
        }
    }

    pub fn enabled(self) -> bool {
        !matches!(self, Self::Off)
    }

    pub fn protocol_enabled(self) -> bool {
        matches!(self, Self::Protocol | Self::Full)
    }

    pub fn render_enabled(self) -> bool {
        matches!(self, Self::Render | Self::Full)
    }
}

#[derive(Debug, Clone)]
pub struct DebugConfig {
    pub profile: DebugProfile,
    pub output_dir: Option<PathBuf>,
}

impl DebugConfig {
    pub fn from_env() -> Self {
        let profile = DebugProfile::from_env();
        let output_dir = if profile.enabled() { Some(configured_debug_output_dir()) } else { None };

        Self { profile, output_dir }
    }
}

#[derive(Debug, Clone)]
pub struct DebugState {
    pub config: DebugConfig,
}

impl DebugState {
    pub fn new(config: DebugConfig) -> Self {
        if let Some(output_dir) = config.output_dir.as_ref()
            && let Err(error) = fs::create_dir_all(output_dir)
        {
            warn!(path = %output_dir.display(), %error, "failed to create debug output directory");
        }

        Self { config }
    }

    pub fn profile(&self) -> DebugProfile {
        self.config.profile
    }

    pub fn output_dir(&self) -> Option<&Path> {
        self.config.output_dir.as_deref()
    }

    pub fn dump_json<T: Serialize>(
        &self,
        file_name: &str,
        value: &T,
    ) -> Result<Option<PathBuf>, String> {
        let Some(output_dir) = self.output_dir() else {
            return Ok(None);
        };

        let path = output_dir.join(file_name);
        let contents = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
        fs::write(&path, contents).map_err(|error| error.to_string())?;
        Ok(Some(path))
    }

    pub fn dump_text(&self, file_name: &str, contents: &str) -> Result<Option<PathBuf>, String> {
        let Some(output_dir) = self.output_dir() else {
            return Ok(None);
        };

        let path = output_dir.join(file_name);
        fs::write(&path, contents).map_err(|error| error.to_string())?;
        Ok(Some(path))
    }
}

impl SpidersWm {
    pub fn dump_debug_state(&self) {
        let snapshot = state_snapshot_for_model(&self.model);
        let _ = self.debug.dump_json("wm-state.json", &snapshot);
        let _ = self.debug.dump_text("debug-profile.txt", &format!("{:?}\n", self.debug.profile()));
    }

    pub fn handle_debug_dump(&self, kind: DebugDumpKind) -> Result<DebugResponse, String> {
        match kind {
            DebugDumpKind::WmState => {
                let snapshot = state_snapshot_for_model(&self.model);
                let path = self
                    .debug
                    .dump_json("wm-state.json", &snapshot)?
                    .map(|path| path.display().to_string());
                Ok(DebugResponse::DumpWritten { kind, path })
            }
            DebugDumpKind::DebugProfile => {
                let path = self
                    .debug
                    .dump_text("debug-profile.txt", &format!("{:?}\n", self.debug.profile()))?
                    .map(|path| path.display().to_string());
                Ok(DebugResponse::DumpWritten { kind, path })
            }
            DebugDumpKind::SceneSnapshot => {
                let scene = self.scene_snapshot_root.as_ref().map(debug_scene_node);
                let path = self
                    .debug
                    .dump_json("scene-snapshot.json", &scene)?
                    .map(|path| path.display().to_string());
                Ok(DebugResponse::DumpWritten { kind, path })
            }
            DebugDumpKind::FrameSync => {
                let frame_sync = debug_frame_sync_dump(self);
                let path = self
                    .debug
                    .dump_json("frame-sync.json", &frame_sync)?
                    .map(|path| path.display().to_string());
                Ok(DebugResponse::DumpWritten { kind, path })
            }
            DebugDumpKind::Seats => {
                let seats = debug_seats_dump(self);
                let path = self
                    .debug
                    .dump_json("seats.json", &seats)?
                    .map(|path| path.display().to_string());
                Ok(DebugResponse::DumpWritten { kind, path })
            }
        }
    }

    pub(crate) fn debug_protocol_event(
        &self,
        event: &'static str,
        window_id: Option<&str>,
        details: impl FnOnce() -> String,
    ) {
        if !self.debug.profile().protocol_enabled() {
            return;
        }

        debug!(event, window_id, details = %details(), "wm protocol debug");
    }

    pub(crate) fn debug_render_event(
        &self,
        event: &'static str,
        window_id: Option<&str>,
        details: impl FnOnce() -> String,
    ) {
        if !self.debug.profile().render_enabled() {
            return;
        }

        debug!(event, window_id, details = %details(), "wm render debug");
    }
}

fn debug_scene_node(node: &LayoutSnapshotNode) -> DebugSceneNode {
    match node {
        LayoutSnapshotNode::Workspace { meta, rect, children, .. } => DebugSceneNode {
            kind: "workspace",
            id: meta.id.clone(),
            name: meta.name.clone(),
            rect: *rect,
            window_id: None,
            children: children.iter().map(debug_scene_node).collect(),
        },
        LayoutSnapshotNode::Group { meta, rect, children, .. } => DebugSceneNode {
            kind: "group",
            id: meta.id.clone(),
            name: meta.name.clone(),
            rect: *rect,
            window_id: None,
            children: children.iter().map(debug_scene_node).collect(),
        },
        LayoutSnapshotNode::Content { meta, rect, children, .. } => DebugSceneNode {
            kind: "content",
            id: meta.id.clone(),
            name: meta.name.clone(),
            rect: *rect,
            window_id: None,
            children: children.iter().map(debug_scene_node).collect(),
        },
        LayoutSnapshotNode::Window { meta, rect, window_id, children, .. } => DebugSceneNode {
            kind: "window",
            id: meta.id.clone(),
            name: meta.name.clone(),
            rect: *rect,
            window_id: window_id.as_ref().map(ToString::to_string),
            children: children.iter().map(debug_scene_node).collect(),
        },
    }
}

fn debug_frame_sync_dump(state: &SpidersWm) -> DebugFrameSyncDump {
    DebugFrameSyncDump {
        closing_overlay_count: state.frame_sync.overlay_count(),
        managed_windows: state
            .managed_windows
            .iter()
            .map(|record| DebugManagedWindowFrameSync {
                window_id: record.id.to_string(),
                mapped: record.mapped,
                closing: state.window_is_closing(&record.id),
                has_close_snapshot: record.frame_sync.has_close_snapshot(),
                has_pending_configures: record.frame_sync.has_pending_configures(),
            })
            .collect(),
    }
}

fn debug_seats_dump(state: &SpidersWm) -> DebugSeatsDump {
    DebugSeatsDump {
        focused_window_id: state.model.focused_window_id.as_ref().map(ToString::to_string),
        seats: state
            .model
            .seats
            .values()
            .map(|seat| DebugSeatState {
                seat_id: seat.id.to_string(),
                focused_window_id: seat.focused_window_id.as_ref().map(ToString::to_string),
                hovered_window_id: seat.hovered_window_id.as_ref().map(ToString::to_string),
                interacted_window_id: seat.interacted_window_id.as_ref().map(ToString::to_string),
            })
            .collect(),
    }
}

fn configured_debug_output_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("SPIDERS_WM_DEBUG_OUTPUT_DIR") {
        return PathBuf::from(path);
    }

    let base =
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join(format!("spiders-wm-debug-{}", std::process::id()))
}
