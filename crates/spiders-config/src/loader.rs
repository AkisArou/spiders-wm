use spiders_shared::wm::SelectedLayout;

use crate::model::{Config, LayoutConfigError, LayoutDefinition};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LayoutLoadError {
    #[error(transparent)]
    Config(#[from] LayoutConfigError),
    #[error("layout module `{module}` source is unavailable")]
    MissingRuntimeSource { module: String },
}

pub trait LayoutSourceLoader {
    fn load_runtime_source(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, LayoutLoadError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct InlineLayoutSourceLoader;

impl LayoutSourceLoader for InlineLayoutSourceLoader {
    fn load_runtime_source(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, LayoutLoadError> {
        let Some(selected_layout) = config.resolve_selected_layout(workspace)? else {
            return Ok(None);
        };

        let runtime_source = selected_layout.runtime_source.clone().ok_or_else(|| {
            LayoutLoadError::MissingRuntimeSource {
                module: selected_layout.module.clone(),
            }
        })?;

        Ok(Some(SelectedLayout {
            runtime_source: Some(runtime_source),
            ..selected_layout
        }))
    }
}

pub fn loaded_layout_definition(
    layout: &LayoutDefinition,
    runtime_source: String,
) -> SelectedLayout {
    SelectedLayout {
        name: layout.name.clone(),
        module: layout.module.clone(),
        stylesheet: layout.stylesheet.clone(),
        runtime_source: Some(runtime_source),
    }
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::{OutputId, WorkspaceId};
    use spiders_shared::wm::{LayoutRef, WorkspaceSnapshot};

    use super::*;
    use crate::model::{Config, LayoutDefinition};

    fn workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_tags: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef {
                name: "master-stack".into(),
            }),
        }
    }

    #[test]
    fn inline_loader_returns_selected_layout_with_runtime_source() {
        let loader = InlineLayoutSourceLoader;
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
                runtime_source: Some("ctx => ({ type: 'workspace', children: [] })".into()),
            }],
            ..Config::default()
        };

        let selected = loader
            .load_runtime_source(&config, &workspace())
            .unwrap()
            .unwrap();

        assert_eq!(selected.module, "layouts/master-stack.js");
        assert_eq!(
            selected.runtime_source.as_deref(),
            Some("ctx => ({ type: 'workspace', children: [] })")
        );
    }

    #[test]
    fn inline_loader_errors_when_runtime_source_is_missing() {
        let loader = InlineLayoutSourceLoader;
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
                runtime_source: None,
            }],
            ..Config::default()
        };

        let error = loader
            .load_runtime_source(&config, &workspace())
            .unwrap_err();

        assert_eq!(
            error,
            LayoutLoadError::MissingRuntimeSource {
                module: "layouts/master-stack.js".into(),
            }
        );
    }
}
