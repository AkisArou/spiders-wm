use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::Config;
use spiders_shared::ids::{WindowId, WorkspaceId};
use spiders_shared::layout::{LayoutRect, LayoutRequest, LayoutResponse};
use spiders_shared::runtime::AuthoringLayoutRuntime;
use spiders_shared::wm::StateSnapshot;

use crate::effects::EffectsRuntimeState;
use crate::startup::{self, StartupLayoutState, StartupSession};
use crate::titlebar::{compute_titlebar_render_plan, TitlebarRenderItem};
use crate::{CompositorLayoutError, LayoutService};

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceLayoutState {
    pub workspace_id: WorkspaceId,
    pub request: LayoutRequest,
    pub response: LayoutResponse,
    pub effects: EffectsRuntimeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowPlacementMode {
    Tiled,
    Floating,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowPlacement {
    pub window_id: WindowId,
    pub mode: WindowPlacementMode,
    pub rect: LayoutRect,
}

#[derive(Debug)]
pub struct CompositorRuntimeState<R> {
    pub layout_service: LayoutService,
    pub startup: StartupSession<R>,
    pub current_layout: Option<WorkspaceLayoutState>,
}

impl WorkspaceLayoutState {
    pub fn from_startup(layout: &StartupLayoutState) -> Self {
        Self {
            workspace_id: layout.workspace_id.clone(),
            request: layout.request.clone(),
            response: layout.response.clone(),
            effects: layout.effects.clone(),
        }
    }
}

impl<R> CompositorRuntimeState<R> {
    pub fn from_startup(layout_service: LayoutService, startup: StartupSession<R>) -> Self {
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

    pub fn current_titlebar_render_plan(&self) -> Vec<TitlebarRenderItem> {
        self.current_layout()
            .map(|layout| {
                let placements = self.current_window_placements();
                compute_titlebar_render_plan(self.state(), layout, &placements)
            })
            .unwrap_or_default()
    }

    pub fn current_window_placements(&self) -> Vec<WindowPlacement> {
        self.current_layout()
            .map(|layout| compute_window_placements(self.state(), layout))
            .unwrap_or_default()
    }

    pub fn state(&self) -> &StateSnapshot {
        &self.startup.state
    }

    pub fn config(&self) -> &Config {
        &self.startup.runtime.config
    }

    pub fn startup_session(&self) -> &StartupSession<R> {
        &self.startup
    }

    pub fn update_from_wm_state(&mut self, state: StateSnapshot) {
        self.startup.state = state;
    }
}

impl<R: AuthoringLayoutRuntime<Config = Config>> CompositorRuntimeState<R> {
    pub fn reload_config(&mut self) -> Result<(), CompositorLayoutError> {
        let config = self.startup.runtime.service.reload_config()?;
        self.startup.runtime.config = config;
        self.recompute_current_layout()
    }

    pub fn recompute_current_layout(&mut self) -> Result<(), CompositorLayoutError> {
        let startup_layout = startup::bootstrap_runtime(
            &self.layout_service,
            &mut self.startup.runtime.service,
            &self.startup.runtime.config,
            &self.startup.state,
        )?;

        self.current_layout = startup_layout
            .as_ref()
            .map(WorkspaceLayoutState::from_startup);
        self.startup.runtime.startup_layout = startup_layout;
        Ok(())
    }

    pub fn window_decoration_policy(
        &self,
        window_id: &spiders_shared::ids::WindowId,
    ) -> Option<crate::effects::WindowDecorationPolicy> {
        self.current_layout
            .as_ref()
            .and_then(|layout| layout.effects.window_decoration_policy(window_id))
    }
}

pub(crate) fn initialize_runtime_state<R: AuthoringLayoutRuntime<Config = Config>>(
    layout_service: LayoutService,
    authoring_layout_service: AuthoringLayoutService<R>,
    config: Config,
    state: StateSnapshot,
) -> Result<CompositorRuntimeState<R>, CompositorLayoutError> {
    let startup = startup::initialize_startup_session(
        &layout_service,
        authoring_layout_service,
        config,
        state,
    )?;

    Ok(CompositorRuntimeState::from_startup(
        layout_service,
        startup,
    ))
}

pub fn compute_window_placements(
    state: &StateSnapshot,
    layout: &WorkspaceLayoutState,
) -> Vec<WindowPlacement> {
    layout
        .response
        .root
        .window_nodes()
        .into_iter()
        .filter_map(|node| {
            let spiders_shared::layout::LayoutSnapshotNode::Window {
                window_id: Some(window_id),
                ..
            } = node
            else {
                return None;
            };

            let window = state
                .windows
                .iter()
                .find(|window| window.id == *window_id)?;
            let mode = if window.floating {
                WindowPlacementMode::Floating
            } else {
                WindowPlacementMode::Tiled
            };
            let rect = match mode {
                WindowPlacementMode::Tiled => node.rect(),
                WindowPlacementMode::Floating => {
                    window.floating_rect.unwrap_or_else(|| node.rect())
                }
            };

            Some(WindowPlacement {
                window_id: window_id.clone(),
                mode,
                rect,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use spiders_config::authoring_layout::AuthoringLayoutService;
    use spiders_runtime_js::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_runtime_js::runtime::QuickJsPreparedLayoutRuntime;
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
                runtime_graph: None,
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
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let authoring_layout_service = AuthoringLayoutService::new(runtime);

        let runtime =
            initialize_runtime_state(LayoutService, authoring_layout_service, config(), state())
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
        assert_eq!(
            runtime.state().current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn runtime_state_carries_effects_for_current_layout() {
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-compositor-runtime-effects-state");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let authoring_layout_service = AuthoringLayoutService::new(runtime);
        let mut config = config();
        config.layouts[0].effects_stylesheet =
            "window { appearance: none; } window::titlebar { background: #111; }".into();
        let mut snapshot = state();
        snapshot.windows.push(spiders_shared::wm::WindowSnapshot {
            id: spiders_shared::ids::WindowId::from("w1"),
            shell: spiders_shared::wm::ShellKind::XdgToplevel,
            app_id: Some("foot".into()),
            title: Some("shell".into()),
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
        });
        snapshot.visible_window_ids = vec![spiders_shared::ids::WindowId::from("w1")];

        let runtime =
            initialize_runtime_state(LayoutService, authoring_layout_service, config, snapshot)
                .unwrap();
        let effects = &runtime.current_layout().unwrap().effects;

        assert!(effects
            .window_style(&spiders_shared::ids::WindowId::from("w1"))
            .is_some());

        let policy = runtime
            .window_decoration_policy(&spiders_shared::ids::WindowId::from("w1"))
            .unwrap();
        assert!(!policy.decorations_visible);

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn runtime_state_exposes_current_titlebar_render_plan() {
        let temp_dir = std::env::temp_dir();
        let runtime_root = temp_dir.join("spiders-compositor-runtime-titlebar-plan");
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        let module_path = runtime_root.join("layouts/master-stack.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let authoring_layout_service = AuthoringLayoutService::new(runtime);
        let mut config = config();
        config.layouts[0].effects_stylesheet =
            "window::titlebar { background: #111; height: 30px; }".into();
        let mut snapshot = state();
        snapshot.windows.push(spiders_shared::wm::WindowSnapshot {
            id: spiders_shared::ids::WindowId::from("w1"),
            shell: spiders_shared::wm::ShellKind::XdgToplevel,
            app_id: Some("foot".into()),
            title: Some("shell".into()),
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
        });
        snapshot.visible_window_ids = vec![spiders_shared::ids::WindowId::from("w1")];

        let runtime =
            initialize_runtime_state(LayoutService, authoring_layout_service, config, snapshot)
                .unwrap();
        let plan = runtime.current_titlebar_render_plan();

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].title, "shell");
        assert_eq!(plan[0].style.background.as_deref(), Some("#111"));

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn compute_window_placements_uses_floating_geometry_as_first_class_mode() {
        let mut snapshot = state();
        snapshot.windows.push(spiders_shared::wm::WindowSnapshot {
            id: spiders_shared::ids::WindowId::from("w1"),
            shell: spiders_shared::wm::ShellKind::XdgToplevel,
            app_id: Some("foot".into()),
            title: Some("shell".into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            floating: true,
            floating_rect: Some(spiders_shared::layout::LayoutRect {
                x: 220.0,
                y: 140.0,
                width: 640.0,
                height: 480.0,
            }),
            fullscreen: false,
            focused: true,
            urgent: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(WorkspaceId::from("ws-1")),
            tags: vec!["1".into()],
        });
        let layout = WorkspaceLayoutState {
            workspace_id: WorkspaceId::from("ws-1"),
            request: LayoutRequest {
                workspace_id: WorkspaceId::from("ws-1"),
                output_id: Some(OutputId::from("out-1")),
                layout_name: Some("master-stack".into()),
                root: spiders_shared::layout::ResolvedLayoutNode::Workspace {
                    meta: spiders_shared::layout::LayoutNodeMeta::default(),
                    children: vec![spiders_shared::layout::ResolvedLayoutNode::Window {
                        meta: spiders_shared::layout::LayoutNodeMeta::default(),
                        window_id: Some(spiders_shared::ids::WindowId::from("w1")),
                    }],
                },
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                space: spiders_shared::layout::LayoutSpace {
                    width: 800.0,
                    height: 600.0,
                },
            },
            response: LayoutResponse {
                root: spiders_shared::layout::LayoutSnapshotNode::Workspace {
                    meta: spiders_shared::layout::LayoutNodeMeta::default(),
                    rect: spiders_shared::layout::LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: 800.0,
                        height: 600.0,
                    },
                    children: vec![spiders_shared::layout::LayoutSnapshotNode::Window {
                        meta: spiders_shared::layout::LayoutNodeMeta::default(),
                        rect: spiders_shared::layout::LayoutRect {
                            x: 10.0,
                            y: 20.0,
                            width: 400.0,
                            height: 300.0,
                        },
                        window_id: Some(spiders_shared::ids::WindowId::from("w1")),
                    }],
                },
            },
            effects: EffectsRuntimeState::default(),
        };

        let placements = compute_window_placements(&snapshot, &layout);

        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].mode, WindowPlacementMode::Floating);
        assert_eq!(placements[0].rect.x, 220.0);
        assert_eq!(placements[0].rect.y, 140.0);
    }
}
