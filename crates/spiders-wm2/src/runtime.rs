use crate::{
    actions,
    app::AppState,
    command::{CommandResult, RuntimeCommand},
    config::{built_in_default_config, ConfigSource},
    layout_runtime::RuntimeLayoutService,
    model::{PointerInteraction, WorkspaceId},
    render::RenderPlan,
    transactions::TransactionManager,
};

use serde_json::json;
use spiders_config::model::LayoutConfigError;
use std::{
    ffi::OsString,
    io::{Read, Write},
    os::unix::net::UnixListener,
    sync::Arc,
    time::Instant,
};
use tracing::warn;

use smithay::{
    desktop::{PopupManager, Space, Window},
    input::{Seat, SeatState},
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, LoopSignal, Mode, PostAction},
        wayland_server::{backend::ClientData, Display, DisplayHandle},
    },
    utils::{Logical, SERIAL_COUNTER},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

#[derive(Debug)]
pub struct SpidersWm2 {
    pub app: AppState,
    pub runtime: RuntimeState,
}

#[derive(Debug)]
pub struct RuntimeState {
    pub start_time: Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub loop_signal: LoopSignal,
    pub pointer_interaction: Option<PointerInteraction>,
    pub layout_service: RuntimeLayoutService,
    pub render_plan: RenderPlan,
    pub transactions: TransactionManager,
    pub smithay: SmithayState,
}

#[derive(Debug)]
pub struct SmithayState {
    pub space: Space<Window>,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    #[allow(dead_code)]
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<SpidersWm2>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,
    pub seat: Seat<SpidersWm2>,
}

impl SpidersWm2 {
    pub fn new(event_loop: &mut EventLoop<Self>, display: Display<Self>) -> Self {
        let runtime = RuntimeState::new(event_loop, display);
        let mut app = AppState::default();
        app.apply_config(built_in_default_config(), ConfigSource::BuiltInDefault);

        if let Some(config_path) = std::env::var_os("SPIDERS_WM2_CONFIG_PATH") {
            let source = match std::env::var("SPIDERS_WM2_CONFIG_SOURCE").ok().as_deref() {
                Some("authored") => ConfigSource::AuthoredConfig,
                Some("prepared") => ConfigSource::PreparedConfig,
                _ => ConfigSource::PreparedConfig,
            };

            if let Err(error) = app.load_config_from_path(&config_path, source) {
                warn!(?config_path, ?source, %error, "failed to load configured wm2 config path");
            }
        }

        let mut state = Self { app, runtime };
        state.refresh_layout_artifacts();

        state
    }

    pub fn reload_config_from_path(
        &mut self,
        path: impl AsRef<std::path::Path>,
        source: ConfigSource,
    ) -> Result<(), LayoutConfigError> {
        self.runtime.layout_service = RuntimeLayoutService::from_paths(
            &spiders_config::model::ConfigPaths::new(path.as_ref(), path.as_ref()),
        );
        self.app.load_config_from_path(path, source)?;
        self.refresh_layout_artifacts();
        self.refresh_active_workspace();
        Ok(())
    }

    pub fn refresh_layout_artifacts(&mut self) {
        let state = self.app.wm.snapshot(
            &self.app.topology.outputs,
            self.app.config_runtime.current(),
        );
        let config = self.app.config_runtime.current().clone();

        for workspace in &state.workspaces {
            if let Ok(Some(evaluation)) = self
                .runtime
                .layout_service
                .evaluate_prepared_for_workspace(&config, &state, workspace)
            {
                self.app.apply_prepared_layout_evaluation(
                    evaluation,
                    self.runtime.layout_service.provenance(),
                );
            }
        }
    }

