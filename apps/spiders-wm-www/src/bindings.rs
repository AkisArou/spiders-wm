use std::sync::OnceLock;

use regex::Regex;

use crate::session::{PreviewSessionCommand, PreviewSessionCommandArg};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBindingsState {
    pub source: String,
    pub mod_key: String,
    pub entries: Vec<ParsedBindingEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBindingEntry {
    pub bind: Vec<String>,
    pub chord: String,
    pub command: PreviewSessionCommand,
    pub command_label: String,
}

pub fn parse_bindings_source(source: &str) -> ParsedBindingsState {
    let mod_key = mod_pattern()
        .captures(source)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| "super".to_string());

    let entries = parse_binding_entries(source, &mod_key);

    ParsedBindingsState {
        source: source.to_string(),
        mod_key,
        entries,
    }
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
    let command_arg = parse_command_arg(command_arg_source.trim());
    let command = PreviewSessionCommand {
        name: command_name.clone(),
        arg: command_arg.clone(),
    };

    Some(ParsedBindingEntry {
        chord: bind
            .iter()
            .map(|token| format_binding_token(token, mod_key))
            .collect::<Vec<_>>()
            .join(" + "),
        bind,
        command_label: format_binding_command(&command_name, command_arg.as_ref()),
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

#[cfg(target_arch = "wasm32")]
pub fn matches_web_keyboard_event(
    entry: &ParsedBindingEntry,
    event: &web_sys::KeyboardEvent,
    mod_key: &str,
) -> bool {
    let Some(key_token) = entry.bind.last() else {
        return false;
    };

    let expected = expected_modifiers(&entry.bind[..entry.bind.len().saturating_sub(1)], mod_key);
    let Some(actual_key) = normalize_keyboard_event_key(event) else {
        return false;
    };

    normalize_binding_key(key_token) == actual_key
        && event.alt_key() == expected.alt
        && event.ctrl_key() == expected.ctrl
        && event.meta_key() == expected.meta
        && event.shift_key() == expected.shift
}

fn format_binding_command(command_name: &str, arg: Option<&PreviewSessionCommandArg>) -> String {
    match arg {
        Some(arg) => format!("{command_name}({})", command_arg_to_string(arg)),
        None => command_name.to_string(),
    }
}

fn parse_command_arg(source: &str) -> Option<PreviewSessionCommandArg> {
    if source.is_empty() {
        return None;
    }

    if let Some(stripped) = source
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return Some(PreviewSessionCommandArg::String(stripped.to_string()));
    }

    if let Ok(number) = source.parse::<i32>() {
        return Some(PreviewSessionCommandArg::Number(number));
    }

    Some(PreviewSessionCommandArg::String(source.to_string()))
}

fn command_arg_to_string(arg: &PreviewSessionCommandArg) -> String {
    match arg {
        PreviewSessionCommandArg::String(value) => value.clone(),
        PreviewSessionCommandArg::Number(value) => value.to_string(),
    }
}

#[cfg(target_arch = "wasm32")]
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

#[cfg(target_arch = "wasm32")]
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

#[cfg(target_arch = "wasm32")]
fn normalize_keyboard_event_key(event: &web_sys::KeyboardEvent) -> Option<String> {
    let code = event.code();

    if let Some(key) = code.strip_prefix("Key") {
        return Some(key.to_lowercase());
    }

    if let Some(key) = code.strip_prefix("Digit") {
        return Some(key.to_lowercase());
    }

    match code.as_str() {
        "Enter" => Some("return".to_string()),
        "Space" => Some("space".to_string()),
        "Comma" => Some("comma".to_string()),
        "Period" => Some("period".to_string()),
        "ArrowLeft" => Some("left".to_string()),
        "ArrowRight" => Some("right".to_string()),
        "ArrowUp" => Some("up".to_string()),
        "ArrowDown" => Some("down".to_string()),
        _ => {
            let key = event.key();
            (!key.is_empty()).then(|| key.to_lowercase())
        }
    }
}

#[cfg(target_arch = "wasm32")]
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

#[cfg(target_arch = "wasm32")]
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
        let source = include_str!("../../spiders-wm-playground/src/spiders-wm/config/bindings.ts");
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

        assert_eq!(resize_left.command.name, "resize_tiled");
        assert_eq!(resize_left.command_label, "resize_tiled(left)");
        assert_eq!(kill_client.command.name, "kill_client");
        assert_eq!(kill_client.command_label, "kill_client");
    }
}
