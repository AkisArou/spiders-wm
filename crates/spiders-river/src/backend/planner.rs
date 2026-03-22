use super::*;
use spiders_tree::LayoutRect;
use crate::actions::{
    active_tiled_window_ids, compute_horizontal_tiled_edges, compute_pointer_render_positions,
    compute_window_borders, configured_mode_for_window, directional_neighbor_window_id,
    inactive_window_ids, top_window_id,
};
use crate::layout_adapter::compute_layout_snapshot;

impl RiverBackendState {
    pub(super) fn plan_tiled_manage_layout(&mut self) -> Vec<ManageWindowPlan> {
        let active_window_ids = self.active_workspace_window_ids();
        if active_window_ids.is_empty() {
            return Vec::new();
        }

        let active_state_ids = active_tiled_window_ids(
            &self.runtime_state,
            &active_window_ids
                .iter()
                .filter_map(|window_id| {
                    self.registry
                        .windows
                        .get(window_id)
                        .map(|window| window.state_id.clone())
                })
                .collect::<Vec<_>>(),
        );
        if active_state_ids.is_empty() {
            return Vec::new();
        }
        let tiled_edges = compute_horizontal_tiled_edges(&active_state_ids);

        if let Some(snapshot) = compute_layout_snapshot(
            &mut self.layout_service,
            &self.config,
            &self.runtime_state,
            &active_state_ids,
        ) {
            return tiled_edges
                .into_iter()
                .filter_map(|edges| {
                    snapshot
                        .find_by_window_id(&edges.window_id)
                        .map(|node| ManageWindowPlan {
                            window_id: edges.window_id,
                            width: node.rect().width.round() as i32,
                            height: node.rect().height.round() as i32,
                            tiled_edges: edges.tiled_edges,
                        })
                })
                .collect();
        }

        let (_, origin_y, total_width, total_height) = self.current_output_geometry();
        compute_horizontal_tiles(&active_state_ids, 0, origin_y, total_width, total_height)
            .into_iter()
            .map(|tile| ManageWindowPlan {
                window_id: tile.window_id,
                width: tile.width,
                height: tile.height,
                tiled_edges: tile.tiled_edges,
            })
            .collect()
    }

    pub(super) fn plan_tiled_render_layout(&mut self) -> Vec<RenderWindowPlan> {
        let active_window_ids = self.active_workspace_window_ids();
        if active_window_ids.is_empty() {
            return Vec::new();
        }

        let (origin_x, origin_y, total_width, total_height) = self.current_output_geometry();
        let active_state_ids = active_tiled_window_ids(
            &self.runtime_state,
            &active_window_ids
                .iter()
                .filter_map(|window_id| {
                    self.registry
                        .windows
                        .get(window_id)
                        .map(|window| window.state_id.clone())
                })
                .collect::<Vec<_>>(),
        );
        if active_state_ids.is_empty() {
            return Vec::new();
        }

        if let Some(snapshot) = compute_layout_snapshot(
            &mut self.layout_service,
            &self.config,
            &self.runtime_state,
            &active_state_ids,
        ) {
            return active_state_ids
                .into_iter()
                .filter_map(|window_id| {
                    snapshot.find_by_window_id(&window_id).map(|node| {
                        let rect = node.rect();
                        RenderWindowPlan {
                            window_id,
                            x: rect.x.round() as i32,
                            y: rect.y.round() as i32,
                            width: rect.width.round() as i32,
                            height: rect.height.round() as i32,
                        }
                    })
                })
                .collect();
        }

        compute_horizontal_tiles(
            &active_state_ids,
            origin_x,
            origin_y,
            total_width,
            total_height,
        )
        .into_iter()
        .map(|tile| RenderWindowPlan {
            window_id: tile.window_id,
            x: tile.x,
            y: tile.y,
            width: tile.width,
            height: tile.height,
        })
        .collect()
    }

    pub(super) fn plan_window_borders(&self) -> Vec<BorderPlan> {
        let all_edges = river_window_v1::Edges::Top
            | river_window_v1::Edges::Bottom
            | river_window_v1::Edges::Left
            | river_window_v1::Edges::Right;
        let active_workspace_window_ids = self.active_workspace_window_state_ids();

        compute_window_borders(&self.runtime_state, &active_workspace_window_ids)
            .into_iter()
            .map(|border| BorderPlan {
                window_id: border.window_id,
                width: border.width,
                edges: all_edges,
                red: border.red,
                green: border.green,
                blue: border.blue,
                alpha: border.alpha,
            })
            .collect()
    }

