use spiders_config::model::{Config, ConfigPaths, config_discovery_options_from_env};
use spiders_config::runtime::build_authoring_layout_service;
use spiders_runtime_js_native::JavaScriptNativeRuntimeProvider;
use tracing::warn;

pub(crate) fn load_config() -> (Option<ConfigPaths>, Config) {
    let paths = match ConfigPaths::discover(config_discovery_options_from_env()) {
        Ok(paths) => paths,
        Err(error) => {
            warn!(%error, "spiders-wm-x could not discover config paths; using empty config");
            return (None, Config::default());
        }
    };

    let js_provider = JavaScriptNativeRuntimeProvider;
    let service = match build_authoring_layout_service(&paths, &[&js_provider]) {
        Ok(service) => service,
        Err(error) => {
            warn!(
                authored_config = %paths.authored_config.display(),
                %error,
                "spiders-wm-x could not build config runtime; using empty config"
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
                "spiders-wm-x failed to load config; using empty config"
            );
            (Some(paths), Config::default())
        }
    }
}

pub(crate) fn configured_workspace_names(config: &Config) -> Vec<String> {
    if config.workspaces.is_empty() { vec!["1".to_string()] } else { config.workspaces.clone() }
}

pub(crate) fn build_layout_service(
    paths: &ConfigPaths,
) -> Option<spiders_config::authoring_layout::AuthoringLayoutService> {
    let js_provider = JavaScriptNativeRuntimeProvider;

    match build_authoring_layout_service(paths, &[&js_provider]) {
        Ok(service) => Some(service),
        Err(error) => {
            warn!(
                authored_config = %paths.authored_config.display(),
                %error,
                "spiders-wm-x could not build authoring layout service"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_workspace_names_fall_back_to_default() {
        assert_eq!(configured_workspace_names(&Config::default()), vec!["1".to_string()]);
    }

    #[test]
    fn configured_workspace_names_preserve_configured_names() {
        let config = Config {
            workspaces: vec!["1".into(), "web".into(), "chat".into()],
            ..Config::default()
        };

        assert_eq!(configured_workspace_names(&config), vec!["1", "web", "chat"]);
    }
}
