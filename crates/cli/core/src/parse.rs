use crate::command::{CliCommand, CliConfigCommand, CliTopLevelCommand, CliWmCommand};
use crate::metadata::{CliDumpKind, CliQuery, CliShell, CliTopic, parse_wm_command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliParseError {
    MissingCommand,
    MissingArgument { expected: &'static str },
    UnknownCommand { token: String },
    UnknownSubcommand { command: &'static str, token: String },
    UnknownValue { flag: &'static str, value: String },
    UnsupportedArgument { token: String },
}

pub fn parse_cli_tokens(tokens: &[&str]) -> Result<CliCommand, CliParseError> {
    if tokens.is_empty() {
        return Err(CliParseError::MissingCommand);
    }

    let mut output_json = false;
    let mut socket_path = None;
    let mut positionals = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        match tokens[index] {
            "--json" => {
                output_json = true;
                index += 1;
            }
            "--socket" => {
                let Some(value) = tokens.get(index + 1) else {
                    return Err(CliParseError::MissingArgument { expected: "socket path" });
                };
                socket_path = Some((*value).to_string());
                index += 2;
            }
            token if token.starts_with('-') => {
                return Err(CliParseError::UnsupportedArgument { token: token.to_string() });
            }
            token => {
                positionals.push(token);
                index += 1;
            }
        }
    }

    let Some(command) = positionals.first().copied() else {
        return Err(CliParseError::MissingCommand);
    };

    let command = match command {
        "config" => parse_config_command(&positionals[1..])?,
        "wm" => parse_wm_command_group(&positionals[1..])?,
        "completions" => parse_completions_command(&positionals[1..])?,
        token => return Err(CliParseError::UnknownCommand { token: token.to_string() }),
    };

    Ok(CliCommand { output_json, socket_path, command })
}

fn parse_config_command(tokens: &[&str]) -> Result<CliTopLevelCommand, CliParseError> {
    let Some(token) = tokens.first().copied() else {
        return Err(CliParseError::MissingArgument { expected: "config subcommand" });
    };

    let command = match token {
        "discover" => CliConfigCommand::Discover,
        "check" => CliConfigCommand::Check,
        "build" => CliConfigCommand::Build,
        value => {
            return Err(CliParseError::UnknownSubcommand {
                command: "config",
                token: value.to_string(),
            });
        }
    };

    Ok(CliTopLevelCommand::Config(command))
}

fn parse_wm_command_group(tokens: &[&str]) -> Result<CliTopLevelCommand, CliParseError> {
    let Some(token) = tokens.first().copied() else {
        return Err(CliParseError::MissingArgument { expected: "wm subcommand" });
    };

    let command = match token {
        "query" => {
            let Some(value) = tokens.get(1).copied() else {
                return Err(CliParseError::MissingArgument { expected: "query name" });
            };
            let Some(query) = CliQuery::parse(value) else {
                return Err(CliParseError::UnknownValue {
                    flag: "query",
                    value: value.to_string(),
                });
            };
            CliWmCommand::Query { query }
        }
        "command" => {
            let Some(value) = tokens.get(1).copied() else {
                return Err(CliParseError::MissingArgument { expected: "command name" });
            };
            let Some(command) = parse_wm_command(value) else {
                return Err(CliParseError::UnknownValue {
                    flag: "command",
                    value: value.to_string(),
                });
            };
            CliWmCommand::Command { command }
        }
        "monitor" => {
            let mut topics = Vec::new();
            for token in &tokens[1..] {
                let Some(topic) = CliTopic::parse(token) else {
                    return Err(CliParseError::UnknownValue {
                        flag: "topic",
                        value: (*token).to_string(),
                    });
                };
                topics.push(topic);
            }
            CliWmCommand::Monitor { topics }
        }
        "debug" => {
            let Some(debug_token) = tokens.get(1).copied() else {
                return Err(CliParseError::MissingArgument { expected: "debug subcommand" });
            };
            if debug_token != "dump" {
                return Err(CliParseError::UnknownSubcommand {
                    command: "wm debug",
                    token: debug_token.to_string(),
                });
            }
            let Some(value) = tokens.get(2).copied() else {
                return Err(CliParseError::MissingArgument { expected: "dump kind" });
            };
            let Some(kind) = CliDumpKind::parse(value) else {
                return Err(CliParseError::UnknownValue { flag: "dump", value: value.to_string() });
            };
            CliWmCommand::DebugDump { kind }
        }
        "smoke" => CliWmCommand::Smoke,
        value => {
            return Err(CliParseError::UnknownSubcommand {
                command: "wm",
                token: value.to_string(),
            });
        }
    };

    Ok(CliTopLevelCommand::Wm(command))
}

fn parse_completions_command(tokens: &[&str]) -> Result<CliTopLevelCommand, CliParseError> {
    let Some(value) = tokens.first().copied() else {
        return Err(CliParseError::MissingArgument { expected: "shell" });
    };

    let shell = match value {
        "zsh" => CliShell::Zsh,
        "bash" => CliShell::Bash,
        "fish" => CliShell::Fish,
        _ => {
            return Err(CliParseError::UnknownValue { flag: "shell", value: value.to_string() });
        }
    };

    Ok(CliTopLevelCommand::Completions { shell })
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_core::command::{LayoutCycleDirection, WmCommand};

    #[test]
    fn parses_wm_query_command_tree() {
        let parsed = parse_cli_tokens(&["wm", "query", "state"]).unwrap();
        assert!(matches!(
            parsed.command,
            CliTopLevelCommand::Wm(CliWmCommand::Query { query: CliQuery::State })
        ));
    }

    #[test]
    fn parses_wm_command_with_socket_and_json() {
        let parsed = parse_cli_tokens(&[
            "--json",
            "--socket",
            "/tmp/spiders.sock",
            "wm",
            "command",
            "cycle-layout-next",
        ])
        .unwrap();

        assert!(parsed.output_json);
        assert_eq!(parsed.socket_path.as_deref(), Some("/tmp/spiders.sock"));
        assert!(matches!(
            parsed.command,
            CliTopLevelCommand::Wm(CliWmCommand::Command {
                command: WmCommand::CycleLayout { direction: Some(LayoutCycleDirection::Next) }
            })
        ));
    }
}