    pub(super) fn plan_window_mode_updates(&self) -> Vec<WindowModePlan> {
        let (origin_x, origin_y, total_width, total_height) = self.current_output_geometry();

        self.active_workspace_window_state_ids()
            .into_iter()
            .filter_map(|window_id| {
                let window = self.runtime_state.windows.get(&window_id)?;
                let mode = configured_mode_for_window(&self.config, window)?;
                let (x, y, width, height) = match &mode {
                    spiders_shared::wm::WindowMode::Floating { rect } => {
                        let rect = rect.unwrap_or(LayoutRect {
                            x: origin_x as f32 + (total_width as f32 * 0.1),
                            y: origin_y as f32 + (total_height as f32 * 0.1),
                            width: (total_width as f32 * 0.8).max(1.0),
                            height: (total_height as f32 * 0.8).max(1.0),
                        });
                        (
                            rect.x.round() as i32,
                            rect.y.round() as i32,
                            rect.width.round() as i32,
                            rect.height.round() as i32,
                        )
                    }
                    spiders_shared::wm::WindowMode::Fullscreen => {
                        (origin_x, origin_y, total_width.max(1), total_height.max(1))
                    }
                    spiders_shared::wm::WindowMode::Tiled => return None,
                };

                Some(WindowModePlan {
                    window_id,
                    mode,
                    x,
                    y,
                    width,
                    height,
                })
            })
            .collect()
    }

    pub(super) fn plan_toggle_floating_command(
        &self,
        seat_id: &ObjectId,
    ) -> Option<WindowModePlan> {
        let window_id = self.seat_focused_state_window_id(seat_id)?;
        let window = self.runtime_state.windows.get(&window_id)?;
        let (origin_x, origin_y, total_width, total_height) = self.current_output_geometry();

        let mode = match &window.mode {
            spiders_shared::wm::WindowMode::Floating { .. } => {
                spiders_shared::wm::WindowMode::Tiled
            }
            spiders_shared::wm::WindowMode::Tiled | spiders_shared::wm::WindowMode::Fullscreen => {
                spiders_shared::wm::WindowMode::Floating {
                    rect: Some(window.last_floating_rect.unwrap_or(
                        LayoutRect {
                            x: origin_x as f32 + (total_width as f32 * 0.1),
                            y: origin_y as f32 + (total_height as f32 * 0.1),
                            width: (total_width as f32 * 0.8).max(1.0),
                            height: (total_height as f32 * 0.8).max(1.0),
                        },
                    )),
                }
            }
        };

        let (x, y, width, height) = match &mode {
            spiders_shared::wm::WindowMode::Tiled => (
                window.x,
                window.y,
                window.width.max(1),
                window.height.max(1),
            ),
            spiders_shared::wm::WindowMode::Floating { rect } => {
                let rect = rect.unwrap();
                (
                    rect.x.round() as i32,
                    rect.y.round() as i32,
                    rect.width.round() as i32,
                    rect.height.round() as i32,
                )
            }
            spiders_shared::wm::WindowMode::Fullscreen => {
                (origin_x, origin_y, total_width.max(1), total_height.max(1))
            }
        };

        Some(WindowModePlan {
            window_id,
            mode,
            x,
            y,
            width,
            height,
        })
    }

