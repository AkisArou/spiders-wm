use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use spiders_shared::api::WmAction;
use spiders_shared::layout::LayoutRequest;
use spiders_shared::layout::{LayoutSpace, ResolvedLayoutNode};
use spiders_shared::wm::{
    LayoutRef, OutputSnapshot, SelectedLayout, StateSnapshot, WorkspaceSnapshot,
};
use thiserror::Error;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InputConfig {
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputConfig {
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayoutDefinition {
    pub name: String,
    pub module: String,
    #[serde(default)]
    pub stylesheet: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floating: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fullscreen: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    pub trigger: String,
    pub action: WmAction,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub options: ConfigOptions,
    #[serde(default)]
    pub inputs: Vec<InputConfig>,
    #[serde(default)]
    pub outputs: Vec<OutputConfig>,
    #[serde(default)]
    pub layouts: Vec<LayoutDefinition>,
    #[serde(default)]
    pub rules: Vec<WindowRule>,
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
    #[error("config file `{path}` could not be read")]
    ReadConfig { path: PathBuf },
    #[error("config file `{path}` is invalid")]
    ParseConfig { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPaths {
    pub authored_config: PathBuf,
    pub runtime_config: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigDiscoveryOptions {
    pub home_dir: Option<PathBuf>,
    pub config_dir_override: Option<PathBuf>,
    pub data_dir_override: Option<PathBuf>,
    pub authored_config_override: Option<PathBuf>,
    pub runtime_config_override: Option<PathBuf>,
}

impl ConfigPaths {
    pub fn new(authored_config: impl Into<PathBuf>, runtime_config: impl Into<PathBuf>) -> Self {
        Self {
            authored_config: authored_config.into(),
            runtime_config: runtime_config.into(),
        }
    }

    pub fn discover(options: ConfigDiscoveryOptions) -> Result<Self, LayoutConfigError> {
        let ConfigDiscoveryOptions {
            home_dir,
            config_dir_override,
            data_dir_override,
            authored_config_override,
            runtime_config_override,
        } = options;

        if let (Some(authored_config), Some(runtime_config)) = (
            authored_config_override.clone(),
            runtime_config_override.clone(),
        ) {
            return Ok(Self::new(authored_config, runtime_config));
        }

        let home_dir = home_dir
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .ok_or_else(|| LayoutConfigError::ReadConfig {
                path: PathBuf::from("$HOME"),
            })?;

        let config_root =
            config_dir_override.unwrap_or_else(|| home_dir.join(".config/spiders-wm"));
        let data_root =
            data_dir_override.unwrap_or_else(|| home_dir.join(".local/share/spiders-wm"));

        let authored_config = authored_config_override.unwrap_or_else(|| {
            if config_root.join("config.ts").exists() {
                config_root.join("config.ts")
            } else {
                config_root.join("config.js")
            }
        });
        let runtime_config =
            runtime_config_override.unwrap_or_else(|| data_root.join("config.json"));

        Ok(Self {
            authored_config,
            runtime_config,
        })
    }
}

impl Config {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, LayoutConfigError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|_| LayoutConfigError::ReadConfig {
            path: path.to_path_buf(),
        })?;

        serde_json::from_str(&text).map_err(|_| LayoutConfigError::ParseConfig {
            path: path.to_path_buf(),
        })
    }

    pub fn layout_by_name(&self, name: &str) -> Option<&LayoutDefinition> {
        self.layouts.iter().find(|layout| layout.name == name)
    }

    pub fn selected_layout<'a>(
        &'a self,
        workspace: &'a WorkspaceSnapshot,
    ) -> Option<&'a LayoutDefinition> {
        workspace
            .effective_layout
            .as_ref()
            .and_then(|layout| self.layout_by_name(&layout.name))
    }

    pub fn resolve_selected_layout(
        &self,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, LayoutConfigError> {
        self.selected_layout(workspace)
            .map(|layout| {
                Ok(SelectedLayout {
                    name: layout.name.clone(),
                    module: layout.module.clone(),
                    stylesheet: layout.stylesheet.clone(),
                })
            })
            .or_else(|| {
                workspace.effective_layout.as_ref().map(|layout| {
                    Err(LayoutConfigError::UnknownLayout {
                        name: layout.name.clone(),
                    })
                })
            })
            .transpose()
    }

    pub fn build_layout_request(
        &self,
        workspace: &WorkspaceSnapshot,
        output: Option<&OutputSnapshot>,
        root: ResolvedLayoutNode,
    ) -> Result<LayoutRequest, LayoutConfigError> {
        let selected_layout = self.resolve_selected_layout(workspace)?;

        Ok(LayoutRequest {
            workspace_id: workspace.id.clone(),
            output_id: output.map(|output| output.id.clone()),
            layout_name: workspace
                .effective_layout
                .as_ref()
                .map(|layout| layout.name.clone()),
            root,
            stylesheet: selected_layout
                .map(|layout| layout.stylesheet)
                .unwrap_or_default(),
            space: LayoutSpace {
                width: output
                    .map(|output| output.logical_width as f32)
                    .unwrap_or_default(),
                height: output
                    .map(|output| output.logical_height as f32)
                    .unwrap_or_default(),
            },
        })
    }

    pub fn build_layout_request_from_state(
        &self,
        state: &StateSnapshot,
        root: ResolvedLayoutNode,
    ) -> Result<Option<LayoutRequest>, LayoutConfigError> {
        let Some(workspace) = state.current_workspace() else {
            return Ok(None);
        };
        let output = workspace
            .output_id
            .as_ref()
            .and_then(|output_id| state.output_by_id(output_id));

        self.build_layout_request(workspace, output, root).map(Some)
    }
}

impl From<&LayoutDefinition> for LayoutRef {
    fn from(value: &LayoutDefinition) -> Self {
        Self {
            name: value.name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_shared::ids::{OutputId, WorkspaceId};
    use spiders_shared::wm::OutputTransform;
    use std::fs;

    fn workspace(layout_name: &str) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_tags: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef {
                name: layout_name.into(),
            }),
        }
    }

    fn output() -> OutputSnapshot {
        OutputSnapshot {
            id: OutputId::from("out-1"),
            name: "HDMI-A-1".into(),
            logical_width: 1920,
            logical_height: 1080,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
        }
    }

    #[test]
    fn selects_layout_definition_by_workspace_layout_ref() {
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
            }],
            ..Config::default()
        };
        let workspace = workspace("master-stack");

        let selected = config.selected_layout(&workspace).unwrap();

        assert_eq!(selected.module, "layouts/master-stack.js");
    }

    #[test]
    fn builds_layout_request_from_config_and_workspace_state() {
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
            }],
            ..Config::default()
        };

        let request = config
            .build_layout_request(
                &workspace("master-stack"),
                Some(&output()),
                ResolvedLayoutNode::Workspace {
                    meta: Default::default(),
                    children: vec![],
                },
            )
            .unwrap();

        assert_eq!(request.layout_name.as_deref(), Some("master-stack"));
        assert_eq!(request.stylesheet, "workspace { display: flex; }");
        assert_eq!(request.space.width, 1920.0);
        assert_eq!(request.space.height, 1080.0);
    }

    #[test]
    fn resolves_selected_layout_into_shared_payload() {
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
            }],
            ..Config::default()
        };

        let selected = config
            .resolve_selected_layout(&workspace("master-stack"))
            .unwrap();

        assert_eq!(
            selected,
            Some(SelectedLayout {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
            })
        );
    }

    #[test]
    fn builds_layout_request_from_state_snapshot() {
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
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
            tag_names: vec!["1".into()],
        };

        let request = config
            .build_layout_request_from_state(
                &state,
                ResolvedLayoutNode::Workspace {
                    meta: Default::default(),
                    children: vec![],
                },
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
            .build_layout_request(
                &workspace("missing"),
                Some(&output()),
                ResolvedLayoutNode::Workspace {
                    meta: Default::default(),
                    children: vec![],
                },
            )
            .unwrap_err();

        assert_eq!(
            error,
            LayoutConfigError::UnknownLayout {
                name: "missing".into(),
            }
        );
    }

    #[test]
    fn loads_config_from_json_path() {
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("spiders-config-test.json");
        fs::write(
            &config_path,
            r#"{"layouts":[{"name":"master-stack","module":"layouts/master-stack.js","stylesheet":"workspace { display: flex; }"}]}"#,
        )
        .unwrap();

        let config = Config::from_path(&config_path).unwrap();

        assert_eq!(config.layouts[0].name, "master-stack");

        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn discovers_default_config_paths_from_home_dir() {
        let temp_dir = std::env::temp_dir();
        let home_dir = temp_dir.join("spiders-config-discovery-home");
        let config_dir = home_dir.join(".config/spiders-wm");
        let data_dir = home_dir.join(".local/share/spiders-wm");
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::create_dir_all(&data_dir);
        fs::write(config_dir.join("config.ts"), "export default {};").unwrap();

        let paths = ConfigPaths::discover(ConfigDiscoveryOptions {
            home_dir: Some(home_dir.clone()),
            ..ConfigDiscoveryOptions::default()
        })
        .unwrap();

        assert!(paths
            .authored_config
            .ends_with(".config/spiders-wm/config.ts"));
        assert!(paths
            .runtime_config
            .ends_with(".local/share/spiders-wm/config.json"));

        let _ = fs::remove_file(config_dir.join("config.ts"));
    }

    #[test]
    fn discovery_prefers_override_directories() {
        let temp_dir = std::env::temp_dir();
        let config_dir = temp_dir.join("spiders-config-override-config");
        let data_dir = temp_dir.join("spiders-config-override-data");
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::create_dir_all(&data_dir);
        fs::write(config_dir.join("config.js"), "module.exports = {};").unwrap();

        let paths = ConfigPaths::discover(ConfigDiscoveryOptions {
            home_dir: Some(temp_dir.clone()),
            config_dir_override: Some(config_dir.clone()),
            data_dir_override: Some(data_dir.clone()),
            authored_config_override: None,
            runtime_config_override: None,
        })
        .unwrap();

        assert_eq!(paths.authored_config, config_dir.join("config.js"));
        assert_eq!(paths.runtime_config, data_dir.join("config.json"));

        let _ = fs::remove_file(config_dir.join("config.js"));
    }

    #[test]
    fn discovery_supports_direct_file_overrides() {
        let temp_dir = std::env::temp_dir();
        let authored = temp_dir.join("spiders-direct-authored.js");
        let runtime = temp_dir.join("spiders-direct-runtime.json");

        let paths = ConfigPaths::discover(ConfigDiscoveryOptions {
            authored_config_override: Some(authored.clone()),
            runtime_config_override: Some(runtime.clone()),
            ..ConfigDiscoveryOptions::default()
        })
        .unwrap();

        assert_eq!(paths.authored_config, authored);
        assert_eq!(paths.runtime_config, runtime);
    }
}
