use spiders_config::model::Config;
use spiders_config::service::AuthoringLayoutService;
use spiders_layout::ast::ValidatedLayoutTree;
use spiders_layout::pipeline::compute_layout_from_request;
use spiders_shared::ids::WorkspaceId;
use spiders_shared::layout::{LayoutRequest, LayoutResponse};
use spiders_shared::runtime::AuthoringLayoutRuntime;
use spiders_shared::wm::StateSnapshot;

use crate::effects::EffectsRuntimeState;
use crate::{build_request_from_context, CompositorLayoutError, LayoutService};

#[derive(Debug)]
pub struct StartupRuntime<R> {
    pub config: Config,
    pub service: AuthoringLayoutService<R>,
    pub startup_layout: Option<StartupLayoutState>,
}

#[derive(Debug)]
pub struct StartupConfig<R> {
    pub runtime: StartupRuntime<R>,
    pub state: StateSnapshot,
}

#[derive(Debug)]
pub struct StartupSession<R> {
    pub runtime: StartupRuntime<R>,
    pub state: StateSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartupLayoutState {
    pub evaluated: spiders_config::service::PreparedLayoutEvaluation,
    pub workspace_id: WorkspaceId,
    pub request: LayoutRequest,
    pub response: LayoutResponse,
    pub effects: EffectsRuntimeState,
}

#[derive(Debug)]
pub struct StartupSequence<R> {
    pub service: LayoutService,
    pub runtime_service: AuthoringLayoutService<R>,
    pub config: Config,
    pub state: StateSnapshot,
}

impl<R> StartupConfig<R> {
    pub fn into_session(self) -> StartupSession<R> {
        StartupSession {
            runtime: self.runtime,
            state: self.state,
        }
    }
}

impl<R> StartupSession<R> {
    pub fn startup_layout(&self) -> Option<&StartupLayoutState> {
        self.runtime.startup_layout.as_ref()
    }

    pub fn startup_request(&self) -> Option<&LayoutRequest> {
        self.startup_layout().map(|layout| &layout.request)
    }

    pub fn startup_response(&self) -> Option<&LayoutResponse> {
        self.startup_layout().map(|layout| &layout.response)
    }

    pub fn startup_workspace_id(&self) -> Option<&WorkspaceId> {
        self.startup_layout().map(|layout| &layout.workspace_id)
    }
}

impl<R: AuthoringLayoutRuntime<Config = Config>> StartupSequence<R> {
    pub fn new(
        service: LayoutService,
        runtime_service: AuthoringLayoutService<R>,
        config: Config,
        state: StateSnapshot,
    ) -> Self {
        Self {
            service,
            runtime_service,
            config,
            state,
        }
    }

