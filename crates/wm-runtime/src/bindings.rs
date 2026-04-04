use std::sync::OnceLock;

use regex::Regex;
use spiders_core::command::FocusDirection;
use spiders_core::command::WmCommand;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedBindingsState {
    pub source: String,
    pub mod_key: String,
    pub entries: Vec<ParsedBindingEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedBindingEntry {
    pub bind: Vec<String>,
    pub chord: String,
    pub command: Option<WmCommand>,
    pub command_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingKeyEvent {
    pub key: String,
    pub alt: bool,
    pub ctrl: bool,
    pub meta: bool,
    pub shift: bool,
}

pub fn parse_bindings_source(source: &str) -> ParsedBindingsState {
    let mod_key = mod_pattern()
        .captures(source)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| "super".to_string());

    let entries = parse_binding_entries(source, &mod_key);

    ParsedBindingsState { source: source.to_string(), mod_key, entries }
}

pub fn format_binding_token(token: &str, mod_key: &str) -> String {
    let resolved = if token == "mod" { mod_key } else { token };

    match resolved {
        "alt" | "mod1" => "Alt".to_string(),
        "super" | "logo" | "mod4" => "Super".to_string(),
        "ctrl" | "control" => "Ctrl".to_string(),
        "shift" => "Shift".to_string(),
        "space" => "Space".to_string(),
        "Return" => "Enter".to_string(),
        _ if resolved.len() == 1 => resolved.to_uppercase(),
        _ => resolved.to_string(),
    }
}

pub fn normalize_key_input(code: &str, key: &str) -> Option<String> {
    if let Some(value) = code.strip_prefix("Key") {
        return Some(value.to_lowercase());
    }

    if let Some(value) = code.strip_prefix("Digit") {
        return Some(value.to_lowercase());
    }

    match code {
        "Enter" => Some("return".to_string()),
        "Space" => Some("space".to_string()),
        "Comma" => Some("comma".to_string()),
        "Period" => Some("period".to_string()),
        "ArrowLeft" => Some("left".to_string()),
        "ArrowRight" => Some("right".to_string()),
        "ArrowUp" => Some("up".to_string()),
        "ArrowDown" => Some("down".to_string()),
        _ => (!key.is_empty()).then(|| key.to_lowercase()),
    }
}

pub fn matches_binding_key_event(
    entry: &ParsedBindingEntry,
    event: &BindingKeyEvent,
    mod_key: &str,
) -> bool {
    let Some(key_token) = entry.bind.last() else {
        return false;
    };

    let expected = expected_modifiers(&entry.bind[..entry.bind.len().saturating_sub(1)], mod_key);

    normalize_binding_key(key_token) == event.key
        && event.alt == expected.alt
        && event.ctrl == expected.ctrl
        && event.meta == expected.meta
        && event.shift == expected.shift
}

fn parse_binding_entries(source: &str, mod_key: &str) -> Vec<ParsedBindingEntry> {
    let Some(entries_source) = entries_array_source(source) else {
        return Vec::new();
    };

    split_top_level_entries(entries_source)
        .into_iter()
        .filter_map(|entry_source| parse_binding_entry(entry_source, mod_key))
        .collect()
}

fn entries_array_source(source: &str) -> Option<&str> {
    let entries_start = source.find("entries")?;
    let array_start = source[entries_start..].find('[')? + entries_start;
    let array_end = find_matching_delimiter(source, array_start, '[', ']')?;

    source.get(array_start + 1..array_end)
}

fn split_top_level_entries(source: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut depth = 0usize;
    let mut entry_start = None;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in source.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        if in_string {
            match character {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match character {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    entry_start = Some(index);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }

                depth -= 1;
                if depth == 0 {
                    if let Some(start) = entry_start.take() {
                        if let Some(entry) = source.get(start..=index) {
                            entries.push(entry);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    entries
}

fn parse_binding_entry(entry_source: &str, mod_key: &str) -> Option<ParsedBindingEntry> {
    let bind_source = bind_pattern().captures(entry_source)?.get(1)?.as_str();
    let bind = token_pattern()
        .captures_iter(bind_source)
        .filter_map(|token| token.get(1))
        .map(|token| token.as_str().to_string())
        .collect::<Vec<_>>();
    let (command_name, command_arg_source) = parse_command_invocation(entry_source)?;
    let command = parse_wm_command(&command_name, command_arg_source.trim());

    Some(ParsedBindingEntry {
        chord: bind
            .iter()
            .map(|token| format_binding_token(token, mod_key))
            .collect::<Vec<_>>()
            .join(" + "),
        bind,
        command_label: format_binding_command(&command_name, command_arg_source),
        command,
    })
}

fn parse_command_invocation(entry_source: &str) -> Option<(String, &str)> {
    let command_start = entry_source.find("commands.")? + "commands.".len();
    let after_prefix = entry_source.get(command_start..)?;
    let open_paren_offset = after_prefix.find('(')?;
    let command_name = after_prefix.get(..open_paren_offset)?.trim().to_string();
    let open_paren_index = command_start + open_paren_offset;
    let close_paren_index = find_matching_delimiter(entry_source, open_paren_index, '(', ')')?;
    let command_arg_source = entry_source.get(open_paren_index + 1..close_paren_index)?;

    Some((command_name, command_arg_source))
}

fn find_matching_delimiter(
    source: &str,
    open_index: usize,
    open_delimiter: char,
    close_delimiter: char,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (relative_index, character) in source.get(open_index..)?.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        if in_string {
            match character {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match character {
            '"' => in_string = true,
            value if value == open_delimiter => depth += 1,
            value if value == close_delimiter => {
                if depth == 0 {
                    return None;
                }

                depth -= 1;
                if depth == 0 {
                    return Some(open_index + relative_index);
                }
            }
            _ => {}
        }
    }

    None
}

fn format_binding_command(command_name: &str, arg_source: &str) -> String {
    let arg_source = arg_source.trim();
    if arg_source.is_empty() {
        command_name.to_string()
    } else {
        format!("{command_name}({})", display_arg_source(arg_source))
    }
}

fn display_arg_source(source: &str) -> String {
    strip_string_quotes(source).unwrap_or(source).to_string()
}

fn parse_wm_command(command_name: &str, arg_source: &str) -> Option<WmCommand> {
    match command_name {
        "spawn" => parse_string_arg(arg_source)
            .map(|command| WmCommand::Spawn { command: command.to_string() }),
        "quit" => Some(WmCommand::Quit),
        "reload_config" => Some(WmCommand::ReloadConfig),
        "focus_next" => Some(WmCommand::FocusNextWindow),
        "focus_prev" => Some(WmCommand::FocusPreviousWindow),
        "focus_dir" => parse_direction_arg(arg_source)
            .map(|direction| WmCommand::FocusDirection { direction }),
        "swap_dir" => parse_direction_arg(arg_source)
            .map(|direction| WmCommand::SwapDirection { direction }),
        "resize_dir" | "resize" => parse_direction_arg(arg_source)
            .map(|direction| WmCommand::ResizeDirection { direction }),
        "resize_tiled" => parse_direction_arg(arg_source)
            .map(|direction| WmCommand::ResizeTiledDirection { direction }),
        "focus_mon_left" => Some(WmCommand::FocusMonitorLeft),
        "focus_mon_right" => Some(WmCommand::FocusMonitorRight),
        "send_mon_left" => Some(WmCommand::SendMonitorLeft),
        "send_mon_right" => Some(WmCommand::SendMonitorRight),
        "view_workspace" => parse_workspace_arg(arg_source)
            .map(|workspace| WmCommand::ViewWorkspace { workspace }),
        "toggle_view_workspace" => parse_workspace_arg(arg_source)
            .map(|workspace| WmCommand::ToggleViewWorkspace { workspace }),
        "assign_workspace" => parse_workspace_arg(arg_source)
            .map(|workspace| WmCommand::AssignFocusedWindowToWorkspace { workspace }),
        "toggle_workspace" => parse_workspace_arg(arg_source)
            .map(|workspace| WmCommand::ToggleAssignFocusedWindowToWorkspace { workspace }),
        "toggle_floating" => Some(WmCommand::ToggleFloating),
        "toggle_fullscreen" => Some(WmCommand::ToggleFullscreen),
        "set_layout" => parse_string_arg(arg_source)
            .map(|name| WmCommand::SetLayout { name: name.to_string() }),
        "cycle_layout" => Some(WmCommand::CycleLayout { direction: None }),
        "move" => parse_direction_arg(arg_source)
            .map(|direction| WmCommand::MoveDirection { direction }),
        "kill_client" => Some(WmCommand::CloseFocusedWindow),
        _ => None,
    }
}

fn parse_string_arg(source: &str) -> Option<&str> {
    let source = source.trim();
    if source.is_empty() {
        return None;
    }

    strip_string_quotes(source).or(Some(source))
}

fn strip_string_quotes(source: &str) -> Option<&str> {
    source.strip_prefix('"').and_then(|value| value.strip_suffix('"'))
}

fn parse_workspace_arg(source: &str) -> Option<u8> {
    source.trim().parse::<u8>().ok().filter(|workspace| *workspace > 0)
}

fn parse_direction_arg(source: &str) -> Option<FocusDirection> {
    match parse_string_arg(source)? {
        "left" => Some(FocusDirection::Left),
        "right" => Some(FocusDirection::Right),
        "up" => Some(FocusDirection::Up),
        "down" => Some(FocusDirection::Down),
        _ => None,
    }
}

fn expected_modifiers(bind: &[String], mod_key: &str) -> ExpectedModifiers {
    let mut expected = ExpectedModifiers::default();

    for token in bind {
        match resolve_modifier_token(token, mod_key) {
            Some("alt") => expected.alt = true,
            Some("ctrl") => expected.ctrl = true,
            Some("meta") => expected.meta = true,
            Some("shift") => expected.shift = true,
            _ => {}
        }
    }

    expected
}

fn resolve_modifier_token<'a>(token: &'a str, mod_key: &'a str) -> Option<&'static str> {
    let resolved = if token == "mod" { mod_key } else { token };

    match resolved {
        "alt" | "mod1" => Some("alt"),
        "ctrl" | "control" => Some("ctrl"),
        "super" | "logo" | "mod4" => Some("meta"),
        "shift" => Some("shift"),
        _ => None,
    }
}

fn normalize_binding_key(token: &str) -> String {
    match token {
        "Return" => "return".to_string(),
        "space" => "space".to_string(),
        "comma" => "comma".to_string(),
        "period" => "period".to_string(),
        _ => token.to_lowercase(),
    }
}

fn bind_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r#"bind:\s*\[([^\]]*?)\]"#).expect("valid bind regex"))
}

fn token_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r#"\"([^\"]+)\""#).expect("valid token regex"))
}

fn mod_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r#"\bmod:\s*\"([^\"]+)\""#).expect("valid mod regex"))
}

#[derive(Default)]
struct ExpectedModifiers {
    alt: bool,
    ctrl: bool,
    meta: bool,
    shift: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiline_resize_tiled_entries_without_consuming_neighbors() {
        let source =
            include_str!("../../../apps/spiders-wm-www/fixtures/spiders-wm/config/bindings.ts");
        let parsed = parse_bindings_source(source);

        let resize_left = parsed
            .entries
            .iter()
            .find(|entry| entry.chord == "Alt + Ctrl + Shift + H")
            .expect("resize_tiled left binding should exist");
        let kill_client = parsed
            .entries
            .iter()
            .find(|entry| entry.chord == "Alt + Q")
            .expect("kill_client binding should exist");

        assert_eq!(
            resize_left.command,
            Some(WmCommand::ResizeTiledDirection {
                direction: FocusDirection::Left,
            })
        );
        assert_eq!(resize_left.command_label, "resize_tiled(left)");
        assert_eq!(kill_client.command, Some(WmCommand::CloseFocusedWindow));
        assert_eq!(kill_client.command_label, "kill_client");
    }

    #[test]
    fn matches_normalized_key_input_against_binding_entry() {
        let entry = ParsedBindingEntry {
            bind: vec!["mod".to_string(), "shift".to_string(), "Return".to_string()],
            chord: "Super + Shift + Enter".to_string(),
            command: Some(WmCommand::Quit),
            command_label: "noop".to_string(),
        };

        let event = BindingKeyEvent {
            key: normalize_key_input("Enter", "Enter").expect("normalized enter"),
            alt: false,
            ctrl: false,
            meta: true,
            shift: true,
        };

        assert!(matches_binding_key_event(&entry, &event, "super"));
    }
}
