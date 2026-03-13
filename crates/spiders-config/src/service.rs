use std::collections::BTreeMap;

use spiders_shared::wm::{LayoutEvaluationContext, LoadedLayout};

use crate::loader::{LayoutLoadError, LayoutSourceLoader};
use crate::model::Config;
use crate::runtime::{LayoutRuntime, LayoutRuntimeError};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ConfigRuntimeServiceError {
    #[error(transparent)]
    Load(#[from] LayoutLoadError),
    #[error(transparent)]
    Runtime(#[from] LayoutRuntimeError),
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
    use crate::model::{Config, LayoutDefinition};
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
}
