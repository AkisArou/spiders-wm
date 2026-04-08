use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use spiders_config::model::{Config, ConfigPaths, config_discovery_options_from_env};
use spiders_config::runtime::build_authoring_layout_service;
use spiders_core::signal::WmSignal;
use spiders_core::types::SpiderPlatform;
use spiders_runtime_js_native::JavaScriptNativeRuntimeProvider;

use smithay::desktop::{PopupManager, Space};
use smithay::input::{Seat, SeatState};
use smithay::reexports::calloop::{
    EventLoop, Interest, LoopHandle, Mode as CalloopMode, PostAction,
    generic::Generic,
    timer::{TimeoutAction, Timer},
};
use smithay::reexports::wayland_server::Display;
use smithay::wayland::compositor::CompositorState;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::pointer_constraints::PointerConstraintsState;
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::xdg_activation::XdgActivationState;
use tracing::warn;

use crate::backend::BackendKind;
use crate::backend::session::{
    BackendSession, NestedSessionState, TtySessionHandle, TtySessionState,
};
use crate::debug::{DebugConfig, DebugState};
use crate::frame_sync::FrameSyncState;
use crate::handlers::ClientState;
use crate::handlers::VirtualKeyboardManagerState;
use crate::runtime::{NoopHost, WmRuntime};
use crate::scene::adapter::SceneLayoutState;
use crate::state::SpidersWm;

pub(crate) fn build_state(
    event_loop: &mut EventLoop<'static, SpidersWm>,
    display: Display<SpidersWm>,
    backend_kind: BackendKind,
) -> SpidersWm {
    let start_time = std::time::Instant::now();
    let display_handle = display.handle();
    let compositor_state = CompositorState::new::<SpidersWm>(&display_handle);
    let xdg_shell_state = XdgShellState::new::<SpidersWm>(&display_handle);
    let xdg_decoration_state = XdgDecorationState::new::<SpidersWm>(&display_handle);
    let shm_state = ShmState::new::<SpidersWm>(&display_handle, vec![]);
    let dmabuf_state = DmabufState::new();
    let _output_manager_state =
        OutputManagerState::new_with_xdg_output::<SpidersWm>(&display_handle);
    let data_device_state = DataDeviceState::new::<SpidersWm>(&display_handle);
    let layer_shell_state = WlrLayerShellState::new::<SpidersWm>(&display_handle);
    let activation_state = XdgActivationState::new::<SpidersWm>(&display_handle);
    let fractional_scale_manager_state =
        FractionalScaleManagerState::new::<SpidersWm>(&display_handle);
    let pointer_constraints_state = PointerConstraintsState::new::<SpidersWm>(&display_handle);
    let relative_pointer_manager_state =
        RelativePointerManagerState::new::<SpidersWm>(&display_handle);
    let virtual_keyboard_manager_state =
        VirtualKeyboardManagerState::new(&display_handle, |_client| true);
    let popups = PopupManager::default();
    let (blocker_cleared_tx, blocker_cleared_rx) = mpsc::channel();
    let mut seat_state = SeatState::new();
    let backend_session = build_backend_session(backend_kind);
    let mut seat: Seat<SpidersWm> =
        seat_state.new_wl_seat(&display_handle, backend_session.seat_name());
    seat.add_keyboard(Default::default(), 200, 25).expect("failed to create keyboard");
    seat.add_pointer();

    let socket_name = init_wayland_listener(display, event_loop);
    let mut model = spiders_core::wm::WmModel::default();
    {
        let mut runtime = WmRuntime::new(&mut model);
        let mut host = NoopHost;
        runtime.ensure_default_workspace("1");
        let _ = runtime.handle_signal(
            &mut host,
            WmSignal::EnsureSeat { seat_id: backend_session.seat_name().into() },
        );
    }
    let (config_paths, config) = load_wm_config(None);
    let ipc_socket_path = crate::ipc::init_ipc_listener(event_loop);
    let debug = DebugState::new(DebugConfig::from_env());

    SpidersWm {
        start_time,
        socket_name,
        display_handle,
        event_loop: event_loop.handle(),
        loop_signal: event_loop.get_signal(),
        blocker_cleared_tx,
        blocker_cleared_rx,
        space: Space::default(),
        popups,
        compositor_state,
        xdg_shell_state,
        _xdg_decoration_state: xdg_decoration_state,
        shm_state,
        dmabuf_state,
        dmabuf_global: None::<DmabufGlobal>,
        seat_state,
        data_device_state,
        layer_shell_state,
        activation_state,
        _fractional_scale_manager_state: fractional_scale_manager_state,
        _pointer_constraints_state: pointer_constraints_state,
        _relative_pointer_manager_state: relative_pointer_manager_state,
        _virtual_keyboard_manager_state: virtual_keyboard_manager_state,
        seat,
        cursor_image_status: smithay::input::pointer::CursorImageStatus::default_named(),
        pointer_location: (0.0, 0.0).into(),
        backend: None,
        focused_surface: None,
        layer_shell_focus_surface: None,
        pending_activation_requests: Vec::new(),
        config_paths: config_paths.clone(),
        config,
        managed_windows: Vec::new(),
        frame_sync: FrameSyncState::default(),
        scene_snapshot_root: None,
        scene_snapshot_roots_by_output: std::collections::BTreeMap::new(),
        ipc: spiders_ipc_native::NativeIpcState::default(),
        ipc_socket_path,
        debug,
        scene: SceneLayoutState::new(config_paths.clone()),
        model,
        next_window_id: 1,
        relayout_queued: false,
        relayout_generation: 0,
        relayout_cause: Default::default(),
    }
}

