use smithay::reexports::{calloop::EventLoop, wayland_server::Display};

mod app;
mod bindings;
mod handlers;
mod runtime;
mod state;
mod wm;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: EventLoop<runtime::SpidersWm2> = EventLoop::try_new()?;
    let display: Display<runtime::SpidersWm2> = Display::new()?;

    let mut state = runtime::SpidersWm2::new(&mut event_loop, display);

    eprintln!("WAYLAND_DISPLAY={:?}", state.runtime.socket_name);

    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
    // init_logging();
    //
    // let mut event_loop: EventLoop<SpidersWm2> = EventLoop::try_new()?;
    // let display: Display<SpidersWm2> = Display::new()?;
    //
    // let mut state = SpidersWm2::new(&mut event_loop, display);
    //
    // crate::winit::init_winit(&mut event_loop, &mut state)?;
    //
    // // SAFETY: this process intentionally sets the child Wayland socket before spawning
    // // optional test clients. This bootstrap binary is single-purpose and does not depend
    // // on concurrent environment mutation correctness.
    // unsafe {
    //     std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);
    // }
    //
    // spawn_client();
    //
    // event_loop.run(None, &mut state, |_| {})?;
}

// fn init_logging() {
//     if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
//         tracing_subscriber::fmt().with_env_filter(env_filter).init();
//     } else {
//         tracing_subscriber::fmt().init();
//     }
// }
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
