use std::collections::BTreeMap;

use spiders_shared::wm::{LayoutEvaluationContext, LoadedLayout};

use crate::loader::{LayoutLoadError, LayoutSourceLoader};
use crate::model::{Config, ConfigDiscoveryOptions, ConfigPaths, LayoutConfigError};
use crate::runtime::{LayoutRuntime, LayoutRuntimeError};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ConfigRuntimeServiceError {
    #[error(transparent)]
    Load(#[from] LayoutLoadError),
    #[error(transparent)]
    Runtime(#[from] LayoutRuntimeError),
    #[error(transparent)]
    Config(#[from] LayoutConfigError),
}

#[derive(Debug)]
pub struct ConfigRuntimeService<L, R> {
    loader: L,
    runtime: R,
    cache: BTreeMap<String, LoadedLayout>,
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

impl<L: LayoutSourceLoader, R: LayoutRuntime> ConfigRuntimeService<L, R> {
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
            Ok(Config::from_authored_path(&paths.authored_config)?)
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
    ) -> Result<Option<&LoadedLayout>, ConfigRuntimeServiceError> {
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
            loaded,
            context,
            layout,
        }))
    }

    pub fn cache(&self) -> &BTreeMap<String, LoadedLayout> {
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
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, StateSnapshot, WorkspaceSnapshot,
    };

    use super::*;
    use crate::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use crate::model::{Config, ConfigDiscoveryOptions, LayoutDefinition};
    use crate::runtime::BoaLayoutRuntime;

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
        let temp_dir = std::env::temp_dir();
        let project_root = temp_dir.join("spiders-service-project");
        let runtime_root = temp_dir.join("spiders-service-runtime");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(&module_path, "ctx => ({ type: 'workspace', children: [] })").unwrap();

        let loader = RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(
            &project_root,
            &runtime_root,
        ));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let mut service = ConfigRuntimeService::new(loader, runtime);
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

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn runtime_service_evaluates_loaded_layout_for_workspace() {
        let temp_dir = std::env::temp_dir();
        let project_root = temp_dir.join("spiders-service-project-eval");
        let runtime_root = temp_dir.join("spiders-service-runtime-eval");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let loader = RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(
            &project_root,
            &runtime_root,
        ));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let mut service = ConfigRuntimeService::new(loader, runtime);
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

        let _ = fs::remove_file(module_path);
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

        let loader = RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", "."));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service: ConfigRuntimeService<_, _> = ConfigRuntimeService::new(loader, runtime);
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

        let loader = RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", "."));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service: ConfigRuntimeService<_, _> = ConfigRuntimeService::new(loader, runtime);
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
        let loader = RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", "."));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service: ConfigRuntimeService<_, _> = ConfigRuntimeService::new(loader, runtime);
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
        let _ = fs::create_dir_all(project_root.join("config"));
        let _ = fs::create_dir_all(project_root.join("layouts/master-stack"));
        fs::write(
            project_root.join("config.ts"),
            r#"
                import { bindings } from "./config/bindings";
                export default {
                  tags: ["1"],
                  bindings,
                  layouts: { default: "master-stack" },
                };
            "#,
        )
        .unwrap();
        fs::write(
            project_root.join("config/bindings.ts"),
            r#"
                import * as actions from "spider-wm/actions";
                export const bindings = {
                  mod: "alt",
                  entries: [{ bind: ["mod", "Return"], action: actions.spawn("foot") }],
                };
            "#,
        )
        .unwrap();
        fs::write(
            project_root.join("layouts/master-stack/index.tsx"),
            "export default function layout() { return { type: 'workspace', children: [] }; }",
        )
        .unwrap();

        let loader = RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", "."));
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let service: ConfigRuntimeService<_, _> = ConfigRuntimeService::new(loader, runtime);
        let config = service
            .load_config(&ConfigPaths::new(
                project_root.join("config.ts"),
                project_root.join("missing-config.json"),
            ))
            .unwrap();

        assert_eq!(config.tags, vec!["1"]);
        assert_eq!(config.bindings.len(), 1);
        assert_eq!(config.layouts.len(), 1);
        assert!(config.layouts[0].runtime_source.is_some());
    }
}
