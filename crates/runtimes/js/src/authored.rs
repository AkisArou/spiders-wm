use std::collections::BTreeMap;
use std::path::Path;

use boa_engine::{Context as JsContext, Source};
use serde_json::Value;
use spiders_shared::api::{FocusDirection, WmAction};

use crate::compile::{bundle_app, compile_app, AppBuildPlan};
use crate::graph::{discover_project_apps, ModuleGraphBuilder};
use spiders_config::model::{
    Binding, Config, ConfigOptions, InputConfig, LayoutConfigError, LayoutDefinition,
    LayoutSelectionConfig, OutputConfig, WindowRule,
};

pub fn load_authored_config(path: impl AsRef<Path>) -> Result<Config, LayoutConfigError> {
    let path = path.as_ref();
    let project =
        discover_project_apps(path).map_err(|error| LayoutConfigError::CompileAuthoredConfig {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;

    let config_graph = ModuleGraphBuilder::new()
        .build(&project.config_app)
        .map_err(|error| LayoutConfigError::CompileAuthoredConfig {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    let config_plan = AppBuildPlan::from_graph(&config_graph);
    let compiled_config =
        compile_app(&config_plan).map_err(|error| LayoutConfigError::CompileAuthoredConfig {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    let bundled_config = bundle_app(&config_graph, &compiled_config).map_err(|error| {
        LayoutConfigError::CompileAuthoredConfig {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    })?;

    let config_value = evaluate_bundled_config(path, &bundled_config.javascript)?;
    let mut config = decode_config_value(path, &config_value)?;

    let global_stylesheet = project
        .global_stylesheet_path
        .as_ref()
        .map(std::fs::read_to_string)
        .transpose()
        .map_err(|_| LayoutConfigError::ReadConfig {
            path: project.config_app.root_dir.join("index.css"),
        })?
        .unwrap_or_default();

    let mut layout_defs = Vec::new();
    for app in &project.layout_apps {
        let graph = ModuleGraphBuilder::new().build(app).map_err(|error| {
            LayoutConfigError::CompileAuthoredConfig {
                path: app.entry_path.clone(),
                message: error.to_string(),
            }
        })?;
        let plan = AppBuildPlan::from_graph(&graph);
        let compiled =
            compile_app(&plan).map_err(|error| LayoutConfigError::CompileAuthoredConfig {
                path: app.entry_path.clone(),
                message: error.to_string(),
            })?;
        let bundled = bundle_app(&graph, &compiled).map_err(|error| {
            LayoutConfigError::CompileAuthoredConfig {
                path: app.entry_path.clone(),
                message: error.to_string(),
            }
        })?;

        layout_defs.push(LayoutDefinition {
            name: app.name.clone(),
            module: layout_runtime_module_path(&app.name),
            stylesheet: bundled.stylesheet,
            effects_stylesheet: global_stylesheet.clone(),
            runtime_source: Some(bundled.javascript),
        });
    }

    config.layouts = layout_defs;
    Ok(config)
}

fn evaluate_bundled_config(path: &Path, source: &str) -> Result<Value, LayoutConfigError> {
    let mut js = JsContext::default();
    let value = js.eval(Source::from_bytes(source)).map_err(|error| {
        LayoutConfigError::EvaluateAuthoredConfig {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    })?;

    value
        .to_json(&mut js)
        .map_err(|error| LayoutConfigError::EvaluateAuthoredConfig {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?
        .ok_or_else(|| LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: "config app returned undefined".into(),
        })
}

fn decode_config_value(path: &Path, value: &Value) -> Result<Config, LayoutConfigError> {
    let root = expect_object(path, value, "root")?;

    Ok(Config {
        tags: decode_tags(root.get("tags"), path)?,
        options: decode_options(root.get("options"), path)?,
        inputs: decode_inputs(root.get("inputs"), path)?,
        outputs: decode_outputs(root.get("outputs"), path)?,
        layouts: Vec::new(),
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

fn decode_tags(value: Option<&Value>, path: &Path) -> Result<Vec<String>, LayoutConfigError> {
    decode_string_array(value, path, "root.tags")
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
    })
}

fn decode_inputs(
    value: Option<&Value>,
    path: &Path,
) -> Result<Vec<InputConfig>, LayoutConfigError> {
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
            xkb_variant: decode_optional_string(
                config.get("xkb_variant"),
                path,
                "input.xkb_variant",
            )?,
            xkb_options: decode_optional_string(
                config.get("xkb_options"),
                path,
                "input.xkb_options",
            )?,
            repeat_rate: decode_optional_u32(config.get("repeat_rate"), path, "input.repeat_rate")?,
            repeat_delay: decode_optional_u32(
                config.get("repeat_delay"),
                path,
                "input.repeat_delay",
            )?,
            natural_scroll: decode_optional_bool(
                config.get("natural_scroll"),
                path,
                "input.natural_scroll",
            )?,
            tap: decode_optional_bool(config.get("tap"), path, "input.tap")?,
            drag_lock: decode_optional_bool(config.get("drag_lock"), path, "input.drag_lock")?,
            accel_profile: decode_optional_string(
                config.get("accel_profile"),
                path,
                "input.accel_profile",
            )?,
            pointer_accel: decode_optional_f64(
                config.get("pointer_accel"),
                path,
                "input.pointer_accel",
            )?,
            left_handed: decode_optional_bool(
                config.get("left_handed"),
                path,
                "input.left_handed",
            )?,
            middle_emulation: decode_optional_bool(
                config.get("middle_emulation"),
                path,
                "input.middle_emulation",
            )?,
            dwt: decode_optional_bool(config.get("dwt"), path, "input.dwt")?,
        });
    }
    inputs.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(inputs)
}

fn decode_outputs(
    value: Option<&Value>,
    path: &Path,
) -> Result<Vec<OutputConfig>, LayoutConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let object = expect_object(path, value, "root.outputs")?;
    let mut outputs = Vec::new();
    for (name, entry) in object {
        let config = expect_object(path, entry, &format!("root.outputs.{name}"))?;
        outputs.push(OutputConfig {
            name: name.clone(),
            mode: decode_optional_string(config.get("mode"), path, "output.mode")?,
            scale: decode_optional_f64(config.get("scale"), path, "output.scale")?,
            transform: decode_optional_string(config.get("transform"), path, "output.transform")?,
            position: decode_optional_string(config.get("position"), path, "output.position")?,
            adaptive_sync: decode_optional_bool(
                config.get("adaptive_sync"),
                path,
                "output.adaptive_sync",
            )?,
            enabled: decode_optional_bool(config.get("enabled"), path, "output.enabled")?,
        });
    }
    outputs.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(outputs)
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
        per_tag: decode_string_array(object.get("per_tag"), path, "root.layouts.per_tag")?,
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
            tags: decode_rule_tags(object.get("tags"), path)?,
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
        let action = decode_action_descriptor(
            required(object, "action", path, "binding.action")?,
            path,
            &format!("root.bindings.entries[{index}].action"),
        )?;
        bindings.push(Binding { trigger, action });
    }
    Ok(bindings)
}

fn decode_action_descriptor(
    value: &Value,
    path: &Path,
    field: &str,
) -> Result<WmAction, LayoutConfigError> {
    let object = expect_object(path, value, field)?;
    let action = expect_string(path, required(object, "_action", path, field)?, field)?;
    let arg = object.get("_arg").unwrap_or(&Value::Null);
    match action {
        "spawn" => Ok(WmAction::Spawn {
            command: expect_string(path, arg, field)?.to_owned(),
        }),
        "reload_config" => Ok(WmAction::ReloadConfig),
        "focus_next" => Ok(WmAction::FocusDirection {
            direction: FocusDirection::Right,
        }),
        "focus_prev" => Ok(WmAction::FocusDirection {
            direction: FocusDirection::Left,
        }),
        "set_layout" => Ok(WmAction::SetLayout {
            name: expect_string(path, arg, field)?.to_owned(),
        }),
        "cycle_layout" => Ok(WmAction::CycleLayout { direction: None }),
        "view_tag" => Ok(WmAction::ViewTag {
            tag: decode_tag_string(path, arg, field)?,
        }),
        "toggle_view_tag" => Ok(WmAction::ToggleViewTag {
            tag: decode_tag_string(path, arg, field)?,
        }),
        "focus_mon_left" => Ok(WmAction::FocusMonitorLeft),
        "focus_mon_right" => Ok(WmAction::FocusMonitorRight),
        "send_mon_left" => Ok(WmAction::SendMonitorLeft),
        "send_mon_right" => Ok(WmAction::SendMonitorRight),
        "toggle_floating" => Ok(WmAction::ToggleFloating),
        "toggle_fullscreen" => Ok(WmAction::ToggleFullscreen),
        "focus_dir" => Ok(WmAction::FocusDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "swap_dir" => Ok(WmAction::SwapDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "resize_dir" => Ok(WmAction::ResizeDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "resize_tiled" => Ok(WmAction::ResizeTiledDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "move" => Ok(WmAction::MoveDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "resize" => Ok(WmAction::ResizeDirection {
            direction: decode_focus_direction(path, arg, field)?,
        }),
        "tag" => Ok(WmAction::TagFocusedWindow {
            tag: decode_tag_string(path, arg, field)?,
        }),
        "toggle_tag" => Ok(WmAction::ToggleTagFocusedWindow {
            tag: decode_tag_string(path, arg, field)?,
        }),
        "kill_client" => Ok(WmAction::CloseFocusedWindow),
        other => Err(LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("unsupported action descriptor `{other}` at {field}"),
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

fn decode_rule_tags(value: Option<&Value>, path: &Path) -> Result<Vec<String>, LayoutConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| decode_tag_string(path, value, "rule.tags"))
            .collect(),
        _ => Ok(vec![decode_tag_string(path, value, "rule.tags")?]),
    }
}

fn decode_tag_string(path: &Path, value: &Value, field: &str) -> Result<String, LayoutConfigError> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        _ => Err(LayoutConfigError::DecodeAuthoredConfig {
            path: path.to_path_buf(),
            message: format!("expected string or number at {field}"),
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
    value
        .map(|value| expect_bool(path, value, field))
        .transpose()
}

fn decode_optional_u32(
    value: Option<&Value>,
    path: &Path,
    field: &str,
) -> Result<Option<u32>, LayoutConfigError> {
    value
        .map(|value| expect_u32(path, value, field))
        .transpose()
}

fn decode_optional_f64(
    value: Option<&Value>,
    path: &Path,
    field: &str,
) -> Result<Option<f64>, LayoutConfigError> {
    value
        .map(|value| expect_f64(path, value, field))
        .transpose()
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

fn layout_runtime_module_path(name: &str) -> String {
    format!("layouts/{name}.bundle.js")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    fn unique_root(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("spiders-config-authored-{name}-{unique}"))
    }

    #[test]
    fn loads_authored_config_and_bundled_layouts() {
        let root = unique_root("project");
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("layouts/master-stack")).unwrap();
        fs::write(root.join("index.css"), "window { appearance: none; }").unwrap();
        fs::write(
            root.join("config.ts"),
            r#"
                import type { SpiderWMConfig } from "spider-wm/config";
                import { bindings } from "./config/bindings";
                import { inputs } from "./config/inputs";
                import { layouts } from "./config/layouts";

                export default {
                  tags: ["1", "2"],
                  options: { sloppyfocus: true },
                  bindings,
                  inputs,
                  layouts,
                } satisfies SpiderWMConfig;
            "#,
        )
        .unwrap();
        fs::write(
            root.join("config/bindings.ts"),
            r#"
                import * as actions from "spider-wm/actions";
                export const bindings = {
                  mod: "alt",
                  entries: [
                    { bind: ["mod", "Return"], action: actions.spawn("foot") },
                    { bind: ["mod", "h"], action: actions.focus_dir("left") },
                    { bind: ["mod", "shift", "h"], action: actions.swap_dir("left") },
                    { bind: ["mod", "ctrl", "h"], action: actions.resize_dir("left") },
                    { bind: ["mod", "ctrl", "shift", "h"], action: actions.resize_tiled("left") },
                    { bind: ["mod", "comma"], action: actions.focus_mon_left() },
                    { bind: ["mod", "period"], action: actions.focus_mon_right() },
                    { bind: ["mod", "f"], action: actions.toggle_fullscreen() },
                  ],
                };
            "#,
        )
        .unwrap();
        fs::write(
            root.join("config/inputs.ts"),
            r#"
                export const inputs = {
                  "type:keyboard": { xkb_layout: "us", repeat_delay: 250 },
                };
            "#,
        )
        .unwrap();
        fs::write(
            root.join("config/layouts.ts"),
            r#"
                export const layouts = {
                  default: "master-stack",
                  per_tag: ["master-stack", "master-stack"],
                  per_monitor: { "eDP-1": "master-stack" },
                };
            "#,
        )
        .unwrap();
        fs::write(
            root.join("layouts/master-stack/index.tsx"),
            r#"
                import "./index.css";
                export default function layout() {
                  return { type: "workspace", children: [] };
                }
            "#,
        )
        .unwrap();
        fs::write(root.join("layouts/master-stack/index.css"), ".master {}").unwrap();

        let config = load_authored_config(root.join("config.ts")).unwrap();

        assert_eq!(config.tags, vec!["1", "2"]);
        assert_eq!(config.options.sloppyfocus, Some(true));
        assert_eq!(config.bindings.len(), 8);
        assert_eq!(config.bindings[0].trigger, "alt+Return");
        assert_eq!(
            config.bindings[0].action,
            WmAction::Spawn {
                command: "foot".into(),
            }
        );
        assert_eq!(
            config.bindings[1].action,
            WmAction::FocusDirection {
                direction: FocusDirection::Left,
            }
        );
        assert_eq!(
            config.bindings[2].action,
            WmAction::SwapDirection {
                direction: FocusDirection::Left,
            }
        );
        assert_eq!(
            config.bindings[3].action,
            WmAction::ResizeDirection {
                direction: FocusDirection::Left,
            }
        );
        assert_eq!(
            config.bindings[4].action,
            WmAction::ResizeTiledDirection {
                direction: FocusDirection::Left,
            }
        );
        assert_eq!(config.bindings[5].action, WmAction::FocusMonitorLeft);
        assert_eq!(config.bindings[6].action, WmAction::FocusMonitorRight);
        assert_eq!(config.bindings[7].action, WmAction::ToggleFullscreen);
        assert_eq!(config.inputs.len(), 1);
        assert_eq!(
            config.layout_selection.default.as_deref(),
            Some("master-stack")
        );
        assert_eq!(config.layouts.len(), 1);
        assert_eq!(config.layouts[0].module, "layouts/master-stack.bundle.js");
        assert!(config.layouts[0]
            .runtime_source
            .as_ref()
            .unwrap()
            .contains("__require"));
        assert!(config.layouts[0].stylesheet.contains(".master {}"));
        assert!(config.layouts[0]
            .effects_stylesheet
            .contains("window { appearance: none; }"));
    }
}