fn build_backend_session(backend_kind: BackendKind) -> BackendSession {
    match backend_kind {
        BackendKind::Winit => {
            BackendSession::Nested(NestedSessionState { seat_name: "winit".into(), active: true })
        }
        BackendKind::Tty => BackendSession::Tty(TtySessionState {
            seat_name: "seat0".into(),
            active: false,
            handle: TtySessionHandle::Placeholder,
        }),
    }
}

pub(crate) fn schedule_queued_relayout_timer(
    event_loop: &LoopHandle<'static, SpidersWm>,
    generation: u64,
    cause: crate::state::RelayoutCause,
) {
    let delay_ms = match cause {
        crate::state::RelayoutCause::General => 20,
        crate::state::RelayoutCause::FirstMapBurst => 60,
    };

    event_loop
        .insert_source(Timer::from_duration(Duration::from_millis(delay_ms)), move |_, _, state| {
            if !state.relayout_queued || state.relayout_generation != generation {
                return TimeoutAction::Drop;
            }

            let retry_delay_ms = match state.relayout_cause {
                crate::state::RelayoutCause::General => 20,
                crate::state::RelayoutCause::FirstMapBurst => 60,
            };

            let pending_unmapped_windows =
                state.managed_windows.iter().filter(|record| !record.mapped).count();
            if pending_unmapped_windows > 0 {
                tracing::debug!(
                    pending_unmapped_windows,
                    window_count = state.managed_window_count(),
                    generation,
                    relayout_cause = ?state.relayout_cause,
                    "wm deferred queued relayout while windows are still pending first map"
                );
                return TimeoutAction::ToDuration(Duration::from_millis(retry_delay_ms));
            }

            state.relayout_queued = false;
            state.schedule_relayout();
            TimeoutAction::Drop
        })
        .expect("failed to register queued relayout timer");
}

