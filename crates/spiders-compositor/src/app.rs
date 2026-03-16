use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::Config;
use spiders_shared::ids::{OutputId, WindowId};
use spiders_shared::runtime::AuthoringLayoutRuntime;
use spiders_shared::wm::{ShellKind, StateSnapshot, WindowSnapshot};
use spiders_wm::{BootstrapEvent, LayerSurfaceMetadata, StartupRegistration};

use crate::actions::ActionError;
use crate::session::CompositorSession;
use crate::topology::{CompositorTopologyState, SurfaceState, TopologyError};
use crate::WindowDecorationPolicy;
use crate::{CompositorLayoutError, LayoutService};

fn append_winit_debug_log(message: &str) {
    let Some(path) = std::env::var_os("SPIDERS_WM_WINIT_DEBUG_LOG_PATH") else {
        return;
    };

    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| {
            use std::io::Write;
            writeln!(file, "{message}")
        });
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeBootstrapDebug {
    pub existing_window_registrations: usize,
    pub new_window_mappings: usize,
    pub last_mapped_window_ids: Vec<WindowId>,
    pub last_state_window_count: usize,
    pub last_visible_window_count: usize,
    pub last_bootstrap_event: Option<String>,
    pub last_bootstrap_event_windows_before: usize,
    pub last_bootstrap_event_windows_after: usize,
    pub last_session_generation: u64,
}

#[derive(Debug)]
pub struct CompositorApp<R> {
    pub session: CompositorSession<R>,
    pub startup: StartupRegistration,
    pub runtime_bootstrap_debug: RuntimeBootstrapDebug,
}

impl<R> CompositorApp<R> {
    pub fn session(&self) -> &CompositorSession<R> {
        &self.session
    }

    pub fn topology(&self) -> &CompositorTopologyState {
        self.session.topology()
    }

    pub fn state(&self) -> &StateSnapshot {
        self.session.state()
    }

    pub fn window_decoration_policies(&self) -> Vec<(WindowId, WindowDecorationPolicy)> {
        self.session.window_decoration_policies()
    }

    pub fn register_popup_surface(
        &mut self,
        surface_id: impl Into<String>,
        output_id: Option<OutputId>,
        parent_surface_id: impl Into<String>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.session
            .register_popup_surface(surface_id, output_id, parent_surface_id)
    }

    pub fn register_layer_surface(
        &mut self,
        surface_id: impl Into<String>,
        output_id: OutputId,
    ) -> Result<&SurfaceState, TopologyError> {
        self.session.register_layer_surface(surface_id, output_id)
    }

    pub fn register_layer_surface_with_metadata(
        &mut self,
        surface_id: impl Into<String>,
        output_id: OutputId,
        metadata: LayerSurfaceMetadata,
    ) -> Result<&SurfaceState, TopologyError> {
        self.session
            .register_layer_surface_with_metadata(surface_id, output_id, metadata)
    }

    pub fn register_unmanaged_surface(
        &mut self,
        surface_id: impl Into<String>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.session.register_unmanaged_surface(surface_id)
    }

    pub fn move_surface_to_output(
        &mut self,
        surface_id: &str,
        output_id: OutputId,
    ) -> Result<(), TopologyError> {
        self.session.move_surface_to_output(surface_id, output_id)
    }

    pub fn unmap_surface(&mut self, surface_id: &str) -> Result<(), TopologyError> {
        self.session.unmap_surface(surface_id)
    }

    pub fn activate_seat(&mut self, seat_name: &str) -> Result<(), TopologyError> {
        self.session.activate_seat(seat_name)
    }

