use spiders_core::command::WmCommand;
use spiders_core::effect::{
    FocusTarget, WindowToggle, WmHostEffect, WorkspaceAssignment, WorkspaceTarget,
};
use tracing::warn;

use crate::session::PreviewRenderAction;

pub trait WmHost {
    fn on_effect(&mut self, effect: WmHostEffect) -> PreviewRenderAction;
}

pub fn dispatch_wm_command<H: WmHost>(host: &mut H, command: WmCommand) -> PreviewRenderAction {
    match command {
        WmCommand::Spawn { command } => host.on_effect(WmHostEffect::SpawnCommand { command }),
        WmCommand::Quit => host.on_effect(WmHostEffect::RequestQuit),
        WmCommand::ViewWorkspace { workspace } => host.on_effect(WmHostEffect::ActivateWorkspace {
            target: WorkspaceTarget::Named(workspace.to_string()),
        }),
        WmCommand::ActivateWorkspace { workspace_id } => {
            host.on_effect(WmHostEffect::ActivateWorkspace {
                target: WorkspaceTarget::Named(workspace_id.0),
            })
        }
        WmCommand::AssignFocusedWindowToWorkspace { workspace } => {
            host.on_effect(WmHostEffect::AssignFocusedWindowToWorkspace {
                assignment: WorkspaceAssignment::Move(workspace),
            })
        }
        WmCommand::ToggleAssignFocusedWindowToWorkspace { workspace } => {
            host.on_effect(WmHostEffect::AssignFocusedWindowToWorkspace {
                assignment: WorkspaceAssignment::Toggle(workspace),
            })
        }
        WmCommand::SpawnTerminal => host.on_effect(WmHostEffect::SpawnTerminal),
        WmCommand::FocusNextWindow => {
            host.on_effect(WmHostEffect::FocusWindow { target: FocusTarget::Next })
        }
        WmCommand::FocusPreviousWindow => {
            host.on_effect(WmHostEffect::FocusWindow { target: FocusTarget::Previous })
        }
        WmCommand::SelectNextWorkspace => {
            host.on_effect(WmHostEffect::ActivateWorkspace { target: WorkspaceTarget::Next })
        }
        WmCommand::SelectPreviousWorkspace => {
            host.on_effect(WmHostEffect::ActivateWorkspace { target: WorkspaceTarget::Previous })
        }
        WmCommand::SelectWorkspace { workspace_id } => {
            host.on_effect(WmHostEffect::ActivateWorkspace {
                target: WorkspaceTarget::Named(workspace_id.0),
            })
        }
        WmCommand::CloseFocusedWindow => host.on_effect(WmHostEffect::CloseFocusedWindow),
        WmCommand::ReloadConfig => host.on_effect(WmHostEffect::ReloadConfig),
        WmCommand::SetLayout { name } => host.on_effect(WmHostEffect::SetLayout { name }),
        WmCommand::CycleLayout { direction } => {
            host.on_effect(WmHostEffect::CycleLayout { direction })
        }
        WmCommand::ToggleFullscreen => {
            host.on_effect(WmHostEffect::ToggleFocusedWindow { toggle: WindowToggle::Fullscreen })
        }
        WmCommand::ToggleFloating => {
            host.on_effect(WmHostEffect::ToggleFocusedWindow { toggle: WindowToggle::Floating })
        }
        WmCommand::FocusDirection { direction } => {
            host.on_effect(WmHostEffect::FocusWindow { target: FocusTarget::Direction(direction) })
        }
        WmCommand::FocusWindow { window_id } => {
            host.on_effect(WmHostEffect::FocusWindow { target: FocusTarget::Window(window_id) })
        }
        WmCommand::SwapDirection { direction } => {
            host.on_effect(WmHostEffect::SwapFocusedWindow { direction })
        }
        WmCommand::ResizeDirection { direction } => {
            warn!(?direction, "resize wm command is intentionally stubbed for now");
            PreviewRenderAction::None
        }
        WmCommand::ResizeTiledDirection { direction } => {
            warn!(?direction, "resize-tiled wm command is intentionally stubbed for now");
            PreviewRenderAction::None
        }
        WmCommand::MoveDirection { direction } => {
            warn!(?direction, "move-direction wm command is intentionally stubbed for now");
            PreviewRenderAction::None
        }
        unsupported => {
            warn!(?unsupported, "ignoring unsupported wm command");
            PreviewRenderAction::None
        }
    }
}
