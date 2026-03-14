use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::layout::SourceLayoutNode;
use crate::wm::{
    LayoutEvaluationContext, LoadedLayout, SelectedLayout, StateSnapshot, WorkspaceSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutModuleContract {
    pub export_name: String,
}

impl Default for LayoutModuleContract {
    fn default() -> Self {
        Self {
            export_name: "default".into(),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RuntimeError {
    #[error("runtime operation is not implemented: {0}")]
    NotImplemented(String),
    #[error("javascript evaluation failed: {message}")]
    JavaScript { message: String },
    #[error("layout module `{name}` did not provide `{export}` export")]
    MissingExport { name: String, export: String },
    #[error("layout module `{name}` export `{export}` is not callable")]
    NonCallableExport { name: String, export: String },
    #[error("layout module `{name}` source is unavailable")]
    MissingRuntimeSource { name: String },
    #[error("js to layout conversion failed for layout `{name}`: {message}")]
    ValueConversion { name: String, message: String },
    #[error("validation failed: {message}")]
    Validation { message: String },
    #[error("config runtime failed: {message}")]
    Config { message: String },
    #[error("runtime failed: {message}")]
    Other { message: String },
}

pub trait LayoutRuntime: std::fmt::Debug {
    type Config;

    fn selected_layout(
        &self,
        config: &Self::Config,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, RuntimeError>;

    fn load_selected_layout(
        &self,
        config: &Self::Config,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<LoadedLayout>, RuntimeError>;

    fn build_context(
        &self,
        state: &StateSnapshot,
        workspace: &WorkspaceSnapshot,
        selected_layout: Option<SelectedLayout>,
    ) -> LayoutEvaluationContext;

    fn evaluate_layout(
        &self,
        loaded_layout: &LoadedLayout,
        context: &LayoutEvaluationContext,
    ) -> Result<SourceLayoutNode, RuntimeError>;

    fn contract(&self) -> LayoutModuleContract;
}

pub trait AuthoredConfigRuntime: std::fmt::Debug {
    type Config;

    fn load_authored_config(&self, path: &Path) -> Result<Self::Config, RuntimeError>;
}

pub trait LayoutSourceLoader<C>: std::fmt::Debug {
    fn load_runtime_source(
        &self,
        config: &C,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<LoadedLayout>, RuntimeError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub name: String,
}
