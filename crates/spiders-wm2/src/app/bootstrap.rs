use std::sync::Arc;
use std::sync::mpsc;

use spiders_config::model::{Config, ConfigDiscoveryOptions, ConfigPaths};

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
use tracing::{info, warn};

use crate::frame_sync::FrameSyncState;
use crate::runtime::{RuntimeCommand, WmRuntime};
use crate::scene::adapter::SceneLayoutState;
use crate::state::{ClientState, SpidersWm};

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
    seat.add_keyboard(Default::default(), 200, 25)
        .expect("failed to create keyboard");
    seat.add_pointer();

    let socket_name = init_wayland_listener(display, event_loop);
    let mut model = crate::model::wm::WmModel::default();
    {
        let mut runtime = WmRuntime::new(&mut model);
        let _ = runtime.execute(RuntimeCommand::EnsureDefaultWorkspace {
            name: "1".to_string(),
        });
        let _ = runtime.execute(RuntimeCommand::EnsureSeat {
            seat_id: "winit".into(),
        });
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
        None => match ConfigPaths::discover(ConfigDiscoveryOptions::from_env()) {
            Ok(paths) => paths,
            Err(error) => {
                warn!(%error, "wm2 could not discover config paths; using empty config");
                return (None, Config::default());
            }
        },
    };

    let service = spiders_runtime_js::build_authoring_layout_service(&paths);
    match service.load_config(&paths) {
        Ok(config) => {
            info!(
                authored_config = %paths.authored_config.display(),
                prepared_config = %paths.prepared_config.display(),
                binding_count = config.bindings.len(),
                "loaded wm2 config"
            );
            (Some(paths), config)
        }
        Err(error) => {
            warn!(
                authored_config = %paths.authored_config.display(),
                prepared_config = %paths.prepared_config.display(),
                %error,
                "wm2 failed to load config; using empty config"
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

    state
        .shm_state
        .update_formats(backend.renderer().shm_formats());

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
        Some(
            state
                .dmabuf_state
                .create_global_with_default_feedback::<SpidersWm>(
                    &state.display_handle,
                    default_feedback,
                ),
        )
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
        size: state
            .backend
            .as_ref()
            .expect("winit backend missing during init")
            .window_size(),
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
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));
    let _ = state.runtime().execute(RuntimeCommand::SyncOutput {
        output_id: "winit".into(),
        name: "winit".to_string(),
        logical_width: mode.size.w as u32,
        logical_height: mode.size.h as u32,
    });

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    event_loop
        .handle()
        .insert_source(winit, move |event, _, state| match event {
            WinitEvent::Resized { size, .. } => {
                output.change_current_state(
                    Some(Mode {
                        size,
                        refresh: 60_000,
                    }),
                    None,
                    None,
                    None,
                );
                let _ = state.runtime().execute(RuntimeCommand::SyncOutput {
                    output_id: "winit".into(),
                    name: "winit".to_string(),
                    logical_width: size.w as u32,
                    logical_height: size.h as u32,
                });
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
                    display
                        .get_mut()
                        .dispatch_clients(state)
                        .expect("failed to dispatch clients");
                }
                Ok(PostAction::Continue)
            },
        )
        .expect("failed to register Wayland display source");

    socket_name
}
