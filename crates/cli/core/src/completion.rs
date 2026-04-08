use crate::metadata::{CliDumpKind, CliQuery, CliTopic, wm_command_specs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub value: String,
    pub help: &'static str,
}

pub fn complete_tokens(tokens: &[&str]) -> Vec<CompletionCandidate> {
    match tokens {
        ["wm", "debug", "dump"] => dump_candidates(),
        ["wm", "debug", "dump", prefix] => filter_candidates(dump_candidates(), prefix),
        ["wm", "debug"] => {
            vec![CompletionCandidate { value: "dump".to_string(), help: "write a debug dump" }]
        }
        ["wm", "debug", prefix] => filter_candidates(
            vec![CompletionCandidate { value: "dump".to_string(), help: "write a debug dump" }],
            prefix,
        ),
        ["wm", "monitor"] => topic_candidates(),
        ["wm", "monitor", prefix] => filter_candidates(topic_candidates(), prefix),
        ["wm", "command"] => command_candidates(),
        ["wm", "command", prefix] => filter_candidates(command_candidates(), prefix),
        ["wm", "query"] => query_candidates(),
        ["wm", "query", prefix] => filter_candidates(query_candidates(), prefix),
        ["completions"] => shell_candidates(),
        ["completions", prefix] => filter_candidates(shell_candidates(), prefix),
        ["wm"] => wm_group_candidates(),
        ["wm", prefix] => filter_candidates(wm_group_candidates(), prefix),
        ["config"] => config_candidates(),
        ["config", prefix] => filter_candidates(config_candidates(), prefix),
        [] => top_level_candidates(),
        [prefix] => filter_candidates(top_level_candidates(), prefix),
        _ => Vec::new(),
    }
}

fn top_level_candidates() -> Vec<CompletionCandidate> {
    vec![
        CompletionCandidate { value: "config".to_string(), help: "config management commands" },
        CompletionCandidate { value: "wm".to_string(), help: "window manager commands" },
        CompletionCandidate {
            value: "completions".to_string(),
            help: "shell completion script generation",
        },
    ]
}

fn config_candidates() -> Vec<CompletionCandidate> {
    vec![
        CompletionCandidate { value: "discover".to_string(), help: "show discovered config paths" },
        CompletionCandidate { value: "check".to_string(), help: "validate config and layouts" },
        CompletionCandidate { value: "build".to_string(), help: "write prepared runtime config" },
    ]
}

fn wm_group_candidates() -> Vec<CompletionCandidate> {
    vec![
        CompletionCandidate { value: "query".to_string(), help: "query compositor state" },
        CompletionCandidate { value: "command".to_string(), help: "send a WM command" },
        CompletionCandidate { value: "monitor".to_string(), help: "subscribe to IPC events" },
        CompletionCandidate { value: "debug".to_string(), help: "debug IPC helpers" },
        CompletionCandidate { value: "smoke".to_string(), help: "run IPC smoke exercise" },
    ]
}

fn query_candidates() -> Vec<CompletionCandidate> {
    CliQuery::ALL
        .into_iter()
        .map(|query| CompletionCandidate { value: query.name().to_string(), help: query.help() })
        .collect()
}

fn command_candidates() -> Vec<CompletionCandidate> {
    wm_command_specs()
        .iter()
        .map(|spec| CompletionCandidate { value: spec.name.to_string(), help: spec.help })
        .collect()
}

fn topic_candidates() -> Vec<CompletionCandidate> {
    CliTopic::ALL
        .into_iter()
        .map(|topic| CompletionCandidate { value: topic.name().to_string(), help: topic.help() })
        .collect()
}

fn dump_candidates() -> Vec<CompletionCandidate> {
    CliDumpKind::ALL
        .into_iter()
        .map(|kind| CompletionCandidate { value: kind.name().to_string(), help: kind.help() })
        .collect()
}

fn shell_candidates() -> Vec<CompletionCandidate> {
    vec![
        CompletionCandidate { value: "zsh".to_string(), help: "generate zsh completion" },
        CompletionCandidate { value: "bash".to_string(), help: "generate bash completion" },
        CompletionCandidate { value: "fish".to_string(), help: "generate fish completion" },
    ]
}

fn filter_candidates(
    candidates: Vec<CompletionCandidate>,
    prefix: &str,
) -> Vec<CompletionCandidate> {
    candidates.into_iter().filter(|candidate| candidate.value.starts_with(prefix)).collect()
}
