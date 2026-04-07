use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use spiders_core::SourceLayoutNode;
use spiders_core::runtime::layout_context::{
    LayoutEvaluationContext, LayoutEvaluationDependencies,
};
use spiders_core::runtime::prepared_layout::PreparedLayout;
use spiders_core::runtime::runtime_contract::PreparedLayoutRuntime;
use spiders_core::runtime::runtime_error::{RuntimeError, RuntimeRefreshSummary};
use spiders_core::snapshot::{StateSnapshot, WorkspaceSnapshot};

use crate::authoring_layout::{AuthoringLayoutService, SourceBundleAuthoringLayoutService};
use crate::model::{Config, ConfigPaths, LayoutConfigError, RuntimeKind};

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedSourceLayout {
    pub layout: SourceLayoutNode,
    pub dependencies: LayoutEvaluationDependencies,
}

pub type SourceBundle = BTreeMap<PathBuf, String>;

pub trait AuthoringConfigRuntime: std::fmt::Debug {
    fn load_authored_config(&self, path: &Path) -> Result<Config, RuntimeError>;
    fn load_prepared_config(&self, path: &Path) -> Result<Config, RuntimeError>;
    fn refresh_prepared_config(
        &self,
        authored: &Path,
        prepared: &Path,
    ) -> Result<RuntimeRefreshSummary, RuntimeError>;
    fn rebuild_prepared_config(
        &self,
        authored: &Path,
        prepared: &Path,
    ) -> Result<RuntimeRefreshSummary, RuntimeError>;
}

#[derive(Debug)]
pub struct RuntimeBundle {
    pub config_runtime: Box<dyn AuthoringConfigRuntime>,
    pub layout_runtime: Box<dyn PreparedLayoutRuntime<Config = Config>>,
}

pub trait RuntimeProvider: std::fmt::Debug {
    fn kind(&self) -> RuntimeKind;
    fn build_runtime_bundle(&self, paths: &ConfigPaths)
    -> Result<RuntimeBundle, LayoutConfigError>;
}

pub trait SourceBundleConfigRuntime: std::fmt::Debug {
    fn load_config<'a>(
        &'a self,
        root_dir: &'a Path,
        entry_path: &'a Path,
        sources: &'a SourceBundle,
    ) -> Pin<Box<dyn Future<Output = Result<Config, LayoutConfigError>> + 'a>>;
}

pub trait SourceBundlePreparedLayoutRuntime: std::fmt::Debug {
    fn prepare_layout<'a>(
        &'a self,
        root_dir: &'a Path,
        sources: &'a SourceBundle,
        config: &'a Config,
        workspace: &'a WorkspaceSnapshot,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PreparedLayout>, LayoutConfigError>> + 'a>>;

    fn build_context(
        &self,
        state: &StateSnapshot,
        workspace: &WorkspaceSnapshot,
        artifact: Option<&PreparedLayout>,
    ) -> LayoutEvaluationContext;

    fn evaluate_layout<'a>(
        &'a self,
        root_dir: &'a Path,
        sources: &'a SourceBundle,
        artifact: &'a PreparedLayout,
        context: &'a LayoutEvaluationContext,
    ) -> Pin<Box<dyn Future<Output = Result<EvaluatedSourceLayout, LayoutConfigError>> + 'a>>;
}

pub trait SourceBundleRuntimeProvider: std::fmt::Debug {
    fn kind(&self) -> RuntimeKind;
    fn build_source_bundle_runtime_bundle(
        &self,
    ) -> Result<SourceBundleRuntimeBundle, LayoutConfigError>;
}

#[derive(Debug)]
pub struct SourceBundleRuntimeBundle {
    pub config_runtime: Box<dyn SourceBundleConfigRuntime>,
    pub layout_runtime: Box<dyn SourceBundlePreparedLayoutRuntime>,
}

pub fn build_authoring_layout_service(
    paths: &ConfigPaths,
    providers: &[&dyn RuntimeProvider],
) -> Result<AuthoringLayoutService, LayoutConfigError> {
    let Some(kind) = paths.runtime_kind() else {
        return Err(LayoutConfigError::ReadConfig { path: paths.authored_config.clone() });
    };

    let Some(provider) = providers.iter().find(|provider| provider.kind() == kind) else {
        return Err(LayoutConfigError::ReadConfig { path: paths.authored_config.clone() });
    };

    let bundle = provider.build_runtime_bundle(paths)?;
    Ok(AuthoringLayoutService::from_runtime_bundle(
        bundle.config_runtime,
        bundle.layout_runtime,
        paths.clone(),
    ))
}

pub async fn load_config_from_source_bundle(
    root_dir: &Path,
    entry_path: &Path,
    sources: &SourceBundle,
    providers: &[&dyn SourceBundleRuntimeProvider],
) -> Result<Config, LayoutConfigError> {
    let Some(kind) = crate::model::runtime_kind_for_path(entry_path) else {
        return Err(LayoutConfigError::ReadConfig { path: entry_path.to_path_buf() });
    };

    let Some(provider) = providers.iter().find(|provider| provider.kind() == kind) else {
        return Err(LayoutConfigError::ReadConfig { path: entry_path.to_path_buf() });
    };

    let bundle = provider.build_source_bundle_runtime_bundle()?;
    bundle.config_runtime.load_config(root_dir, entry_path, sources).await
}

pub fn build_source_bundle_authoring_layout_service(
    entry_path: &Path,
    providers: &[&dyn SourceBundleRuntimeProvider],
) -> Result<SourceBundleAuthoringLayoutService, LayoutConfigError> {
    let Some(kind) = crate::model::runtime_kind_for_path(entry_path) else {
        return Err(LayoutConfigError::ReadConfig { path: entry_path.to_path_buf() });
    };

    let Some(provider) = providers.iter().find(|provider| provider.kind() == kind) else {
        return Err(LayoutConfigError::ReadConfig { path: entry_path.to_path_buf() });
    };

    let bundle = provider.build_source_bundle_runtime_bundle()?;
    Ok(SourceBundleAuthoringLayoutService::from_runtime_bundle(
        bundle.config_runtime,
        bundle.layout_runtime,
    ))
}
