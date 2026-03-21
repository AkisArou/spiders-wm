use std::collections::{BTreeMap, VecDeque};

use spiders_config::model::Config;
use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
use spiders_shared::layout::LayoutRect;
use spiders_shared::wm::{
    OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowMode, WindowSnapshot,
    WorkspaceSnapshot,
};

use crate::model::{OutputState, SeatPointerOpState, SeatState, WindowState, WorkspaceState};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WmState {
    pub workspaces: BTreeMap<WorkspaceId, WorkspaceState>,
    pub outputs: BTreeMap<OutputId, OutputState>,
    pub seats: BTreeMap<String, SeatState>,
    pub windows: BTreeMap<WindowId, WindowState>,
    pub window_stack: VecDeque<WindowId>,
    pub current_output_id: Option<OutputId>,
    pub current_workspace_id: Option<WorkspaceId>,
    pub focused_window_id: Option<WindowId>,
}

impl WmState {
    pub fn from_config(config: &Config) -> Self {
        let names = if config.workspaces.is_empty() {
            (1..=9).map(|n| n.to_string()).collect::<Vec<_>>()
        } else {
            config.workspaces.clone()
        };

        let workspaces = names
            .iter()
            .enumerate()
            .map(|(idx, name)| {
                let id = WorkspaceId::from(name.as_str());
                let tag_mask = 1_u32.checked_shl(idx as u32).unwrap_or(0);
                (
                    id.clone(),
                    WorkspaceState {
                        id,
                        name: name.clone(),
                        tag_mask,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();

        let current_workspace_id = workspaces.keys().next().cloned();

        Self {
            workspaces,
            current_workspace_id,
            ..Self::default()
        }
    }

    pub fn workspace_names(&self) -> Vec<&str> {
        self.workspaces
            .values()
            .map(|workspace| workspace.name.as_str())
            .collect()
    }

    pub fn current_workspace_name(&self) -> Option<&str> {
        let current = self.current_workspace_id.as_ref()?;
        self.workspaces
            .get(current)
            .map(|workspace| workspace.name.as_str())
    }

    pub fn insert_output(&mut self, output_id: OutputId, name: String) {
        let focused_workspace_id = self.current_workspace_id.clone();
        self.outputs.insert(
            output_id.clone(),
            OutputState {
                id: output_id,
                name,
                logical_x: 0,
                logical_y: 0,
                logical_width: 0,
                logical_height: 0,
                enabled: true,
                focused_workspace_id,
            },
        );
        if self.current_workspace_id.is_none() {
            self.current_workspace_id = self.workspaces.keys().next().cloned();
        }
    }

    pub fn remove_output(&mut self, output_id: &OutputId) {
        self.outputs.remove(output_id);
        if self.current_output_id.as_ref() == Some(output_id) {
            self.current_output_id = self.outputs.keys().next().cloned();
        }
    }

    pub fn set_output_name(&mut self, output_id: &OutputId, name: String) {
        if let Some(output) = self.outputs.get_mut(output_id) {
            output.name = name;
        }
    }

    pub fn set_output_position(&mut self, output_id: &OutputId, logical_x: i32, logical_y: i32) {
        if let Some(output) = self.outputs.get_mut(output_id) {
            output.logical_x = logical_x;
            output.logical_y = logical_y;
        }
    }

    pub fn set_output_dimensions(
        &mut self,
        output_id: &OutputId,
        logical_width: u32,
        logical_height: u32,
    ) {
        if let Some(output) = self.outputs.get_mut(output_id) {
            output.logical_width = logical_width;
            output.logical_height = logical_height;
        }
    }

    pub fn set_output_enabled(&mut self, output_id: &OutputId, enabled: bool) {
        if let Some(output) = self.outputs.get_mut(output_id) {
            output.enabled = enabled;
        }
    }

    pub fn focus_output(&mut self, output_id: &OutputId) {
        if self.outputs.contains_key(output_id) {
            self.current_output_id = Some(output_id.clone());
            if let Some(workspace_id) = self
                .outputs
                .get(output_id)
                .and_then(|output| output.focused_workspace_id.clone())
            {
                self.current_workspace_id = Some(workspace_id);
            }
        }
    }

    pub fn assign_workspace_to_output(&mut self, output_id: &OutputId, workspace_id: &WorkspaceId) {
        if let Some(output) = self.outputs.get_mut(output_id) {
            output.focused_workspace_id = Some(workspace_id.clone());
        }
    }

    pub fn insert_seat(&mut self, name: String) {
        self.seats.insert(
            name.clone(),
            SeatState {
                name,
                focused_window_id: None,
                hovered_window_id: None,
                interacted_window_id: None,
                pointer_op: SeatPointerOpState::None,
                pointer_op_dx: 0,
                pointer_op_dy: 0,
                pointer_op_release: false,
            },
        );
    }

    pub fn remove_seat(&mut self, name: &str) {
        self.seats.remove(name);
    }

    pub fn set_seat_focused_window(&mut self, seat_name: &str, window_id: Option<WindowId>) {
        if let Some(seat) = self.seats.get_mut(seat_name) {
            seat.focused_window_id = window_id;
        }
    }

    pub fn set_seat_hovered_window(&mut self, seat_name: &str, window_id: Option<WindowId>) {
        if let Some(seat) = self.seats.get_mut(seat_name) {
            seat.hovered_window_id = window_id;
        }
    }

    pub fn set_seat_interacted_window(&mut self, seat_name: &str, window_id: Option<WindowId>) {
        if let Some(seat) = self.seats.get_mut(seat_name) {
            seat.interacted_window_id = window_id;
        }
    }

    pub fn set_seat_pointer_op(&mut self, seat_name: &str, pointer_op: SeatPointerOpState) {
        if let Some(seat) = self.seats.get_mut(seat_name) {
            seat.pointer_op = pointer_op;
        }
    }

    pub fn set_seat_pointer_delta(&mut self, seat_name: &str, dx: i32, dy: i32) {
        if let Some(seat) = self.seats.get_mut(seat_name) {
            seat.pointer_op_dx = dx;
            seat.pointer_op_dy = dy;
        }
    }

    pub fn set_seat_pointer_release(&mut self, seat_name: &str, release: bool) {
        if let Some(seat) = self.seats.get_mut(seat_name) {
            seat.pointer_op_release = release;
        }
    }

    pub fn insert_window(&mut self, window_id: WindowId) {
        self.window_stack.push_back(window_id.clone());
        self.windows.insert(
            window_id.clone(),
            WindowState {
                id: window_id,
                app_id: None,
                title: None,
                class: None,
                instance: None,
                role: None,
                window_type: None,
                identifier: None,
                unreliable_pid: None,
                output_id: self.current_output_id.clone(),
                workspace_ids: self.current_workspace_id.iter().cloned().collect(),
                is_new: true,
                closed: false,
                mapped: true,
                mode: WindowMode::Tiled,
                focused: false,
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                last_floating_rect: None,
            },
        );
    }

    pub fn remove_window(&mut self, window_id: &WindowId) {
        self.window_stack.retain(|id| id != window_id);
        self.windows.remove(window_id);
        if self.focused_window_id.as_ref() == Some(window_id) {
            self.focused_window_id = None;
        }
    }

    pub fn set_window_app_id(&mut self, window_id: &WindowId, app_id: Option<String>) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.app_id = app_id;
        }
    }

    pub fn set_window_title(&mut self, window_id: &WindowId, title: Option<String>) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.title = title;
        }
    }

