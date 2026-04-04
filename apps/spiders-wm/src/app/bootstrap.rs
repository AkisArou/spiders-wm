use std::sync::Arc;
use std::sync::mpsc;

use spiders_config::model::{Config, ConfigPaths, config_discovery_options_from_env};
use spiders_config::runtime::build_authoring_layout_service;
use spiders_core::signal::WmSignal;
use spiders_runtime_js_native::JavaScriptNativeRuntimeProvider;

use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{ImportDma, ImportMemWl};
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::desktop::{PopupManager, Space};
use smithay::input::{Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::{
    EventLoop, Interest, Mode as CalloopMode, PostAction, generic::Generic,
};
use smithay::reexports::wayland_server::Display;
use smithay::utils::Transform;
use smithay::wayland::compositor::CompositorState;
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::warn;

use crate::frame_sync::FrameSyncState;
use crate::handlers::ClientState;
use crate::runtime::{NoopHost, WmRuntime};
use crate::scene::adapter::SceneLayoutState;
use crate::state::SpidersWm;

pub(crate) fn build_state(
    event_loop: &mut EventLoop<'static, SpidersWm>,
    display: Display<SpidersWm>,
) -> SpidersWm {
    let start_time = std::time::Instant::now();
    let display_handle = display.handle();
    let compositor_state = CompositorState::new::<SpidersWm>(&display_handle);
    let xdg_shell_state = XdgShellState::new::<SpidersWm>(&display_handle);
    let shm_state = ShmState::new::<SpidersWm>(&display_handle, vec![]);
    let dmabuf_state = DmabufState::new();
    let _output_manager_state =
        OutputManagerState::new_with_xdg_output::<SpidersWm>(&display_handle);
    let data_device_state = DataDeviceState::new::<SpidersWm>(&display_handle);
    let popups = PopupManager::default();
    let (blocker_cleared_tx, blocker_cleared_rx) = mpsc::channel();
    let mut seat_state = SeatState::new();
    let mut seat: Seat<SpidersWm> = seat_state.new_wl_seat(&display_handle, "winit");
    seat.add_keyboard(Default::default(), 200, 25).expect("failed to create keyboard");
    seat.add_pointer();

    let socket_name = init_wayland_listener(display, event_loop);
    let mut model = spiders_core::wm::WmModel::default();
    {
        let mut runtime = WmRuntime::new(&mut model);
        let mut host = NoopHost;
        runtime.ensure_default_workspace("1");
        let _ = runtime.handle_signal(&mut host, WmSignal::EnsureSeat { seat_id: "winit".into() });
    }
    let (config_paths, config) = load_wm_config(None);
    let ipc_socket_path = crate::ipc::init_ipc_listener(event_loop);

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
        shm_state,
        dmabuf_state,
        dmabuf_global: None::<DmabufGlobal>,
        seat_state,
        data_device_state,
        seat,
        backend: None::<WinitGraphicsBackend<GlesRenderer>>,
        focused_surface: None,
        config_paths: config_paths.clone(),
        config,
        managed_windows: Vec::new(),
        frame_sync: FrameSyncState::default(),
        ipc_server: spiders_ipc::IpcServerState::new(),
        ipc_clients: std::collections::BTreeMap::new(),
        ipc_socket_path,
        scene: SceneLayoutState::new(config_paths.clone()),
        model,
        next_window_id: 1,
    }
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

    let js_provider = JavaScriptNativeRuntimeProvider;
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

pub(crate) fn init_winit(
    event_loop: &mut EventLoop<'static, SpidersWm>,
    state: &mut SpidersWm,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit) = winit::init::<GlesRenderer>()?;

    state.shm_state.update_formats(backend.renderer().shm_formats());

    let render_node = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .and_then(|device| device.try_get_render_node());

    let dmabuf_default_feedback = match render_node {
        Ok(Some(node)) => {
            let dmabuf_formats = backend.renderer().dmabuf_formats();
            DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats)
                .build()
                .map(Some)
                .map_err(|err| err.to_string())
        }
        Ok(None) => {
            warn!("failed to query render node, dmabuf will use v3");
            Ok(None)
        }
        Err(err) => {
            warn!(?err, "failed to query EGL render node, dmabuf will use v3");
            Ok(None)
        }
    }
    .expect("failed to build dmabuf feedback");

    state.dmabuf_global = if let Some(default_feedback) = dmabuf_default_feedback.as_ref() {
        Some(state.dmabuf_state.create_global_with_default_feedback::<SpidersWm>(
            &state.display_handle,
            default_feedback,
        ))
    } else {
        let dmabuf_formats = backend.renderer().dmabuf_formats();
        if dmabuf_formats.iter().next().is_some() {
            Some(
                state
                    .dmabuf_state
                    .create_global::<SpidersWm>(&state.display_handle, dmabuf_formats),
            )
        } else {
            None
        }
    };

    state.backend = Some(backend);

    let mode = Mode {
        size: state.backend.as_ref().expect("winit backend missing during init").window_size(),
        refresh: 60_000,
    };

    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Winit".into(),
            serial_number: "Unknown".into(),
        },
    );
    let _global = output.create_global::<SpidersWm>(&state.display_handle);
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, Some((0, 0).into()));
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));
    let config = state.config.clone();
    let startup_events = {
        let mut runtime = state.runtime();
        let mut events = runtime.handle_signal(
            &mut NoopHost,
            WmSignal::OutputSynced {
                output_id: "winit".into(),
                name: "winit".to_string(),
                logical_width: mode.size.w as u32,
                logical_height: mode.size.h as u32,
            },
        );
        runtime.sync_layout_selection_defaults(&config);
        events.extend(runtime.take_events());
        events
    };
    state.broadcast_runtime_events(startup_events);

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    event_loop.handle().insert_source(winit, move |event, _, state| match event {
        WinitEvent::Resized { size, .. } => {
            output.change_current_state(Some(Mode { size, refresh: 60_000 }), None, None, None);
            let config = state.config.clone();
            let events = {
                let mut runtime = state.runtime();
                let mut events = runtime.handle_signal(
                    &mut NoopHost,
                    WmSignal::OutputSynced {
                        output_id: "winit".into(),
                        name: "winit".to_string(),
                        logical_width: size.w as u32,
                        logical_height: size.h as u32,
                    },
                );
                runtime.sync_layout_selection_defaults(&config);
                events.extend(runtime.take_events());
                events
            };
            state.broadcast_runtime_events(events);
            state.schedule_relayout();
        }
        WinitEvent::Input(event) => state.process_input_event(event),
        WinitEvent::Redraw => state.render_output_frame(&output, &mut damage_tracker),
        WinitEvent::CloseRequested => state.loop_signal.stop(),
        _ => {}
    })?;

    Ok(())
}

fn init_wayland_listener(
    display: Display<SpidersWm>,
    event_loop: &mut EventLoop<'static, SpidersWm>,
) -> std::ffi::OsString {
    let listening_socket =
        ListeningSocketSource::new_auto().expect("failed to create Wayland socket");
    let socket_name = listening_socket.socket_name().to_os_string();
    let loop_handle = event_loop.handle();

    loop_handle
        .insert_source(listening_socket, move |client_stream, _, state| {
            state
                .display_handle
                .insert_client(client_stream, Arc::new(ClientState::default()))
                .expect("failed to insert Wayland client");
        })
        .expect("failed to register Wayland listening socket");

    loop_handle
        .insert_source(
            Generic::new(display, Interest::READ, CalloopMode::Level),
            |_, display, state| {
                unsafe {
                    display.get_mut().dispatch_clients(state).expect("failed to dispatch clients");
                }
                Ok(PostAction::Continue)
            },
        )
        .expect("failed to register Wayland display source");

    socket_name
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
