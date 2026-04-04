use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;
use spiders_config::model::{
    Binding, Config, ConfigOptions, InputConfig, LayoutConfigError, LayoutDefinition,
    LayoutSelectionConfig, TitlebarFontConfig, WindowRule,
};
use spiders_core::command::{FocusDirection, WmCommand};

pub fn decode_config_value(path: &Path, value: &Value) -> Result<Config, LayoutConfigError> {
    let root = expect_object(path, value, "root")?;

    Ok(Config {
        workspaces: decode_workspaces(root.get("workspaces"), path)?,
        options: decode_options(root.get("options"), path)?,
        inputs: decode_inputs(root.get("inputs"), path)?,
        layouts: Vec::new(),
        global_stylesheet_path: None,
        layout_selection: decode_layout_selection(root.get("layouts"), path)?,
        rules: decode_rules(root.get("rules"), path)?,
        bindings: decode_bindings(root.get("bindings"), path)?,
        autostart: decode_string_array(root.get("autostart"), path, "root.autostart")?,
        autostart_once: decode_string_array(
            root.get("autostart_once"),
            path,
            "root.autostart_once",
        )?,
    })
}

pub fn validate_layout_selection(
    path: &Path,
    selection: &LayoutSelectionConfig,
    layouts: &[LayoutDefinition],
) -> Result<(), LayoutConfigError> {
    let known = layouts.iter().map(|layout| layout.name.as_str()).collect::<Vec<_>>();
    let is_known = |name: &str| known.iter().any(|known_name| *known_name == name);

    if let Some(default) = &selection.default
        && !is_known(default)
    {
        return Err(LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!(
                "selected layout `{default}` is not defined by discovered layout modules"
            ),
        });
    }

    for layout in selection
        .per_workspace
        .iter()
        .chain(selection.per_monitor.values())
    {
        if !is_known(layout) {
            return Err(LayoutConfigError::DecodeAuthoredConfig {
                path: path.to_path_buf(),
                message: format!(
                    "selected layout `{layout}` is not defined by discovered layout modules"
                ),
            });
        }
    }

    Ok(())
}

fn decode_workspaces(value: Option<&Value>, path: &Path) -> Result<Vec<String>, LayoutConfigError> {
    decode_string_array(value, path, "root.workspaces")
}

fn decode_options(value: Option<&Value>, path: &Path) -> Result<ConfigOptions, LayoutConfigError> {
    let Some(value) = value else {
        return Ok(ConfigOptions::default());
    };
    let object = expect_object(path, value, "root.options")?;
    Ok(ConfigOptions {
        mod_key: None,
        sloppyfocus: decode_optional_bool(
            object.get("sloppyfocus"),
            path,
            "root.options.sloppyfocus",
        )?,
        attach: decode_optional_string(object.get("attach"), path, "root.options.attach")?,
        titlebar_font: decode_titlebar_font(object.get("titlebar_font"), path)?,
    })
}

fn decode_titlebar_font(
    value: Option<&Value>,
    path: &Path,
) -> Result<Option<TitlebarFontConfig>, LayoutConfigError> {
    let Some(value) = value else {
        return Ok(None);
    };

    let object = expect_object(path, value, "root.options.titlebar_font")?;
    Ok(Some(TitlebarFontConfig {
        regular_path: decode_optional_string(
            object.get("regular_path"),
            path,
            "root.options.titlebar_font.regular_path",
        )?,
        bold_path: decode_optional_string(
            object.get("bold_path"),
            path,
            "root.options.titlebar_font.bold_path",
        )?,
    }))
}

