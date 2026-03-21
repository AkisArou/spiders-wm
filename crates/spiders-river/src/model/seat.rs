use spiders_shared::ids::WindowId;

use crate::protocol::river_window_management_v1::river_window_v1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeatPointerOpState {
    None,
    Move {
        window_id: WindowId,
        start_x: i32,
        start_y: i32,
    },
    Resize {
        window_id: WindowId,
        start_x: i32,
        start_y: i32,
        start_width: i32,
        start_height: i32,
        edges: river_window_v1::Edges,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatState {
    pub name: String,
    pub focused_window_id: Option<WindowId>,
    pub hovered_window_id: Option<WindowId>,
    pub interacted_window_id: Option<WindowId>,
    pub pointer_op: SeatPointerOpState,
    pub pointer_op_dx: i32,
    pub pointer_op_dy: i32,
    pub pointer_op_release: bool,
}
