use std::path::Path;

use spiders_config::{
    authoring_layout::{
        AuthoringLayoutService, AuthoringLayoutServiceError, PreparedLayoutEvaluation,
    },
    model::{Config, ConfigPaths, LayoutConfigError},
};
use spiders_shared::{
    layout::{SlotTake, SourceLayoutNode},
    runtime::{
        AuthoringLayoutRuntime, LayoutModuleContract, PreparedLayout, PreparedLayoutRuntime,
        RuntimeError, RuntimeRefreshSummary,
    },
    wm::{LayoutEvaluationContext, WorkspaceSnapshot},
};

use crate::config::LayoutTreeSource;

#[cfg(feature = "built-in-layout-runtime")]
#[derive(Debug, Clone, Default)]
pub struct BuiltInLayoutRuntime;

#[cfg(feature = "built-in-layout-runtime")]
pub type BuiltInLayoutService = AuthoringLayoutService<BuiltInLayoutRuntime>;
pub type JsLayoutService = AuthoringLayoutService<spiders_runtime_js::DefaultLayoutRuntime>;

#[derive(Debug)]
pub enum RuntimeLayoutService {
    #[cfg(feature = "built-in-layout-runtime")]
    BuiltIn(BuiltInLayoutService),
    Js(JsLayoutService),
}

impl RuntimeLayoutService {
    #[cfg(feature = "built-in-layout-runtime")]
    pub fn built_in() -> Self {
        Self::BuiltIn(BuiltInLayoutService::new(BuiltInLayoutRuntime))
    }

    pub fn from_paths(paths: &ConfigPaths) -> Self {
        Self::Js(spiders_runtime_js::build_authoring_layout_service(paths))
    }

    pub fn evaluate_prepared_for_workspace(
        &mut self,
        config: &Config,
        state: &spiders_shared::wm::StateSnapshot,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayoutEvaluation>, AuthoringLayoutServiceError> {
        match self {
            #[cfg(feature = "built-in-layout-runtime")]
            Self::BuiltIn(service) => {
                service.evaluate_prepared_for_workspace(config, state, workspace)
            }
            Self::Js(service) => service.evaluate_prepared_for_workspace(config, state, workspace),
        }
    }

    pub fn provenance(&self) -> LayoutTreeSource {
        match self {
            #[cfg(feature = "built-in-layout-runtime")]
            Self::BuiltIn(_) => LayoutTreeSource::BuiltIn,
            Self::Js(_) => LayoutTreeSource::JsRuntime,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            #[cfg(feature = "built-in-layout-runtime")]
            Self::BuiltIn(_) => "built-in",
            Self::Js(_) => "js-runtime",
        }
    }
}

#[cfg(feature = "built-in-layout-runtime")]
impl PreparedLayoutRuntime for BuiltInLayoutRuntime {
    type Config = Config;

    fn prepare_layout(
        &self,
        config: &Self::Config,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayout>, RuntimeError> {
        let selected = config
            .resolve_selected_layout(workspace)
            .map_err(map_config_error)?;

        Ok(selected.map(|selected| PreparedLayout {
            selected,
            runtime_graph: spiders_shared::runtime::JavaScriptModuleGraph {
                entry: "builtin://layout".into(),
                modules: vec![],
            },
        }))
    }

    fn build_context(
        &self,
        state: &spiders_shared::wm::StateSnapshot,
        workspace: &WorkspaceSnapshot,
        artifact: Option<&PreparedLayout>,
    ) -> LayoutEvaluationContext {
        state.layout_context(
            workspace,
            artifact.map(|artifact| artifact.selected.clone()),
        )
    }

    fn evaluate_layout(
        &self,
        _artifact: &PreparedLayout,
        _context: &LayoutEvaluationContext,
    ) -> Result<SourceLayoutNode, RuntimeError> {
        Ok(SourceLayoutNode::Workspace {
            meta: Default::default(),
            children: vec![SourceLayoutNode::Slot {
                meta: Default::default(),
                window_match: None,
                take: SlotTake::Remaining(spiders_shared::layout::RemainingTake::Remaining),
            }],
        })
    }

    fn contract(&self) -> LayoutModuleContract {
        LayoutModuleContract::default()
    }
}

#[cfg(feature = "built-in-layout-runtime")]
impl AuthoringLayoutRuntime for BuiltInLayoutRuntime {
    fn load_authored_config(&self, path: &Path) -> Result<Self::Config, RuntimeError> {
        Config::from_path(path).map_err(map_config_error)
    }

    fn load_prepared_config(&self, path: &Path) -> Result<Self::Config, RuntimeError> {
        Config::from_path(path).map_err(map_config_error)
    }

    fn refresh_prepared_config(
        &self,
        _authored: &Path,
        _runtime: &Path,
    ) -> Result<RuntimeRefreshSummary, RuntimeError> {
        Ok(RuntimeRefreshSummary::default())
    }

    fn rebuild_prepared_config(
        &self,
        _authored: &Path,
        _runtime: &Path,
    ) -> Result<RuntimeRefreshSummary, RuntimeError> {
        Ok(RuntimeRefreshSummary::default())
    }
}

fn map_config_error(error: LayoutConfigError) -> RuntimeError {
    RuntimeError::Config {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "built-in-layout-runtime")]
    use super::{BuiltInLayoutRuntime, RuntimeLayoutService};
    use spiders_config::model::{Config, LayoutDefinition, LayoutSelectionConfig};
    use spiders_shared::{
        ids::{OutputId, WorkspaceId},
        runtime::PreparedLayoutRuntime,
        wm::{LayoutRef, OutputSnapshot, OutputTransform, StateSnapshot, WorkspaceSnapshot},
    };

    #[cfg(feature = "built-in-layout-runtime")]
    #[test]
    fn built_in_runtime_prepares_selected_layout() {
        let runtime = BuiltInLayoutRuntime;
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "columns".into(),
                module: "builtin://columns".into(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                runtime_graph: None,
            }],
            layout_selection: LayoutSelectionConfig {
                default: Some("columns".into()),
                ..LayoutSelectionConfig::default()
            },
            ..Config::default()
        };
        let workspace = WorkspaceSnapshot {
            id: WorkspaceId::from("1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_workspaces: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef {
                name: "columns".into(),
            }),
        };

        let prepared = runtime
            .prepare_layout(&config, &workspace)
            .unwrap()
            .unwrap();

        assert_eq!(prepared.selected.name, "columns");
    }

    #[cfg(feature = "built-in-layout-runtime")]
    #[test]
    fn runtime_layout_service_built_in_evaluates_workspace() {
        let mut service = RuntimeLayoutService::built_in();
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "columns".into(),
                module: "builtin://columns".into(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                runtime_graph: None,
            }],
            layout_selection: LayoutSelectionConfig {
                default: Some("columns".into()),
                ..LayoutSelectionConfig::default()
            },
            ..Config::default()
        };
        let workspace = WorkspaceSnapshot {
            id: WorkspaceId::from("1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_workspaces: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef {
                name: "columns".into(),
            }),
        };
        let state = StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "winit".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 1200,
                logical_height: 700,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("1")),
            }],
            workspaces: vec![workspace.clone()],
            windows: vec![],
            visible_window_ids: vec![],
            workspace_names: vec!["1".into()],
        };

        let evaluation = service
            .evaluate_prepared_for_workspace(&config, &state, &workspace)
            .unwrap()
            .unwrap();

        assert_eq!(evaluation.artifact.selected.name, "columns");
    }
}