fn decode_inputs(value: Option<&Value>, path: &Path) -> Result<Vec<InputConfig>, LayoutConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let object = expect_object(path, value, "root.inputs")?;
    let mut inputs = Vec::new();
    for (name, entry) in object {
        let config = expect_object(path, entry, &format!("root.inputs.{name}"))?;
        inputs.push(InputConfig {
            name: name.clone(),
            xkb_layout: decode_optional_string(config.get("xkb_layout"), path, "input.xkb_layout")?,
            xkb_model: decode_optional_string(config.get("xkb_model"), path, "input.xkb_model")?,
            xkb_variant: decode_optional_string(config.get("xkb_variant"), path, "input.xkb_variant")?,
            xkb_options: decode_optional_string(config.get("xkb_options"), path, "input.xkb_options")?,
            repeat_rate: decode_optional_u32(config.get("repeat_rate"), path, "input.repeat_rate")?,
            repeat_delay: decode_optional_u32(config.get("repeat_delay"), path, "input.repeat_delay")?,
            natural_scroll: decode_optional_bool(config.get("natural_scroll"), path, "input.natural_scroll")?,
            tap: decode_optional_bool(config.get("tap"), path, "input.tap")?,
            drag_lock: decode_optional_bool(config.get("drag_lock"), path, "input.drag_lock")?,
            accel_profile: decode_optional_string(config.get("accel_profile"), path, "input.accel_profile")?,
            pointer_accel: decode_optional_f64(config.get("pointer_accel"), path, "input.pointer_accel")?,
            left_handed: decode_optional_bool(config.get("left_handed"), path, "input.left_handed")?,
            middle_emulation: decode_optional_bool(config.get("middle_emulation"), path, "input.middle_emulation")?,
            dwt: decode_optional_bool(config.get("dwt"), path, "input.dwt")?,
        });
    }
    inputs.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(inputs)
}

fn decode_layout_selection(
    value: Option<&Value>,
    path: &Path,
) -> Result<LayoutSelectionConfig, LayoutConfigError> {
    let Some(value) = value else {
        return Ok(LayoutSelectionConfig::default());
    };
    let object = expect_object(path, value, "root.layouts")?;
    let per_monitor = match object.get("per_monitor") {
        Some(value) => {
            let map = expect_object(path, value, "root.layouts.per_monitor")?;
            let mut out = BTreeMap::new();
            for (name, value) in map {
                out.insert(
                    name.clone(),
                    expect_string(path, value, &format!("root.layouts.per_monitor.{name}"))?
                        .to_owned(),
                );
            }
            out
        }
        None => BTreeMap::new(),
    };

    Ok(LayoutSelectionConfig {
        default: decode_optional_string(object.get("default"), path, "root.layouts.default")?,
        per_workspace: decode_string_array(
            object.get("per_workspace"),
            path,
            "root.layouts.per_workspace",
        )?,
        per_monitor,
    })
}

fn decode_rules(value: Option<&Value>, path: &Path) -> Result<Vec<WindowRule>, LayoutConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let entries = expect_array(path, value, "root.rules")?;
    let mut rules = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let object = expect_object(path, entry, &format!("root.rules[{index}]"))?;
        rules.push(WindowRule {
            app_id: decode_optional_string(object.get("app_id"), path, "rule.app_id")?,
            title: decode_optional_string(object.get("title"), path, "rule.title")?,
            workspaces: decode_rule_workspaces(object.get("workspaces"), path)?,
            floating: decode_optional_bool(object.get("floating"), path, "rule.floating")?,
            fullscreen: decode_optional_bool(object.get("fullscreen"), path, "rule.fullscreen")?,
            monitor: decode_optional_stringish(object.get("monitor"), path, "rule.monitor")?,
        });
    }
    Ok(rules)
}

fn decode_bindings(value: Option<&Value>, path: &Path) -> Result<Vec<Binding>, LayoutConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let object = expect_object(path, value, "root.bindings")?;
    let mod_key = decode_optional_string(object.get("mod"), path, "root.bindings.mod")?;
    let entries = match object.get("entries") {
        Some(value) => expect_array(path, value, "root.bindings.entries")?,
        None => return Ok(Vec::new()),
    };

    let mut bindings = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let object = expect_object(path, entry, &format!("root.bindings.entries[{index}]"))?;
        let bind = expect_array(
            path,
            required(object, "bind", path, "binding.bind")?,
            "binding.bind",
        )?;
        let trigger = bind
            .iter()
            .map(|token| {
                let token = expect_string(path, token, "binding.bind[]")?;
                Ok(if token == "mod" {
                    mod_key.clone().unwrap_or_else(|| "mod".into())
                } else {
                    token.to_owned()
                })
            })
            .collect::<Result<Vec<_>, LayoutConfigError>>()?
            .join("+");
        let command = decode_command_descriptor(
            required(object, "command", path, "binding.command")?,
            path,
            &format!("root.bindings.entries[{index}].command"),
        )?;
        bindings.push(Binding { trigger, command });
    }
    Ok(bindings)
}