    pub fn set_window_class(&mut self, window_id: &WindowId, class: Option<String>) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.class = class;
        }
    }

    pub fn set_window_instance(&mut self, window_id: &WindowId, instance: Option<String>) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.instance = instance;
        }
    }

    pub fn set_window_role(&mut self, window_id: &WindowId, role: Option<String>) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.role = role;
        }
    }

    pub fn set_window_type(&mut self, window_id: &WindowId, window_type: Option<String>) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.window_type = window_type;
        }
    }

    pub fn set_window_identifier(&mut self, window_id: &WindowId, identifier: Option<String>) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.identifier = identifier;
        }
    }

    pub fn set_window_unreliable_pid(&mut self, window_id: &WindowId, unreliable_pid: Option<u32>) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.unreliable_pid = unreliable_pid;
        }
    }

    pub fn set_window_output(&mut self, window_id: &WindowId, output_id: Option<OutputId>) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.output_id = output_id;
        }
    }

    pub fn set_window_geometry(
        &mut self,
        window_id: &WindowId,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.x = x;
            window.y = y;
            window.width = width;
            window.height = height;
            if matches!(window.mode, WindowMode::Floating { .. }) {
                window.last_floating_rect = Some(LayoutRect {
                    x: x as f32,
                    y: y as f32,
                    width: width as f32,
                    height: height as f32,
                });
            }
        }
    }

    pub fn set_window_size(&mut self, window_id: &WindowId, width: i32, height: i32) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.width = width;
            window.height = height;
        }
    }

    pub fn set_window_new(&mut self, window_id: &WindowId, is_new: bool) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.is_new = is_new;
        }
    }

    pub fn set_window_closed(&mut self, window_id: &WindowId, closed: bool) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.closed = closed;
        }
    }

    pub fn set_window_workspace(&mut self, window_id: &WindowId, workspace_id: &WorkspaceId) {
        if let Some(window) = self.windows.get_mut(window_id) {
            window.workspace_ids.clear();
            window.workspace_ids.push(workspace_id.clone());
        }
    }

    pub fn set_window_mode(&mut self, window_id: &WindowId, mode: WindowMode) {
        if let Some(window) = self.windows.get_mut(window_id) {
            if let WindowMode::Floating { rect } = &mode {
                if let Some(rect) = rect {
                    window.last_floating_rect = Some(*rect);
                }
            }
            window.mode = mode;
        }
    }

    pub fn focus_window(&mut self, window_id: &WindowId) {
        self.window_stack.retain(|id| id != window_id);
        self.window_stack.push_back(window_id.clone());
        for window in self.windows.values_mut() {
            window.focused = &window.id == window_id;
        }
        if let Some(window) = self.windows.get(window_id) {
            self.focused_window_id = Some(window_id.clone());
            if let Some(output_id) = window.output_id.as_ref() {
                self.current_output_id = Some(output_id.clone());
            }
            if let Some(workspace_id) = window.workspace_ids.first() {
                self.current_workspace_id = Some(workspace_id.clone());
            }
        }
    }

    pub fn visible_window_ids(&self) -> Vec<WindowId> {
        let current_workspace_id = self.current_workspace_id.as_ref();

        self.windows
            .values()
            .filter(|window| {
                window.mapped
                    && current_workspace_id.is_none_or(|workspace_id| {
                        window.workspace_ids.iter().any(|id| id == workspace_id)
                    })
            })
            .map(|window| window.id.clone())
            .collect()
    }

    pub fn move_window_to_top(&mut self, window_id: &WindowId) {
        self.window_stack.retain(|id| id != window_id);
        self.window_stack.push_back(window_id.clone());
    }

    pub fn move_window_in_stack(&mut self, window_id: &WindowId, delta: isize) -> bool {
        let Some(index) = self.window_stack.iter().position(|id| id == window_id) else {
            return false;
        };
        let next_index = index as isize + delta;
        if next_index < 0 || next_index >= self.window_stack.len() as isize {
            return false;
        }
        let next_index = next_index as usize;
        self.window_stack.swap(index, next_index);
        true
    }

    pub fn swap_windows_in_stack(&mut self, first: &WindowId, second: &WindowId) -> bool {
        let Some(first_index) = self.window_stack.iter().position(|id| id == first) else {
            return false;
        };
        let Some(second_index) = self.window_stack.iter().position(|id| id == second) else {
            return false;
        };
        self.window_stack.swap(first_index, second_index);
        true
    }

    pub fn as_state_snapshot(&self) -> StateSnapshot {
        let outputs = self
            .outputs
            .values()
            .cloned()
            .map(|output| OutputSnapshot {
                id: output.id,
                name: output.name,
                logical_x: output.logical_x,
                logical_y: output.logical_y,
                logical_width: output.logical_width,
                logical_height: output.logical_height,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: output.enabled,
                current_workspace_id: output.focused_workspace_id,
            })
            .collect();

        let workspaces = self
            .workspaces
            .values()
            .cloned()
            .map(|workspace| {
                let output_id = self
                    .outputs
                    .values()
                    .find(|output| output.focused_workspace_id.as_ref() == Some(&workspace.id))
                    .map(|output| output.id.clone());
                let focused = self.current_workspace_id.as_ref() == Some(&workspace.id);

                WorkspaceSnapshot {
                    id: workspace.id,
                    name: workspace.name.clone(),
                    output_id,
                    active_workspaces: vec![workspace.name],
                    focused,
                    visible: focused,
                    effective_layout: None,
                }
            })
            .collect();

        let windows = self
            .windows
            .values()
            .cloned()
            .map(|window| WindowSnapshot {
                id: window.id,
                shell: ShellKind::Unknown,
                app_id: window.app_id,
                title: window.title,
                class: window.class,
                instance: window.instance,
                role: window.role,
                window_type: window.window_type,
                mapped: window.mapped,
                mode: window.mode,
                focused: window.focused,
                urgent: false,
                output_id: window.output_id,
                workspace_id: window.workspace_ids.first().cloned(),
                workspaces: window
                    .workspace_ids
                    .iter()
                    .filter_map(|workspace_id| self.workspaces.get(workspace_id))
                    .map(|workspace| workspace.name.clone())
                    .collect(),
            })
            .collect();

        StateSnapshot {
            focused_window_id: self.focused_window_id.clone(),
            current_output_id: self.current_output_id.clone(),
            current_workspace_id: self.current_workspace_id.clone(),
            outputs,
            workspaces,
            windows,
            visible_window_ids: self.visible_window_ids(),
            workspace_names: self
                .workspace_names()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }
}
