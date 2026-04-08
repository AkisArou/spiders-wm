use spiders_core::command::WmCommand;

use crate::metadata::{CliDumpKind, CliQuery, CliShell, CliTopic};

#[derive(Debug, Clone, PartialEq)]
pub enum CliTopLevelCommand {
    Config(CliConfigCommand),
    Wm(CliWmCommand),
    Completions { shell: CliShell },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CliConfigCommand {
    Discover,
    Check,
    Build,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CliWmCommand {
    Query { query: CliQuery },
    Command { command: WmCommand },
    Monitor { topics: Vec<CliTopic> },
    DebugDump { kind: CliDumpKind },
    Smoke,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CliCommand {
    pub output_json: bool,
    pub socket_path: Option<String>,
    pub command: CliTopLevelCommand,
}
