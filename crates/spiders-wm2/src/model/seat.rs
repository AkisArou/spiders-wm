use super::{SeatId, WindowId};

/// Seat-focused model state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SeatModel {
    pub id: SeatId,
    pub focused_window_id: Option<WindowId>,
    pub hovered_window_id: Option<WindowId>,
    pub interacted_window_id: Option<WindowId>,
}
