use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use spiders_scene::MotionTrackState;
use spiders_core::{WindowId, WorkspaceId};
use wayland_backend::client::ObjectId;

use crate::action_bridge::RiverCommand;
use crate::protocol::river_window_management_v1::river_window_v1;
use crate::protocol::river_xkb_config::river_xkb_keymap_v1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTransitionDirection {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTransitionState {
    pub from_workspace_id: WorkspaceId,
    pub to_workspace_id: WorkspaceId,
    pub direction: WorkspaceTransitionDirection,
}

#[derive(Debug, Default)]
pub struct BackendTransientState {
    pub seat_command_mailbox: HashMap<ObjectId, VecDeque<RiverCommand>>,
    pub window_pointer_move_requests: HashMap<ObjectId, ObjectId>,
    pub window_pointer_resize_requests: HashMap<ObjectId, (ObjectId, river_window_v1::Edges)>,
    pub initialized_seat_bindings: HashSet<ObjectId>,
    pub output_global_links: HashMap<ObjectId, u32>,
    pub seat_global_links: HashMap<ObjectId, u32>,
    pub pending_output_removals: HashSet<ObjectId>,
    pub pending_seat_removals: HashSet<ObjectId>,
    pub pending_xkb_keymaps: HashMap<ObjectId, ObjectId>,
    pub pending_xkb_keymap_context: HashMap<ObjectId, String>,
    pub xkb_keymap_proxies: HashMap<ObjectId, river_xkb_keymap_v1::RiverXkbKeymapV1>,
    pub pending_input_results: HashMap<ObjectId, String>,
    pub motion_state: HashMap<WindowId, WindowMotionState>,
    pub motion_active_windows: HashSet<WindowId>,
    pub motion_frame_time: Option<Instant>,
    pub motion_has_active_animations: bool,
    pub workspace_transition: Option<WorkspaceTransitionState>,
}

#[derive(Debug, Default)]
pub struct WindowMotionState {
    pub layout: MotionTrackState,
    pub titlebar: MotionTrackState,
}
