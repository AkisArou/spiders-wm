use std::collections::BTreeMap;

use spiders_shared::runtime::{
    AuthoringLayoutRuntime, LayoutEvaluationContext, PreparedLayout, RuntimeError,
};
use spiders_shared::wm::{LayoutRef, StateSnapshot, WorkspaceSnapshot};
use spiders_tree::{SourceLayoutNode, WorkspaceId};

use super::config_paths;
use super::prepared_cache;
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

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedLayoutEvaluation {
    pub artifact: PreparedLayout,
    pub context: LayoutEvaluationContext,
    pub layout: SourceLayoutNode,
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
        config_paths::discover_config_paths(options)
    }

    pub fn load_config(&self, paths: &ConfigPaths) -> Result<Config, AuthoringLayoutServiceError> {
        Ok(self.load_config_with_cache_update(paths)?.0)
    }

    pub fn load_config_with_cache_update(
        &self,
        paths: &ConfigPaths,
    ) -> Result<
        (Config, Option<spiders_shared::runtime::RuntimeRefreshSummary>),
        AuthoringLayoutServiceError,
    > {
        prepared_cache::load_config_with_cache_update(&self.runtime, paths)
    }

    pub fn load_authored_config(
        &self,
        paths: &ConfigPaths,
    ) -> Result<Config, AuthoringLayoutServiceError> {
        prepared_cache::load_authored_config(&self.runtime, paths)
    }

    pub fn write_prepared_config(
        &self,
        paths: &ConfigPaths,
        _config: &Config,
    ) -> Result<spiders_shared::runtime::RuntimeRefreshSummary, AuthoringLayoutServiceError> {
        prepared_cache::write_prepared_config(&self.runtime, paths)
    }

    pub fn reload_config(&mut self) -> Result<Config, AuthoringLayoutServiceError> {
        let config = prepared_cache::reload_config(&self.runtime, self.paths.as_ref())?;
        self.cache.clear();
        Ok(config)
    }

    pub fn validate_layout_modules(
        &self,
        config: &Config,
    ) -> Result<Vec<String>, AuthoringLayoutServiceError> {
        let mut errors = Vec::new();

        for layout in &config.layouts {
            let workspace = validation_workspace(&layout.name);

            if let Err(error) = self.runtime.prepare_layout(config, &workspace) {
                errors.push(format!("{}: {error}", layout.name));
            }
        }

        Ok(errors)
    }

    pub fn prepare_for_workspace(
        &mut self,
        config: &Config,
        workspace: &WorkspaceSnapshot,
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
        state: &StateSnapshot,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayoutEvaluation>, AuthoringLayoutServiceError> {
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

fn validation_workspace(layout_name: &str) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        id: WorkspaceId::from("validation"),
        name: "validation".into(),
        output_id: None,
        active_workspaces: vec![],
        focused: true,
        visible: true,
        effective_layout: Some(LayoutRef {
            name: layout_name.into(),
        }),
    }
}