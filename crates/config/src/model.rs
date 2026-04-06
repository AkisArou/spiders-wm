use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use spiders_core::command::WmCommand;
use spiders_core::runtime::layout_context::LayoutWindowContext;
use spiders_core::runtime::prepared_layout::{PreparedLayout, SelectedLayout};
use spiders_core::snapshot::{OutputSnapshot, StateSnapshot, WorkspaceSnapshot};
use spiders_core::types::LayoutRef;
use spiders_core::{LayoutSpace, ResolvedLayoutNode};
use spiders_scene::SceneRequest;
use spiders_titlebar_core::{
    ResolvedTitlebarContext, resolve_titlebar_tree, select_titlebar_rule,
    titlebar_tree_to_runtime_node,
};
use spiders_titlebar_core::{TitlebarRule, decode_titlebar_rules};
use thiserror::Error;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TitlebarFontConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regular_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sloppyfocus: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attach: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub titlebar_font: Option<TitlebarFontConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InputConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xkb_layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xkb_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xkb_variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xkb_options: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_rate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_delay: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural_scroll: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tap: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drag_lock: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accel_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointer_accel: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_handed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle_emulation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dwt: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutDefinition {
    pub name: String,
    pub directory: String,
    pub module: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stylesheet_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_cache_payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LayoutSelectionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_workspace: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub per_monitor: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspaces: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floating: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fullscreen: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub trigger: String,
    pub command: WmCommand,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub workspaces: Vec<String>,
    #[serde(default)]
    pub options: ConfigOptions,
    #[serde(default)]
    pub inputs: Vec<InputConfig>,
    #[serde(default)]
    pub layouts: Vec<LayoutDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_stylesheet_path: Option<String>,
    #[serde(default)]
    pub layout_selection: LayoutSelectionConfig,
    #[serde(default)]
    pub rules: Vec<WindowRule>,
    #[serde(default)]
    pub titlebars: Vec<serde_json::Value>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub autostart: Vec<String>,
    #[serde(default)]
    pub autostart_once: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LayoutConfigError {
    #[error("layout `{name}` is not defined in config")]
    UnknownLayout { name: String },
    #[error("prepared artifact layout mismatch: expected `{expected}`, got `{actual}`")]
    ArtifactLayoutMismatch { expected: String, actual: String },
    #[error("config file `{path}` could not be read")]
    ReadConfig { path: PathBuf },
    #[error("config file `{path}` is invalid")]
    ParseConfig { path: PathBuf },
    #[error("authored config `{path}` could not be compiled: {message}")]
    CompileAuthoredConfig { path: PathBuf, message: String },
    #[error("authored config `{path}` could not be evaluated: {message}")]
    EvaluateAuthoredConfig { path: PathBuf, message: String },
    #[error("authored config `{path}` could not be decoded: {message}")]
    DecodeAuthoredConfig { path: PathBuf, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPaths {
    pub authored_config: PathBuf,
    pub prepared_config: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigDiscoveryOptions {
    pub home_dir: Option<PathBuf>,
    pub config_dir_override: Option<PathBuf>,
    pub cache_dir_override: Option<PathBuf>,
    pub authored_config_override: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    JavaScript,
    Lua,
    Rust,
}

pub fn config_discovery_options_from_env() -> ConfigDiscoveryOptions {
    ConfigDiscoveryOptions {
        home_dir: std::env::var_os("SPIDERS_WM_HOME")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from),
        config_dir_override: std::env::var_os("SPIDERS_WM_CONFIG_DIR").map(PathBuf::from),
        cache_dir_override: std::env::var_os("SPIDERS_WM_CACHE_DIR").map(PathBuf::from),
        authored_config_override: std::env::var_os("SPIDERS_WM_AUTHORED_CONFIG").map(PathBuf::from),
    }
}

impl ConfigPaths {
    pub fn new(authored_config: impl Into<PathBuf>, prepared_config: impl Into<PathBuf>) -> Self {
        Self { authored_config: authored_config.into(), prepared_config: prepared_config.into() }
    }

    pub fn discover(options: ConfigDiscoveryOptions) -> Result<Self, LayoutConfigError> {
        let ConfigDiscoveryOptions {
            home_dir,
            config_dir_override,
            cache_dir_override,
            authored_config_override,
        } = options;

        let config_root =
            match config_dir_override {
                Some(path) => path,
                None => home_dir.as_ref().map(|path| path.join(".config/spiders-wm")).ok_or_else(
                    || LayoutConfigError::ReadConfig {
                        path: PathBuf::from(
                            "config discovery requires a home_dir or config_dir_override",
                        ),
                    },
                )?,
            };
        let cache_root =
            match cache_dir_override {
                Some(path) => path,
                None => home_dir.as_ref().map(|path| path.join(".cache/spiders-wm")).ok_or_else(
                    || LayoutConfigError::ReadConfig {
                        path: PathBuf::from(
                            "config discovery requires a home_dir or cache_dir_override",
                        ),
                    },
                )?,
            };

        let authored_config = authored_config_override.unwrap_or_else(|| {
            ["config.tsx", "config.ts", "config.jsx", "config.js"]
                .into_iter()
                .map(|name| config_root.join(name))
                .find(|candidate| candidate.exists())
                .unwrap_or_else(|| config_root.join("config.ts"))
        });
        let prepared_config = cache_root.join("config.js");

        Ok(Self { authored_config, prepared_config })
    }

    pub fn runtime_kind(&self) -> Option<RuntimeKind> {
        runtime_kind_for_path(&self.authored_config)
    }
}

pub fn runtime_kind_for_path(path: &Path) -> Option<RuntimeKind> {
    let file_name = path.file_name()?.to_str()?;
    if file_name.ends_with(".ts")
        || file_name.ends_with(".tsx")
        || file_name.ends_with(".js")
        || file_name.ends_with(".jsx")
        || file_name.ends_with(".mjs")
        || file_name.ends_with(".cjs")
    {
        return Some(RuntimeKind::JavaScript);
    }
    if file_name.ends_with(".lua") {
        return Some(RuntimeKind::Lua);
    }
    if file_name.ends_with(".rs") {
        return Some(RuntimeKind::Rust);
    }
    None
}

impl Config {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, LayoutConfigError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|_| LayoutConfigError::ReadConfig { path: path.to_path_buf() })?;

        serde_json::from_str(&text)
            .map_err(|_| LayoutConfigError::ParseConfig { path: path.to_path_buf() })
    }

    pub fn layout_by_name(&self, name: &str) -> Option<&LayoutDefinition> {
        self.layouts.iter().find(|layout| layout.name == name)
    }

    pub fn decode_titlebar_rules(&self) -> Result<Vec<TitlebarRule>, serde_json::Error> {
        decode_titlebar_rules(&JsonValue::Array(self.titlebars.clone()))
    }

    pub fn resolve_titlebar_runtime_node(
        &self,
        window: &LayoutWindowContext,
        workspace_name: Option<&str>,
        slot_name: Option<&str>,
    ) -> Result<Option<ResolvedLayoutNode>, serde_json::Error> {
        let rules = self.decode_titlebar_rules()?;
        if rules.is_empty() {
            return Ok(None);
        }

        let mut context = ResolvedTitlebarContext::from(window);
        context.workspace = workspace_name.map(str::to_string);
        context.slot = slot_name.map(str::to_string);

        let Some(rule) = select_titlebar_rule(&rules, &context) else {
            return Ok(None);
        };
        if rule.disabled {
            return Ok(None);
        }

        Ok(Some(titlebar_tree_to_runtime_node(&resolve_titlebar_tree(rule, &context))))
    }

    pub fn selected_layout<'a>(
        &'a self,
        workspace: &'a WorkspaceSnapshot,
    ) -> Option<&'a LayoutDefinition> {
        workspace.effective_layout.as_ref().and_then(|layout| self.layout_by_name(&layout.name))
    }

    pub fn resolve_selected_layout(
        &self,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, LayoutConfigError> {
        self.selected_layout(workspace)
            .map(|layout| {
                Ok(SelectedLayout {
                    name: layout.name.clone(),
                    directory: layout.directory.clone(),
                    module: layout.module.clone(),
                })
            })
            .or_else(|| {
                workspace.effective_layout.as_ref().map(|layout| {
                    Err(LayoutConfigError::UnknownLayout { name: layout.name.clone() })
                })
            })
            .transpose()
    }

    pub fn build_scene_request(
        &self,
        workspace: &WorkspaceSnapshot,
        output: Option<&OutputSnapshot>,
        root: ResolvedLayoutNode,
        artifact: &PreparedLayout,
    ) -> Result<SceneRequest, LayoutConfigError> {
        let selected_layout = self.resolve_selected_layout(workspace)?;

        if let Some(selected_layout) = selected_layout.as_ref() {
            if selected_layout.name != artifact.selected.name {
                return Err(LayoutConfigError::ArtifactLayoutMismatch {
                    expected: selected_layout.name.clone(),
                    actual: artifact.selected.name.clone(),
                });
            }
        }

        Ok(SceneRequest {
            workspace_id: workspace.id.clone(),
            output_id: output.map(|output| output.id.clone()),
            layout_name: workspace.effective_layout.as_ref().map(|layout| layout.name.clone()),
            root,
            stylesheets: artifact.stylesheets.clone(),
            space: LayoutSpace {
                width: output.map(|output| output.logical_width as f32).unwrap_or_default(),
                height: output.map(|output| output.logical_height as f32).unwrap_or_default(),
            },
        })
    }

    pub fn build_scene_request_from_state(
        &self,
        state: &StateSnapshot,
        root: ResolvedLayoutNode,
        artifact: &PreparedLayout,
    ) -> Result<Option<SceneRequest>, LayoutConfigError> {
        let Some(workspace) = state.current_workspace() else {
            return Ok(None);
        };
        let output =
            workspace.output_id.as_ref().and_then(|output_id| state.output_by_id(output_id));

        self.build_scene_request(workspace, output, root, artifact).map(Some)
    }
}