pub(crate) fn load_wm_config(existing_paths: Option<ConfigPaths>) -> (Option<ConfigPaths>, Config) {
    let paths = match existing_paths {
        Some(paths) => paths,
        None => match ConfigPaths::discover(config_discovery_options_from_env()) {
            Ok(paths) => paths,
            Err(error) => {
                warn!(%error, "wm could not discover config paths; using empty config");
                return (None, Config::default());
            }
        },
    };

    let js_provider = JavaScriptNativeRuntimeProvider::new(SpiderPlatform::Wayland);
    let service = match build_authoring_layout_service(&paths, &[&js_provider]) {
        Ok(service) => service,
        Err(error) => {
            warn!(
                authored_config = %paths.authored_config.display(),
                %error,
                "wm could not build config runtime; using empty config"
            );
            return (Some(paths), Config::default());
        }
    };
    match service.load_config(&paths) {
        Ok(config) => (Some(paths), config),
        Err(error) => {
            warn!(
                authored_config = %paths.authored_config.display(),
                prepared_config = %paths.prepared_config.display(),
                %error,
                "wm failed to load config; using empty config"
            );
            (Some(paths), Config::default())
        }
    }
}

fn init_wayland_listener(
    display: Display<SpidersWm>,
    event_loop: &mut EventLoop<'static, SpidersWm>,
) -> std::ffi::OsString {
    let listening_socket = configured_wayland_listener();
    let socket_name = listening_socket.socket_name().to_os_string();
    let loop_handle = event_loop.handle();

    loop_handle
        .insert_source(listening_socket, move |client_stream, _, state| {
            state
                .display_handle
                .insert_client(client_stream, Arc::new(ClientState::default()))
                .expect("failed to insert Wayland client");
            if let Err(error) = state.display_handle.flush_clients() {
                warn!(?error, "failed to flush Wayland clients after insert");
            }
        })
        .expect("failed to register Wayland listening socket");

    loop_handle
        .insert_source(
            Generic::new(display, Interest::READ, CalloopMode::Level),
            |_, display, state| {
                unsafe {
                    display.get_mut().dispatch_clients(state).expect("failed to dispatch clients");
                }
                if let Err(error) = state.display_handle.flush_clients() {
                    warn!(?error, "failed to flush Wayland clients after dispatch");
                }
                Ok(PostAction::Continue)
            },
        )
        .expect("failed to register Wayland display source");

    socket_name
}