    pub fn handle_runtime_command(&mut self, command: RuntimeCommand) -> CommandResult {
        match command {
            RuntimeCommand::ReloadConfig => {
                if let Some(config_path) = std::env::var_os("SPIDERS_WM2_CONFIG_PATH") {
                    let source = match std::env::var("SPIDERS_WM2_CONFIG_SOURCE").ok().as_deref() {
                        Some("authored") => ConfigSource::AuthoredConfig,
                        _ => ConfigSource::PreparedConfig,
                    };
                    match self.reload_config_from_path(&config_path, source) {
                        Ok(()) => CommandResult {
                            ok: true,
                            message: "reloaded config".into(),
                            payload: None,
                        },
                        Err(error) => CommandResult {
                            ok: false,
                            message: format!("reload failed: {error}"),
                            payload: None,
                        },
                    }
                } else {
                    CommandResult {
                        ok: false,
                        message: "SPIDERS_WM2_CONFIG_PATH is not set".into(),
                        payload: None,
                    }
                }
            }
            RuntimeCommand::DumpTransaction => CommandResult {
                ok: true,
                message: format!(
                    "config_revision={} layout_tree_revision={} render_dirty={} {}",
                    self.app.config_runtime.revision(),
                    self.app.config_runtime.layout_tree_revision(),
                    self.runtime.render_plan.is_dirty(),
                    self.runtime
                        .transactions
                        .pending_debug_summary(&self.app.wm)
                        .unwrap_or_else(|| "no pending transaction".into())
                ),
                payload: Some(json!({
                    "pending": self.runtime.transactions.pending().map(|pending| json!({
                        "id": pending.id,
                        "affected_windows": pending.affected_windows,
                        "affected_workspaces": pending.affected_workspaces,
                        "affected_outputs": pending.affected_outputs,
                    })),
                    "committed": self.runtime.transactions.committed().map(|snapshot| json!({
                        "focused_window_id": snapshot.focused_window_id,
                        "current_output_id": snapshot.current_output_id,
                        "current_workspace_id": snapshot.current_workspace_id,
                        "visible_window_ids": snapshot.visible_window_ids,
                    })),
                    "history": self.runtime.transactions.history().iter().map(|entry| json!({
                        "id": entry.id,
                        "reason": format!("{:?}", entry.reason),
                        "affected_window_count": entry.affected_window_count,
                        "affected_workspace_count": entry.affected_workspace_count,
                        "affected_output_count": entry.affected_output_count,
                    })).collect::<Vec<_>>(),
                    "render_dirty": self.runtime.render_plan.is_dirty(),
                })),
            },
            RuntimeCommand::SwitchWorkspace(workspace_id) => {
                self.switch_workspace(workspace_id);
                CommandResult {
                    ok: true,
                    message: format!("switched workspace to {}", self.app.wm.active_workspace),
                    payload: None,
                }
            }
            RuntimeCommand::RefreshLayoutArtifacts => {
                self.refresh_layout_artifacts();
                self.refresh_active_workspace();
                CommandResult {
                    ok: true,
                    message: "refreshed layout artifacts".into(),
                    payload: None,
                }
            }
            RuntimeCommand::DumpGeometry => CommandResult {
                ok: true,
                message: "dumped geometry".into(),
                payload: Some(json!({
                    "desired": self.app.layout.desired_tiled_window_rects.iter().map(|(window_id, rect)| {
                        json!({
                            "window_id": window_id,
                            "x": rect.loc.x,
                            "y": rect.loc.y,
                            "width": rect.size.w,
                            "height": rect.size.h,
                        })
                    }).collect::<Vec<_>>(),
                    "committed": self.app.layout.committed_tiled_window_rects.iter().map(|(window_id, rect)| {
                        json!({
                            "window_id": window_id,
                            "x": rect.loc.x,
                            "y": rect.loc.y,
                            "width": rect.size.w,
                            "height": rect.size.h,
                        })
                    }).collect::<Vec<_>>(),
                })),
            },
            RuntimeCommand::DumpLayoutTree => CommandResult {
                ok: true,
                message: "dumped layout tree".into(),
                payload: Some(json!({
                    "desired": self.app.layout.desired_layout_snapshots,
                    "committed": self.app.layout.committed_layout_snapshots,
                })),
            },
            RuntimeCommand::DumpLayoutArtifacts => {
                let snapshot = self.app.wm.snapshot(
                    &self.app.topology.outputs,
                    self.app.config_runtime.current(),
                );

                CommandResult {
                    ok: true,
                    message: "dumped layout artifacts".into(),
                    payload: Some(json!({
                        "config_revision": self.app.config_runtime.revision(),
                        "layout_tree_revision": self.app.config_runtime.layout_tree_revision(),
                        "config_source": format!("{:?}", self.app.config_runtime.source()),
                        "installed_layouts": self.app.config_runtime.installed_layout_names(),
                        "workspaces": snapshot.workspaces.iter().map(|workspace| {
                            json!({
                                "workspace_id": workspace.id,
                                "workspace_name": workspace.name,
                                "effective_layout": workspace.effective_layout.as_ref().map(|layout| layout.name.clone()),
                                "selected_layout_installed": workspace.effective_layout.as_ref().map(|layout| {
                                    self.app.config_runtime.layout_tree(&layout.name).is_some()
                                }),
                                "selected_layout_source": workspace.effective_layout.as_ref().and_then(|layout| {
                                    self.app.config_runtime.layout_tree_source(&layout.name)
                                }).map(|source| format!("{:?}", source)),
                            })
                        }).collect::<Vec<_>>()
                    })),
                }
            }
            RuntimeCommand::DumpRuntime => CommandResult {
                ok: true,
                message: "dumped runtime".into(),
                payload: Some(json!({
                    "backend": "winit",
                    "layout_runtime": self.runtime.layout_service.label(),
                    "layout_runtime_provenance": format!("{:?}", self.runtime.layout_service.provenance()),
                    "socket_name": self.runtime.socket_name.to_string_lossy(),
                    "control_socket": std::env::var_os("SPIDERS_WM2_CONTROL_SOCKET").map(|path| path.to_string_lossy().to_string()),
                    "config_path": std::env::var_os("SPIDERS_WM2_CONFIG_PATH").map(|path| path.to_string_lossy().to_string()),
                    "features": {
                        "built_in_layout_runtime": cfg!(feature = "built-in-layout-runtime"),
                    },
                    "output_count": self.app.topology.outputs.len(),
                    "window_count": self.app.wm.windows.len(),
                })),
            },
            RuntimeCommand::ListOutputs => CommandResult {
                ok: true,
                message: "listed outputs".into(),
                payload: Some(json!({
                    "backend": "winit",
                    "focused_output": self.app.wm.focused_output,
                    "pending_transaction": self.runtime.transactions.pending().map(|pending| pending.id),
                    "outputs": self.app.topology.outputs.values().map(|output| {
                        json!({
                            "id": output.id,
                            "name": output.name,
                            "enabled": output.enabled,
                            "current_workspace": output.current_workspace,
                            "logical_size": output.logical_size,
                            "dirty": self.runtime.render_plan.should_render_output(&output.id),
                            "capabilities": {
                                "renderable": output.enabled,
                                "single_window_backend": true,
                                "transactional_damage_tracking": true,
                            },
                        })
                    }).collect::<Vec<_>>()
                })),
            },
            RuntimeCommand::ListWorkspaces => CommandResult {
                ok: true,
                message: "listed workspaces".into(),
                payload: Some(json!({
                    "active_workspace": self.app.wm.active_workspace,
                    "focused_window": self.app.wm.focused_window,
                    "pending_transaction": self.runtime.transactions.pending().map(|pending| pending.id),
                    "workspaces": self.app.wm.workspaces.values().map(|workspace| {
                        json!({
                            "id": workspace.id,
                            "name": workspace.name,
                            "output": workspace.output,
                            "windows": workspace.windows,
                        })
                    }).collect::<Vec<_>>()
                })),
            },
            RuntimeCommand::ListWindows => CommandResult {
                ok: true,
                message: "listed windows".into(),
                payload: Some(json!({
                    "focused_window": self.app.wm.focused_window,
                    "pending_transaction": self.runtime.transactions.pending().map(|pending| pending.id),
                    "windows": self.app.wm.windows.values().map(|window| {
                        json!({
                            "id": window.id,
                            "workspace": window.workspace,
                            "output": window.output,
                            "mode": format!("{:?}", window.mode),
                            "mapped": window.mapped,
                            "title": window.title,
                            "app_id": window.app_id,
                            "focused": self.app.wm.focused_window.as_ref() == Some(&window.id),
                        })
                    }).collect::<Vec<_>>()
                })),
            },
        }
    }