    pub(super) fn plan_toggle_fullscreen_command(
        &self,
        seat_id: &ObjectId,
    ) -> Option<WindowModePlan> {
        let window_id = self.seat_focused_state_window_id(seat_id)?;
        let window = self.runtime_state.windows.get(&window_id)?;
        let (origin_x, origin_y, total_width, total_height) = self.current_output_geometry();

        let mode = match &window.mode {
            spiders_shared::wm::WindowMode::Fullscreen => {
                if let Some(rect) = window.last_floating_rect {
                    spiders_shared::wm::WindowMode::Floating { rect: Some(rect) }
                } else {
                    spiders_shared::wm::WindowMode::Tiled
                }
            }
            spiders_shared::wm::WindowMode::Tiled
            | spiders_shared::wm::WindowMode::Floating { .. } => {
                spiders_shared::wm::WindowMode::Fullscreen
            }
        };

        let (x, y, width, height) = match &mode {
            spiders_shared::wm::WindowMode::Fullscreen => {
                (origin_x, origin_y, total_width.max(1), total_height.max(1))
            }
            spiders_shared::wm::WindowMode::Tiled => (
                window.x,
                window.y,
                window.width.max(1),
                window.height.max(1),
            ),
            spiders_shared::wm::WindowMode::Floating { rect } => {
                let rect = rect.as_ref()?;
                (
                    rect.x.round() as i32,
                    rect.y.round() as i32,
                    rect.width.round() as i32,
                    rect.height.round() as i32,
                )
            }
        };

        Some(WindowModePlan {
            window_id,
            mode,
            x,
            y,
            width,
            height,
        })
    }

    pub(super) fn plan_focus_for_seat(&self, _seat_id: &ObjectId) -> FocusPlan {
        let top_window_id = top_window_id(&self.active_workspace_window_state_ids());

        match top_window_id {
            Some(window_id) => FocusPlan::FocusWindow { window_id },
            None => FocusPlan::ClearFocus,
        }
    }

    pub(super) fn plan_close_focused_window(&self, seat_id: &ObjectId) -> Option<CloseWindowPlan> {
        self.seat_focused_state_window_id(seat_id)
            .map(|window_id| CloseWindowPlan { window_id })
    }

    pub(super) fn plan_activate_workspace_command(
        &self,
        workspace_id: spiders_tree::WorkspaceId,
    ) -> ActivateWorkspacePlan {
        ActivateWorkspacePlan {
            workspace_id,
            focus: FocusPlan::ClearFocus,
        }
    }

    pub(super) fn plan_move_focused_window_to_workspace_command(
        &self,
        seat_id: &ObjectId,
        workspace_id: spiders_tree::WorkspaceId,
    ) -> Option<MoveFocusedWindowToWorkspacePlan> {
        let window_id = self.seat_focused_state_window_id(seat_id)?;
        let focus = self.plan_focus_for_seat(seat_id);

        Some(MoveFocusedWindowToWorkspacePlan {
            window_id,
            workspace_id,
            focus,
        })
    }

    pub(super) fn plan_move_direction_command(
        &self,
        seat_id: &ObjectId,
        direction: FocusDirection,
    ) -> Option<MoveWindowInWorkspacePlan> {
        let window_id = self.seat_focused_state_window_id(seat_id)?;
        let active_window_ids = self.active_workspace_window_state_ids();
        let target_window_id = directional_neighbor_window_id(
            &self.runtime_state,
            &active_window_ids,
            &window_id,
            direction,
        )?;

        Some(MoveWindowInWorkspacePlan {
            window_id: window_id.clone(),
            target_window_id,
            focus: FocusPlan::FocusWindow { window_id },
        })
    }

    pub(super) fn plan_focus_window_command(
        &self,
        window_id: spiders_tree::WindowId,
    ) -> Option<(MoveWindowToTopPlan, FocusPlan)> {
        self.window_object_id(&window_id)?;
        Some((
            MoveWindowToTopPlan {
                window_id: window_id.clone(),
            },
            FocusPlan::FocusWindow { window_id },
        ))
    }

    pub(super) fn plan_focus_direction_command(
        &self,
        seat_id: &ObjectId,
        direction: FocusDirection,
    ) -> Option<(MoveWindowToTopPlan, FocusPlan)> {
        let active_state_ids = self.active_workspace_window_state_ids();
        if active_state_ids.len() <= 1 {
            return None;
        }

        let focused_state_id = self
            .seat_focused_state_window_id(seat_id)
            .or_else(|| active_state_ids.last().cloned());

        let target_state_id = focus_target_in_direction(
            &self.runtime_state,
            &active_state_ids,
            direction,
            focused_state_id.as_ref(),
        )?;

        Some((
            MoveWindowToTopPlan {
                window_id: target_state_id.clone(),
            },
            FocusPlan::FocusWindow {
                window_id: target_state_id,
            },
        ))
    }

