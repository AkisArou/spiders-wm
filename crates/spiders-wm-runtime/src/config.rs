use spiders_config::model::{Config, ConfigDiscoveryOptions, ConfigPaths};
use tracing::{info, warn};

pub use spiders_config::authoring_layout::AuthoringLayoutService;
pub use spiders_runtime_js::DefaultLayoutRuntime;

pub fn build_authoring_layout_service(
    paths: &ConfigPaths,
) -> AuthoringLayoutService<DefaultLayoutRuntime> {
    spiders_runtime_js::build_authoring_layout_service(paths)
}

pub fn config_discovery_options_from_env() -> ConfigDiscoveryOptions {
    ConfigDiscoveryOptions {
        home_dir: std::env::var_os("SPIDERS_WM_HOME")
            .or_else(|| std::env::var_os("HOME"))
            .map(std::path::PathBuf::from),
        config_dir_override: std::env::var_os("SPIDERS_WM_CONFIG_DIR")
            .map(std::path::PathBuf::from),
        cache_dir_override: std::env::var_os("SPIDERS_WM_CACHE_DIR")
            .map(std::path::PathBuf::from),
        authored_config_override: std::env::var_os("SPIDERS_WM_AUTHORED_CONFIG")
            .map(std::path::PathBuf::from),
    }
}

pub fn load_config(
    existing_paths: Option<ConfigPaths>,
    discovery_options: ConfigDiscoveryOptions,
) -> (Option<ConfigPaths>, Config) {
    let paths = match existing_paths {
        Some(paths) => paths,
        None => match ConfigPaths::discover(discovery_options) {
            Ok(paths) => paths,
            Err(error) => {
                warn!(%error, "wm runtime could not discover config paths; using empty config");
                return (None, Config::default());
            }
        },
    };

    let service = build_authoring_layout_service(&paths);
    match service.load_config(&paths) {
        Ok(config) => {
            info!(
                authored_config = %paths.authored_config.display(),
                prepared_config = %paths.prepared_config.display(),
                binding_count = config.bindings.len(),
                "loaded wm runtime config"
            );
            (Some(paths), config)
        }
        Err(error) => {
            warn!(
                authored_config = %paths.authored_config.display(),
                prepared_config = %paths.prepared_config.display(),
                %error,
                "wm runtime failed to load config; using empty config"
            );
            (Some(paths), Config::default())
        }
    }
}

pub fn parse_workspace_names(source: &str) -> Vec<String> {
    let workspaces_source = source
        .split("workspaces:")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .unwrap_or_default();
    let mut workspaces = Vec::new();
    let mut remaining = workspaces_source;

    while let Some(start) = remaining.find('"') {
        let after_start = &remaining[start + 1..];
        let Some(end) = after_start.find('"') else {
            break;
        };
        let name = &after_start[..end];
        if !name.is_empty() {
            workspaces.push(name.to_string());
        }
        remaining = &after_start[end + 1..];
    }

    if workspaces.is_empty() {
        vec!["1".to_string(), "2".to_string(), "3".to_string()]
    } else {
        workspaces
    }
}