    pub fn switch_workspace(&mut self, workspace_id: WorkspaceId) {
        actions::switch_to_workspace(&mut self.app.wm, workspace_id);
        self.refresh_active_workspace();
    }

    pub fn move_focused_window_to_workspace(&mut self, workspace_id: WorkspaceId) {
        actions::move_focused_window_to_workspace(&mut self.app.wm, workspace_id);
        self.refresh_active_workspace();
    }

    pub fn focus_next_window(&mut self) {
        actions::focus_next_window(&mut self.app.wm);
        if let Some(output_id) = self.app.wm.focused_output.clone() {
            self.runtime.render_plan.mark_output_dirty(output_id);
        }

        let focused_surface = self
            .app
            .wm
            .focused_window
            .clone()
            .and_then(|window_id| self.app.bindings.surface_for_window(&window_id));

        self.focus_window_surface(focused_surface, SERIAL_COUNTER.next_serial());
    }

    pub fn focus_previous_window(&mut self) {
        actions::focus_previous_window(&mut self.app.wm);
        if let Some(output_id) = self.app.wm.focused_output.clone() {
            self.runtime.render_plan.mark_output_dirty(output_id);
        }

        let focused_surface = self
            .app
            .wm
            .focused_window
            .clone()
            .and_then(|window_id| self.app.bindings.surface_for_window(&window_id));

        self.focus_window_surface(focused_surface, SERIAL_COUNTER.next_serial());
    }

    pub fn swap_focused_window_with_next(&mut self) {
        actions::swap_focused_window_with_next(&mut self.app.wm);
        self.refresh_active_workspace();
    }

    pub fn swap_focused_window_with_previous(&mut self) {
        actions::swap_focused_window_with_previous(&mut self.app.wm);
        self.refresh_active_workspace();
    }

