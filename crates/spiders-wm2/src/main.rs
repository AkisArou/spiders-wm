mod actions;
mod compositor;
mod frame_sync;
mod handlers;
mod model;
mod runtime;
mod state;
mod winit;

use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
use state::SpidersWm;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let mut event_loop: EventLoop<'static, SpidersWm> = EventLoop::try_new()?;
    let display: Display<SpidersWm> = Display::new()?;

    let mut state = SpidersWm::new(&mut event_loop, display);
    winit::init_winit(&mut event_loop, &mut state)?;

    // Child processes should connect to the nested compositor socket.
    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);
    }

    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
}

fn init_logging() {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }
}
