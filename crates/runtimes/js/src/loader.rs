use spiders_shared::runtime::{
    PreparedLayout, PreparedStylesheet, PreparedStylesheets, RuntimeError, SelectedLayout,
};

use spiders_config::model::{Config, LayoutConfigError, LayoutDefinition};

use crate::module_graph::{JavaScriptModule, JavaScriptModuleGraph};
use crate::payload::{decode_runtime_graph_payload, encode_runtime_graph_payload};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePathResolver {
    pub project_root: std::path::PathBuf,
    pub runtime_root: std::path::PathBuf,
}

impl RuntimePathResolver {
    pub fn new(
        project_root: impl Into<std::path::PathBuf>,
        runtime_root: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            project_root: project_root.into(),
            runtime_root: runtime_root.into(),
        }
    }

    pub fn resolve_module_path(&self, module: &str) -> std::path::PathBuf {
        let module_path = std::path::Path::new(module);
        if module_path.is_absolute() {
            return module_path.to_path_buf();
        }

        let runtime_candidate = self.runtime_root.join(module_path);
        if runtime_candidate.exists() {
            return runtime_candidate;
        }

        self.project_root.join(module_path)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LayoutLoadError {
    #[error(transparent)]
    Config(#[from] LayoutConfigError),
    #[error("layout module `{module}` graph is unavailable")]
    MissingRuntimeSource { module: String },
    #[error("layout module `{module}` runtime payload is invalid: {message}")]
    InvalidRuntimePayload { module: String, message: String },
}

#[derive(Debug, Default, Clone, Copy)]
pub struct InlineLayoutSourceLoader;

#[derive(Debug, Default, Clone, Copy)]
pub struct FsLayoutSourceLoader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeProjectLayoutSourceLoader {
    resolver: RuntimePathResolver,
}

pub trait JsLayoutSourceLoader: std::fmt::Debug {
    fn load_runtime_source(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayout>, RuntimeError>;
}

impl RuntimeProjectLayoutSourceLoader {
    pub fn new(resolver: RuntimePathResolver) -> Self {
        Self { resolver }
    }

    pub fn load_definition(
        &self,
        layout: &LayoutDefinition,
    ) -> Result<PreparedLayout, LayoutLoadError> {
        let module_path = self.resolver.resolve_module_path(&layout.module);
        if let Some(runtime_payload) = layout.runtime_cache_payload.clone() {
            let runtime_graph = decode_runtime_graph_payload(&runtime_payload).map_err(|error| {
                LayoutLoadError::InvalidRuntimePayload {
                    module: layout.module.clone(),
                    message: error.to_string(),
                }
            })?;
            return Ok(loaded_layout_definition(
                layout,
                module_path.to_string_lossy().into_owned(),
                runtime_graph,
            ));
        }
        let runtime_source = std::fs::read_to_string(&module_path).map_err(|_| {
            LayoutLoadError::MissingRuntimeSource {
                module: module_path.to_string_lossy().into_owned(),
            }
        })?;

        Ok(loaded_layout_definition(
            layout,
            module_path.to_string_lossy().into_owned(),
            single_module_graph(module_path.to_string_lossy().into_owned(), runtime_source),
        ))
    }
}

impl JsLayoutSourceLoader for InlineLayoutSourceLoader {
    fn load_runtime_source(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayout>, RuntimeError> {
        let Some(selected_layout) =
            config
                .resolve_selected_layout(workspace)
                .map_err(|error| RuntimeError::Config {
                    message: error.to_string(),
                })?
        else {
            return Ok(None);
        };

        Err(RuntimeError::MissingRuntimeSource {
            name: selected_layout.module,
        })
    }
}

impl FsLayoutSourceLoader {
    pub fn load_definition(
        &self,
        layout: &LayoutDefinition,
    ) -> Result<PreparedLayout, LayoutLoadError> {
        if let Some(runtime_payload) = layout.runtime_cache_payload.clone() {
            let runtime_graph = decode_runtime_graph_payload(&runtime_payload).map_err(|error| {
                LayoutLoadError::InvalidRuntimePayload {
                    module: layout.module.clone(),
                    message: error.to_string(),
                }
            })?;
            return Ok(loaded_layout_definition(
                layout,
                layout.module.clone(),
                runtime_graph,
            ));
        }
        let runtime_source = std::fs::read_to_string(&layout.module).map_err(|_| {
            LayoutLoadError::MissingRuntimeSource {
                module: layout.module.clone(),
            }
        })?;

        Ok(loaded_layout_definition(
            layout,
            layout.module.clone(),
            single_module_graph(layout.module.clone(), runtime_source),
        ))
    }
}

impl JsLayoutSourceLoader for FsLayoutSourceLoader {
    fn load_runtime_source(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayout>, RuntimeError> {
        let Some(layout) = config.selected_layout(workspace) else {
            return Ok(None);
        };

        self.load_definition(layout)
            .map(Some)
            .map_err(|error| RuntimeError::Other {
                message: error.to_string(),
            })
    }
}

impl JsLayoutSourceLoader for RuntimeProjectLayoutSourceLoader {
    fn load_runtime_source(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayout>, RuntimeError> {
        let Some(layout) = config.selected_layout(workspace) else {
            return Ok(None);
        };

        self.load_definition(layout)
            .map(Some)
            .map_err(|error| RuntimeError::Other {
                message: error.to_string(),
            })
    }
}

pub fn loaded_layout_definition(
    layout: &LayoutDefinition,
    module: String,
    runtime_graph: JavaScriptModuleGraph,
) -> PreparedLayout {
    PreparedLayout {
        selected: SelectedLayout {
            name: layout.name.clone(),
            directory: layout.directory.clone(),
            module,
        },
        runtime_payload: layout
            .runtime_cache_payload
            .clone()
            .unwrap_or_else(|| encode_runtime_graph_payload(&runtime_graph)),
        stylesheets: PreparedStylesheets {
            layout: layout
                .stylesheet_path
                .as_ref()
                .map(|path| load_stylesheet_asset(path)),
        },
    }
}

fn load_stylesheet_asset(path: &str) -> PreparedStylesheet {
    PreparedStylesheet {
        path: path.into(),
        source: std::fs::read_to_string(path).unwrap_or_default(),
    }
}

fn single_module_graph(
    module: String,
    source: String,
) -> JavaScriptModuleGraph {
    JavaScriptModuleGraph {
        entry: module.clone(),
        modules: vec![JavaScriptModule {
            specifier: module,
            source: normalize_runtime_module_source(&source),
            resolved_imports: Default::default(),
        }],
    }
}

fn normalize_runtime_module_source(source: &str) -> String {
    let trimmed = source.trim();
    if trimmed.contains("export default")
        || trimmed.contains("export {")
        || trimmed.contains("export function")
    {
        source.to_owned()
    } else {
        format!("export default ({trimmed});")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use spiders_tree::{OutputId, WorkspaceId};
    use spiders_shared::wm::{LayoutRef, WorkspaceSnapshot};

    use super::*;
    use crate::payload::decode_runtime_graph_payload;
    use spiders_config::model::{Config, LayoutDefinition};

    fn workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_workspaces: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef {
                name: "master-stack".into(),
            }),
        }
    }

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spiders-config/tests/fixtures")
    }

    #[test]
    fn inline_loader_errors_when_runtime_source_is_missing() {
        let loader = InlineLayoutSourceLoader;
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_cache_payload: None,
            }],
            ..Config::default()
        };

        let error = loader
            .load_runtime_source(&config, &workspace())
            .unwrap_err();

        assert_eq!(
            error,
            RuntimeError::MissingRuntimeSource {
                name: "layouts/master-stack.js".into(),
            }
        );
    }

    #[test]
    fn inline_loader_errors_when_selected_module_has_no_inline_source() {
        let loader = InlineLayoutSourceLoader;
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_cache_payload: None,
            }],
            ..Config::default()
        };

        let error = loader
            .load_runtime_source(&config, &workspace())
            .unwrap_err();

        assert_eq!(
            error,
            RuntimeError::MissingRuntimeSource {
                name: "layouts/master-stack.js".into(),
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
            directory: "layouts/master-stack".into(),
            module: module_path.to_string_lossy().into_owned(),
            stylesheet_path: Some("layouts/master-stack/index.css".into()),
            runtime_cache_payload: None,
        };

        let loaded = loader.load_definition(&definition).unwrap();
        let loaded_graph = decode_runtime_graph_payload(&loaded.runtime_payload).unwrap();

        assert_eq!(loaded.selected.module, definition.module);
        assert_eq!(loaded_graph.entry, definition.module);
        assert_eq!(loaded_graph.modules.len(), 1);
        assert_eq!(
            loaded_graph.modules[0].source,
            "export default (ctx => ({ type: 'workspace', children: [] }));"
        );

        let _ = fs::remove_file(module_path);
    }

    #[test]
    fn runtime_path_resolver_prefers_runtime_root_then_project_root() {
        let temp_dir = std::env::temp_dir();
        let project_root = temp_dir.join("spiders-loader-project");
        let runtime_root = temp_dir.join("spiders-loader-runtime");
        let _ = fs::create_dir_all(project_root.join("layouts"));
        let _ = fs::create_dir_all(runtime_root.join("layouts"));

        let resolver = RuntimePathResolver::new(&project_root, &runtime_root);
        let runtime_path = runtime_root.join("layouts/master-stack.js");
        fs::write(&runtime_path, "runtime").unwrap();

        assert_eq!(
            resolver.resolve_module_path("layouts/master-stack.js"),
            runtime_path
        );

        let _ = fs::remove_file(runtime_path);
        let project_path = project_root.join("layouts/master-stack.js");
        fs::write(&project_path, "project").unwrap();

        assert_eq!(
            resolver.resolve_module_path("layouts/master-stack.js"),
            project_path
        );

        let _ = fs::remove_file(project_path);
    }

    #[test]
    fn runtime_project_loader_reads_from_resolved_runtime_location() {
        let fixtures = fixture_root();
        let project_root = fixtures.join("project");
        let runtime_root = fixtures.join("runtime");
        let module_path = runtime_root.join("layouts/master-stack.js");

        let loader = RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(
            &project_root,
            &runtime_root,
        ));
        let definition = LayoutDefinition {
            name: "master-stack".into(),
            directory: "layouts/master-stack".into(),
            module: "layouts/master-stack.js".into(),
            stylesheet_path: Some("layouts/master-stack/index.css".into()),
            runtime_cache_payload: None,
        };

        let loaded = loader.load_definition(&definition).unwrap();
        let loaded_graph = decode_runtime_graph_payload(&loaded.runtime_payload).unwrap();

        assert_eq!(loaded.selected.module, module_path.to_string_lossy());
        assert!(loaded_graph.modules[0].source.contains("workspace"));
    }

    #[test]
    fn runtime_project_loader_falls_back_to_project_root_fixture() {
        let fixtures = fixture_root();
        let project_root = fixtures.join("project");
        let runtime_root = fixtures.join("runtime-missing");

        let loader = RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(
            &project_root,
            &runtime_root,
        ));
        let definition = LayoutDefinition {
            name: "fallback".into(),
            directory: "layouts/fallback".into(),
            module: "layouts/fallback.js".into(),
            stylesheet_path: Some("layouts/fallback/index.css".into()),
            runtime_cache_payload: None,
        };

        let loaded = loader.load_definition(&definition).unwrap();
        let loaded_graph = decode_runtime_graph_payload(&loaded.runtime_payload).unwrap();

        assert!(loaded.selected.module.ends_with("layouts/fallback.js"));
        assert!(
            loaded_graph.modules[0]
                .source
                .contains("fallback-group")
        );
    }

    #[test]
    fn loaded_layout_definition_preserves_stylesheet_paths_when_source_missing() {
        let missing_global = "/tmp/spiders-wm-missing-global.css";
        let missing_layout = "/tmp/spiders-wm-missing-layout.css";
        let definition = LayoutDefinition {
            name: "master-stack".into(),
            directory: "layouts/master-stack".into(),
            module: "layouts/master-stack.js".into(),
            stylesheet_path: Some(missing_layout.into()),
            runtime_cache_payload: None,
        };
        let config = Config {
            layouts: vec![definition.clone()],
            global_stylesheet_path: Some(missing_global.into()),
            ..Config::default()
        };

        let loaded = loaded_layout_definition(
            &definition,
            definition.module.clone(),
            single_module_graph(definition.module.clone(), "export default () => null".into()),
        );

        assert_eq!(
            loaded.stylesheets.layout.as_ref().map(|sheet| sheet.path.as_str()),
            Some(missing_layout)
        );
        assert_eq!(
            loaded.stylesheets.layout.as_ref().map(|sheet| sheet.source.as_str()),
            Some("")
        );
        assert_eq!(config.global_stylesheet_path.as_deref(), Some(missing_global));
    }
}