    pub fn activate_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.session.activate_output(output_id)
    }

    pub fn disable_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.session.disable_output(output_id)
    }

    pub fn enable_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.session.enable_output(output_id)
    }

    pub fn remove_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.session.unregister_output(output_id)
    }

    pub fn remove_seat(&mut self, seat_name: &str) -> Result<(), TopologyError> {
        self.session.unregister_seat(seat_name)
    }

    pub fn remove_surface(&mut self, surface_id: &str) -> Result<(), TopologyError> {
        self.session.unregister_surface(surface_id)
    }

    pub fn remove_window_surface(&mut self, window_id: &WindowId) -> Result<(), TopologyError> {
        self.session.unregister_window_surface(window_id)
    }

    fn inferred_workspace_for_output(
        &self,
        output_id: Option<&OutputId>,
    ) -> Option<spiders_shared::ids::WorkspaceId> {
        output_id
            .and_then(|output_id| {
                self.state()
                    .outputs
                    .iter()
                    .find(|output| output.id == *output_id)
                    .and_then(|output| output.current_workspace_id.clone())
            })
            .or_else(|| self.state().current_workspace_id.clone())
    }

    fn inferred_tags_for_workspace(
        &self,
        workspace_id: Option<&spiders_shared::ids::WorkspaceId>,
    ) -> Vec<String> {
        workspace_id
            .and_then(|workspace_id| self.state().workspace_by_id(workspace_id))
            .map(|workspace| workspace.active_tags.clone())
            .unwrap_or_default()
    }

    fn backend_window_snapshot(
        &self,
        surface_id: &str,
        window_id: WindowId,
        output_id: Option<OutputId>,
    ) -> WindowSnapshot {
        let output_id = output_id.or_else(|| self.state().current_output_id.clone());
        let workspace_id = self.inferred_workspace_for_output(output_id.as_ref());
        let tags = self.inferred_tags_for_workspace(workspace_id.as_ref());

        WindowSnapshot {
            id: window_id,
            shell: ShellKind::XdgToplevel,
            app_id: None,
            title: Some(surface_id.to_owned()),
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
            output_id,
            workspace_id,
            tags,
        }
    }
}

impl<R: AuthoringLayoutRuntime<Config = Config>> CompositorApp<R> {
    pub fn apply_runtime_bootstrap_event(
        &mut self,
        event: BootstrapEvent,
    ) -> Result<(), ActionError> {
        append_winit_debug_log(&format!(
            "app.apply_runtime_bootstrap_event start event={event:?} windows={} visible={} gen={}",
            self.state().windows.len(),
            self.state().visible_window_ids.len(),
            self.session.debug_generation()
        ));
        match event {
            BootstrapEvent::RegisterWindowSurface {
                surface_id,
                window_id,
                output_id,
            } => {
                let known_window = self
                    .state()
                    .windows
                    .iter()
                    .any(|window| window.id == window_id);

                if known_window {
                    self.runtime_bootstrap_debug.existing_window_registrations += 1;
                    let _ = self
                        .session
                        .register_window_surface(surface_id, window_id, output_id)
                        .map_err(|error| {
                            ActionError::Layout(CompositorLayoutError::Runtime(
                                spiders_shared::runtime::RuntimeError::Other {
                                    message: error.to_string(),
                                },
                            ))
                        })?;
                    self.runtime_bootstrap_debug.last_state_window_count =
                        self.state().windows.len();
                    self.runtime_bootstrap_debug.last_visible_window_count =
                        self.state().visible_window_ids.len();
                    self.runtime_bootstrap_debug.last_session_generation =
                        self.session.debug_generation();
                } else {
                    self.runtime_bootstrap_debug.new_window_mappings += 1;
                    self.runtime_bootstrap_debug
                        .last_mapped_window_ids
                        .push(window_id.clone());
                    if self.runtime_bootstrap_debug.last_mapped_window_ids.len() > 16 {
                        self.runtime_bootstrap_debug
                            .last_mapped_window_ids
                            .remove(0);
                    }
                    let window =
                        self.backend_window_snapshot(&surface_id, window_id.clone(), output_id);
                    let _ = self.session.map_window_to_surface(surface_id, window)?;
                    append_winit_debug_log(&format!(
                        "app.apply_runtime_bootstrap_event prefocus_new_window window={window_id:?}"
                    ));
                    let _ = self.session.focus_window(&window_id);
                    self.runtime_bootstrap_debug.last_state_window_count =
                        self.state().windows.len();
                    self.runtime_bootstrap_debug.last_visible_window_count =
                        self.state().visible_window_ids.len();
                    self.runtime_bootstrap_debug.last_session_generation =
                        self.session.debug_generation();
                }
                append_winit_debug_log(&format!(
                    "app.apply_runtime_bootstrap_event end register_window_surface known_window={} windows={} visible={} gen={}",
                    known_window,
                    self.state().windows.len(),
                    self.state().visible_window_ids.len(),
                    self.session.debug_generation()
                ));
                Ok(())
            }
            other => self.apply_bootstrap_event(other).map_err(|error| {
                ActionError::Layout(CompositorLayoutError::Runtime(
                    spiders_shared::runtime::RuntimeError::Other {
                        message: error.to_string(),
                    },
                ))
            }),
        }
    }

