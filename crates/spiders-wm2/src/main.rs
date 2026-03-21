use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
use tracing_subscriber::{fmt::writer::BoxMakeWriter, EnvFilter};

mod actions;
mod app;
mod backend;
mod bindings;
mod command;
mod config;
mod handlers;
mod layout;
mod layout_runtime;
mod model;
mod placement;
mod render;
mod runtime;
mod runtime_support;
mod transactions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging()?;

    let mut event_loop: EventLoop<runtime::SpidersWm2> = EventLoop::try_new()?;
    let display: Display<runtime::SpidersWm2> = Display::new()?;

    let mut state = runtime::SpidersWm2::new(&mut event_loop, display);

    crate::backend::winit::init_winit(&mut event_loop, &mut state)?;

    eprintln!("WAYLAND_DISPLAY={:?}", state.runtime.socket_name);

    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
}

fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    let log_path =
        std::env::var("SPIDERS_WM2_LOG_PATH").unwrap_or_else(|_| "/tmp/spiders-wm2.log".into());
    let writer = std::fs::File::create(&log_path)?;
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(
            "info,spiders_wm2::transactions=trace,spiders_wm2::layout=trace,spiders_wm2::runtime_debug=trace",
        )
    });

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(BoxMakeWriter::new(writer))
        .with_ansi(false)
        .init();

    tracing::info!(log_path, "wm2 logging initialized");
    Ok(())
}
//
// fn spawn_client() {
//     let mut args = std::env::args().skip(1);
//     let flag = args.next();
//     let arg = args.next();
//
//     match (flag.as_deref(), arg) {
//         (Some("-c") | Some("--command"), Some(command)) => {
//             std::process::Command::new(command).spawn().ok();
//         }
//         _ => {
//             std::process::Command::new("weston-terminal").spawn().ok();
//         }
//     }
// }
