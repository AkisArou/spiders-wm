use std::env;
use std::sync::Once;

use tracing_subscriber::EnvFilter;

static INIT: Once = Once::new();

/// Initialize process-wide tracing subscriber once.
///
/// Priority for filter directives:
/// 1) SPIDERS_LOG
/// 2) RUST_LOG
/// 3) sensible default (info)
///
/// If WAYLAND_DEBUG=1 and neither SPIDERS_LOG nor RUST_LOG is set,
/// Wayland crates are raised to debug.
pub fn init(process_name: &str) {
    INIT.call_once(|| {
        let wayland_debug = env::var_os("WAYLAND_DEBUG")
            .map(|value| value == "1" || value == "true" || value == "TRUE")
            .unwrap_or(false);

        let filter = resolve_filter(process_name, wayland_debug);
        let ansi_enabled = should_enable_ansi();

        let _ = tracing_log::LogTracer::init();

        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_ansi(ansi_enabled)
            .with_writer(std::io::stderr)
            .compact()
            .finish();

        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

fn resolve_filter(process_name: &str, wayland_debug: bool) -> EnvFilter {
    if let Ok(value) = env::var("SPIDERS_LOG") {
        return EnvFilter::new(value);
    }

    if let Ok(value) = env::var("RUST_LOG") {
        return EnvFilter::new(value);
    }

    let mut directives = vec![
        "warn".to_string(),
        format!("{}=info", process_name),
        "spiders_config=info".to_string(),
        "spiders_ipc=info".to_string(),
        "spiders_runtime_js=info".to_string(),
        "spiders_wm=info".to_string(),
        "spiders_scene=info".to_string(),
    ];

    if wayland_debug {
        directives.push("wayland_backend=debug".to_string());
        directives.push("wayland_client=debug".to_string());
    }

    EnvFilter::new(directives.join(","))
}

fn should_enable_ansi() -> bool {
    env::var_os("NO_COLOR").is_none() && env::var_os("TERM").is_some()
}
