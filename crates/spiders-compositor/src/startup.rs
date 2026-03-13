use spiders_config::model::Config;
use spiders_config::runtime::LayoutRuntime;
use spiders_config::service::ConfigRuntimeService;
use spiders_layout::ast::ValidatedLayoutTree;
use spiders_layout::pipeline::compute_layout_from_request;
use spiders_shared::ids::WorkspaceId;
use spiders_shared::layout::{LayoutRequest, LayoutResponse};
use spiders_shared::wm::StateSnapshot;

use crate::{build_request_from_context, CompositorLayoutError, LayoutService};

#[derive(Debug)]
pub struct StartupRuntime<L, R> {
    pub config: Config,
    pub service: ConfigRuntimeService<L, R>,
    pub startup_layout: Option<StartupLayoutState>,
}

#[derive(Debug)]
pub struct StartupConfig<L, R> {
    pub runtime: StartupRuntime<L, R>,
    pub state: StateSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartupLayoutState {
    pub evaluated: spiders_config::service::EvaluatedLayout,
    pub workspace_id: WorkspaceId,
    pub request: LayoutRequest,
    pub response: LayoutResponse,
}

#[derive(Debug)]
pub struct StartupSequence<L, R> {
    pub service: LayoutService,
    pub runtime_service: ConfigRuntimeService<L, R>,
    pub config: Config,
    pub state: StateSnapshot,
}

impl<L: spiders_config::loader::LayoutSourceLoader, R: LayoutRuntime> StartupSequence<L, R> {
    pub fn new(
        service: LayoutService,
        runtime_service: ConfigRuntimeService<L, R>,
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

    pub fn bootstrap(self) -> Result<StartupConfig<L, R>, CompositorLayoutError> {
        self.service
            .initialize_startup_config(self.runtime_service, self.config, self.state)
    }
}

pub(crate) fn bootstrap_runtime<L: spiders_config::loader::LayoutSourceLoader, R: LayoutRuntime>(
    _service: &LayoutService,
    runtime_service: &mut ConfigRuntimeService<L, R>,
    config: &Config,
    state: &StateSnapshot,
) -> Result<Option<StartupLayoutState>, CompositorLayoutError> {
    let Some(workspace) = state.current_workspace() else {
        return Ok(None);
    };

    Ok(runtime_service
        .evaluate_for_workspace(config, state, workspace)?
        .map(
            |evaluated| -> Result<StartupLayoutState, CompositorLayoutError> {
                let validated = ValidatedLayoutTree::new(evaluated.layout.clone())?;
                let resolved = validated.resolve(&state.windows)?;
                let request = build_request_from_context(
                    evaluated.context.clone(),
                    evaluated.loaded.selected.clone(),
                    resolved.root,
                );
                let response = compute_layout_from_request(&request)?;

                Ok(StartupLayoutState {
                    workspace_id: workspace.id.clone(),
                    evaluated,
                    request,
                    response,
                })
            },
        )
        .transpose()?)
}

pub(crate) fn initialize_startup_runtime<
    L: spiders_config::loader::LayoutSourceLoader,
    R: LayoutRuntime,
>(
    service: &LayoutService,
    mut runtime_service: ConfigRuntimeService<L, R>,
    config: Config,
    state: &StateSnapshot,
) -> Result<StartupRuntime<L, R>, CompositorLayoutError> {
    let startup_layout = bootstrap_runtime(service, &mut runtime_service, &config, state)?;

    Ok(StartupRuntime {
        config,
        service: runtime_service,
        startup_layout,
    })
}

pub(crate) fn initialize_startup_config<
    L: spiders_config::loader::LayoutSourceLoader,
    R: LayoutRuntime,
>(
    service: &LayoutService,
    runtime_service: ConfigRuntimeService<L, R>,
    config: Config,
    state: StateSnapshot,
) -> Result<StartupConfig<L, R>, CompositorLayoutError> {
    let runtime = initialize_startup_runtime(service, runtime_service, config, &state)?;

    Ok(StartupConfig { runtime, state })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use spiders_config::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_config::runtime::BoaLayoutRuntime;
    use spiders_config::service::ConfigRuntimeService;
    use spiders_shared::ids::{OutputId, WorkspaceId};
    use spiders_shared::wm::{OutputSnapshot, OutputTransform, StateSnapshot, WorkspaceSnapshot};

    use super::*;

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: None,
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
        let runtime = BoaLayoutRuntime::with_loader(loader.clone());
        let runtime_service = ConfigRuntimeService::new(loader, runtime);
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
}