    pub fn apply_bootstrap_event(&mut self, event: BootstrapEvent) -> Result<(), TopologyError> {
        self.runtime_bootstrap_debug.last_bootstrap_event = Some(format!("{event:?}"));
        self.runtime_bootstrap_debug
            .last_bootstrap_event_windows_before = self.state().windows.len();
        match event {
            BootstrapEvent::RegisterSeat { seat_name, active } => {
                self.session.register_seat(seat_name.clone());
                if active {
                    self.activate_seat(&seat_name)?;
                }
            }
            BootstrapEvent::RegisterOutput { output_id, active } => {
                self.session.register_startup_seeded_output(&output_id)?;
                if active {
                    self.activate_output(&output_id)?;
                }
            }
            BootstrapEvent::RegisterOutputSnapshot { output, active } => {
                let output_id = output.id.clone();
                self.session.register_backend_output_snapshot(output);
                if active {
                    self.activate_output(&output_id)?;
                }
            }
            BootstrapEvent::ActivateOutput { output_id } => {
                self.activate_output(&output_id)?;
            }
            BootstrapEvent::EnableOutput { output_id } => {
                self.enable_output(&output_id)?;
            }
            BootstrapEvent::DisableOutput { output_id } => {
                self.disable_output(&output_id)?;
            }
            BootstrapEvent::RemoveOutput { output_id } => {
                self.remove_output(&output_id)?;
            }
            BootstrapEvent::RegisterWindowSurface {
                surface_id,
                window_id,
                output_id,
            } => {
                let known_window = self
                    .state()
                    .windows
                    .iter()
                    .any(|window| window.id == window_id);

                if known_window {
                    let _ = self
                        .session
                        .register_window_surface(surface_id, window_id, output_id)?;
                } else {
                    let window =
                        self.backend_window_snapshot(&surface_id, window_id.clone(), output_id);
                    let _ = self
                        .session
                        .map_window_to_surface(surface_id, window)
                        .map_err(|error| TopologyError::SurfaceNotFound(error.to_string()))?;
                }
            }
            BootstrapEvent::RegisterPopupSurface {
                surface_id,
                output_id,
                parent_surface_id,
            } => {
                let _ = self.register_popup_surface(surface_id, output_id, parent_surface_id)?;
            }
            BootstrapEvent::RegisterLayerSurface {
                surface_id,
                output_id,
                metadata,
            } => {
                let _ =
                    self.register_layer_surface_with_metadata(surface_id, output_id, metadata)?;
            }
            BootstrapEvent::RegisterUnmanagedSurface { surface_id } => {
                let _ = self.register_unmanaged_surface(surface_id)?;
            }
            BootstrapEvent::RemoveSurface { surface_id } => {
                if let Some(window_id) = self
                    .topology()
                    .surface(&surface_id)
                    .and_then(|surface| surface.window_id.clone())
                {
                    let next_focus = self
                        .session
                        .preferred_focus_after_window_removed(&window_id);
                    if let Some(next_focus) = next_focus.as_ref() {
                        append_winit_debug_log(&format!(
                            "app.remove_surface prefocus closed={window_id:?} next={next_focus:?}"
                        ));
                        let _ = self.session.focus_window(next_focus);
                    }
                    let _ = self.session.destroy_window(&window_id);
                    if let Some(next_focus) = next_focus {
                        append_winit_debug_log(&format!(
                            "app.remove_surface restoring_focus closed={window_id:?} next={next_focus:?}"
                        ));
                        let _ = self.session.focus_window(&next_focus);
                    }
                }
                self.remove_surface(&surface_id)?;
            }
            BootstrapEvent::RemoveWindowSurface { window_id } => {
                let next_focus = self
                    .session
                    .preferred_focus_after_window_removed(&window_id);
                if let Some(next_focus) = next_focus.as_ref() {
                    append_winit_debug_log(&format!(
                        "app.remove_window_surface prefocus closed={window_id:?} next={next_focus:?}"
                    ));
                    let _ = self.session.focus_window(next_focus);
                }
                let _ = self.session.destroy_window(&window_id);
                if let Some(next_focus) = next_focus {
                    append_winit_debug_log(&format!(
                        "app.remove_window_surface restoring_focus closed={window_id:?} next={next_focus:?}"
                    ));
                    let _ = self.session.focus_window(&next_focus);
                }
                self.remove_window_surface(&window_id)?;
            }
            BootstrapEvent::MoveSurfaceToOutput {
                surface_id,
                output_id,
            } => {
                self.move_surface_to_output(&surface_id, output_id)?;
            }
            BootstrapEvent::FocusSeat {
                seat_name,
                window_id,
                output_id,
            } => {
                if self.topology().seat(&seat_name).is_none() {
                    self.session.register_seat(seat_name.clone());
                }
                self.session.focus_seat(&seat_name, window_id, output_id)?;
            }
            BootstrapEvent::UnmapSurface { surface_id } => {
                self.unmap_surface(&surface_id)?;
            }
            BootstrapEvent::RemoveSeat { seat_name } => {
                self.remove_seat(&seat_name)?;
            }
        }

        self.runtime_bootstrap_debug
            .last_bootstrap_event_windows_after = self.state().windows.len();

        Ok(())
    }

