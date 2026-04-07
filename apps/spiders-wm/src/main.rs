mod actions;
mod app;
mod compositor;
mod debug;
mod frame_sync;
mod handlers;
mod ipc;
mod runtime;
mod scene;
mod state;

use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
use state::SpidersWm;
use tracing::info;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let mut event_loop: EventLoop<'static, SpidersWm> = EventLoop::try_new()?;
    let display: Display<SpidersWm> = Display::new()?;

    let mut state = app::bootstrap::build_state(&mut event_loop, display);
    app::bootstrap::init_winit(&mut event_loop, &mut state)?;
    if state.debug.profile().enabled() {
        state.dump_debug_state();
    }
    info!(wayland_display = %state.socket_name.to_string_lossy(), ipc_socket = ?state.ipc_socket_path, "wm initialized sockets");
    info!(debug_profile = ?state.debug.profile(), debug_output_dir = ?state.debug.output_dir(), "wm debug platform initialized");

    // Child processes should connect to the nested compositor socket.
    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);
        if let Some(ipc_socket_path) = state.ipc_socket_path.as_ref() {
            std::env::set_var("SPIDERS_WM_IPC_SOCKET", ipc_socket_path);
        }
    }

    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
}

fn init_logging() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_from_env("SPIDERS_LOG"))
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .init();
}