fn decode_command_descriptor(
    value: &Value,
    path: &Path,
    field: &str,
) -> Result<WmCommand, LayoutConfigError> {
    let object = expect_object(path, value, field)?;
    let command = expect_string(path, required(object, "_command", path, field)?, field)?;
    let arg = object.get("_arg").unwrap_or(&Value::Null);
    match command {
        "spawn" => Ok(WmCommand::Spawn {
            command: expect_string(path, arg, field)?.to_owned(),
        }),
        "reload_config" => Ok(WmCommand::ReloadConfig),
        "focus_next" => Ok(WmCommand::FocusDirection {
            direction: FocusDirection::Right,
        }),
        "focus_prev" => Ok(WmCommand::FocusDirection {
            direction: FocusDirection::Left,
        }),
        "set_layout" => Ok(WmCommand::SetLayout {
            name: expect_string(path, arg, field)?.to_owned(),
        }),
        "cycle_layout" => Ok(WmCommand::CycleLayout { direction: None }),
        "view_workspace" => Ok(WmCommand::ViewWorkspace {
            workspace: decode_workspace_shortcut(path, arg, field)?,
        }),
        "toggle_view_workspace" => Ok(WmCommand::ToggleViewWorkspace {
            workspace: decode_workspace_shortcut(path, arg, field)?,
        }),
        "focus_mon_left" => Ok(WmCommand::FocusMonitorLeft),
        "focus_mon_right" => Ok(WmCommand::FocusMonitorRight),
        "send_mon_left" => Ok(WmCommand::SendMonitorLeft),
        "send_mon_right" => Ok(WmCommand::SendMonitorRight),
        "toggle_floating" => Ok(WmCommand::ToggleFloating),
        "toggle_fullscreen" => Ok(WmCommand::ToggleFullscreen),
        "focus_dir" => Ok(WmCommand::FocusDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "swap_dir" => Ok(WmCommand::SwapDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "resize_dir" => Ok(WmCommand::ResizeDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "resize_tiled" => Ok(WmCommand::ResizeTiledDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "move" => Ok(WmCommand::MoveDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "resize" => Ok(WmCommand::ResizeDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "assign_workspace" => Ok(WmCommand::AssignFocusedWindowToWorkspace {
            workspace: decode_workspace_shortcut(path, arg, field)?,
        }),
        "toggle_workspace" => Ok(WmCommand::ToggleAssignFocusedWindowToWorkspace {
            workspace: decode_workspace_shortcut(path, arg, field)?,
        }),
        "kill_client" => Ok(WmCommand::CloseFocusedWindow),
        other => Err(LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("unsupported command descriptor `{other}` at {field}"),
        }),
    }
}

fn decode_focus_direction(
    path: &Path,
    value: &Value,
    field: &str,
) -> Result<FocusDirection, LayoutConfigError> {
    match expect_string(path, value, field)? {
        "left" => Ok(FocusDirection::Left),
        "right" => Ok(FocusDirection::Right),
        "up" => Ok(FocusDirection::Up),
        "down" => Ok(FocusDirection::Down),
        other => Err(LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("unsupported focus direction `{other}` at {field}"),
        }),
    }
}

fn decode_rule_workspaces(
    value: Option<&Value>,
    path: &Path,
) -> Result<Vec<String>, LayoutConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| decode_workspace_string(path, value, "rule.workspaces"))
            .collect(),
        _ => Ok(vec![decode_workspace_string(path, value, "rule.workspaces")?]),
    }
}

fn decode_workspace_string(
    path: &Path,
    value: &Value,
    field: &str,
) -> Result<String, LayoutConfigError> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        _ => Err(LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("expected string or number at {field}"),
        }),
    }
}