    pub fn initialize(
        layout_service: LayoutService,
        authoring_layout_service: AuthoringLayoutService<R>,
        config: Config,
        state: StateSnapshot,
    ) -> Result<Self, CompositorLayoutError> {
        Self::initialize_with_registration(
            layout_service,
            authoring_layout_service,
            config,
            state.clone(),
            StartupRegistration::from_state(&state),
        )
    }

    pub fn initialize_with_registration(
        layout_service: LayoutService,
        authoring_layout_service: AuthoringLayoutService<R>,
        config: Config,
        state: StateSnapshot,
        startup: StartupRegistration,
    ) -> Result<Self, CompositorLayoutError> {
        let mut session =
            CompositorSession::initialize(layout_service, authoring_layout_service, config, state)?;

        for seat in &startup.seats {
            session.register_seat(seat.clone());
        }
        for output in &startup.outputs {
            let _ = session.register_output(output.clone());
        }
        if let Some(active_seat) = startup.active_seat.as_deref() {
            let _ = session.activate_seat(active_seat);
        }
        if let Some(active_output) = startup.active_output.as_ref() {
            let _ = session.activate_output(active_output);
        }

        Ok(Self {
            session,
            startup,
            runtime_bootstrap_debug: RuntimeBootstrapDebug::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_config::authoring_layout::AuthoringLayoutService;
    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_runtime_js::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
    use spiders_runtime_js::runtime::QuickJsPreparedLayoutRuntime;
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };
    use spiders_wm::LayerSurfaceTier;
    use spiders_wm::{LayerExclusiveZone, LayerKeyboardInteractivity};

    use super::*;

    fn config() -> Config {
        Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                runtime_graph: None,
            }],
            ..Config::default()
        }
    }

    fn state() -> StateSnapshot {
        StateSnapshot {
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
                effective_layout: Some(LayoutRef {
                    name: "master-stack".into(),
                }),
            }],
            windows: vec![WindowSnapshot {
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
            }],
            visible_window_ids: vec![WindowId::from("w1")],
            tag_names: vec!["1".into()],
        }
    }

    #[test]
    fn app_initializes_session_and_registers_startup_topology() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-runtime-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        let app = CompositorApp::initialize(LayoutService, service, config(), state()).unwrap();

        assert_eq!(app.startup.seats, vec!["seat-0".to_string()]);
        assert_eq!(app.startup.outputs, vec![OutputId::from("out-1")]);
        assert_eq!(app.startup.active_seat.as_deref(), Some("seat-0"));
        assert_eq!(app.startup.active_output, Some(OutputId::from("out-1")));
        assert!(app.topology().seat("seat-0").is_some());
        assert!(app.topology().output(&OutputId::from("out-1")).is_some());
        assert_eq!(app.topology().active_seat_name.as_deref(), Some("seat-0"));
        assert_eq!(
            app.topology().active_output_id,
            Some(OutputId::from("out-1"))
        );
        assert_eq!(
            app.state().current_workspace_id,
            Some(WorkspaceId::from("ws-1"))
        );
    }

    #[test]
    fn app_initializes_with_custom_startup_registration() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-custom-startup-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);
        let mut snapshot = state();
        snapshot.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });

        let app = CompositorApp::initialize_with_registration(
            LayoutService,
            service,
            config(),
            snapshot,
            StartupRegistration {
                seats: vec!["seat-a".into(), "seat-b".into()],
                outputs: vec![OutputId::from("out-1"), OutputId::from("out-2")],
                active_seat: Some("seat-b".into()),
                active_output: Some(OutputId::from("out-2")),
            },
        )
        .unwrap();

        assert_eq!(app.topology().active_seat_name.as_deref(), Some("seat-b"));
        assert_eq!(
            app.topology().active_output_id,
            Some(OutputId::from("out-2"))
        );
    }

    #[test]
    fn app_registers_and_moves_backend_agnostic_surfaces() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-surface-runtime-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        let mut app = CompositorApp::initialize(LayoutService, service, config(), state()).unwrap();
        app.session.register_output_snapshot(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });

        app.register_popup_surface("popup-1", Some(OutputId::from("out-1")), "window-w1")
            .unwrap();
        app.register_layer_surface_with_metadata(
            "layer-1",
            OutputId::from("out-1"),
            LayerSurfaceMetadata {
                namespace: "panel".into(),
                tier: LayerSurfaceTier::Top,
                keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                exclusive_zone: LayerExclusiveZone::Exclusive(32),
            },
        )
        .unwrap();
        app.register_unmanaged_surface("overlay-1").unwrap();
        app.move_surface_to_output("layer-1", OutputId::from("out-2"))
            .unwrap();
        app.unmap_surface("popup-1").unwrap();

        assert_eq!(
            app.topology()
                .surface("popup-1")
                .unwrap()
                .parent_surface_id
                .as_deref(),
            Some("window-w1")
        );
        assert!(!app.topology().surface("popup-1").unwrap().mapped);
        assert_eq!(
            app.topology().surface("layer-1").unwrap().output_id,
            Some(OutputId::from("out-2"))
        );
        assert_eq!(
            app.topology().surface("layer-1").unwrap().layer_metadata,
            Some(LayerSurfaceMetadata {
                namespace: "panel".into(),
                tier: LayerSurfaceTier::Top,
                keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                exclusive_zone: LayerExclusiveZone::Exclusive(32),
            })
        );
        assert_eq!(
            app.topology().surface("overlay-1").unwrap().role,
            crate::SurfaceRole::Unmanaged
        );
    }

    #[test]
    fn app_register_window_surface_maps_backend_window_into_state() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-backend-window-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        let mut snapshot = state();
        snapshot.windows.clear();
        snapshot.visible_window_ids.clear();
        snapshot.focused_window_id = None;

        let mut app =
            CompositorApp::initialize(LayoutService, service, config(), snapshot).unwrap();

        app.apply_runtime_bootstrap_event(BootstrapEvent::RegisterWindowSurface {
            surface_id: "wl-surface-test-1".into(),
            window_id: WindowId::from("smithay-window-1"),
            output_id: Some(OutputId::from("out-1")),
        })
        .unwrap();

        let window = app
            .state()
            .windows
            .iter()
            .find(|window| window.id == WindowId::from("smithay-window-1"))
            .unwrap();
        assert!(window.mapped);
        assert_eq!(window.output_id, Some(OutputId::from("out-1")));
        assert_eq!(window.workspace_id, Some(WorkspaceId::from("ws-1")));
        assert!(app
            .state()
            .visible_window_ids
            .iter()
            .any(|window_id| window_id == &WindowId::from("smithay-window-1")));
        assert!(app.session.current_layout().is_some());
        assert_eq!(
            app.topology()
                .surface("wl-surface-test-1")
                .unwrap()
                .window_id,
            Some(WindowId::from("smithay-window-1"))
        );
    }

    #[test]
    fn app_register_window_surface_defaults_backend_window_to_current_output() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!(
            "spiders-app-backend-window-default-output-{unique}"
        ));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        let mut snapshot = state();
        snapshot.windows.clear();
        snapshot.visible_window_ids.clear();
        snapshot.focused_window_id = None;

        let mut app =
            CompositorApp::initialize(LayoutService, service, config(), snapshot).unwrap();

        app.apply_runtime_bootstrap_event(BootstrapEvent::RegisterWindowSurface {
            surface_id: "wl-surface-test-2".into(),
            window_id: WindowId::from("smithay-window-2"),
            output_id: None,
        })
        .unwrap();

        let window = app
            .state()
            .windows
            .iter()
            .find(|window| window.id == WindowId::from("smithay-window-2"))
            .unwrap();
        assert_eq!(window.output_id, Some(OutputId::from("out-1")));
        assert_eq!(window.workspace_id, Some(WorkspaceId::from("ws-1")));
    }

    #[test]
    fn app_tracks_output_and_seat_lifecycle_changes() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-seat-output-runtime-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        let mut app = CompositorApp::initialize(LayoutService, service, config(), state()).unwrap();
        app.session.register_output_snapshot(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });
        app.session.register_seat("seat-1");

        app.activate_output(&OutputId::from("out-2")).unwrap();
        app.disable_output(&OutputId::from("out-2")).unwrap();
        app.enable_output(&OutputId::from("out-2")).unwrap();
        app.activate_seat("seat-1").unwrap();

        assert_eq!(
            app.topology().active_output_id,
            Some(OutputId::from("out-1"))
        );
        assert!(
            app.topology()
                .output(&OutputId::from("out-2"))
                .unwrap()
                .snapshot
                .enabled
        );
        assert_eq!(app.topology().active_seat_name.as_deref(), Some("seat-1"));
    }

    #[test]
    fn app_applies_backend_agnostic_bootstrap_events() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-bootstrap-events-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        let mut app = CompositorApp::initialize(LayoutService, service, config(), state()).unwrap();
        app.session.register_output_snapshot(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });

        app.apply_bootstrap_event(BootstrapEvent::RegisterSeat {
            seat_name: "seat-1".into(),
            active: true,
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::ActivateOutput {
            output_id: OutputId::from("out-2"),
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::RegisterWindowSurface {
            surface_id: "window-w1".into(),
            window_id: WindowId::from("w1"),
            output_id: Some(OutputId::from("out-1")),
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::FocusSeat {
            seat_name: "seat-1".into(),
            window_id: Some(WindowId::from("w1")),
            output_id: Some(OutputId::from("out-1")),
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::RegisterPopupSurface {
            surface_id: "popup-1".into(),
            output_id: Some(OutputId::from("out-1")),
            parent_surface_id: "window-w1".into(),
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::RegisterLayerSurface {
            surface_id: "layer-1".into(),
            output_id: OutputId::from("out-2"),
            metadata: LayerSurfaceMetadata {
                namespace: String::new(),
                tier: spiders_wm::LayerSurfaceTier::Background,
                keyboard_interactivity: spiders_wm::LayerKeyboardInteractivity::None,
                exclusive_zone: spiders_wm::LayerExclusiveZone::Neutral,
            },
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::MoveSurfaceToOutput {
            surface_id: "popup-1".into(),
            output_id: OutputId::from("out-2"),
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::UnmapSurface {
            surface_id: "layer-1".into(),
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::RemoveWindowSurface {
            window_id: WindowId::from("w1"),
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::DisableOutput {
            output_id: OutputId::from("out-2"),
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::EnableOutput {
            output_id: OutputId::from("out-2"),
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::RemoveSeat {
            seat_name: "seat-1".into(),
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::RemoveSurface {
            surface_id: "popup-1".into(),
        })
        .ok();
        app.apply_bootstrap_event(BootstrapEvent::RemoveOutput {
            output_id: OutputId::from("out-2"),
        })
        .unwrap();

        assert!(app.topology().seat("seat-1").is_none());
        assert!(app.topology().surface("popup-1").is_none());
        assert!(app.topology().surface("window-w1").is_none());
        assert!(app.topology().output(&OutputId::from("out-2")).is_none());
        assert!(!app.topology().surface("layer-1").unwrap().mapped);
    }

    #[test]
    fn app_remove_surface_bootstrap_event_destroys_mapped_window_state() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-remove-surface-window-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        let mut snapshot = state();
        snapshot.windows.clear();
        snapshot.visible_window_ids.clear();
        snapshot.focused_window_id = None;

        let mut app =
            CompositorApp::initialize(LayoutService, service, config(), snapshot).unwrap();

        app.apply_runtime_bootstrap_event(BootstrapEvent::RegisterWindowSurface {
            surface_id: "wl-surface-test-remove".into(),
            window_id: WindowId::from("smithay-window-remove"),
            output_id: Some(OutputId::from("out-1")),
        })
        .unwrap();

        app.apply_bootstrap_event(BootstrapEvent::RemoveSurface {
            surface_id: "wl-surface-test-remove".into(),
        })
        .unwrap();

        assert!(app.topology().surface("wl-surface-test-remove").is_none());
        assert!(app
            .state()
            .windows
            .iter()
            .all(|window| window.id != WindowId::from("smithay-window-remove")));
        assert!(app.state().visible_window_ids.is_empty());
    }

    #[test]
    fn app_remove_surface_preserves_layout_based_neighbor_focus() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-remove-surface-focus-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        let mut snapshot = state();
        snapshot.windows.clear();
        snapshot.visible_window_ids.clear();
        snapshot.focused_window_id = None;

        let mut app =
            CompositorApp::initialize(LayoutService, service, config(), snapshot).unwrap();

        app.apply_runtime_bootstrap_event(BootstrapEvent::RegisterWindowSurface {
            surface_id: "wl-surface-test-1".into(),
            window_id: WindowId::from("smithay-window-1"),
            output_id: Some(OutputId::from("out-1")),
        })
        .unwrap();
        app.apply_runtime_bootstrap_event(BootstrapEvent::RegisterWindowSurface {
            surface_id: "wl-surface-test-2".into(),
            window_id: WindowId::from("smithay-window-2"),
            output_id: Some(OutputId::from("out-1")),
        })
        .unwrap();
        app.apply_runtime_bootstrap_event(BootstrapEvent::RegisterWindowSurface {
            surface_id: "wl-surface-test-3".into(),
            window_id: WindowId::from("smithay-window-3"),
            output_id: Some(OutputId::from("out-1")),
        })
        .unwrap();

        let _ = app
            .session
            .focus_window(&WindowId::from("smithay-window-3"));

        app.apply_bootstrap_event(BootstrapEvent::RemoveSurface {
            surface_id: "wl-surface-test-3".into(),
        })
        .unwrap();

        assert_eq!(
            app.state().focused_window_id,
            Some(WindowId::from("smithay-window-2"))
        );
    }

    #[test]
    fn app_remove_surface_preserves_latest_seat_focus_neighbor() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-remove-surface-seat-focus-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        let mut snapshot = state();
        snapshot.windows.clear();
        snapshot.visible_window_ids.clear();
        snapshot.focused_window_id = None;

        let mut app =
            CompositorApp::initialize(LayoutService, service, config(), snapshot).unwrap();

        for id in 1..=4 {
            app.apply_runtime_bootstrap_event(BootstrapEvent::RegisterWindowSurface {
                surface_id: format!("wl-surface-test-{id}"),
                window_id: WindowId::from(format!("smithay-window-{id}")),
                output_id: Some(OutputId::from("out-1")),
            })
            .unwrap();
        }

        for id in 1..=4 {
            app.apply_bootstrap_event(BootstrapEvent::FocusSeat {
                seat_name: "seat-0".into(),
                window_id: Some(WindowId::from(format!("smithay-window-{id}"))),
                output_id: Some(OutputId::from("out-1")),
            })
            .unwrap();
        }

        assert_eq!(
            app.state().focused_window_id,
            Some(WindowId::from("smithay-window-4"))
        );

        app.apply_bootstrap_event(BootstrapEvent::RemoveSurface {
            surface_id: "wl-surface-test-4".into(),
        })
        .unwrap();

        assert_eq!(
            app.state().focused_window_id,
            Some(WindowId::from("smithay-window-3"))
        );
    }

    #[test]
    fn app_applies_seat_focus_bootstrap_event() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let runtime_root = temp_dir.join(format!("spiders-app-seat-focus-{unique}"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));
        fs::write(
            runtime_root.join("layouts/master-stack.js"),
            "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest' }] })",
        )
        .unwrap();

        let loader =
            RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
        let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
        let service = AuthoringLayoutService::new(runtime);

        let mut app = CompositorApp::initialize(LayoutService, service, config(), state()).unwrap();
        app.apply_bootstrap_event(BootstrapEvent::RegisterSeat {
            seat_name: "seat-focus".into(),
            active: true,
        })
        .unwrap();
        app.apply_bootstrap_event(BootstrapEvent::FocusSeat {
            seat_name: "seat-focus".into(),
            window_id: Some(WindowId::from("w-focus")),
            output_id: Some(OutputId::from("out-1")),
        })
        .unwrap();

        let seat = app.topology().seat("seat-focus").unwrap();
        assert_eq!(seat.focused_window_id, Some(WindowId::from("w-focus")));
        assert_eq!(seat.focused_output_id, Some(OutputId::from("out-1")));
    }
}