impl From<&LayoutDefinition> for LayoutRef {
    fn from(value: &LayoutDefinition) -> Self {
        Self { name: value.name.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_core::runtime::layout_context::LayoutWindowContext;
    use spiders_core::runtime::prepared_layout::{PreparedLayout, PreparedStylesheets};
    use spiders_core::snapshot::OutputSnapshot;
    use spiders_core::types::{LayoutRef, OutputTransform};
    use spiders_core::{OutputId, WorkspaceId};
    use std::fs;

    fn workspace(layout_name: &str) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_workspaces: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef { name: layout_name.into() }),
        }
    }

    fn output() -> OutputSnapshot {
        OutputSnapshot {
            id: OutputId::from("out-1"),
            name: "HDMI-A-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 1920,
            logical_height: 1080,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
        }
    }

    fn artifact(layout_name: &str, module: &str) -> PreparedLayout {
        PreparedLayout {
            selected: SelectedLayout {
                name: layout_name.into(),
                directory: "layouts/master-stack".into(),
                module: module.into(),
            },
            runtime_payload: serde_json::json!({
                "entry": module,
                "modules": [],
            }),
            stylesheets: PreparedStylesheets::default(),
        }
    }

    fn layout_window_context(app_id: &str, title: &str) -> LayoutWindowContext {
        LayoutWindowContext {
            id: spiders_core::WindowId::from("win-1"),
            app_id: Some(app_id.into()),
            title: Some(title.into()),
            class: None,
            instance: None,
            role: None,
            shell: Some("xdg-toplevel".into()),
            window_type: None,
            floating: false,
            fullscreen: false,
            focused: true,
        }
    }

    #[test]
    fn selects_layout_definition_by_workspace_layout_ref() {
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_cache_payload: None,
            }],
            ..Config::default()
        };
        let workspace = workspace("master-stack");

        let selected = config.selected_layout(&workspace).unwrap();

        assert_eq!(selected.module, "layouts/master-stack.js");
    }

    #[test]
    fn builds_scene_request_from_config_and_workspace_state() {
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_cache_payload: None,
            }],
            ..Config::default()
        };

        let request = config
            .build_scene_request(
                &workspace("master-stack"),
                Some(&output()),
                ResolvedLayoutNode::Workspace { meta: Default::default(), children: vec![] },
                &artifact("master-stack", "layouts/master-stack.js"),
            )
            .unwrap();

        assert_eq!(request.layout_name.as_deref(), Some("master-stack"));
        assert_eq!(request.space.width, 1920.0);
        assert_eq!(request.space.height, 1080.0);
    }

    #[test]
    fn resolves_selected_layout_into_shared_payload() {
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_cache_payload: None,
            }],
            ..Config::default()
        };

        let selected = config.resolve_selected_layout(&workspace("master-stack")).unwrap();

        assert_eq!(
            selected,
            Some(SelectedLayout {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
            })
        );
    }

    #[test]
    fn builds_scene_request_from_state_snapshot() {
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_cache_payload: None,
            }],
            ..Config::default()
        };
        let state = StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![output()],
            workspaces: vec![workspace("master-stack")],
            windows: vec![],
            visible_window_ids: vec![],
            workspace_names: vec!["1".into()],
        };

        let request = config
            .build_scene_request_from_state(
                &state,
                ResolvedLayoutNode::Workspace { meta: Default::default(), children: vec![] },
                &artifact("master-stack", "layouts/master-stack.js"),
            )
            .unwrap()
            .unwrap();

        assert_eq!(request.layout_name.as_deref(), Some("master-stack"));
        assert_eq!(request.space.width, 1920.0);
        assert_eq!(request.space.height, 1080.0);
    }

    #[test]
    fn missing_layout_definition_returns_error() {
        let config = Config::default();
        let error = config
            .build_scene_request(
                &workspace("missing"),
                Some(&output()),
                ResolvedLayoutNode::Workspace { meta: Default::default(), children: vec![] },
                &artifact("missing", "layouts/missing.js"),
            )
            .unwrap_err();

        assert_eq!(error, LayoutConfigError::UnknownLayout { name: "missing".into() });
    }

    #[test]
    fn scene_request_rejects_mismatched_prepared_artifact() {
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_cache_payload: None,
            }],
            ..Config::default()
        };

        let error = config
            .build_scene_request(
                &workspace("master-stack"),
                Some(&output()),
                ResolvedLayoutNode::Workspace { meta: Default::default(), children: vec![] },
                &artifact("secondary", "layouts/secondary.js"),
            )
            .unwrap_err();

        assert_eq!(
            error,
            LayoutConfigError::ArtifactLayoutMismatch {
                expected: "master-stack".into(),
                actual: "secondary".into(),
            }
        );
    }

    #[test]
    fn loads_config_from_json_path() {
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("spiders-config-test.json");
        fs::write(
            &config_path,
            r#"{"layouts":[{"name":"master-stack","directory":"layouts/master-stack","module":"layouts/master-stack.js","stylesheet_path":"layouts/master-stack/index.css"}]}"#,
        )
        .unwrap();

        let config = Config::from_path(&config_path).unwrap();

        assert_eq!(config.layouts[0].name, "master-stack");

        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn rejects_legacy_runtime_payload_field_in_layout_definition() {
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("spiders-config-legacy-runtime-payload-test.json");
        fs::write(
            &config_path,
            r#"{"layouts":[{"name":"master-stack","directory":"layouts/master-stack","module":"layouts/master-stack.js","runtime_payload":{"entry":"layouts/master-stack.js","modules":[]}}]}"#,
        )
        .unwrap();

        let error = Config::from_path(&config_path).unwrap_err();

        assert_eq!(error, LayoutConfigError::ParseConfig { path: config_path.clone() });

        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn resolve_titlebar_runtime_node_selects_and_resolves_matching_rule() {
        let config = Config {
            titlebars: vec![serde_json::json!({
                "type": "titlebar",
                "props": { "class": "default-titlebar" },
                "children": [
                    {
                        "type": "titlebar.text",
                        "props": { "class": "label" },
                        "children": ["DEV"]
                    }
                ]
            })],
            ..Config::default()
        };

        let runtime = config
            .resolve_titlebar_runtime_node(
                &layout_window_context("firefox", "Mozilla Firefox"),
                Some("code"),
                Some("main"),
            )
            .expect("titlebar rules should decode")
            .expect("a titlebar should be resolved");

        match runtime {
            ResolvedLayoutNode::Content { meta, children, .. } => {
                assert_eq!(meta.name.as_deref(), Some("titlebar"));
                assert_eq!(meta.class, vec!["default-titlebar"]);
                assert_eq!(children.len(), 1);
            }
            other => panic!("expected content node, got {other:?}"),
        }
    }

    #[test]
    fn resolve_titlebar_runtime_node_returns_none_for_disabled_match() {
        let config = Config {
            titlebars: vec![
                serde_json::json!({
                    "type": "titlebar",
                    "props": { "class": "default-titlebar" },
                    "children": []
                }),
                serde_json::json!({
                    "type": "titlebar",
                    "props": {
                        "when": { "appId": "foot" },
                        "disabled": true
                    },
                    "children": []
                }),
            ],
            ..Config::default()
        };

        let runtime = config
            .resolve_titlebar_runtime_node(
                &layout_window_context("foot", "Terminal"),
                Some("code"),
                None,
            )
            .expect("titlebar rules should decode");

        assert!(runtime.is_none());
    }

    #[test]
    fn discovers_default_config_paths_from_home_dir() {
        let temp_dir = std::env::temp_dir();
        let home_dir = temp_dir.join("spiders-config-discovery-home");
        let config_dir = home_dir.join(".config/spiders-wm");
        let data_dir = home_dir.join(".cache/spiders-wm");
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::create_dir_all(&data_dir);
        fs::write(config_dir.join("config.tsx"), "export default {};").unwrap();

        let paths = ConfigPaths::discover(ConfigDiscoveryOptions {
            home_dir: Some(home_dir.clone()),
            ..ConfigDiscoveryOptions::default()
        })
        .unwrap();

        assert!(paths.authored_config.ends_with(".config/spiders-wm/config.tsx"));
        assert!(paths.prepared_config.ends_with(".cache/spiders-wm/config.js"));

        let _ = fs::remove_file(config_dir.join("config.tsx"));
    }

    #[test]
    fn discovery_prefers_override_directories() {
        let temp_dir = std::env::temp_dir();
        let config_dir = temp_dir.join("spiders-config-override-config");
        let cache_dir = temp_dir.join("spiders-config-override-cache");
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::create_dir_all(&cache_dir);
        fs::write(config_dir.join("config.js"), "module.exports = {};").unwrap();

        let paths = ConfigPaths::discover(ConfigDiscoveryOptions {
            home_dir: Some(temp_dir.clone()),
            config_dir_override: Some(config_dir.clone()),
            cache_dir_override: Some(cache_dir.clone()),
            authored_config_override: None,
        })
        .unwrap();

        assert_eq!(paths.authored_config, config_dir.join("config.js"));
        assert_eq!(paths.prepared_config, cache_dir.join("config.js"));

        let _ = fs::remove_file(config_dir.join("config.js"));
    }

    #[test]
    fn config_paths_new_supports_direct_file_overrides() {
        let temp_dir = std::env::temp_dir();
        let authored = temp_dir.join("spiders-direct-authored.js");
        let runtime = temp_dir.join("spiders-direct-runtime.js");

        let paths = ConfigPaths::new(authored.clone(), runtime.clone());

        assert_eq!(paths.authored_config, authored);
        assert_eq!(paths.prepared_config, runtime);
    }

    #[test]
    fn runtime_kind_detects_javascript_lua_and_rust_configs() {
        assert_eq!(
            runtime_kind_for_path(Path::new("/tmp/config.ts")),
            Some(RuntimeKind::JavaScript)
        );
        assert_eq!(
            runtime_kind_for_path(Path::new("/tmp/config.jsx")),
            Some(RuntimeKind::JavaScript)
        );
        assert_eq!(runtime_kind_for_path(Path::new("/tmp/config.lua")), Some(RuntimeKind::Lua));
        assert_eq!(runtime_kind_for_path(Path::new("/tmp/config.rs")), Some(RuntimeKind::Rust));
        assert_eq!(runtime_kind_for_path(Path::new("/tmp/config.txt")), None);
    }

    #[test]
    fn config_paths_runtime_kind_uses_authored_config_path() {
        let paths = ConfigPaths::new("/tmp/config.ts", "/tmp/config.js");

        assert_eq!(paths.runtime_kind(), Some(RuntimeKind::JavaScript));
    }
}
