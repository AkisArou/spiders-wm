use spiders_shared::wm::{LoadedLayout, SelectedLayout};

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
    ) -> Result<Option<LoadedLayout>, LayoutLoadError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct InlineLayoutSourceLoader;

#[derive(Debug, Default, Clone, Copy)]
pub struct FsLayoutSourceLoader;

impl LayoutSourceLoader for InlineLayoutSourceLoader {
    fn load_runtime_source(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<LoadedLayout>, LayoutLoadError> {
        let Some(selected_layout) = config.resolve_selected_layout(workspace)? else {
            return Ok(None);
        };

        Err(LayoutLoadError::MissingRuntimeSource {
            module: selected_layout.module,
        })
    }
}

impl FsLayoutSourceLoader {
    pub fn load_definition(
        &self,
        layout: &LayoutDefinition,
    ) -> Result<LoadedLayout, LayoutLoadError> {
        let runtime_source = std::fs::read_to_string(&layout.module).map_err(|_| {
            LayoutLoadError::MissingRuntimeSource {
                module: layout.module.clone(),
            }
        })?;

        Ok(loaded_layout_definition(layout, runtime_source))
    }
}

impl LayoutSourceLoader for FsLayoutSourceLoader {
    fn load_runtime_source(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<LoadedLayout>, LayoutLoadError> {
        let Some(layout) = config.selected_layout(workspace) else {
            return Ok(None);
        };

        self.load_definition(layout).map(Some)
    }
}

pub fn loaded_layout_definition(layout: &LayoutDefinition, runtime_source: String) -> LoadedLayout {
    LoadedLayout {
        selected: SelectedLayout {
            name: layout.name.clone(),
            module: layout.module.clone(),
            stylesheet: layout.stylesheet.clone(),
        },
        runtime_source,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

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
    fn inline_loader_errors_when_runtime_source_is_missing() {
        let loader = InlineLayoutSourceLoader;
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
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

    #[test]
    fn inline_loader_errors_when_selected_module_has_no_inline_source() {
        let loader = InlineLayoutSourceLoader;
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
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

    #[test]
    fn fs_loader_reads_runtime_source_from_module_path() {
        let loader = FsLayoutSourceLoader;
        let temp_dir = std::env::temp_dir();
        let module_path = temp_dir.join("spiders-layout-loader-test.js");
        fs::write(&module_path, "ctx => ({ type: 'workspace', children: [] })").unwrap();

        let definition = LayoutDefinition {
            name: "master-stack".into(),
            module: module_path.to_string_lossy().into_owned(),
            stylesheet: "workspace { display: flex; }".into(),
        };

        let loaded = loader.load_definition(&definition).unwrap();

        assert_eq!(loaded.selected.module, definition.module);
        assert_eq!(
            loaded.runtime_source,
            "ctx => ({ type: 'workspace', children: [] })"
        );

        let _ = fs::remove_file(module_path);
    }
}