    pub fn toggle_floating_focused_window(&mut self) {
        actions::toggle_floating_focused_window(&mut self.app.wm);

        self.refresh_active_workspace();
    }

    pub fn toggle_fullscreen_focused_window(&mut self) {
        actions::toggle_fullscreen_focused_window(&mut self.app.wm);
        self.refresh_active_workspace();
    }

    pub(crate) fn output_rect(&self) -> Option<smithay::utils::Rectangle<i32, Logical>> {
        self.runtime
            .smithay
            .space
            .outputs()
            .next()
            .and_then(|output| self.runtime.smithay.space.output_geometry(output))
    }
}

impl RuntimeState {
    fn new(event_loop: &mut EventLoop<SpidersWm2>, display: Display<SpidersWm2>) -> Self {
        let start_time = Instant::now();
        let display_handle = display.handle();
        let socket_name = Self::init_wayland_listener(display, event_loop);
        let loop_signal = event_loop.get_signal();
        let smithay = SmithayState::new(&display_handle);

        Self {
            start_time,
            socket_name,
            display_handle,
            loop_signal,
            pointer_interaction: None,
            layout_service: if let Some(config_path) = std::env::var_os("SPIDERS_WM2_CONFIG_PATH") {
                RuntimeLayoutService::from_paths(&spiders_config::model::ConfigPaths::new(
                    &config_path,
                    &config_path,
                ))
            } else {
                #[cfg(feature = "built-in-layout-runtime")]
                {
                    RuntimeLayoutService::built_in()
                }
                #[cfg(not(feature = "built-in-layout-runtime"))]
                {
                    RuntimeLayoutService::from_paths(&spiders_config::model::ConfigPaths::new(
                        "./config/spiders.js",
                        "./config/runtime/spiders.json",
                    ))
                }
            },
            render_plan: RenderPlan::default(),
            transactions: TransactionManager::default(),
            smithay,
        }
    }

    fn init_wayland_listener(
        display: Display<SpidersWm2>,
        event_loop: &mut EventLoop<SpidersWm2>,
    ) -> OsString {
        let listening_socket =
            ListeningSocketSource::new_auto().expect("failed to create wayland socket");
        let socket_name = listening_socket.socket_name().to_os_string();

        let loop_handle = event_loop.handle();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                state
                    .runtime
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .expect("failed to insert client");
            })
            .expect("failed to add listening socket source");

        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| unsafe {
                    display.get_mut().dispatch_clients(state).unwrap();

                    Ok(PostAction::Continue)
                },
            )
            .expect("failed to add wayland display source");

        if let Some(control_socket_path) = std::env::var_os("SPIDERS_WM2_CONTROL_SOCKET") {
            let _ = std::fs::remove_file(&control_socket_path);
            let listener = UnixListener::bind(&control_socket_path)
                .expect("failed to bind wm2 control socket");
            listener
                .set_nonblocking(true)
                .expect("failed to make wm2 control socket nonblocking");

            loop_handle
                .insert_source(
                    Generic::new(listener, Interest::READ, Mode::Level),
                    |_, listener, state| unsafe {
                        while let Ok((mut stream, _addr)) = listener.get_mut().accept() {
                            let mut command = String::new();
                            let _ = stream.read_to_string(&mut command);

                            if let Some(command) = RuntimeCommand::parse(&command) {
                                let result = state.handle_runtime_command(command);
                                let _ =
                                    stream.write_all(format!("{}\n", result.to_json()).as_bytes());
                            } else {
                                let _ = stream
                                    .write_all(b"{\"ok\":false,\"message\":\"unknown command\"}\n");
                            }
                        }

                        Ok(PostAction::Continue)
                    },
                )
                .expect("failed to add wm2 control socket source");
        }

        socket_name
    }
}

impl SmithayState {
    fn new(display_handle: &DisplayHandle) -> Self {
        let compositor_state = CompositorState::new::<SpidersWm2>(display_handle);
        let xdg_shell_state = XdgShellState::new::<SpidersWm2>(display_handle);
        let shm_state = ShmState::new::<SpidersWm2>(display_handle, vec![]);
        let output_manager_state =
            OutputManagerState::new_with_xdg_output::<SpidersWm2>(display_handle);
        let data_device_state = DataDeviceState::new::<SpidersWm2>(display_handle);

        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(display_handle, "winit");

        seat.add_keyboard(Default::default(), 200, 25)
            .expect("failed to create keyboard");
        seat.add_pointer();

        Self {
            space: Space::default(),
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups: PopupManager::default(),
            seat,
        }
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: smithay::reexports::wayland_server::backend::ClientId) {}

    fn disconnected(
        &self,
        _client_id: smithay::reexports::wayland_server::backend::ClientId,
        _reason: smithay::reexports::wayland_server::backend::DisconnectReason,
    ) {
    }
}
