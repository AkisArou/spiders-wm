use spiders_core::command::{FocusDirection, LayoutCycleDirection, WmCommand};
use spiders_core::query::QueryRequest;
use spiders_ipc_core::{DebugDumpKind, IpcSubscriptionTopic};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliShell {
    Zsh,
    Bash,
    Fish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliQuery {
    State,
    FocusedWindow,
    CurrentOutput,
    CurrentWorkspace,
    MonitorList,
    WorkspaceNames,
}

impl CliQuery {
    pub const ALL: [Self; 6] = [
        Self::State,
        Self::FocusedWindow,
        Self::CurrentOutput,
        Self::CurrentWorkspace,
        Self::MonitorList,
        Self::WorkspaceNames,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::FocusedWindow => "focused-window",
            Self::CurrentOutput => "current-output",
            Self::CurrentWorkspace => "current-workspace",
            Self::MonitorList => "monitor-list",
            Self::WorkspaceNames => "workspace-names",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Self::State => "full compositor state snapshot",
            Self::FocusedWindow => "currently focused window",
            Self::CurrentOutput => "currently focused output",
            Self::CurrentWorkspace => "currently focused workspace",
            Self::MonitorList => "list outputs and geometry",
            Self::WorkspaceNames => "configured workspace names",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|candidate| candidate.name() == token)
    }

    pub fn to_runtime(self) -> QueryRequest {
        match self {
            Self::State => QueryRequest::State,
            Self::FocusedWindow => QueryRequest::FocusedWindow,
            Self::CurrentOutput => QueryRequest::CurrentOutput,
            Self::CurrentWorkspace => QueryRequest::CurrentWorkspace,
            Self::MonitorList => QueryRequest::MonitorList,
            Self::WorkspaceNames => QueryRequest::WorkspaceNames,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliTopic {
    All,
    Focus,
    Windows,
    Workspaces,
    Layout,
    Config,
}

impl CliTopic {
    pub const ALL: [Self; 6] =
        [Self::All, Self::Focus, Self::Windows, Self::Workspaces, Self::Layout, Self::Config];

    pub fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Focus => "focus",
            Self::Windows => "windows",
            Self::Workspaces => "workspaces",
            Self::Layout => "layout",
            Self::Config => "config",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Self::All => "all event topics",
            Self::Focus => "focus events",
            Self::Windows => "window lifecycle and mode events",
            Self::Workspaces => "workspace and output events",
            Self::Layout => "layout change events",
            Self::Config => "config reload events",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|candidate| candidate.name() == token)
    }

    pub fn to_runtime(self) -> IpcSubscriptionTopic {
        match self {
            Self::All => IpcSubscriptionTopic::All,
            Self::Focus => IpcSubscriptionTopic::Focus,
            Self::Windows => IpcSubscriptionTopic::Windows,
            Self::Workspaces => IpcSubscriptionTopic::Workspaces,
            Self::Layout => IpcSubscriptionTopic::Layout,
            Self::Config => IpcSubscriptionTopic::Config,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliDumpKind {
    WmState,
    DebugProfile,
    SceneSnapshot,
    FrameSync,
    Seats,
}

impl CliDumpKind {
    pub const ALL: [Self; 5] =
        [Self::WmState, Self::DebugProfile, Self::SceneSnapshot, Self::FrameSync, Self::Seats];

    pub fn name(self) -> &'static str {
        match self {
            Self::WmState => "wm-state",
            Self::DebugProfile => "debug-profile",
            Self::SceneSnapshot => "scene-snapshot",
            Self::FrameSync => "frame-sync",
            Self::Seats => "seats",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Self::WmState => "dump compositor state",
            Self::DebugProfile => "dump debug profiling data",
            Self::SceneSnapshot => "dump current scene snapshot",
            Self::FrameSync => "dump frame sync information",
            Self::Seats => "dump input seat information",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|candidate| candidate.name() == token)
    }

    pub fn to_runtime(self) -> DebugDumpKind {
        match self {
            Self::WmState => DebugDumpKind::WmState,
            Self::DebugProfile => DebugDumpKind::DebugProfile,
            Self::SceneSnapshot => DebugDumpKind::SceneSnapshot,
            Self::FrameSync => DebugDumpKind::FrameSync,
            Self::Seats => DebugDumpKind::Seats,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliCommandSpec {
    pub name: &'static str,
    pub help: &'static str,
}

pub fn wm_command_specs() -> &'static [CliCommandSpec] {
    &[
        CliCommandSpec { name: "close-focused-window", help: "close the currently focused window" },
        CliCommandSpec { name: "toggle-floating", help: "toggle floating mode on focused window" },
        CliCommandSpec { name: "toggle-fullscreen", help: "toggle fullscreen on focused window" },
        CliCommandSpec { name: "reload-config", help: "reload window manager config" },
        CliCommandSpec { name: "focus-next-window", help: "focus next window in stack" },
        CliCommandSpec { name: "focus-previous-window", help: "focus previous window in stack" },
        CliCommandSpec { name: "select-next-workspace", help: "focus next workspace" },
        CliCommandSpec { name: "select-previous-workspace", help: "focus previous workspace" },
        CliCommandSpec { name: "cycle-layout-next", help: "switch to next layout" },
        CliCommandSpec { name: "cycle-layout-previous", help: "switch to previous layout" },
        CliCommandSpec { name: "focus-left", help: "focus window to the left" },
        CliCommandSpec { name: "focus-right", help: "focus window to the right" },
        CliCommandSpec { name: "focus-up", help: "focus window above" },
        CliCommandSpec { name: "focus-down", help: "focus window below" },
        CliCommandSpec { name: "set-layout:<name>", help: "activate a layout by name" },
        CliCommandSpec { name: "select-workspace:<id>", help: "focus a workspace by id" },
        CliCommandSpec { name: "spawn:<command>", help: "spawn an external process" },
    ]
}

pub fn parse_wm_command(token: &str) -> Option<WmCommand> {
    if let Some(value) = token.strip_prefix("spawn:") {
        return Some(WmCommand::Spawn { command: value.to_string() });
    }

    if let Some(value) = token.strip_prefix("set-layout:") {
        return Some(WmCommand::SetLayout { name: value.to_string() });
    }

    if let Some(value) = token.strip_prefix("select-workspace:") {
        return Some(WmCommand::SelectWorkspace { workspace_id: value.into() });
    }

    match token {
        "close-focused-window" => Some(WmCommand::CloseFocusedWindow),
        "toggle-floating" => Some(WmCommand::ToggleFloating),
        "toggle-fullscreen" => Some(WmCommand::ToggleFullscreen),
        "reload-config" => Some(WmCommand::ReloadConfig),
        "focus-next-window" => Some(WmCommand::FocusNextWindow),
        "focus-previous-window" => Some(WmCommand::FocusPreviousWindow),
        "select-next-workspace" => Some(WmCommand::SelectNextWorkspace),
        "select-previous-workspace" => Some(WmCommand::SelectPreviousWorkspace),
        "cycle-layout-next" => {
            Some(WmCommand::CycleLayout { direction: Some(LayoutCycleDirection::Next) })
        }
        "cycle-layout-previous" => {
            Some(WmCommand::CycleLayout { direction: Some(LayoutCycleDirection::Previous) })
        }
        "focus-left" => Some(WmCommand::FocusDirection { direction: FocusDirection::Left }),
        "focus-right" => Some(WmCommand::FocusDirection { direction: FocusDirection::Right }),
        "focus-up" => Some(WmCommand::FocusDirection { direction: FocusDirection::Up }),
        "focus-down" => Some(WmCommand::FocusDirection { direction: FocusDirection::Down }),
        _ => None,
    }
}
