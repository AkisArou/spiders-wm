use spiders_config::model::Config;
use spiders_config::runtime::LayoutRuntime;
use spiders_config::service::ConfigRuntimeService;
use spiders_shared::ids::WorkspaceId;
use spiders_shared::layout::{LayoutRequest, LayoutResponse};
use spiders_shared::wm::StateSnapshot;

use crate::startup::{self, StartupLayoutState, StartupSession};
use crate::{CompositorLayoutError, LayoutService};

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceLayoutState {
    pub workspace_id: WorkspaceId,
    pub request: LayoutRequest,
    pub response: LayoutResponse,
}

#[derive(Debug)]
pub struct CompositorRuntimeState<L, R> {
    pub layout_service: LayoutService,
    pub startup: StartupSession<L, R>,
    pub current_layout: Option<WorkspaceLayoutState>,
}

impl WorkspaceLayoutState {
    pub fn from_startup(layout: &StartupLayoutState) -> Self {
        Self {
            workspace_id: layout.workspace_id.clone(),
            request: layout.request.clone(),
            response: layout.response.clone(),
        }
    }
}

impl<L, R> CompositorRuntimeState<L, R> {
    pub fn from_startup(layout_service: LayoutService, startup: StartupSession<L, R>) -> Self {
        let current_layout = startup
            .startup_layout()
            .map(WorkspaceLayoutState::from_startup);

        Self {
            layout_service,
            startup,
            current_layout,
        }
    }

    pub fn current_layout(&self) -> Option<&WorkspaceLayoutState> {
        self.current_layout.as_ref()
    }

    pub fn current_workspace_id(&self) -> Option<&WorkspaceId> {
        self.current_layout().map(|layout| &layout.workspace_id)
    }

    pub fn state(&self) -> &StateSnapshot {
        &self.startup.state
    }

    pub fn startup_session(&self) -> &StartupSession<L, R> {
        &self.startup
    }
}

pub(crate) fn initialize_runtime_state<
    L: spiders_config::loader::LayoutSourceLoader,
    R: LayoutRuntime,
>(
    layout_service: LayoutService,
    runtime_service: ConfigRuntimeService<L, R>,
    config: Config,
    state: StateSnapshot,
) -> Result<CompositorRuntimeState<L, R>, CompositorLayoutError> {
    let startup =
        startup::initialize_startup_session(&layout_service, runtime_service, config, state)?;

    Ok(CompositorRuntimeState::from_startup(
        layout_service,
        startup,
    ))
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
    fn initializes_runtime_state_with_current_workspace_layout() {
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-compositor-runtime-state");
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

        let runtime =
            initialize_runtime_state(LayoutService, runtime_service, config(), state()).unwrap();

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
        assert_eq!(
            runtime.state().current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );

        let _ = fs::remove_file(module_path);
    }
}