fn decode_workspace_shortcut(
    path: &Path,
    value: &Value,
    field: &str,
) -> Result<u8, LayoutConfigError> {
    match value {
        Value::Number(value) => {
            let Some(number) = value.as_u64() else {
                return Err(LayoutConfigError::DecodeAuthoredConfig {
                    path: path.to_path_buf(),
                    message: format!("expected integer workspace shortcut at {field}"),
                });
            };

            if (1..=9).contains(&number) {
                Ok(number as u8)
            } else {
                Err(LayoutConfigError::DecodeAuthoredConfig {
                    path: path.to_path_buf(),
                    message: format!("workspace shortcut must be between 1 and 9 at {field}"),
                })
            }
        }
        _ => Err(LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("expected numeric workspace shortcut at {field}"),
        }),
    }
}

fn decode_optional_string(
    value: Option<&Value>,
    path: &Path,
    field: &str,
) -> Result<Option<String>, LayoutConfigError> {
    value
        .map(|value| expect_string(path, value, field).map(str::to_owned))
        .transpose()
}

fn decode_optional_stringish(
    value: Option<&Value>,
    path: &Path,
    field: &str,
) -> Result<Option<String>, LayoutConfigError> {
    match value {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Number(value)) => Ok(Some(value.to_string())),
        Some(_) => Err(LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("expected string or number at {field}"),
        }),
        None => Ok(None),
    }
}

fn decode_optional_bool(
    value: Option<&Value>,
    path: &Path,
    field: &str,
) -> Result<Option<bool>, LayoutConfigError> {
    value.map(|value| expect_bool(path, value, field)).transpose()
}

fn decode_optional_u32(
    value: Option<&Value>,
    path: &Path,
    field: &str,
) -> Result<Option<u32>, LayoutConfigError> {
    value.map(|value| expect_u32(path, value, field)).transpose()
}

fn decode_optional_f64(
    value: Option<&Value>,
    path: &Path,
    field: &str,
) -> Result<Option<f64>, LayoutConfigError> {
    value.map(|value| expect_f64(path, value, field)).transpose()
}

fn decode_string_array(
    value: Option<&Value>,
    path: &Path,
    field: &str,
) -> Result<Vec<String>, LayoutConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = expect_array(path, value, field)?;
    items
        .iter()
        .map(|value| expect_string(path, value, field).map(str::to_owned))
        .collect()
}

fn expect_object<'a>(
    path: &Path,
    value: &'a Value,
    field: &str,
) -> Result<&'a serde_json::Map<String, Value>, LayoutConfigError> {
    value
        .as_object()
        .ok_or_else(|| LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("expected object at {field}"),
        })
}

fn expect_array<'a>(
    path: &Path,
    value: &'a Value,
    field: &str,
) -> Result<&'a Vec<Value>, LayoutConfigError> {
    value
        .as_array()
        .ok_or_else(|| LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("expected array at {field}"),
        })
}

fn expect_string<'a>(
    path: &Path,
    value: &'a Value,
    field: &str,
) -> Result<&'a str, LayoutConfigError> {
    value
        .as_str()
        .ok_or_else(|| LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("expected string at {field}"),
        })
}

fn expect_bool(path: &Path, value: &Value, field: &str) -> Result<bool, LayoutConfigError> {
    value
        .as_bool()
        .ok_or_else(|| LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("expected boolean at {field}"),
        })
}

fn expect_u32(path: &Path, value: &Value, field: &str) -> Result<u32, LayoutConfigError> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("expected unsigned integer at {field}"),
        })
}

fn expect_f64(path: &Path, value: &Value, field: &str) -> Result<f64, LayoutConfigError> {
    value
        .as_f64()
        .ok_or_else(|| LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("expected number at {field}"),
        })
}

fn required<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
    path: &Path,
    field: &str,
) -> Result<&'a Value, LayoutConfigError> {
    object
        .get(key)
        .ok_or_else(|| LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("missing required field `{key}` at {field}"),
        })
}
