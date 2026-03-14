use std::collections::BTreeMap;

use spiders_shared::runtime::{
    AuthoringRuntime, LayoutSourceLoader, RuntimeArtifact, RuntimeError,
};
use spiders_shared::wm::{LayoutEvaluationContext, LoadedLayout};

use crate::model::{Config, ConfigDiscoveryOptions, ConfigPaths, LayoutConfigError};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ConfigRuntimeServiceError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Config(#[from] LayoutConfigError),
}

#[derive(Debug)]
pub struct ConfigRuntimeService<L, R> {
    loader: L,
    runtime: R,
    cache: BTreeMap<String, RuntimeArtifact>,
}

impl<L, R> ConfigRuntimeService<L, R> {
    pub fn new(loader: L, runtime: R) -> Self {
        Self {
            loader,
            runtime,
            cache: BTreeMap::new(),
        }
    }
}

impl<L, R> ConfigRuntimeService<L, R>
where
    L: LayoutSourceLoader<Config>,
    R: AuthoringRuntime<Config = Config>,
{
    pub fn discover_config_paths(
        &self,
        options: ConfigDiscoveryOptions,
    ) -> Result<ConfigPaths, ConfigRuntimeServiceError> {
        Ok(ConfigPaths::discover(options)?)
    }

    pub fn load_config(&self, paths: &ConfigPaths) -> Result<Config, ConfigRuntimeServiceError> {
        if paths.runtime_config.exists() {
            Ok(Config::from_path(&paths.runtime_config)?)
        } else {
            Ok(self.runtime.load_authored_config(&paths.authored_config)?)
        }
    }

    pub fn validate_layout_modules(
        &self,
        config: &Config,
    ) -> Result<Vec<String>, ConfigRuntimeServiceError> {
        let mut errors = Vec::new();

        for layout in &config.layouts {
            let workspace = spiders_shared::wm::WorkspaceSnapshot {
                id: spiders_shared::ids::WorkspaceId::from("validation"),
                name: "validation".into(),
                output_id: None,
                active_tags: vec![],
                focused: true,
                visible: true,
                effective_layout: Some(spiders_shared::wm::LayoutRef {
                    name: layout.name.clone(),
                }),
            };

            if let Err(error) = self.loader.load_runtime_source(config, &workspace) {
                errors.push(format!("{}: {error}", layout.name));
            }
        }

        Ok(errors)
    }

    pub fn load_for_workspace(
        &mut self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<&RuntimeArtifact>, ConfigRuntimeServiceError> {
        let Some(loaded) = self.loader.load_runtime_source(config, workspace)? else {
            return Ok(None);
        };

        let key = loaded.selected.name.clone();
        self.cache.insert(key.clone(), loaded);
        Ok(self.cache.get(&key))
    }

    pub fn evaluate_for_workspace(
        &mut self,
        config: &Config,
        state: &spiders_shared::wm::StateSnapshot,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<crate::service::EvaluatedLayout>, ConfigRuntimeServiceError> {
        let Some(selected) = self.runtime.selected_layout(config, workspace)? else {
            return Ok(None);
        };
        let Some(loaded) = self.load_for_workspace(config, workspace)?.cloned() else {
            return Ok(None);
        };
        let context = self.runtime.build_context(state, workspace, Some(selected));
        let layout = self.runtime.evaluate_layout(&loaded, &context)?;

        Ok(Some(EvaluatedLayout {
            loaded: loaded.into(),
            context,
            layout,
        }))
    }

    pub fn cache(&self) -> &BTreeMap<String, RuntimeArtifact> {
        &self.cache
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedLayout {
    pub loaded: LoadedLayout,
    pub context: LayoutEvaluationContext,
    pub layout: spiders_shared::layout::SourceLayoutNode,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use spiders_shared::ids::{OutputId, WorkspaceId};
    use spiders_shared::layout::SourceLayoutNode;
    use spiders_shared::runtime::{
        AuthoringRuntime, LayoutModuleContract, LayoutRuntime, LayoutSourceLoader, RuntimeArtifact,
        RuntimeError,
    };
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, SelectedLayout, StateSnapshot,
        WorkspaceSnapshot,
    };

    use super::*;
    use crate::model::{Config, ConfigDiscoveryOptions, LayoutDefinition};

    #[derive(Debug, Clone)]
    struct StubLoader {
        loaded: Option<RuntimeArtifact>,
        error_message: Option<String>,
    }

    impl LayoutSourceLoader<Config> for StubLoader {
        fn load_runtime_source(
            &self,
            _config: &Config,
            _workspace: &WorkspaceSnapshot,
        ) -> Result<Option<RuntimeArtifact>, RuntimeError> {
            if let Some(message) = &self.error_message {
                return Err(RuntimeError::Other {
                    message: message.clone(),
                });
            }

            Ok(self.loaded.clone())
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct StubRuntime;

    impl LayoutRuntime for StubRuntime {
        type Config = Config;

        fn selected_layout(
            &self,
            config: &Self::Config,
            workspace: &WorkspaceSnapshot,
        ) -> Result<Option<SelectedLayout>, RuntimeError> {
            config
                .resolve_selected_layout(workspace)
                .map_err(|error| RuntimeError::Config {
                    message: error.to_string(),
                })
        }

        fn load_selected_layout(
            &self,
            _config: &Self::Config,
            _workspace: &WorkspaceSnapshot,
        ) -> Result<Option<RuntimeArtifact>, RuntimeError> {
            Ok(None)
        }

        fn build_context(
            &self,
            state: &StateSnapshot,
            workspace: &WorkspaceSnapshot,
            selected_layout: Option<SelectedLayout>,
        ) -> spiders_shared::wm::LayoutEvaluationContext {
            state.layout_context(workspace, selected_layout)
        }

        fn evaluate_layout(
            &self,
            _loaded_layout: &RuntimeArtifact,
            _context: &spiders_shared::wm::LayoutEvaluationContext,
        ) -> Result<SourceLayoutNode, RuntimeError> {
            Ok(SourceLayoutNode::Workspace {
                meta: Default::default(),
                children: vec![],
            })
        }

        fn contract(&self) -> LayoutModuleContract {
            LayoutModuleContract::default()
        }
    }

    impl AuthoringRuntime for StubRuntime {
        fn load_authored_config(
            &self,
            _path: &std::path::Path,
        ) -> Result<Self::Config, RuntimeError> {
            Err(RuntimeError::NotImplemented(
                "authored config loading".into(),
            ))
        }
    }

    #[derive(Debug, Clone, Default)]
    struct StubAuthoredRuntime {
        config: Config,
    }

    impl LayoutRuntime for StubAuthoredRuntime {
        type Config = Config;

        fn selected_layout(
            &self,
            config: &Self::Config,
            workspace: &WorkspaceSnapshot,
        ) -> Result<Option<SelectedLayout>, RuntimeError> {
            StubRuntime.selected_layout(config, workspace)
        }

        fn load_selected_layout(
            &self,
            _config: &Self::Config,
            _workspace: &WorkspaceSnapshot,
        ) -> Result<Option<RuntimeArtifact>, RuntimeError> {
            Ok(None)
        }

        fn build_context(
            &self,
            state: &StateSnapshot,
            workspace: &WorkspaceSnapshot,
            selected_layout: Option<SelectedLayout>,
        ) -> spiders_shared::wm::LayoutEvaluationContext {
            StubRuntime.build_context(state, workspace, selected_layout)
        }

        fn evaluate_layout(
            &self,
            loaded_layout: &RuntimeArtifact,
            context: &spiders_shared::wm::LayoutEvaluationContext,
        ) -> Result<SourceLayoutNode, RuntimeError> {
            StubRuntime.evaluate_layout(loaded_layout, context)
        }

        fn contract(&self) -> LayoutModuleContract {
            LayoutModuleContract::default()
        }
    }

    impl AuthoringRuntime for StubAuthoredRuntime {
        fn load_authored_config(
            &self,
            _path: &std::path::Path,
        ) -> Result<Self::Config, RuntimeError> {
            Ok(self.config.clone())
        }
    }

    fn loaded_layout(name: &str, module: &str) -> RuntimeArtifact {
        RuntimeArtifact {
            selected: SelectedLayout {
                name: name.into(),
                module: module.into(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
            },
            runtime_source: "ctx => ({ type: 'workspace', children: [] })".into(),
        }
    }

    fn workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_tags: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef {
                name: "master-stack".into(),
            }),
        }
    }

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
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
            }],
            workspaces: vec![workspace()],
            windows: vec![],
            visible_window_ids: vec![],
            tag_names: vec!["1".into()],
        }
    }

    #[test]
    fn runtime_service_loads_and_caches_runtime_artifact() {
        let loader = StubLoader {
            loaded: Some(loaded_layout("master-stack", "layouts/master-stack.js")),
            error_message: None,
        };
        let mut service = ConfigRuntimeService::new(loader, StubRuntime);
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                runtime_source: None,
            }],
            ..Config::default()
        };

        let loaded = service
            .load_for_workspace(&config, &workspace())
            .unwrap()
            .unwrap();

        assert_eq!(loaded.selected.name, "master-stack");
        assert!(service.cache().contains_key("master-stack"));
    }

    #[test]
    fn runtime_service_evaluates_loaded_layout_for_workspace() {
        let loader = StubLoader {
            loaded: Some(loaded_layout("master-stack", "layouts/master-stack.js")),
            error_message: None,
        };
        let mut service = ConfigRuntimeService::new(loader, StubRuntime);
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                runtime_source: None,
            }],
            ..Config::default()
        };

        let evaluated = service
            .evaluate_for_workspace(&config, &state(), &workspace())
            .unwrap()
            .unwrap();

        assert_eq!(evaluated.loaded.selected.name, "master-stack");
        assert!(matches!(
            evaluated.layout,
            spiders_shared::layout::SourceLayoutNode::Workspace { .. }
        ));
    }

    #[test]
    fn runtime_service_loads_config_from_runtime_path() {
        let temp_dir = std::env::temp_dir();
        let runtime_config_path = temp_dir.join("spiders-runtime-config.json");
        fs::write(
            &runtime_config_path,
            r#"{"layouts":[{"name":"master-stack","module":"layouts/master-stack.js","stylesheet":"workspace { display: flex; }"}]}"#,
        )
        .unwrap();

        let service: ConfigRuntimeService<_, _> = ConfigRuntimeService::new(
            StubLoader {
                loaded: None,
                error_message: None,
            },
            StubRuntime,
        );
        let config = service
            .load_config(&ConfigPaths::new("unused", &runtime_config_path))
            .unwrap();

        assert_eq!(config.layouts[0].module, "layouts/master-stack.js");

        let _ = fs::remove_file(runtime_config_path);
    }

    #[test]
    fn runtime_service_discovers_config_paths_from_options() {
        let temp_dir = std::env::temp_dir();
        let home_dir = temp_dir.join("spiders-service-discovery-home");
        let config_dir = home_dir.join(".config/spiders-wm");
        let data_dir = home_dir.join(".local/share/spiders-wm");
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::create_dir_all(&data_dir);
        fs::write(config_dir.join("config.ts"), "export default {};").unwrap();

        let service: ConfigRuntimeService<_, _> = ConfigRuntimeService::new(
            StubLoader {
                loaded: None,
                error_message: None,
            },
            StubRuntime,
        );
        let paths = service
            .discover_config_paths(ConfigDiscoveryOptions {
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
    fn runtime_service_reports_missing_layout_module_sources() {
        let service: ConfigRuntimeService<_, _> = ConfigRuntimeService::new(
            StubLoader {
                loaded: None,
                error_message: Some(
                    "layout module `layouts/missing.js` source is unavailable".into(),
                ),
            },
            StubRuntime,
        );
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "missing".into(),
                module: "layouts/missing.js".into(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                runtime_source: None,
            }],
            ..Config::default()
        };

        let errors = service.validate_layout_modules(&config).unwrap();

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("missing"));
    }

    #[test]
    fn runtime_service_loads_authored_config_when_runtime_json_is_missing() {
        let project_root = std::env::temp_dir().join("spiders-service-authored-config");
        let authored_config = Config {
            tags: vec!["1".into()],
            bindings: vec![],
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.bundle.js".into(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                runtime_source: Some("ctx => ({ type: 'workspace', children: [] })".into()),
            }],
            ..Config::default()
        };

        let service: ConfigRuntimeService<_, _> = ConfigRuntimeService::new(
            StubLoader {
                loaded: None,
                error_message: None,
            },
            StubAuthoredRuntime {
                config: authored_config,
            },
        );
        let config = service
            .load_config(&ConfigPaths::new(
                project_root.join("config.ts"),
                project_root.join("missing-config.json"),
            ))
            .unwrap();

        assert_eq!(config.tags, vec!["1"]);
        assert_eq!(config.bindings.len(), 0);
        assert_eq!(config.layouts.len(), 1);
        assert!(config.layouts[0].runtime_source.is_some());
    }
}
