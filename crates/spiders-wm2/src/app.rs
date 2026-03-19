use crate::{
    actions::workspace::ensure_workspace,
    bindings::SmithayBindings,
    config::{ConfigRuntimeState, ConfigSource},
    layout::LayoutState,
    model::{TopologyState, WmState, WorkspaceId},
};
use spiders_config::authoring_layout::PreparedLayoutEvaluation;
use spiders_config::model::{Config, LayoutConfigError};
use std::path::Path;

#[derive(Debug, Default)]
pub struct AppState {
    pub topology: TopologyState,
    pub layout: LayoutState,
    pub config_runtime: ConfigRuntimeState,
    pub wm: WmState,
    pub bindings: SmithayBindings,
}

impl AppState {
    pub fn apply_config(&mut self, config: Config, source: ConfigSource) {
        let workspace_names = config.workspaces.clone();
        self.config_runtime.replace(config, source);

        for workspace_name in workspace_names {
            ensure_workspace(&mut self.wm, WorkspaceId::from(workspace_name));
        }
    }

    pub fn load_config_from_path(
        &mut self,
        path: impl AsRef<Path>,
        source: ConfigSource,
    ) -> Result<(), LayoutConfigError> {
        let config = Config::from_path(path)?;
        self.apply_config(config, source);
        Ok(())
    }

    pub fn apply_prepared_layout_evaluation(&mut self, evaluation: PreparedLayoutEvaluation) {
        self.config_runtime
            .install_layout_tree(evaluation.artifact.selected.name, evaluation.layout);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::config::built_in_default_config;
    use spiders_shared::{
        layout::SourceLayoutNode,
        runtime::{JavaScriptModuleGraph, PreparedLayout},
        wm::{
            LayoutEvaluationContext, LayoutMonitorContext, LayoutWorkspaceContext, SelectedLayout,
        },
    };

    #[test]
    fn apply_config_tracks_runtime_and_workspace_catalog() {
        let mut app = AppState::default();
        let mut config = built_in_default_config();
        config.workspaces = vec!["alpha".into(), "beta".into()];

        app.apply_config(config, ConfigSource::PreparedConfig);

        assert_eq!(app.config_runtime.revision(), 1);
        assert_eq!(app.config_runtime.source(), ConfigSource::PreparedConfig);
        assert!(app.wm.workspaces.contains_key(&WorkspaceId::from("alpha")));
        assert!(app.wm.workspaces.contains_key(&WorkspaceId::from("beta")));
    }

    #[test]
    fn load_config_from_path_applies_authored_source() {
        let mut app = AppState::default();
        let path = std::env::temp_dir().join("spiders-wm2-config-test.json");

        fs::write(
            &path,
            r#"{
                "workspaces": ["dev"],
                "layouts": [],
                "layout_selection": {"default": null, "per_workspace": [], "per_monitor": {}}
            }"#,
        )
        .unwrap();

        app.load_config_from_path(&path, ConfigSource::AuthoredConfig)
            .unwrap();

        assert_eq!(app.config_runtime.source(), ConfigSource::AuthoredConfig);
        assert!(app.wm.workspaces.contains_key(&WorkspaceId::from("dev")));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn apply_prepared_layout_evaluation_installs_layout_tree() {
        let mut app = AppState::default();

        app.apply_prepared_layout_evaluation(PreparedLayoutEvaluation {
            artifact: PreparedLayout {
                selected: SelectedLayout {
                    name: "columns".into(),
                    module: "layouts/columns.js".into(),
                    stylesheet: String::new(),
                    effects_stylesheet: String::new(),
                },
                runtime_graph: JavaScriptModuleGraph {
                    entry: "layouts/columns.js".into(),
                    modules: vec![],
                },
            },
            context: LayoutEvaluationContext {
                monitor: LayoutMonitorContext {
                    name: "winit".into(),
                    width: 1200,
                    height: 700,
                    scale: None,
                },
                workspace: LayoutWorkspaceContext {
                    name: "1".into(),
                    workspaces: vec!["1".into()],
                    window_count: 0,
                },
                windows: vec![],
                state: None,
                workspace_id: WorkspaceId::from("1"),
                output: None,
                selected_layout_name: Some("columns".into()),
                space: spiders_shared::layout::LayoutSpace {
                    width: 1200.0,
                    height: 700.0,
                },
            },
            layout: SourceLayoutNode::Workspace {
                meta: Default::default(),
                children: vec![],
            },
        });

        assert!(app.config_runtime.layout_tree("columns").is_some());
        assert_eq!(app.config_runtime.layout_tree_revision(), 1);
    }
}
