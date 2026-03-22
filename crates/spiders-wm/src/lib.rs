pub mod actions;
pub mod action_bridge;
pub mod backend;
pub mod layout_adapter;
pub mod model;
pub mod protocol;
pub mod runtime;

use anyhow::Result;
use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::{Config, ConfigDiscoveryOptions, ConfigPaths};
use spiders_runtime_js::{build_authoring_layout_service, DefaultLayoutRuntime};

use crate::model::WmState;

#[derive(Debug)]
pub struct SpidersWm {
    paths: ConfigPaths,
    layout_service: AuthoringLayoutService<DefaultLayoutRuntime>,
    config: Config,
    state: WmState,
}

impl SpidersWm {
    pub fn discover(options: ConfigDiscoveryOptions) -> Result<Self> {
        let paths = ConfigPaths::discover(options)?;
        Self::from_paths(paths)
    }

    pub fn from_paths(paths: ConfigPaths) -> Result<Self> {
        let layout_service = build_authoring_layout_service(&paths);
        let config = layout_service.load_config(&paths)?;
        let state = WmState::from_config(&config);

        Ok(Self {
            paths,
            layout_service,
            config,
            state,
        })
    }

    pub fn connect(&self) -> Result<backend::RiverConnection> {
        backend::RiverConnection::connect(self.paths.clone(), self.config.clone(), self.state.clone())
    }

    pub fn reload_config(&mut self) -> Result<&Config> {
        self.config = self.layout_service.reload_config()?;
        self.state = WmState::from_config(&self.config);
        Ok(&self.config)
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    pub fn state(&self) -> &WmState {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use spiders_config::model::{Config, ConfigPaths};

    use super::*;

    #[test]
    fn runtime_state_bootstraps_from_config_workspaces() {
        let config = Config {
            workspaces: vec!["1".into(), "2".into(), "web".into()],
            ..Config::default()
        };

        let state = WmState::from_config(&config);

        assert_eq!(state.workspace_names(), vec!["1", "2", "web"]);
        assert_eq!(state.current_workspace_name(), Some("1"));
    }

    #[test]
    fn config_paths_are_exposed() {
        let paths = ConfigPaths::new("/tmp/config.ts", "/tmp/config.js");
        let runtime = SpidersWm {
            paths: paths.clone(),
            layout_service: build_authoring_layout_service(&paths),
            config: Config::default(),
            state: WmState::default(),
        };

        assert_eq!(runtime.paths().authored_config, paths.authored_config);
        assert_eq!(runtime.paths().prepared_config, paths.prepared_config);
    }

    #[test]
    fn runtime_state_snapshot_includes_bootstrapped_workspace() {
        let config = Config {
            workspaces: vec!["1".into(), "2".into()],
            ..Config::default()
        };

        let state = WmState::from_config(&config);
        let snapshot = state.as_state_snapshot();

        assert_eq!(snapshot.workspace_names, vec!["1", "2"]);
        assert_eq!(snapshot.current_workspace_id, Some("1".into()));
    }
}
