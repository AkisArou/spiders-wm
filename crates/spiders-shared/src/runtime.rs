use std::path::Path;

use serde::{Deserialize, Serialize};

use spiders_tree::{LayoutSpace, OutputId, SourceLayoutNode, WindowId, WorkspaceId};
use crate::wm::{OutputSnapshot, StateSnapshot, WorkspaceSnapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedLayout {
    pub name: String,
    pub directory: String,
    pub module: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LayoutEvaluationContext {
    pub monitor: LayoutMonitorContext,
    pub workspace: LayoutWorkspaceContext,
    pub windows: Vec<LayoutWindowContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<LayoutStateContext>,
    #[serde(skip)]
    pub workspace_id: WorkspaceId,
    #[serde(skip)]
    pub output: Option<OutputSnapshot>,
    #[serde(skip)]
    pub selected_layout_name: Option<String>,
    #[serde(skip)]
    pub space: LayoutSpace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutMonitorContext {
    pub name: String,
    pub width: u32,
    pub height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutWorkspaceContext {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspaces: Vec<String>,
    #[serde(rename = "windowCount")]
    pub window_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutWindowContext {
    pub id: WindowId,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub class: Option<String>,
    pub instance: Option<String>,
    pub role: Option<String>,
    pub shell: Option<String>,
    pub window_type: Option<String>,
    pub floating: bool,
    pub fullscreen: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutStateContext {
    pub focused_window_id: Option<WindowId>,
    pub current_output_id: Option<OutputId>,
    pub current_workspace_id: Option<WorkspaceId>,
    pub visible_window_ids: Vec<WindowId>,
    pub workspace_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_layout_name: Option<String>,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeRefreshSummary {
    pub refreshed_files: usize,
    pub pruned_files: usize,
}

impl RuntimeRefreshSummary {
    pub fn is_noop(self) -> bool {
        self.refreshed_files == 0 && self.pruned_files == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedLayout {
    pub selected: SelectedLayout,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub runtime_payload: serde_json::Value,
    #[serde(default)]
    pub stylesheets: PreparedStylesheets,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedStylesheet {
    pub path: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PreparedStylesheets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<PreparedStylesheet>,
}

impl PreparedStylesheets {
    pub fn combined_source(&self) -> String {
        self.layout
            .as_ref()
            .map(|stylesheet| stylesheet.source.clone())
            .unwrap_or_default()
    }
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
    fn load_prepared_config(&self, path: &Path) -> Result<Self::Config, RuntimeError>;
    fn refresh_prepared_config(
        &self,
        authored: &Path,
        runtime: &Path,
    ) -> Result<RuntimeRefreshSummary, RuntimeError>;
    fn rebuild_prepared_config(
        &self,
        authored: &Path,
        runtime: &Path,
    ) -> Result<RuntimeRefreshSummary, RuntimeError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub name: String,
}
