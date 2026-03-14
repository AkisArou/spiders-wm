use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::layout::SourceLayoutNode;
use crate::wm::{LayoutEvaluationContext, SelectedLayout, StateSnapshot, WorkspaceSnapshot};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedLayout {
    pub selected: SelectedLayout,
    pub runtime_source: String,
}

pub trait PreparedLayoutRuntime: std::fmt::Debug {
    type Config;

    fn prepare_layout(
        &self,
        config: &Self::Config,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayout>, RuntimeError>;

    fn build_context(
        &self,
        state: &StateSnapshot,
        workspace: &WorkspaceSnapshot,
        artifact: Option<&PreparedLayout>,
    ) -> LayoutEvaluationContext;

    fn evaluate_layout(
        &self,
        artifact: &PreparedLayout,
        context: &LayoutEvaluationContext,
    ) -> Result<SourceLayoutNode, RuntimeError>;

    fn contract(&self) -> LayoutModuleContract;
}

pub trait AuthoringLayoutRuntime: PreparedLayoutRuntime {
    fn load_authored_config(&self, path: &Path) -> Result<Self::Config, RuntimeError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub name: String,
}
