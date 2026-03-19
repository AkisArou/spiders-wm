use smithay::utils::{Logical, Point, Rectangle};

use crate::model::WindowId;

#[derive(Debug, Clone)]
pub struct FloatingDragState {
    pub window_id: WindowId,
    pub pointer_offset: Point<f64, Logical>,
}

#[derive(Debug, Clone)]
pub struct FloatingResizeState {
    pub window_id: WindowId,
    pub pointer_origin: Point<f64, Logical>,
    pub initial_rect: Rectangle<i32, Logical>,
}

#[derive(Debug, Clone)]
pub enum PointerInteraction {
    Move(FloatingDragState),
    Resize(FloatingResizeState),
}
