use anyhow::Result;

use crate::backend::BackendApp;
use crate::cli::{CliOptions, RunMode};

pub(crate) fn run() -> Result<()> {
    spiders_logging::init("spiders_wm_x");

    let options = CliOptions::parse(std::env::args().skip(1))?;
    if options.help {
        print_help();
        return Ok(());
    }

    let mut app = BackendApp::connect()?;
    app.log_bootstrap();

    match options.run_mode {
        RunMode::Bootstrap => {}
        RunMode::DumpState => app.print_state_dump()?,
        RunMode::Observe => app.observe(options.event_limit, options.idle_timeout_ms)?,
        RunMode::Manage => app.manage()?,
    }

    Ok(())
}

fn print_help() {
    println!(
        "spiders-wm-x\n\nUSAGE:\n    cargo run -p spiders-wm-x -- [OPTIONS]\n\nOPTIONS:\n    --dump-state           Print the bootstrapped WM state snapshot as JSON\n    --observe              Attach to the target X server and log events\n    --manage               Try to become the X11 window manager on the target screen\n    --event-limit <count>  Stop observation after <count> events\n    --idle-timeout-ms <n>  Stop observation after <n> ms without events\n    -h, --help             Show this help text"
    );
}
