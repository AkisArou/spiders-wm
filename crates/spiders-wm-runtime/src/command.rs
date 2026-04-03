use spiders_core::command::{FocusDirection, LayoutCycleDirection, WmCommand};

pub fn display_command_label(command: &WmCommand) -> String {
    match command {
        WmCommand::Spawn { command } => format!("spawn({command})"),
        WmCommand::Quit => "quit".to_string(),
        WmCommand::ReloadConfig => "reload_config".to_string(),
        WmCommand::SetLayout { name } => format!("set_layout({name})"),
        WmCommand::CycleLayout { direction } => match direction {
            Some(direction) => format!("cycle_layout({})", display_cycle_direction(*direction)),
            None => "cycle_layout".to_string(),
        },
        WmCommand::ViewWorkspace { workspace } => format!("view_workspace({workspace})"),
        WmCommand::ToggleViewWorkspace { workspace } => {
            format!("toggle_view_workspace({workspace})")
        }
        WmCommand::ActivateWorkspace { workspace_id } => {
            format!("activate_workspace({})", workspace_id.as_str())
        }
        WmCommand::AssignWorkspace { workspace_id, output_id } => {
            format!("assign_workspace({}, {})", workspace_id.as_str(), output_id.as_str())
        }
        WmCommand::FocusMonitorLeft => "focus_mon_left".to_string(),
        WmCommand::FocusMonitorRight => "focus_mon_right".to_string(),
        WmCommand::SendMonitorLeft => "send_mon_left".to_string(),
        WmCommand::SendMonitorRight => "send_mon_right".to_string(),
        WmCommand::ToggleFloating => "toggle_floating".to_string(),
        WmCommand::ToggleFullscreen => "toggle_fullscreen".to_string(),
        WmCommand::AssignFocusedWindowToWorkspace { workspace } => {
            format!("assign_workspace({workspace})")
        }
        WmCommand::ToggleAssignFocusedWindowToWorkspace { workspace } => {
            format!("toggle_workspace({workspace})")
        }
        WmCommand::FocusWindow { window_id } => format!("focus_window({})", window_id.as_str()),
        WmCommand::SetFloatingWindowGeometry { window_id, .. } => {
            format!("set_floating_window_geometry({})", window_id.as_str())
        }
        WmCommand::FocusDirection { direction } => {
            format!("focus_dir({})", display_direction(*direction))
        }
        WmCommand::SwapDirection { direction } => {
            format!("swap_dir({})", display_direction(*direction))
        }
        WmCommand::ResizeDirection { direction } => {
            format!("resize_dir({})", display_direction(*direction))
        }
        WmCommand::ResizeTiledDirection { direction } => {
            format!("resize_tiled({})", display_direction(*direction))
        }
        WmCommand::MoveDirection { direction } => {
            format!("move({})", display_direction(*direction))
        }
        WmCommand::SpawnTerminal => "spawn_terminal".to_string(),
        WmCommand::FocusNextWindow => "focus_next".to_string(),
        WmCommand::FocusPreviousWindow => "focus_prev".to_string(),
        WmCommand::SelectNextWorkspace => "select_next_workspace".to_string(),
        WmCommand::SelectPreviousWorkspace => "select_previous_workspace".to_string(),
        WmCommand::SelectWorkspace { workspace_id } => {
            format!("select_workspace({})", workspace_id.as_str())
        }
        WmCommand::CloseFocusedWindow => "kill_client".to_string(),
    }
}

fn display_direction(direction: FocusDirection) -> &'static str {
    match direction {
        FocusDirection::Left => "left",
        FocusDirection::Right => "right",
        FocusDirection::Up => "up",
        FocusDirection::Down => "down",
    }
}

fn display_cycle_direction(direction: LayoutCycleDirection) -> &'static str {
    match direction {
        LayoutCycleDirection::Next => "next",
        LayoutCycleDirection::Previous => "previous",
    }
}