fn configured_wayland_listener() -> ListeningSocketSource {
    if let Some(socket_name) = std::env::var_os("SPIDERS_WM_WAYLAND_SOCKET") {
        let socket_name = socket_name.to_string_lossy().into_owned();
        return ListeningSocketSource::with_name(&socket_name)
            .expect("failed to create Wayland socket from SPIDERS_WM_WAYLAND_SOCKET");
    }

    ListeningSocketSource::new_auto().expect("failed to create Wayland socket")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use spiders_config::model::{Config, ConfigPaths, LayoutDefinition, LayoutSelectionConfig};
    use spiders_core::command::WmCommand;
    use spiders_core::wm::WmModel;
    use spiders_wm_runtime::WmRuntime;

    use super::load_wm_config;
    use crate::backend::winit::{default_winit_output_size, sanitize_winit_output_size};
    use smithay::utils::{Physical, Size};

    fn unique_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("spiders-wm-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("failed to create temp root");
        root
    }

    fn write_authored_config(path: &Path, command_expression: &str) {
        fs::write(
            path,
            format!(
                r#"
import * as commands from "@spiders-wm/sdk/commands";

export default {{
  workspaces: ["1", "2"],
  bindings: {{
    mod: "super",
    entries: [
      {{ bind: ["mod", "Return"], command: {command_expression} }},
    ],
  }},
}};
"#,
            ),
        )
        .expect("failed to write authored config");
    }

    #[test]
    fn sanitize_winit_output_size_rejects_placeholder_sizes() {
        assert_eq!(sanitize_winit_output_size(Size::<i32, Physical>::from((2, 2))), None);
        assert_eq!(sanitize_winit_output_size(Size::<i32, Physical>::from((63, 200))), None);
    }

    #[test]
    fn sanitize_winit_output_size_keeps_real_sizes() {
        assert_eq!(
            sanitize_winit_output_size(Size::<i32, Physical>::from((1280, 800))),
            Some(Size::<i32, Physical>::from((1280, 800)))
        );
        assert_eq!(default_winit_output_size(), Size::<i32, Physical>::from((1280, 800)));
    }

    #[test]
    fn load_wm_config_with_paths_decodes_authored_toggle_workspace_binding() {
        let root = unique_root("config-load");
        let project_root = root.join("project");
        let cache_root = root.join("cache");
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&cache_root).unwrap();

        let authored_config = project_root.join("config.ts");
        let prepared_config = cache_root.join("config.js");
        write_authored_config(&authored_config, "commands.toggle_workspace(2)");

        let (paths, config) =
            load_wm_config(Some(ConfigPaths::new(&authored_config, &prepared_config)));

        assert_eq!(paths, Some(ConfigPaths::new(&authored_config, &prepared_config)));
        assert!(prepared_config.exists());
        assert_eq!(config.workspaces, vec!["1".to_string(), "2".to_string()]);
        assert_eq!(config.bindings.len(), 1);
        assert_eq!(config.bindings[0].trigger, "super+Return");
        assert_eq!(
            config.bindings[0].command,
            WmCommand::ToggleAssignFocusedWindowToWorkspace { workspace: 2 }
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_wm_config_with_paths_refreshes_prepared_config_after_authored_changes() {
        let root = unique_root("config-reload");
        let project_root = root.join("project");
        let cache_root = root.join("cache");
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&cache_root).unwrap();

        let authored_config = project_root.join("config.ts");
        let prepared_config = cache_root.join("config.js");
        let paths = ConfigPaths::new(&authored_config, &prepared_config);

        write_authored_config(&authored_config, "commands.toggle_fullscreen()");
        let (_, initial_config) = load_wm_config(Some(paths.clone()));
        assert_eq!(initial_config.bindings.len(), 1);
        assert_eq!(initial_config.bindings[0].command, WmCommand::ToggleFullscreen);

        std::thread::sleep(Duration::from_millis(20));
        write_authored_config(&authored_config, "commands.reload_config()");

        let (_, reloaded_config) = load_wm_config(Some(paths));
        assert_eq!(reloaded_config.bindings.len(), 1);
        assert_eq!(reloaded_config.bindings[0].trigger, "super+Return");
        assert_eq!(reloaded_config.bindings[0].command, WmCommand::ReloadConfig);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_layout_defaults_follow_loaded_config_selection() {
        let config = Config {
            workspaces: vec!["1".to_string(), "2".to_string()],
            layouts: vec![
                LayoutDefinition {
                    name: "master-stack".to_string(),
                    directory: "layouts/master-stack".to_string(),
                    module: "layouts/master-stack.js".to_string(),
                    stylesheet_path: None,
                    runtime_cache_payload: None,
                },
                LayoutDefinition {
                    name: "focus-repro".to_string(),
                    directory: "layouts/focus-repro".to_string(),
                    module: "layouts/focus-repro.js".to_string(),
                    stylesheet_path: None,
                    runtime_cache_payload: None,
                },
            ],
            layout_selection: LayoutSelectionConfig {
                default: Some("master-stack".to_string()),
                per_workspace: vec!["focus-repro".to_string(), "master-stack".to_string()],
                per_monitor: Default::default(),
            },
            ..Config::default()
        };
        let mut model = WmModel::default();
        let mut runtime = WmRuntime::new(&mut model);
        let workspace_1 = runtime.ensure_workspace("1");
        let workspace_2 = runtime.ensure_workspace("2");
        runtime.sync_layout_selection_defaults(&config);

        assert_eq!(
            model
                .workspaces
                .get(&workspace_1)
                .and_then(|workspace| workspace.effective_layout.as_ref())
                .map(|layout| layout.name.as_str()),
            Some("focus-repro")
        );
        assert_eq!(
            model
                .workspaces
                .get(&workspace_2)
                .and_then(|workspace| workspace.effective_layout.as_ref())
                .map(|layout| layout.name.as_str()),
            Some("master-stack")
        );
    }
}
