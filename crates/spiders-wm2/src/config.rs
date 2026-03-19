use std::collections::BTreeMap;

use spiders_config::model::Config;
use spiders_config::model::{LayoutDefinition, LayoutSelectionConfig};
use spiders_shared::layout::SourceLayoutNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfigSource {
    #[default]
    BuiltInDefault,
    PreparedConfig,
    AuthoredConfig,
}

#[derive(Debug, Default)]
pub struct ConfigRuntimeState {
    revision: u64,
    layout_tree_revision: u64,
    source: ConfigSource,
    current: Config,
    layout_trees: BTreeMap<String, SourceLayoutNode>,
}

impl ConfigRuntimeState {
    pub fn current(&self) -> &Config {
        &self.current
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn source(&self) -> ConfigSource {
        self.source
    }

    pub fn layout_tree_revision(&self) -> u64 {
        self.layout_tree_revision
    }

    pub fn layout_tree(&self, layout_name: &str) -> Option<&SourceLayoutNode> {
        self.layout_trees.get(layout_name)
    }

    pub fn replace(&mut self, config: Config, source: ConfigSource) {
        self.revision += 1;
        self.source = source;
        self.current = config;
        self.layout_trees.clear();
    }

    pub fn install_layout_tree(&mut self, layout_name: impl Into<String>, tree: SourceLayoutNode) {
        self.layout_tree_revision += 1;
        self.layout_trees.insert(layout_name.into(), tree);
    }
}

pub fn built_in_default_config() -> Config {
    Config {
        workspaces: (1..=9).map(|index| index.to_string()).collect(),
        layouts: vec![LayoutDefinition {
            name: "columns".into(),
            module: "builtin://columns".into(),
            stylesheet: "workspace { display: flex; flex-direction: row; width: 100%; height: 100%; } window { flex-basis: 0px; flex-grow: 1; min-width: 0px; height: 100%; }".into(),
            effects_stylesheet: String::new(),
            runtime_graph: None,
        }],
        layout_selection: LayoutSelectionConfig {
            default: Some("columns".into()),
            ..LayoutSelectionConfig::default()
        },
        ..Config::default()
    }
}

#[cfg(test)]
mod tests {
    use spiders_config::model::LayoutDefinition;
    use spiders_shared::layout::SourceLayoutNode;

    use super::{ConfigRuntimeState, ConfigSource};

    #[test]
    fn replacing_config_updates_revision_and_source() {
        let mut state = ConfigRuntimeState::default();
        let mut config = spiders_config::model::Config::default();
        config.layouts.push(LayoutDefinition {
            name: "columns".into(),
            module: "layouts/columns.js".into(),
            ..LayoutDefinition::default()
        });

        state.replace(config, ConfigSource::PreparedConfig);

        assert_eq!(state.revision(), 1);
        assert_eq!(state.source(), ConfigSource::PreparedConfig);
        assert_eq!(state.current().layouts.len(), 1);
    }

    #[test]
    fn installing_layout_tree_tracks_layout_tree_revision() {
        let mut state = ConfigRuntimeState::default();
        state.install_layout_tree(
            "columns",
            SourceLayoutNode::Workspace {
                meta: Default::default(),
                children: vec![],
            },
        );

        assert_eq!(state.layout_tree_revision(), 1);
        assert!(state.layout_tree("columns").is_some());
    }
}
