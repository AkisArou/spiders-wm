mod command;
mod completion;
mod completions;
mod metadata;
mod parse;

pub use command::{CliCommand, CliConfigCommand, CliTopLevelCommand, CliWmCommand};
pub use completion::{CompletionCandidate, complete_tokens};
pub use completions::render_completion_script;
pub use metadata::{
    CliCommandSpec, CliDumpKind, CliQuery, CliShell, CliTopic, parse_wm_command, wm_command_specs,
};
pub use parse::{CliParseError, parse_cli_tokens};