    pub fn bootstrap(self) -> Result<StartupConfig<R>, CompositorLayoutError> {
        self.service
            .initialize_startup_config(self.runtime_service, self.config, self.state)
    }
}

pub(crate) fn bootstrap_runtime<R: AuthoringLayoutRuntime<Config = Config>>(
    _service: &LayoutService,
    runtime_service: &mut AuthoringLayoutService<R>,
    config: &Config,
    state: &StateSnapshot,
) -> Result<Option<StartupLayoutState>, CompositorLayoutError> {
    let Some(workspace) = state.current_workspace() else {
        return Ok(None);
    };

    Ok(runtime_service
        .evaluate_prepared_for_workspace(config, state, workspace)?
        .map(
            |evaluated| -> Result<StartupLayoutState, CompositorLayoutError> {
                let workspace_windows = state.windows_for_workspace(workspace);
                let validated = ValidatedLayoutTree::new(evaluated.layout.clone())?;
                let resolved = validated.resolve(&workspace_windows)?;
                let request = build_request_from_context(
                    evaluated.context.clone(),
                    evaluated.artifact.selected.clone(),
                    resolved.root,
                );
                let response = compute_layout_from_request(&request)?;
                let mut effects =
                    EffectsRuntimeState::from_stylesheet(&request.effects_stylesheet)?;
                effects.recompute_for_workspace(state, workspace);

                Ok(StartupLayoutState {
                    workspace_id: workspace.id.clone(),
                    evaluated,
                    request,
                    response,
                    effects,
                })
            },
        )
        .transpose()?)
}

pub(crate) fn initialize_startup_runtime<R: AuthoringLayoutRuntime<Config = Config>>(
    service: &LayoutService,
    mut runtime_service: AuthoringLayoutService<R>,
    config: Config,
    state: &StateSnapshot,
) -> Result<StartupRuntime<R>, CompositorLayoutError> {
    let startup_layout = bootstrap_runtime(service, &mut runtime_service, &config, state)?;

    Ok(StartupRuntime {
        config,
        service: runtime_service,
        startup_layout,
    })
}

pub(crate) fn initialize_startup_config<R: AuthoringLayoutRuntime<Config = Config>>(
    service: &LayoutService,
    runtime_service: AuthoringLayoutService<R>,
    config: Config,
    state: StateSnapshot,
) -> Result<StartupConfig<R>, CompositorLayoutError> {
    let runtime = initialize_startup_runtime(service, runtime_service, config, &state)?;

    Ok(StartupConfig { runtime, state })
}

pub(crate) fn initialize_startup_session<R: AuthoringLayoutRuntime<Config = Config>>(
    service: &LayoutService,
    runtime_service: AuthoringLayoutService<R>,
    config: Config,
    state: StateSnapshot,
) -> Result<StartupSession<R>, CompositorLayoutError> {
    Ok(initialize_startup_config(service, runtime_service, config, state)?.into_session())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use spiders_config::service::AuthoringLayoutService;
    use spiders_runtime_js::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_runtime_js::runtime::BoaPreparedLayoutRuntime;
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    use super::*;

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_x: 0,
                logical_y: 0,
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
            visible_window_ids: vec![],
            tag_names: vec!["1".into()],
        }
    }

    fn config() -> Config {
        Config {
            layouts: vec![spiders_config::model::LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                runtime_source: None,
            }],
            ..Config::default()
        }
    }

    #[test]
    fn startup_sequence_bootstraps_startup_config() {
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-startup-sequence-runtime");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaPreparedLayoutRuntime::with_loader(loader.clone());
        let runtime_service = AuthoringLayoutService::new(runtime);
        let sequence = StartupSequence::new(LayoutService, runtime_service, config(), state());

        let startup = sequence.bootstrap().unwrap();

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

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn startup_config_converts_into_session_with_layout_accessors() {
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-startup-session-runtime");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaPreparedLayoutRuntime::with_loader(loader.clone());
        let runtime_service = AuthoringLayoutService::new(runtime);

        let session =
            initialize_startup_session(&LayoutService, runtime_service, config(), state()).unwrap();

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
        assert_eq!(
            session.state.current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn startup_runtime_filters_resolution_to_current_workspace_windows() {
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-startup-window-filter-runtime");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'visible' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = BoaPreparedLayoutRuntime::with_loader(loader.clone());
        let mut runtime_service = AuthoringLayoutService::new(runtime);
        let config = config();
        let state = StateSnapshot {
            focused_window_id: Some(WindowId::from("w1")),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_x: 0,
                logical_y: 0,
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
            windows: vec![
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
                    app_id: Some("discord".into()),
                    title: Some("Discord".into()),
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
                    output_id: Some(OutputId::from("out-2")),
                    workspace_id: Some(WorkspaceId::from("ws-2")),
                    tags: vec!["2".into()],
                },
            ],
            visible_window_ids: vec![WindowId::from("w1")],
            tag_names: vec!["1".into(), "2".into()],
        };

        let startup = bootstrap_runtime(&LayoutService, &mut runtime_service, &config, &state)
            .unwrap()
            .unwrap();

        assert_eq!(startup.response.root.window_nodes().len(), 1);
        assert!(startup
            .response
            .root
            .find_by_window_id(&WindowId::from("w1"))
            .is_some());
        assert!(startup
            .response
            .root
            .find_by_window_id(&WindowId::from("w2"))
            .is_none());

        let _ = fs::remove_file(module_path);
    }
}
