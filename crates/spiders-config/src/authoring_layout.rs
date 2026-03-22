use std::collections::BTreeMap;

use spiders_shared::runtime::{
    AuthoringLayoutRuntime, LayoutEvaluationContext, PreparedLayout, RuntimeError,
    RuntimeRefreshSummary,
};
use spiders_tree::SourceLayoutNode;

use crate::model::{Config, ConfigDiscoveryOptions, ConfigPaths, LayoutConfigError};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AuthoringLayoutServiceError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Config(#[from] LayoutConfigError),
}

#[derive(Debug)]
pub struct AuthoringLayoutService<R> {
    runtime: R,
    cache: BTreeMap<String, PreparedLayout>,
    paths: Option<ConfigPaths>,
}

impl<R> AuthoringLayoutService<R> {
    pub fn new(runtime: R) -> Self {
        Self {
            runtime,
            cache: BTreeMap::new(),
            paths: None,
        }
    }

    pub fn with_paths(runtime: R, paths: ConfigPaths) -> Self {
        Self {
            runtime,
            cache: BTreeMap::new(),
            paths: Some(paths),
        }
    }
}

impl<R> AuthoringLayoutService<R>
where
    R: AuthoringLayoutRuntime<Config = Config>,
{
    pub fn discover_config_paths(
        &self,
        options: ConfigDiscoveryOptions,
    ) -> Result<ConfigPaths, AuthoringLayoutServiceError> {
        Ok(ConfigPaths::discover(options)?)
    }

    pub fn load_config(&self, paths: &ConfigPaths) -> Result<Config, AuthoringLayoutServiceError> {
        Ok(self.load_config_with_cache_update(paths)?.0)
    }

    pub fn load_config_with_cache_update(
        &self,
        paths: &ConfigPaths,
    ) -> Result<(Config, Option<RuntimeRefreshSummary>), AuthoringLayoutServiceError> {
        if paths.authored_config.exists() {
            let update = self
                .runtime
                .refresh_prepared_config(&paths.authored_config, &paths.prepared_config)?;
            Ok((
                self.runtime.load_prepared_config(&paths.prepared_config)?,
                Some(update),
            ))
        } else if paths.prepared_config.exists() {
            Ok((
                self.runtime.load_prepared_config(&paths.prepared_config)?,
                None,
            ))
        } else {
            Ok((
                self.runtime.load_authored_config(&paths.authored_config)?,
                None,
            ))
        }
    }

    pub fn load_authored_config(
        &self,
        paths: &ConfigPaths,
    ) -> Result<Config, AuthoringLayoutServiceError> {
        Ok(self.runtime.load_authored_config(&paths.authored_config)?)
    }

    pub fn write_prepared_config(
        &self,
        paths: &ConfigPaths,
        _config: &Config,
    ) -> Result<RuntimeRefreshSummary, AuthoringLayoutServiceError> {
        Ok(self
            .runtime
            .rebuild_prepared_config(&paths.authored_config, &paths.prepared_config)?)
    }

    pub fn reload_config(&mut self) -> Result<Config, AuthoringLayoutServiceError> {
        let Some(paths) = self.paths.as_ref() else {
            return Err(RuntimeError::Other {
                message: "prepared config reload requires configured paths".into(),
            }
            .into());
        };

        let _ = self
            .runtime
            .rebuild_prepared_config(&paths.authored_config, &paths.prepared_config)?;
        self.cache.clear();
        Ok(self.runtime.load_prepared_config(&paths.prepared_config)?)
    }

    pub fn validate_layout_modules(
        &self,
        config: &Config,
    ) -> Result<Vec<String>, AuthoringLayoutServiceError> {
        let mut errors = Vec::new();

        for layout in &config.layouts {
            let workspace = spiders_shared::wm::WorkspaceSnapshot {
                id: spiders_tree::WorkspaceId::from("validation"),
                name: "validation".into(),
                output_id: None,
                active_workspaces: vec![],
                focused: true,
                visible: true,
                effective_layout: Some(spiders_shared::wm::LayoutRef {
                    name: layout.name.clone(),
                }),
            };

            if let Err(error) = self.runtime.prepare_layout(config, &workspace) {
                errors.push(format!("{}: {error}", layout.name));
            }
        }

        Ok(errors)
    }

    pub fn prepare_for_workspace(
        &mut self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<&PreparedLayout>, AuthoringLayoutServiceError> {
        let Some(loaded) = self.runtime.prepare_layout(config, workspace)? else {
            return Ok(None);
        };

        let key = loaded.selected.name.clone();
        self.cache.insert(key.clone(), loaded);
        Ok(self.cache.get(&key))
    }

    pub fn evaluate_prepared_for_workspace(
        &mut self,
        config: &Config,
        state: &spiders_shared::wm::StateSnapshot,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<
        Option<crate::authoring_layout::PreparedLayoutEvaluation>,
        AuthoringLayoutServiceError,
    > {
        let Some(loaded) = self.prepare_for_workspace(config, workspace)?.cloned() else {
            return Ok(None);
        };
        let context = self.runtime.build_context(state, workspace, Some(&loaded));
        let layout = self.runtime.evaluate_layout(&loaded, &context)?;

        Ok(Some(PreparedLayoutEvaluation {
            artifact: loaded,
            context,
            layout,
        }))
    }

    pub fn cache(&self) -> &BTreeMap<String, PreparedLayout> {
        &self.cache
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedLayoutEvaluation {
    pub artifact: PreparedLayout,
    pub context: LayoutEvaluationContext,
    pub layout: SourceLayoutNode,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use spiders_tree::{OutputId, WorkspaceId};
    use spiders_shared::runtime::{
        AuthoringLayoutRuntime, LayoutModuleContract, PreparedLayout, PreparedLayoutRuntime,
        RuntimeError, SelectedLayout,
    };
    use spiders_shared::wm::{LayoutRef, OutputSnapshot, OutputTransform, StateSnapshot, WorkspaceSnapshot};

    use super::*;
    use crate::model::{
        Config, ConfigDiscoveryOptions, JavaScriptModule, JavaScriptModuleGraph,
        LayoutDefinition,
    };

    #[derive(Debug, Clone)]
    struct StubRuntime {
        loaded: Option<PreparedLayout>,
        error_message: Option<String>,
    }

    impl PreparedLayoutRuntime for StubRuntime {
        type Config = Config;

        fn prepare_layout(
            &self,
            _config: &Self::Config,
            _workspace: &WorkspaceSnapshot,
        ) -> Result<Option<PreparedLayout>, RuntimeError> {
            if let Some(message) = &self.error_message {
                return Err(RuntimeError::Other {
                    message: message.clone(),
                });
            }

            Ok(self.loaded.clone())
        }

        fn build_context(
            &self,
            state: &StateSnapshot,
            workspace: &WorkspaceSnapshot,
            artifact: Option<&PreparedLayout>,
        ) -> spiders_shared::runtime::LayoutEvaluationContext {
            state.layout_context(
                workspace,
                artifact.map(|artifact| artifact.selected.clone()),
            )
        }

        fn evaluate_layout(
            &self,
            _prepared_layout: &PreparedLayout,
            _context: &spiders_shared::runtime::LayoutEvaluationContext,
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

    impl AuthoringLayoutRuntime for StubRuntime {
        fn load_authored_config(
            &self,
            _path: &std::path::Path,
        ) -> Result<Self::Config, RuntimeError> {
            Err(RuntimeError::NotImplemented(
                "authored config loading".into(),
            ))
        }

        fn load_prepared_config(
            &self,
            _path: &std::path::Path,
        ) -> Result<Self::Config, RuntimeError> {
            Ok(Config {
                layouts: vec![LayoutDefinition {
                    name: "master-stack".into(),
                    directory: "layouts/master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet_path: Some("layouts/master-stack/index.css".into()),
                    runtime_graph: None,
                }],
                ..Config::default()
            })
        }

        fn refresh_prepared_config(
            &self,
            _authored: &std::path::Path,
            _runtime: &std::path::Path,
        ) -> Result<RuntimeRefreshSummary, RuntimeError> {
            Ok(RuntimeRefreshSummary::default())
        }

        fn rebuild_prepared_config(
            &self,
            _authored: &std::path::Path,
            _runtime: &std::path::Path,
        ) -> Result<RuntimeRefreshSummary, RuntimeError> {
            Err(RuntimeError::NotImplemented(
                "prepared config rebuild".into(),
            ))
        }
    }

    #[derive(Debug, Clone, Default)]
    struct StubAuthoredRuntime {
        loaded: Option<PreparedLayout>,
        error_message: Option<String>,
        config: Config,
    }

    impl PreparedLayoutRuntime for StubAuthoredRuntime {
        type Config = Config;

        fn prepare_layout(
            &self,
            _config: &Self::Config,
            _workspace: &WorkspaceSnapshot,
        ) -> Result<Option<PreparedLayout>, RuntimeError> {
            if let Some(message) = &self.error_message {
                return Err(RuntimeError::Other {
                    message: message.clone(),
                });
            }

            Ok(self.loaded.clone())
        }

        fn build_context(
            &self,
            state: &StateSnapshot,
            workspace: &WorkspaceSnapshot,
            artifact: Option<&PreparedLayout>,
        ) -> spiders_shared::runtime::LayoutEvaluationContext {
            StubRuntime {
                loaded: None,
                error_message: None,
            }
            .build_context(state, workspace, artifact)
        }

        fn evaluate_layout(
            &self,
            prepared_layout: &PreparedLayout,
            context: &spiders_shared::runtime::LayoutEvaluationContext,
        ) -> Result<SourceLayoutNode, RuntimeError> {
            StubRuntime {
                loaded: None,
                error_message: None,
            }
            .evaluate_layout(prepared_layout, context)
        }

        fn contract(&self) -> LayoutModuleContract {
            LayoutModuleContract::default()
        }
    }

    impl AuthoringLayoutRuntime for StubAuthoredRuntime {
        fn load_authored_config(
            &self,
            _path: &std::path::Path,
        ) -> Result<Self::Config, RuntimeError> {
            Ok(self.config.clone())
        }

        fn load_prepared_config(
            &self,
            _path: &std::path::Path,
        ) -> Result<Self::Config, RuntimeError> {
            Ok(self.config.clone())
        }

        fn refresh_prepared_config(
            &self,
            _authored: &std::path::Path,
            _runtime: &std::path::Path,
        ) -> Result<RuntimeRefreshSummary, RuntimeError> {
            Ok(RuntimeRefreshSummary::default())
        }

        fn rebuild_prepared_config(
            &self,
            _authored: &std::path::Path,
            _runtime: &std::path::Path,
        ) -> Result<RuntimeRefreshSummary, RuntimeError> {
            Ok(RuntimeRefreshSummary::default())
        }
    }

    fn prepared_layout(name: &str, module: &str) -> PreparedLayout {
        PreparedLayout {
            selected: SelectedLayout {
                name: name.into(),
                directory: "layouts/master-stack".into(),
                module: module.into(),
            },
            runtime_payload: serde_json::to_value(single_module_graph(module)).unwrap(),
            stylesheets: Default::default(),
        }
    }

    fn single_module_graph(module: &str) -> JavaScriptModuleGraph {
        JavaScriptModuleGraph {
            entry: module.into(),
            modules: vec![JavaScriptModule {
                specifier: module.into(),
                source: "export default (ctx => ({ type: 'workspace', children: [] }));".into(),
                resolved_imports: Default::default(),
            }],
        }
    }

    fn workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_workspaces: vec!["1".into()],
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
            workspace_names: vec!["1".into()],
        }
    }

    #[test]
    fn authoring_layout_service_loads_and_caches_prepared_layout() {
        let runtime = StubRuntime {
            loaded: Some(prepared_layout("master-stack", "layouts/master-stack.js")),
            error_message: None,
        };
        let mut service = AuthoringLayoutService::new(runtime);
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_graph: None,
            }],
            ..Config::default()
        };

        let loaded = service
            .prepare_for_workspace(&config, &workspace())
            .unwrap()
            .unwrap();

        assert_eq!(loaded.selected.name, "master-stack");
        assert!(service.cache().contains_key("master-stack"));
    }

    #[test]
    fn authoring_layout_service_evaluates_prepared_layout_for_workspace() {
        let runtime = StubRuntime {
            loaded: Some(prepared_layout("master-stack", "layouts/master-stack.js")),
            error_message: None,
        };
        let mut service = AuthoringLayoutService::new(runtime);
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_graph: None,
            }],
            ..Config::default()
        };

        let evaluated = service
            .evaluate_prepared_for_workspace(&config, &state(), &workspace())
            .unwrap()
            .unwrap();

        assert_eq!(evaluated.artifact.selected.name, "master-stack");
        assert!(matches!(
            evaluated.layout,
            SourceLayoutNode::Workspace { .. }
        ));
    }

    #[test]
    fn authoring_layout_service_loads_config_from_runtime_path() {
        let temp_dir = std::env::temp_dir();
        let prepared_config_path = temp_dir.join("spiders-runtime-config.js");
        fs::write(&prepared_config_path, "export default {};").unwrap();

        let service: AuthoringLayoutService<_> = AuthoringLayoutService::new(StubRuntime {
            loaded: None,
            error_message: None,
        });
        let config = service
            .load_config(&ConfigPaths::new("unused", &prepared_config_path))
            .unwrap();

        assert_eq!(config.layouts[0].module, "layouts/master-stack.js");

        let _ = fs::remove_file(prepared_config_path);
    }

    #[test]
    fn authoring_layout_service_discovers_config_paths_from_options() {
        let temp_dir = std::env::temp_dir();
        let home_dir = temp_dir.join("spiders-service-discovery-home");
        let config_dir = home_dir.join(".config/spiders-wm");
        let data_dir = home_dir.join(".cache/spiders-wm");
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::create_dir_all(&data_dir);
        fs::write(config_dir.join("config.ts"), "export default {};").unwrap();

        let service: AuthoringLayoutService<_> = AuthoringLayoutService::new(StubRuntime {
            loaded: None,
            error_message: None,
        });
        let paths = service
            .discover_config_paths(ConfigDiscoveryOptions {
                home_dir: Some(home_dir.clone()),
                ..ConfigDiscoveryOptions::default()
            })
            .unwrap();

        assert!(
            paths
                .authored_config
                .ends_with(".config/spiders-wm/config.ts")
        );
        assert!(
            paths
                .prepared_config
                .ends_with(".cache/spiders-wm/config.js")
        );

        let _ = fs::remove_file(config_dir.join("config.ts"));
    }

    #[test]
    fn authoring_layout_service_reports_missing_layout_module_sources() {
        let service: AuthoringLayoutService<_> = AuthoringLayoutService::new(StubRuntime {
            loaded: None,
            error_message: Some("layout module `layouts/missing.js` source is unavailable".into()),
        });
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "missing".into(),
                directory: "layouts/missing".into(),
                module: "layouts/missing.js".into(),
                stylesheet_path: Some("layouts/missing/index.css".into()),
                runtime_graph: None,
            }],
            ..Config::default()
        };

        let errors = service.validate_layout_modules(&config).unwrap();

        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("missing"));
    }

    #[test]
    fn authoring_layout_service_loads_authored_config_when_runtime_js_is_missing() {
        let project_root = std::env::temp_dir().join("spiders-service-authored-config");
        let authored_config = Config {
            workspaces: vec!["1".into()],
            bindings: vec![],
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack/index.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_graph: Some(single_module_graph("layouts/master-stack/index.js")),
            }],
            ..Config::default()
        };

        let service: AuthoringLayoutService<_> = AuthoringLayoutService::new(StubAuthoredRuntime {
            loaded: None,
            error_message: None,
            config: authored_config,
        });
        let config = service
            .load_config(&ConfigPaths::new(
                project_root.join("config.ts"),
                project_root.join("missing-config.js"),
            ))
            .unwrap();

        assert_eq!(config.workspaces, vec!["1"]);
        assert_eq!(config.bindings.len(), 0);
        assert_eq!(config.layouts.len(), 1);
        assert!(config.layouts[0].runtime_graph.is_some());
    }
}
