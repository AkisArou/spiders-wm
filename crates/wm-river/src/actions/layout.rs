use spiders_core::WindowId;
use spiders_core::types::WindowMode;

use crate::model::{SeatPointerOpState, WmState};

use crate::protocol::river_window_management_v1::river_window_v1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HorizontalTile {
    pub window_id: WindowId,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub tiled_edges: river_window_v1::Edges,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowBorder {
    pub window_id: WindowId,
    pub width: i32,
    pub red: u32,
    pub green: u32,
    pub blue: u32,
    pub alpha: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowPosition {
    pub window_id: WindowId,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowTiledEdges {
    pub window_id: WindowId,
    pub tiled_edges: river_window_v1::Edges,
}

pub fn compute_horizontal_tiles(
    window_ids: &[WindowId],
    origin_x: i32,
    origin_y: i32,
    total_width: i32,
    total_height: i32,
) -> Vec<HorizontalTile> {
    let count = window_ids.len() as i32;
    if count == 0 {
        return Vec::new();
    }

    window_ids
        .iter()
        .enumerate()
        .map(|(index, window_id)| {
            let index = index as i32;
            let left = origin_x + (total_width * index) / count;
            let right = origin_x + (total_width * (index + 1)) / count;
            let mut tiled_edges = river_window_v1::Edges::Top | river_window_v1::Edges::Bottom;

            if index > 0 {
                tiled_edges |= river_window_v1::Edges::Left;
            }
            if index < count - 1 {
                tiled_edges |= river_window_v1::Edges::Right;
            }

            HorizontalTile {
                window_id: window_id.clone(),
                x: left,
                y: origin_y,
                width: (right - left).max(1),
                height: total_height.max(1),
                tiled_edges,
            }
        })
        .collect()
}

pub fn compute_horizontal_tiled_edges(window_ids: &[WindowId]) -> Vec<WindowTiledEdges> {
    let count = window_ids.len() as i32;
    if count == 0 {
        return Vec::new();
    }

    window_ids
        .iter()
        .enumerate()
        .map(|(index, window_id)| {
            let index = index as i32;
            let mut tiled_edges = river_window_v1::Edges::Top | river_window_v1::Edges::Bottom;

            if index > 0 {
                tiled_edges |= river_window_v1::Edges::Left;
            }
            if index < count - 1 {
                tiled_edges |= river_window_v1::Edges::Right;
            }

            WindowTiledEdges { window_id: window_id.clone(), tiled_edges }
        })
        .collect()
}

pub fn inactive_window_ids(
    active_window_ids: &[WindowId],
    stacked_window_ids: &[WindowId],
) -> Vec<WindowId> {
    stacked_window_ids
        .iter()
        .filter(|window_id| !active_window_ids.iter().any(|id| id == *window_id))
        .cloned()
        .collect()
}

pub fn active_tiled_window_ids(state: &WmState, active_window_ids: &[WindowId]) -> Vec<WindowId> {
    active_window_ids
        .iter()
        .filter(|window_id| {
            state
                .windows
                .get(*window_id)
                .is_some_and(|window| window.mapped && matches!(window.mode, WindowMode::Tiled))
        })
        .cloned()
        .collect()
}

pub fn compute_window_borders(
    state: &WmState,
    active_window_ids: &[WindowId],
) -> Vec<WindowBorder> {
    let focused_window_id = state.focused_window_id.clone();

    state
        .windows
        .values()
        .map(|window| {
            let visible = active_window_ids.iter().any(|id| id == &window.id);
            let focused = focused_window_id.as_ref() == Some(&window.id);
            let (width, red, green, blue, alpha) = if !visible {
                (0, 0, 0, 0, 0)
            } else if focused {
                (3, 0x00ff_ffff, 0x00a8_ffff, 0x0038_ffff, 0xffff_ffff)
            } else {
                (1, 0x0028_ffff, 0x0028_ffff, 0x0032_ffff, 0xffff_ffff)
            };

            WindowBorder { window_id: window.id.clone(), width, red, green, blue, alpha }
        })
        .collect()
}

pub fn compute_pointer_render_positions(state: &WmState) -> Vec<WindowPosition> {
    state
        .seats
        .values()
        .filter_map(|seat| match &seat.pointer_op {
            SeatPointerOpState::None => None,
            SeatPointerOpState::Move { window_id, start_x, start_y } => Some(WindowPosition {
                window_id: window_id.clone(),
                x: start_x + seat.pointer_op_dx,
                y: start_y + seat.pointer_op_dy,
            }),
            SeatPointerOpState::Resize {
                window_id,
                start_x,
                start_y,
                start_width,
                start_height,
                edges,
            } => {
                let (current_width, current_height) = state
                    .windows
                    .get(window_id)
                    .map(|window| (window.width, window.height))
                    .unwrap_or((0, 0));
                let mut x = *start_x;
                let mut y = *start_y;
                if edges.contains(river_window_v1::Edges::Left) {
                    x += start_width - current_width;
                }
                if edges.contains(river_window_v1::Edges::Top) {
                    y += start_height - current_height;
                }
                Some(WindowPosition { window_id: window_id.clone(), x, y })
            }
        })
        .collect()
}