    pub(super) fn plan_pointer_render_ops(&self) -> Vec<PointerRenderPlan> {
        compute_pointer_render_positions(&self.runtime_state)
            .into_iter()
            .map(|position| PointerRenderPlan {
                window_id: position.window_id,
                x: position.x,
                y: position.y,
            })
            .collect()
    }

    pub(super) fn plan_inactive_tiled_windows(&self) -> Vec<ClearTiledStatePlan> {
        inactive_window_ids(
            &self.active_workspace_window_state_ids(),
            &self
                .runtime_state
                .window_stack
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .into_iter()
        .map(|window_id| ClearTiledStatePlan { window_id })
        .collect()
    }

    pub(super) fn plan_offscreen_windows(&self) -> Vec<OffscreenWindowPlan> {
        inactive_window_ids(
            &self.active_workspace_window_state_ids(),
            &self
                .runtime_state
                .window_stack
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .into_iter()
        .map(|window_id| OffscreenWindowPlan {
            window_id,
            x: -20_000,
            y: -20_000,
        })
        .collect()
    }

    pub(super) fn apply_tiled_manage_layout(&mut self) {
        let clear_plan = self.plan_inactive_tiled_windows();
        self.apply_clear_tiled_state_plan(&clear_plan);

        if !self.active_workspace_window_ids().is_empty() {
            let plan = self.plan_tiled_manage_layout();
            self.apply_manage_window_plan(&plan);
        }
    }

    pub(super) fn apply_tiled_render_layout(&mut self) {
        let offscreen_plan = self.plan_offscreen_windows();
        self.apply_offscreen_window_plan(&offscreen_plan);

        if !self.active_workspace_window_ids().is_empty() {
            let plan = self.plan_tiled_render_layout();
            self.apply_render_window_plan(&plan);
        }
    }

    pub(super) fn apply_window_borders(&mut self) {
        let plan = self.plan_window_borders();
        self.apply_border_plan(&plan);
    }

    pub(super) fn has_active_pointer_op(&self) -> bool {
        self.runtime_state
            .seats
            .values()
            .any(|seat| !matches!(seat.pointer_op, SeatPointerOpState::None))
    }

    pub(super) fn focus_top_window_for_seat(&mut self, seat_id: &ObjectId) {
        let plan = self.plan_focus_for_seat(seat_id);
        self.apply_focus_plan(seat_id, &plan);
    }

    pub(super) fn plan_command(&self, seat_id: &ObjectId, command: RiverCommand) -> CommandPlan {
        match command {
            RiverCommand::Spawn { command } => CommandPlan::Spawn { command },
            RiverCommand::ActivateWorkspace { workspace_id } => {
                CommandPlan::ActivateWorkspace(self.plan_activate_workspace_command(workspace_id))
            }
            RiverCommand::AssignFocusedWindowToWorkspace { workspace_id } => self
                .plan_move_focused_window_to_workspace_command(seat_id, workspace_id)
                .map(CommandPlan::MoveFocusedWindowToWorkspace)
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::MoveDirection { direction } => self
                .plan_move_direction_command(seat_id, direction)
                .map(CommandPlan::MoveWindowInWorkspace)
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::ToggleFloating => self
                .plan_toggle_floating_command(seat_id)
                .map(CommandPlan::SetWindowMode)
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::ToggleFullscreen => self
                .plan_toggle_fullscreen_command(seat_id)
                .map(CommandPlan::SetWindowMode)
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::FocusOutput { output_id } => CommandPlan::FocusOutput { output_id },
            RiverCommand::FocusWindow { window_id } => self
                .plan_focus_window_command(window_id)
                .map(|(stack, focus)| CommandPlan::FocusWindow { stack, focus })
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::CloseFocusedWindow => CommandPlan::CloseFocusedWindow,
            RiverCommand::FocusDirection { direction } => self
                .plan_focus_direction_command(seat_id, direction)
                .map(|(stack, focus)| CommandPlan::FocusDirection { stack, focus })
                .unwrap_or(CommandPlan::Noop),
            RiverCommand::ReloadConfig
            | RiverCommand::SetLayout { .. }
            | RiverCommand::CycleLayoutNext
            | RiverCommand::CycleLayoutPrevious
            | RiverCommand::SetFloatingWindowGeometry { .. }
            | RiverCommand::Unsupported { .. } => CommandPlan::Noop,
        }
    }
}
