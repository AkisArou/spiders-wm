pub mod actions;
pub mod app;
pub mod backend;
pub mod controller;
pub mod effects;
pub mod host;
pub mod ipc;
pub mod runner;
pub mod runtime;
pub mod scenario;
pub mod script;
pub mod session;
pub mod smithay_adapter;
pub mod smithay_runtime;
pub mod smithay_state;
pub mod smithay_workspace;
pub mod startup;
pub mod titlebar;
pub mod topology;
pub mod transcript;
pub mod wm;

use spiders_config::model::{Config, LayoutConfigError};
use spiders_config::service::{ConfigRuntimeService, ConfigRuntimeServiceError};
use spiders_effects::EffectsCssParseError;
use spiders_layout::ast::{LayoutValidationError, ValidatedLayoutTree};
use spiders_layout::pipeline::{compute_layout_from_request, LayoutPipelineError};
use spiders_shared::layout::{LayoutRequest, LayoutResponse, LayoutSpace, ResolvedLayoutNode};
use spiders_shared::runtime::{AuthoringLayoutRuntime, RuntimeError};
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
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Validation(#[from] LayoutValidationError),
    #[error(transparent)]
    Resolve(#[from] spiders_layout::ast::LayoutResolveError),
    #[error(transparent)]
    Service(#[from] ConfigRuntimeServiceError),
    #[error(transparent)]
    EffectsParse(#[from] EffectsCssParseError),
}

pub trait LayoutEngine {
    fn layout_workspace(
        &self,
        request: &LayoutRequest,
    ) -> Result<LayoutResponse, CompositorLayoutError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LayoutService;

pub use app::CompositorApp;
pub use backend::{
    BackendDiscoveryEvent, BackendSessionReport, BackendSessionState, BackendSnapshotSummary,
    BackendSource, BackendTopologySnapshot,
};
pub use controller::CompositorController;
pub use effects::{
    decoration_visible, resolve_window_effect_style, titlebar_visible,
    window_decoration_policy_for_style, EffectsRuntimeState, WindowDecorationPolicy,
    WindowEffectsState,
};
pub use host::CompositorHost;
pub use ipc::{CompositorIpcError, CompositorIpcHost, IpcPumpReport};
pub use runner::{BootstrapRunner, BootstrapRunnerError};
pub use runtime::{CompositorRuntimeState, WorkspaceLayoutState};
pub use session::{CompositorSession, SessionUpdate};
pub use smithay_adapter::{
    SmithayAdapter, SmithayAdapterEvent, SmithayOutputDescriptor, SmithaySeatDescriptor,
};
#[cfg(feature = "smithay-winit")]
pub use smithay_runtime::{bootstrap_winit, SmithayBootstrap, SmithayWinitRuntime};
#[cfg(feature = "smithay-winit")]
pub use smithay_runtime::{initialize_smithay_workspace_export, initialize_winit_controller};
pub use smithay_runtime::{
    SmithayBootstrapSnapshot, SmithayRuntimeError, SmithayRuntimeSnapshot, SmithayStartupReport,
};
#[cfg(feature = "smithay-winit")]
pub use smithay_state::{
    SmithayClientState, SmithayKnownLayerSurface, SmithayKnownPopupSurface, SmithayKnownSurface,
    SmithayKnownSurfacesSnapshot, SmithayKnownToplevelSurface, SmithayKnownUnmanagedSurface,
    SmithayPopupParentSnapshot, SmithayStateError, SmithayStateSnapshot, SmithaySurfaceRoleCounts,
    SmithayTitlebarRenderSnapshot, SpidersSmithayState,
};
#[cfg(feature = "smithay-winit")]
pub use smithay_workspace::{
    WorkspaceHandler, WorkspaceManagerDebugSnapshot, WorkspaceManagerState,
};
pub use spiders_wm::{
    BootstrapDiagnostics, BootstrapEvent, BootstrapFailureTrace, BootstrapRunTrace,
    BootstrapScenario, BootstrapScript, BootstrapScriptKind, BootstrapTranscript,
    CompositorTopologyState, ControllerCommand, ControllerCommandReport, ControllerPhase,
    ControllerReport, OutputState, SeatState, StartupRegistration, SurfaceRole, SurfaceState,
    TopologyError, WmState, WmStateError,
};
pub use startup::{
    StartupConfig, StartupLayoutState, StartupRuntime, StartupSequence, StartupSession,
};
pub use titlebar::{compute_titlebar_render_plan, TitlebarRenderItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceLayoutSource<'a> {
    pub workspace: &'a WorkspaceSnapshot,
    pub output: Option<&'a OutputSnapshot>,
    pub layout: Option<&'a LayoutRef>,
    pub stylesheet: &'a str,
    pub effects_stylesheet: &'a str,
}

impl LayoutService {
    pub fn initialize_startup_runtime<R: AuthoringLayoutRuntime<Config = Config>>(
        &self,
        service: ConfigRuntimeService<R>,
        config: Config,
        state: &StateSnapshot,
    ) -> Result<StartupRuntime<R>, CompositorLayoutError> {
        startup::initialize_startup_runtime(self, service, config, state)
    }

    pub fn initialize_startup_config<R: AuthoringLayoutRuntime<Config = Config>>(
        &self,
        service: ConfigRuntimeService<R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<StartupConfig<R>, CompositorLayoutError> {
        startup::initialize_startup_config(self, service, config, state)
    }

    pub fn initialize_startup_session<R: AuthoringLayoutRuntime<Config = Config>>(
        &self,
        service: ConfigRuntimeService<R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<StartupSession<R>, CompositorLayoutError> {
        startup::initialize_startup_session(self, service, config, state)
    }

    pub fn initialize_runtime_state<R: AuthoringLayoutRuntime<Config = Config>>(
        &self,
        service: ConfigRuntimeService<R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<CompositorRuntimeState<R>, CompositorLayoutError> {
        runtime::initialize_runtime_state(*self, service, config, state)
    }

    pub fn bootstrap_runtime<R: AuthoringLayoutRuntime<Config = Config>>(
        &self,
        service: &mut ConfigRuntimeService<R>,
        config: &Config,
        state: &StateSnapshot,
    ) -> Result<Option<StartupLayoutState>, CompositorLayoutError> {
        startup::bootstrap_runtime(self, service, config, state)
    }

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
            effects_stylesheet: source.effects_stylesheet.to_owned(),
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

    pub fn evaluate_and_layout_current_workspace<R: AuthoringLayoutRuntime<Config = Config>>(
        &self,
        runtime: &R,
        config: &Config,
        state: &StateSnapshot,
        windows: &[WindowSnapshot],
    ) -> Result<Option<LayoutResponse>, CompositorLayoutError> {
        let Some(workspace) = state.current_workspace() else {
            return Ok(None);
        };
        let Some(loaded_layout) = runtime.prepare_layout(config, workspace)? else {
            return Ok(None);
        };
        let context = runtime.build_context(state, workspace, Some(&loaded_layout));
        let source = runtime.evaluate_layout(&loaded_layout, &context)?;
        let validated = ValidatedLayoutTree::new(source)?;
        let resolved = validated.resolve(windows)?;
        let request = build_request_from_context(context, loaded_layout.selected, resolved.root);

        Ok(Some(compute_layout_from_request(&request)?))
    }
}

pub(crate) fn build_request_from_context(
    context: LayoutEvaluationContext,
    selected_layout: SelectedLayout,
    root: ResolvedLayoutNode,
) -> LayoutRequest {
    LayoutRequest {
        workspace_id: context.workspace_id,
        output_id: context.output.map(|output| output.id),
        layout_name: Some(selected_layout.name),
        root,
        stylesheet: selected_layout.stylesheet,
        effects_stylesheet: selected_layout.effects_stylesheet,
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

    fn workspace_snapshot() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_tags: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(spiders_shared::wm::LayoutRef {
                name: "master-stack".into(),
            }),
        }
    }

    fn output_snapshot(width: u32, height: u32) -> OutputSnapshot {
        OutputSnapshot {
            id: OutputId::from("out-1"),
            name: "HDMI-A-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: width,
            logical_height: height,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
        }
    }

    fn state_snapshot(width: u32, height: u32) -> StateSnapshot {
        StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![output_snapshot(width, height)],
            workspaces: vec![workspace_snapshot()],
            windows: vec![],
            visible_window_ids: vec![],
            tag_names: vec!["1".into()],
        }
    }

    fn layout_config(stylesheet: &str, module: &str) -> Config {
        Config {
            layouts: vec![spiders_config::model::LayoutDefinition {
                name: "master-stack".into(),
                module: module.into(),
                stylesheet: stylesheet.into(),
                effects_stylesheet: String::new(),
                runtime_source: None,
            }],
            ..Config::default()
        }
    }

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
            effects_stylesheet: String::new(),
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
        let workspace = workspace_snapshot();
        let output = output_snapshot(1920, 1080);
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
                effects_stylesheet: "window { appearance: none; }",
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
                effects_stylesheet: "window { appearance: none; }".into(),
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
        let config = layout_config("workspace { display: flex; }", "layouts/master-stack.js");
        let workspace = workspace_snapshot();
        let output = output_snapshot(1600, 900);

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
        let config = layout_config("workspace { display: flex; }", "layouts/master-stack.js");
        let state = state_snapshot(1280, 720);

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
        let runtime = spiders_runtime_js::runtime::BoaLayoutRuntime::with_loader(
            spiders_runtime_js::loader::FsLayoutSourceLoader,
        );
        let config = layout_config(
            "workspace { display: flex; flex-direction: row; width: 800px; height: 600px; } #main { width: 250px; } .rest { flex-grow: 1; }",
            &module_path.to_string_lossy(),
        );
        let mut state = state_snapshot(800, 600);
        state.focused_window_id = Some(WindowId::from("w1"));
        state.visible_window_ids = vec![WindowId::from("w1"), WindowId::from("w2")];
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
                floating_rect: None,
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
                floating_rect: None,
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

    #[test]
    fn layout_service_bootstraps_runtime_service_for_current_workspace() {
        let service = LayoutService;
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-bootstrap-runtime");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let loader = spiders_runtime_js::loader::RuntimeProjectLayoutSourceLoader::new(
            spiders_runtime_js::loader::RuntimePathResolver::new(".", &runtime_root),
        );
        let runtime = spiders_runtime_js::runtime::BoaLayoutRuntime::with_loader(loader.clone());
        let mut runtime_service = spiders_config::service::ConfigRuntimeService::new(runtime);
        let config = layout_config("", "layouts/master-stack.js");
        let state = state_snapshot(800, 600);

        let evaluated = service
            .bootstrap_runtime(&mut runtime_service, &config, &state)
            .unwrap()
            .unwrap();

        assert_eq!(evaluated.workspace_id, WorkspaceId::from("ws-1"));
        assert_eq!(evaluated.evaluated.artifact.selected.name, "master-stack");
        assert_eq!(
            evaluated.request.layout_name.as_deref(),
            Some("master-stack")
        );
        assert_eq!(evaluated.response.root.window_nodes().len(), 1);
        assert!(matches!(
            evaluated.evaluated.layout,
            spiders_shared::layout::SourceLayoutNode::Workspace { .. }
        ));

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn layout_service_initializes_startup_runtime_state() {
        let service = LayoutService;
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-startup-runtime");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let loader = spiders_runtime_js::loader::RuntimeProjectLayoutSourceLoader::new(
            spiders_runtime_js::loader::RuntimePathResolver::new(".", &runtime_root),
        );
        let runtime = spiders_runtime_js::runtime::BoaLayoutRuntime::with_loader(loader.clone());
        let runtime_service = spiders_config::service::ConfigRuntimeService::new(runtime);
        let config = layout_config("", "layouts/master-stack.js");
        let state = state_snapshot(800, 600);

        let startup = service
            .initialize_startup_runtime(runtime_service, config, &state)
            .unwrap();

        assert!(startup.startup_layout.is_some());
        assert_eq!(startup.config.layouts.len(), 1);
        assert_eq!(
            startup
                .startup_layout
                .as_ref()
                .unwrap()
                .request
                .layout_name
                .as_deref(),
            Some("master-stack")
        );
        assert_eq!(
            startup
                .startup_layout
                .as_ref()
                .unwrap()
                .response
                .root
                .window_nodes()
                .len(),
            1
        );

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn layout_service_initializes_startup_config_object() {
        let service = LayoutService;
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-startup-config-runtime");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let loader = spiders_runtime_js::loader::RuntimeProjectLayoutSourceLoader::new(
            spiders_runtime_js::loader::RuntimePathResolver::new(".", &runtime_root),
        );
        let runtime = spiders_runtime_js::runtime::BoaLayoutRuntime::with_loader(loader.clone());
        let runtime_service = spiders_config::service::ConfigRuntimeService::new(runtime);
        let config = layout_config("", "layouts/master-stack.js");
        let state = state_snapshot(800, 600);

        let startup = service
            .initialize_startup_config(runtime_service, config, state)
            .unwrap();

        assert!(startup.runtime.startup_layout.is_some());
        assert_eq!(
            startup
                .runtime
                .startup_layout
                .as_ref()
                .unwrap()
                .request
                .layout_name
                .as_deref(),
            Some("master-stack")
        );
        assert_eq!(
            startup
                .runtime
                .startup_layout
                .as_ref()
                .unwrap()
                .response
                .root
                .window_nodes()
                .len(),
            1
        );
        assert_eq!(
            startup.state.current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn layout_service_initializes_startup_session_object() {
        let service = LayoutService;
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-startup-session-object-runtime");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let loader = spiders_runtime_js::loader::RuntimeProjectLayoutSourceLoader::new(
            spiders_runtime_js::loader::RuntimePathResolver::new(".", &runtime_root),
        );
        let runtime = spiders_runtime_js::runtime::BoaLayoutRuntime::with_loader(loader.clone());
        let runtime_service = spiders_config::service::ConfigRuntimeService::new(runtime);
        let config = layout_config("", "layouts/master-stack.js");
        let state = state_snapshot(800, 600);

        let session = service
            .initialize_startup_session(runtime_service, config, state)
            .unwrap();

        assert_eq!(
            session.startup_workspace_id(),
            Some(&WorkspaceId::from("ws-1"))
        );
        assert_eq!(
            session
                .startup_request()
                .and_then(|request| request.layout_name.as_deref()),
            Some("master-stack")
        );
        assert_eq!(
            session
                .startup_response()
                .map(|response| response.root.window_nodes().len()),
            Some(1)
        );

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn layout_service_initializes_compositor_runtime_state() {
        let service = LayoutService;
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-compositor-runtime-object");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let loader = spiders_runtime_js::loader::RuntimeProjectLayoutSourceLoader::new(
            spiders_runtime_js::loader::RuntimePathResolver::new(".", &runtime_root),
        );
        let runtime = spiders_runtime_js::runtime::BoaLayoutRuntime::with_loader(loader.clone());
        let runtime_service = spiders_config::service::ConfigRuntimeService::new(runtime);
        let config = layout_config("", "layouts/master-stack.js");
        let state = state_snapshot(800, 600);

        let runtime = service
            .initialize_runtime_state(runtime_service, config, state)
            .unwrap();

        assert_eq!(
            runtime.current_workspace_id(),
            Some(&WorkspaceId::from("ws-1"))
        );
        assert_eq!(
            runtime
                .current_layout()
                .and_then(|layout| layout.request.layout_name.as_deref()),
            Some("master-stack")
        );
        assert_eq!(
            runtime
                .current_layout()
                .map(|layout| layout.response.root.window_nodes().len()),
            Some(1)
        );

        let _ = fs::remove_file(module_path);
    }
}
