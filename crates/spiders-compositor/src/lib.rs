use spiders_config::model::{Config, LayoutConfigError};
use spiders_config::runtime::{LayoutRuntime, LayoutRuntimeError};
use spiders_layout::ast::{LayoutValidationError, ValidatedLayoutTree};
use spiders_layout::pipeline::{compute_layout_from_request, LayoutPipelineError};
use spiders_shared::layout::{LayoutRequest, LayoutResponse, LayoutSpace, ResolvedLayoutNode};
use spiders_shared::wm::{
    LayoutEvaluationContext, LayoutRef, OutputSnapshot, SelectedLayout, StateSnapshot,
    WindowSnapshot, WorkspaceSnapshot,
};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CompositorLayoutError {
    #[error(transparent)]
    Pipeline(#[from] LayoutPipelineError),
    #[error(transparent)]
    Config(#[from] LayoutConfigError),
    #[error(transparent)]
    Runtime(#[from] LayoutRuntimeError),
    #[error(transparent)]
    Validation(#[from] LayoutValidationError),
    #[error(transparent)]
    Resolve(#[from] spiders_layout::ast::LayoutResolveError),
}

pub trait LayoutEngine {
    fn layout_workspace(
        &self,
        request: &LayoutRequest,
    ) -> Result<LayoutResponse, CompositorLayoutError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LayoutService;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceLayoutSource<'a> {
    pub workspace: &'a WorkspaceSnapshot,
    pub output: Option<&'a OutputSnapshot>,
    pub layout: Option<&'a LayoutRef>,
    pub stylesheet: &'a str,
}

impl LayoutService {
    pub fn make_request(
        &self,
        source: WorkspaceLayoutSource<'_>,
        root: ResolvedLayoutNode,
    ) -> LayoutRequest {
        LayoutRequest {
            workspace_id: source.workspace.id.clone(),
            output_id: source.output.map(|output| output.id.clone()),
            layout_name: source.layout.map(|layout| layout.name.clone()),
            root,
            stylesheet: source.stylesheet.to_owned(),
            space: LayoutSpace {
                width: source
                    .output
                    .map(|output| output.logical_width as f32)
                    .unwrap_or_default(),
                height: source
                    .output
                    .map(|output| output.logical_height as f32)
                    .unwrap_or_default(),
            },
        }
    }

    pub fn make_request_from_config(
        &self,
        config: &Config,
        workspace: &WorkspaceSnapshot,
        output: Option<&OutputSnapshot>,
        root: ResolvedLayoutNode,
    ) -> Result<LayoutRequest, CompositorLayoutError> {
        Ok(config.build_layout_request(workspace, output, root)?)
    }

    pub fn make_request_from_state(
        &self,
        config: &Config,
        state: &StateSnapshot,
        root: ResolvedLayoutNode,
    ) -> Result<Option<LayoutRequest>, CompositorLayoutError> {
        Ok(config.build_layout_request_from_state(state, root)?)
    }

    pub fn selected_layout_from_config(
        &self,
        config: &Config,
        workspace: &WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, CompositorLayoutError> {
        Ok(config.resolve_selected_layout(workspace)?)
    }

    pub fn evaluate_and_layout_current_workspace<R: LayoutRuntime>(
        &self,
        runtime: &R,
        config: &Config,
        state: &StateSnapshot,
        windows: &[WindowSnapshot],
    ) -> Result<Option<LayoutResponse>, CompositorLayoutError> {
        let Some(workspace) = state.current_workspace() else {
            return Ok(None);
        };
        let Some(selected_layout) = runtime.selected_layout(config, workspace)? else {
            return Ok(None);
        };
        let Some(loaded_layout) = runtime.load_selected_layout(config, workspace)? else {
            return Ok(None);
        };
        let context = runtime.build_context(state, workspace, Some(selected_layout.clone()));
        let source = runtime.evaluate_layout(&loaded_layout, &context)?;
        let validated = ValidatedLayoutTree::new(source)?;
        let resolved = validated.resolve(windows)?;
        let request = build_request_from_context(context, selected_layout, resolved.root);

        Ok(Some(compute_layout_from_request(&request)?))
    }
}

fn build_request_from_context(
    context: LayoutEvaluationContext,
    selected_layout: SelectedLayout,
    root: ResolvedLayoutNode,
) -> LayoutRequest {
    LayoutRequest {
        workspace_id: context.workspace.id,
        output_id: context.output.map(|output| output.id),
        layout_name: Some(selected_layout.name),
        root,
        stylesheet: selected_layout.stylesheet,
        space: context.space,
    }
}

impl LayoutEngine for LayoutService {
    fn layout_workspace(
        &self,
        request: &LayoutRequest,
    ) -> Result<LayoutResponse, CompositorLayoutError> {
        Ok(compute_layout_from_request(request)?)
    }
}

pub fn crate_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use std::fs;

    use spiders_shared::ids::WindowId;
    use spiders_shared::ids::{OutputId, WorkspaceId};
    use spiders_shared::layout::{
        LayoutNodeMeta, LayoutRect, LayoutRequest, LayoutResponse, LayoutSnapshotNode, LayoutSpace,
        ResolvedLayoutNode,
    };
    use spiders_shared::wm::{
        OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    use super::*;

    #[test]
    fn layout_service_exposes_shared_snapshot_boundary() {
        let service = LayoutService;
        let request = LayoutRequest {
            workspace_id: WorkspaceId::from("ws-1"),
            output_id: Some(OutputId::from("out-1")),
            layout_name: None,
            root: ResolvedLayoutNode::Workspace {
                meta: LayoutNodeMeta::default(),
                children: vec![ResolvedLayoutNode::Window {
                    meta: LayoutNodeMeta {
                        id: Some("main".into()),
                        ..LayoutNodeMeta::default()
                    },
                    window_id: Some(WindowId::from("w1")),
                }],
            },
            stylesheet:
                "workspace { display: flex; width: 300px; height: 200px; } #main { width: 120px; }"
                    .into(),
            space: LayoutSpace {
                width: 300.0,
                height: 200.0,
            },
        };

        let response = service.layout_workspace(&request).unwrap();

        assert_eq!(
            response,
            LayoutResponse {
                root: LayoutSnapshotNode::Workspace {
                    meta: LayoutNodeMeta::default(),
                    rect: LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: 300.0,
                        height: 200.0,
                    },
                    children: vec![LayoutSnapshotNode::Window {
                        meta: LayoutNodeMeta {
                            id: Some("main".into()),
                            ..LayoutNodeMeta::default()
                        },
                        rect: LayoutRect {
                            x: 0.0,
                            y: 0.0,
                            width: 120.0,
                            height: 200.0,
                        },
                        window_id: Some(WindowId::from("w1")),
                    }],
                },
            }
        );
    }

    #[test]
    fn layout_service_builds_workspace_scoped_request_from_snapshots() {
        let service = LayoutService;
        let workspace = WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_tags: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(spiders_shared::wm::LayoutRef {
                name: "master-stack".into(),
            }),
        };
        let output = OutputSnapshot {
            id: OutputId::from("out-1"),
            name: "HDMI-A-1".into(),
            logical_width: 1920,
            logical_height: 1080,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
        };
        let root = ResolvedLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![],
        };

        let request = service.make_request(
            WorkspaceLayoutSource {
                workspace: &workspace,
                output: Some(&output),
                layout: workspace.effective_layout.as_ref(),
                stylesheet: "workspace { display: flex; }",
            },
            root.clone(),
        );

        assert_eq!(
            request,
            LayoutRequest {
                workspace_id: WorkspaceId::from("ws-1"),
                output_id: Some(OutputId::from("out-1")),
                layout_name: Some("master-stack".into()),
                root,
                stylesheet: "workspace { display: flex; }".into(),
                space: LayoutSpace {
                    width: 1920.0,
                    height: 1080.0,
                },
            }
        );
    }

    #[test]
    fn layout_service_builds_request_from_config_selection() {
        let service = LayoutService;
        let config = Config {
            layouts: vec![spiders_config::model::LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
            }],
            ..Config::default()
        };
        let workspace = WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_tags: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(spiders_shared::wm::LayoutRef {
                name: "master-stack".into(),
            }),
        };
        let output = OutputSnapshot {
            id: OutputId::from("out-1"),
            name: "HDMI-A-1".into(),
            logical_width: 1600,
            logical_height: 900,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
        };

        let request = service
            .make_request_from_config(
                &config,
                &workspace,
                Some(&output),
                ResolvedLayoutNode::Workspace {
                    meta: LayoutNodeMeta::default(),
                    children: vec![],
                },
            )
            .unwrap();

        assert_eq!(request.layout_name.as_deref(), Some("master-stack"));
        assert_eq!(request.stylesheet, "workspace { display: flex; }");
        assert_eq!(request.space.width, 1600.0);
        assert_eq!(request.space.height, 900.0);
    }

    #[test]
    fn layout_service_builds_request_from_state_snapshot() {
        let service = LayoutService;
        let config = Config {
            layouts: vec![spiders_config::model::LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
            }],
            ..Config::default()
        };
        let state = StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_width: 1280,
                logical_height: 720,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![WorkspaceSnapshot {
                id: WorkspaceId::from("ws-1"),
                name: "1".into(),
                output_id: Some(OutputId::from("out-1")),
                active_tags: vec!["1".into()],
                focused: true,
                visible: true,
                effective_layout: Some(spiders_shared::wm::LayoutRef {
                    name: "master-stack".into(),
                }),
            }],
            windows: vec![],
            visible_window_ids: vec![],
            tag_names: vec!["1".into()],
        };

        let request = service
            .make_request_from_state(
                &config,
                &state,
                ResolvedLayoutNode::Workspace {
                    meta: LayoutNodeMeta::default(),
                    children: vec![],
                },
            )
            .unwrap()
            .unwrap();

        assert_eq!(request.layout_name.as_deref(), Some("master-stack"));
        assert_eq!(request.space.width, 1280.0);
        assert_eq!(request.space.height, 720.0);
    }

    #[test]
    fn layout_service_evaluates_js_layout_and_computes_geometry() {
        let service = LayoutService;
        let temp_dir = std::env::temp_dir();
        let module_path = temp_dir.join("spiders-compositor-layout-test.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main', match: 'app_id=\"firefox\"' }, { type: 'slot', id: 'rest', class: ['rest'] }] })",
        )
        .unwrap();
        let runtime = spiders_config::runtime::BoaLayoutRuntime::with_loader(
            spiders_config::loader::FsLayoutSourceLoader,
        );
        let config = Config {
            layouts: vec![spiders_config::model::LayoutDefinition {
                name: "master-stack".into(),
                module: module_path.to_string_lossy().into_owned(),
                stylesheet: "workspace { display: flex; flex-direction: row; width: 800px; height: 600px; } #main { width: 250px; } .rest { flex-grow: 1; }".into(),
            }],
            ..Config::default()
        };
        let state = StateSnapshot {
            focused_window_id: Some(WindowId::from("w1")),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_width: 800,
                logical_height: 600,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![WorkspaceSnapshot {
                id: WorkspaceId::from("ws-1"),
                name: "1".into(),
                output_id: Some(OutputId::from("out-1")),
                active_tags: vec!["1".into()],
                focused: true,
                visible: true,
                effective_layout: Some(spiders_shared::wm::LayoutRef {
                    name: "master-stack".into(),
                }),
            }],
            windows: vec![],
            visible_window_ids: vec![WindowId::from("w1"), WindowId::from("w2")],
            tag_names: vec!["1".into()],
        };
        let windows = vec![
            WindowSnapshot {
                id: WindowId::from("w1"),
                shell: ShellKind::XdgToplevel,
                app_id: Some("firefox".into()),
                title: Some("Firefox".into()),
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: true,
                floating: false,
                fullscreen: false,
                focused: true,
                urgent: false,
                output_id: Some(OutputId::from("out-1")),
                workspace_id: Some(WorkspaceId::from("ws-1")),
                tags: vec!["1".into()],
            },
            WindowSnapshot {
                id: WindowId::from("w2"),
                shell: ShellKind::XdgToplevel,
                app_id: Some("alacritty".into()),
                title: Some("Terminal".into()),
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: true,
                floating: false,
                fullscreen: false,
                focused: false,
                urgent: false,
                output_id: Some(OutputId::from("out-1")),
                workspace_id: Some(WorkspaceId::from("ws-1")),
                tags: vec!["1".into()],
            },
        ];

        let response = service
            .evaluate_and_layout_current_workspace(&runtime, &config, &state, &windows)
            .unwrap()
            .unwrap();

        let main = response.root.find_by_node_id("main").unwrap();
        let rest = response.root.find_by_node_id("rest").unwrap();

        assert_eq!(main.rect().width, 250.0);
        assert_eq!(rest.rect().x, 250.0);
        assert_eq!(rest.rect().width, 550.0);

        let _ = fs::remove_file(module_path);
    }
}
