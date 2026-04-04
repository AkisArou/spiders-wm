use std::collections::BTreeMap;

use spiders_core::runtime::layout_context::LayoutEvaluationContext;
use spiders_core::runtime::prepared_layout::PreparedLayout;
use spiders_core::snapshot::{StateSnapshot, WorkspaceSnapshot};
use spiders_core::SourceLayoutNode;

use crate::model::{Config, LayoutConfigError};
use crate::runtime::{SourceBundle, SourceBundleConfigRuntime, SourceBundlePreparedLayoutRuntime};

#[derive(Debug)]
pub struct SourceBundleAuthoringLayoutService {
    config_runtime: Box<dyn SourceBundleConfigRuntime>,
    layout_runtime: Box<dyn SourceBundlePreparedLayoutRuntime>,
    cache: BTreeMap<String, PreparedLayout>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedSourceBundleLayoutEvaluation {
    pub artifact: PreparedLayout,
    pub context: LayoutEvaluationContext,
    pub layout: SourceLayoutNode,
}

impl SourceBundleAuthoringLayoutService {
    pub(crate) fn from_runtime_bundle(
        config_runtime: Box<dyn SourceBundleConfigRuntime>,
        layout_runtime: Box<dyn SourceBundlePreparedLayoutRuntime>,
    ) -> Self {
        Self { config_runtime, layout_runtime, cache: BTreeMap::new() }
    }

    pub async fn load_config(
        &self,
        root_dir: &std::path::Path,
        entry_path: &std::path::Path,
        sources: &SourceBundle,
    ) -> Result<Config, LayoutConfigError> {
        self.config_runtime.load_config(root_dir, entry_path, sources).await
    }

    pub async fn prepare_for_workspace(
        &mut self,
        root_dir: &std::path::Path,
        sources: &SourceBundle,
        config: &Config,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<&PreparedLayout>, LayoutConfigError> {
        let Some(loaded) = self
            .layout_runtime
            .prepare_layout(root_dir, sources, config, workspace)
            .await?
        else {
            return Ok(None);
        };

        let key = loaded.selected.name.clone();
        self.cache.insert(key.clone(), loaded);
        Ok(self.cache.get(&key))
    }

    pub async fn evaluate_prepared_for_workspace(
        &mut self,
        root_dir: &std::path::Path,
        sources: &SourceBundle,
        config: &Config,
        state: &StateSnapshot,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<PreparedSourceBundleLayoutEvaluation>, LayoutConfigError> {
        let Some(loaded) = self.prepare_for_workspace(root_dir, sources, config, workspace).await?.cloned()
        else {
            return Ok(None);
        };

        let context = self.layout_runtime.build_context(state, workspace, Some(&loaded));
        let layout = self.layout_runtime.evaluate_layout(root_dir, sources, &loaded, &context).await?;

        Ok(Some(PreparedSourceBundleLayoutEvaluation { artifact: loaded, context, layout }))
    }
}
