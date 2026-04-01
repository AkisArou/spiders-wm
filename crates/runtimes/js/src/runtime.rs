use std::collections::BTreeMap;

use serde::Deserialize;
use spiders_config::model::Config;
use spiders_scene::ast::{
    AuthoredLayoutNode, AuthoredNodeMeta, LayoutValidationError, ValidatedLayoutTree,
};
use spiders_shared::runtime::layout_context::LayoutEvaluationContext;
use spiders_shared::runtime::prepared_layout::{PreparedLayout, SelectedLayout};
use spiders_shared::runtime::runtime_contract::{
    AuthoringLayoutRuntime, LayoutModuleContract, PreparedLayoutRuntime,
};
use spiders_shared::runtime::runtime_error::RuntimeError;
use spiders_tree::{SlotTake, SourceLayoutNode};
use tracing::{debug, warn};

use crate::loader::{InlineLayoutSourceLoader, JsLayoutSourceLoader};
use crate::module_graph::{JavaScriptModule, JavaScriptModuleGraph};
use crate::module_graph_runtime::call_entry_export_with_json_arg;
use crate::payload::{decode_runtime_graph_payload, encode_runtime_graph_payload};

#[cfg(test)]
use crate::loader::FsLayoutSourceLoader;

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodePath(Vec<String>);

impl DecodePath {
    fn root() -> Self {
        Self(vec!["root".into()])
    }

    fn field(&self, field: &str) -> Self {
        let mut path = self.0.clone();
        path.push(field.to_owned());
        Self(path)
    }

    fn index(&self, index: usize) -> Self {
        let mut path = self.0.clone();
        path.push(format!("[{index}]"));
        Self(path)
    }

    fn display(&self) -> String {
        self.0.join(".")
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
struct JsAuthoredNodeMeta {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    class: JsClassName,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    data: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(untagged)]
enum JsClassName {
    #[default]
    Missing,
    One(String),
    Many(Vec<String>),
}

impl JsClassName {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::Missing => Vec::new(),
            Self::One(value) => value
                .split_ascii_whitespace()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect(),
            Self::Many(values) => values,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
struct JsAuthoredNodeProps {
    #[serde(flatten)]
    meta: JsAuthoredNodeMeta,
    #[serde(default, rename = "match")]
    match_expr: Option<String>,
    #[serde(default)]
    take: Option<SlotTake>,
}

impl JsAuthoredNodeProps {
    fn merge(self, nested: JsAuthoredNodeProps) -> Self {
        Self {
            meta: self.meta.merge(nested.meta),
            match_expr: nested.match_expr.or(self.match_expr),
            take: nested.take.or(self.take),
        }
    }
}

impl JsAuthoredNodeMeta {
    fn merge(self, nested: JsAuthoredNodeMeta) -> Self {
        Self {
            id: nested.id.or(self.id),
            class: match nested.class {
                JsClassName::Missing => self.class,
                other => other,
            },
            name: nested.name.or(self.name),
            data: if nested.data.is_empty() {
                self.data
            } else {
                nested.data
            },
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum JsAuthoredLayoutNode {
    Workspace {
        #[serde(default)]
        props: Option<JsAuthoredNodeProps>,
        #[serde(flatten)]
        legacy: JsAuthoredNodeProps,
        #[serde(default)]
        children: Vec<JsAuthoredLayoutNode>,
    },
    Group {
        #[serde(default)]
        props: Option<JsAuthoredNodeProps>,
        #[serde(flatten)]
        legacy: JsAuthoredNodeProps,
        #[serde(default)]
        children: Vec<JsAuthoredLayoutNode>,
    },
    Window {
        #[serde(default)]
        props: Option<JsAuthoredNodeProps>,
        #[serde(flatten)]
        legacy: JsAuthoredNodeProps,
    },
    Slot {
        #[serde(default)]
        props: Option<JsAuthoredNodeProps>,
        #[serde(flatten)]
        legacy: JsAuthoredNodeProps,
    },
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PreparedLayoutRuntimeError {
    #[error("layout `{name}` evaluation is not implemented yet")]
    NotImplemented { name: String },
    #[error(transparent)]
    Validation(#[from] LayoutValidationError),
    #[error("javascript evaluation failed: {message}")]
    JavaScript { message: String },
    #[error("layout module `{name}` did not provide `{export}` export")]
    MissingExport { name: String, export: String },
    #[error("layout module `{name}` export `{export}` is not callable")]
    NonCallableExport { name: String, export: String },
    #[error("js to layout conversion failed for layout `{name}`: {message}")]
    ValueConversion { name: String, message: String },
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StubPreparedLayoutRuntime;

#[derive(Debug)]
pub struct QuickJsPreparedLayoutRuntime<L = InlineLayoutSourceLoader> {
    contract: LayoutModuleContract,
    loader: L,
}

impl Default for QuickJsPreparedLayoutRuntime<InlineLayoutSourceLoader> {
    fn default() -> Self {
        Self {
            contract: LayoutModuleContract::default(),
            loader: InlineLayoutSourceLoader,
        }
    }
}

impl QuickJsPreparedLayoutRuntime<InlineLayoutSourceLoader> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<L> QuickJsPreparedLayoutRuntime<L> {
    pub fn with_loader(loader: L) -> Self {
        Self {
            contract: LayoutModuleContract::default(),
            loader,
        }
    }

    pub fn evaluate_module_source(
        &self,
        selected_layout: &SelectedLayout,
        context: &LayoutEvaluationContext,
        source: &str,
    ) -> Result<SourceLayoutNode, PreparedLayoutRuntimeError> {
        self.evaluate_module_graph(
            selected_layout,
            context,
            &JavaScriptModuleGraph {
                entry: selected_layout.module.clone(),
                modules: vec![JavaScriptModule {
                    specifier: selected_layout.module.clone(),
                    source: format!("export default ({source});"),
                    resolved_imports: Default::default(),
                }],
            },
        )
    }

    pub fn normalize_authored_layout(
        &self,
        root: AuthoredLayoutNode,
    ) -> Result<SourceLayoutNode, PreparedLayoutRuntimeError> {
        Ok(ValidatedLayoutTree::from_authored(root)?.root)
    }

    fn evaluate_module_graph(
        &self,
        selected_layout: &SelectedLayout,
        context: &LayoutEvaluationContext,
        graph: &JavaScriptModuleGraph,
    ) -> Result<SourceLayoutNode, PreparedLayoutRuntimeError> {
        let context_value = serde_json::to_value(context).map_err(|error| {
            PreparedLayoutRuntimeError::JavaScript {
                message: error.to_string(),
            }
        })?;

        let json = call_entry_export_with_json_arg(
            graph,
            &selected_layout.module,
            &self.contract.export_name,
            &context_value,
        )
        .map_err(|error| match error {
            crate::module_graph_runtime::ModuleGraphRuntimeError::JavaScript { message } => {
                PreparedLayoutRuntimeError::JavaScript { message }
            }
            crate::module_graph_runtime::ModuleGraphRuntimeError::MissingExport {
                name,
                export,
            } => PreparedLayoutRuntimeError::MissingExport { name, export },
            crate::module_graph_runtime::ModuleGraphRuntimeError::NonCallableExport {
                name,
                export,
            } => PreparedLayoutRuntimeError::NonCallableExport { name, export },
        })?
        .ok_or_else(|| PreparedLayoutRuntimeError::ValueConversion {
            name: selected_layout.name.clone(),
            message: "layout function returned undefined".into(),
        })?;

        let authored =
            decode_authored_layout_node(&json, &DecodePath::root()).map_err(|message| {
                PreparedLayoutRuntimeError::ValueConversion {
                    name: selected_layout.name.clone(),
                    message,
                }
            })?;

        self.normalize_authored_layout(authored)
    }
}

impl<L: JsLayoutSourceLoader> QuickJsPreparedLayoutRuntime<L> {
    pub fn prepare_layout(
        &self,
        config: &Config,
        workspace: &spiders_shared::snapshot::WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayout>, RuntimeError> {
        debug!(workspace_id = %workspace.id, workspace_name = %workspace.name, "loading runtime source for layout preparation");
        let result = self.loader.load_runtime_source(config, workspace);
        if let Err(error) = &result {
            warn!(
                %error,
                workspace_id = %workspace.id,
                workspace_name = %workspace.name,
                "failed to load runtime source for workspace"
            );
        }
        result
    }
}

impl PreparedLayoutRuntime for StubPreparedLayoutRuntime {
    type Config = Config;

    fn prepare_layout(
        &self,
        config: &Self::Config,
        workspace: &spiders_shared::snapshot::WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayout>, RuntimeError> {
        Ok(config
            .resolve_selected_layout(workspace)
            .map_err(|error| RuntimeError::Config {
                message: error.to_string(),
            })?
            .map(|selected| PreparedLayout {
                selected,
                runtime_payload: encode_runtime_graph_payload(&JavaScriptModuleGraph {
                    entry: String::new(),
                    modules: Vec::new(),
                }),
                stylesheets: spiders_shared::runtime::prepared_layout::PreparedStylesheets::default(
                ),
            }))
    }

    fn build_context(
        &self,
        state: &spiders_shared::snapshot::StateSnapshot,
        workspace: &spiders_shared::snapshot::WorkspaceSnapshot,
        artifact: Option<&PreparedLayout>,
    ) -> LayoutEvaluationContext {
        state.layout_context(
            workspace,
            artifact.map(|artifact| artifact.selected.clone()),
        )
    }

    fn evaluate_layout(
        &self,
        loaded_layout: &PreparedLayout,
        _context: &LayoutEvaluationContext,
    ) -> Result<SourceLayoutNode, RuntimeError> {
        Err(RuntimeError::NotImplemented(format!(
            "layout {}",
            loaded_layout.selected.name
        )))
    }

    fn contract(&self) -> LayoutModuleContract {
        LayoutModuleContract::default()
    }
}

impl<L: JsLayoutSourceLoader> PreparedLayoutRuntime for QuickJsPreparedLayoutRuntime<L> {
    type Config = Config;

    fn prepare_layout(
        &self,
        config: &Self::Config,
        workspace: &spiders_shared::snapshot::WorkspaceSnapshot,
    ) -> Result<Option<PreparedLayout>, RuntimeError> {
        QuickJsPreparedLayoutRuntime::prepare_layout(self, config, workspace)
    }

    fn build_context(
        &self,
        state: &spiders_shared::snapshot::StateSnapshot,
        workspace: &spiders_shared::snapshot::WorkspaceSnapshot,
        artifact: Option<&PreparedLayout>,
    ) -> LayoutEvaluationContext {
        state.layout_context(
            workspace,
            artifact.map(|artifact| artifact.selected.clone()),
        )
    }

    fn evaluate_layout(
        &self,
        loaded_layout: &PreparedLayout,
        context: &LayoutEvaluationContext,
    ) -> Result<SourceLayoutNode, RuntimeError> {
        debug!(layout = %loaded_layout.selected.name, module = %loaded_layout.selected.module, "evaluating prepared layout module graph");
        let runtime_graph = decode_runtime_graph_payload(&loaded_layout.runtime_payload)?;
        let result = self.evaluate_module_graph(&loaded_layout.selected, context, &runtime_graph);

        if let Err(error) = &result {
            warn!(layout = %loaded_layout.selected.name, module = %loaded_layout.selected.module, %error, "layout evaluation failed");
        }

        result.map_err(|error| RuntimeError::Other {
            message: error.to_string(),
        })
    }

    fn contract(&self) -> LayoutModuleContract {
        self.contract.clone()
    }
}

impl<L: JsLayoutSourceLoader> AuthoringLayoutRuntime for QuickJsPreparedLayoutRuntime<L> {
    fn load_authored_config(&self, path: &std::path::Path) -> Result<Self::Config, RuntimeError> {
        debug!(path = %path.display(), "loading authored config");
        let result = crate::authored::load_authored_config(path);
        if let Err(error) = &result {
            warn!(path = %path.display(), %error, "failed loading authored config");
        }
        result.map_err(|error| RuntimeError::Config {
            message: error.to_string(),
        })
    }

    fn load_prepared_config(&self, path: &std::path::Path) -> Result<Self::Config, RuntimeError> {
        debug!(path = %path.display(), "loading prepared config");
        let result = crate::authored::load_prepared_config(path);
        if let Err(error) = &result {
            warn!(path = %path.display(), %error, "failed loading prepared config");
        }
        result.map_err(|error| RuntimeError::Config {
            message: error.to_string(),
        })
    }

    fn refresh_prepared_config(
        &self,
        authored: &std::path::Path,
        runtime: &std::path::Path,
    ) -> Result<spiders_shared::runtime::runtime_error::RuntimeRefreshSummary, RuntimeError> {
        debug!(authored = %authored.display(), runtime = %runtime.display(), "refreshing prepared config");
        crate::authored::refresh_prepared_config(authored, runtime)
            .map(runtime_refresh_summary)
            .map_err(|error| RuntimeError::Config {
                message: error.to_string(),
            })
    }

    fn rebuild_prepared_config(
        &self,
        authored: &std::path::Path,
        runtime: &std::path::Path,
    ) -> Result<spiders_shared::runtime::runtime_error::RuntimeRefreshSummary, RuntimeError> {
        debug!(authored = %authored.display(), runtime = %runtime.display(), "rebuilding prepared config");
        crate::authored::rebuild_prepared_config(authored, runtime)
            .map(runtime_refresh_summary)
            .map_err(|error| RuntimeError::Config {
                message: error.to_string(),
            })
    }
}

impl AuthoringLayoutRuntime for StubPreparedLayoutRuntime {
    fn load_authored_config(&self, _path: &std::path::Path) -> Result<Self::Config, RuntimeError> {
        Err(RuntimeError::NotImplemented(
            "authored config loading".into(),
        ))
    }

    fn load_prepared_config(&self, _path: &std::path::Path) -> Result<Self::Config, RuntimeError> {
        Err(RuntimeError::NotImplemented(
            "runtime config loading".into(),
        ))
    }

    fn refresh_prepared_config(
        &self,
        _authored: &std::path::Path,
        _runtime: &std::path::Path,
    ) -> Result<spiders_shared::runtime::runtime_error::RuntimeRefreshSummary, RuntimeError> {
        Err(RuntimeError::NotImplemented(
            "prepared config refresh".into(),
        ))
    }

    fn rebuild_prepared_config(
        &self,
        _authored: &std::path::Path,
        _runtime: &std::path::Path,
    ) -> Result<spiders_shared::runtime::runtime_error::RuntimeRefreshSummary, RuntimeError> {
        Err(RuntimeError::NotImplemented(
            "prepared config rebuild".into(),
        ))
    }
}

fn runtime_refresh_summary(
    update: crate::authored::JsRuntimeCacheUpdate,
) -> spiders_shared::runtime::runtime_error::RuntimeRefreshSummary {
    spiders_shared::runtime::runtime_error::RuntimeRefreshSummary {
        refreshed_files: update.rebuilt_files + update.copied_stylesheets,
        pruned_files: update.pruned_files,
    }
}

fn decode_authored_layout_node(
    value: &serde_json::Value,
    path: &DecodePath,
) -> Result<AuthoredLayoutNode, String> {
    let node: JsAuthoredLayoutNode = serde_json::from_value(value.clone())
        .map_err(|error| format!("{}: {error}", path.display()))?;

    decode_authored_layout_node_from_node(node, path)
}

fn decode_children(
    children: Vec<JsAuthoredLayoutNode>,
    path: &DecodePath,
) -> Result<Vec<AuthoredLayoutNode>, String> {
    children
        .into_iter()
        .enumerate()
        .map(|(index, child)| decode_authored_layout_node_from_node(child, &path.index(index)))
        .collect()
}

fn decode_authored_layout_node_from_node(
    node: JsAuthoredLayoutNode,
    path: &DecodePath,
) -> Result<AuthoredLayoutNode, String> {
    Ok(match node {
        JsAuthoredLayoutNode::Workspace {
            props,
            legacy,
            children,
        } => {
            let props = merge_node_props(props, legacy);
            AuthoredLayoutNode::Workspace {
                meta: decode_meta(props.meta),
                children: decode_children(children, &path.field("children"))?,
            }
        }
        JsAuthoredLayoutNode::Group {
            props,
            legacy,
            children,
        } => {
            let props = merge_node_props(props, legacy);
            AuthoredLayoutNode::Group {
                meta: decode_meta(props.meta),
                children: decode_children(children, &path.field("children"))?,
            }
        }
        JsAuthoredLayoutNode::Window { props, legacy } => {
            let props = merge_node_props(props, legacy);
            AuthoredLayoutNode::Window {
                meta: decode_meta(props.meta),
                match_expr: props.match_expr,
            }
        }
        JsAuthoredLayoutNode::Slot { props, legacy } => {
            let props = merge_node_props(props, legacy);
            AuthoredLayoutNode::Slot {
                meta: decode_meta(props.meta),
                match_expr: props.match_expr,
                take: props.take.unwrap_or_default(),
            }
        }
    })
}

fn merge_node_props(
    props: Option<JsAuthoredNodeProps>,
    legacy: JsAuthoredNodeProps,
) -> JsAuthoredNodeProps {
    match props {
        Some(props) => legacy.merge(props),
        None => legacy,
    }
}

fn decode_meta(meta: JsAuthoredNodeMeta) -> AuthoredNodeMeta {
    AuthoredNodeMeta {
        id: meta.id,
        class: meta.class.into_vec(),
        name: meta.name,
        data: meta.data,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_shared::snapshot::{OutputSnapshot, StateSnapshot, WorkspaceSnapshot};
    use spiders_shared::types::{LayoutRef, OutputTransform};
    use spiders_tree::{OutputId, WorkspaceId};

    use super::*;

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

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 1920,
                logical_height: 1080,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![workspace()],
            windows: vec![],
            visible_window_ids: vec![],
            workspace_names: vec!["1".into()],
        }
    }

    #[test]
    fn quickjs_runtime_exposes_default_export_contract() {
        let runtime = QuickJsPreparedLayoutRuntime::new();
        assert_eq!(runtime.contract().export_name, "default");
    }

    #[test]
    fn quickjs_runtime_decodes_js_layout_object_into_normalized_tree() {
        let runtime = QuickJsPreparedLayoutRuntime::new();
        let layout = runtime
            .evaluate_module_source(
                &SelectedLayout {
                    name: "master-stack".into(),
                    directory: "layouts/master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                },
                &state().layout_context(&workspace(), None),
                "ctx => ({ type: 'workspace', children: [{ type: 'window', match: 'app_id=\"firefox\"' }] })",
            )
            .unwrap();

        assert!(matches!(layout, SourceLayoutNode::Workspace { .. }));
    }

    #[test]
    fn decode_authored_layout_node_preserves_jsx_props_metadata() {
        let value = json!({
            "type": "workspace",
            "props": { "id": "root" },
            "children": [{
                "type": "group",
                "props": { "id": "frame" },
                "children": [{
                    "type": "slot",
                    "props": { "id": "master", "class": "master-slot", "take": 1 },
                    "children": []
                }]
            }]
        });

        let decoded = decode_authored_layout_node(&value, &DecodePath::root()).unwrap();

        let AuthoredLayoutNode::Workspace { meta, children } = decoded else {
            panic!("expected workspace root");
        };
        assert_eq!(meta.id.as_deref(), Some("root"));

        let AuthoredLayoutNode::Group {
            meta: group_meta,
            children: group_children,
        } = &children[0]
        else {
            panic!("expected frame group");
        };
        assert_eq!(group_meta.id.as_deref(), Some("frame"));

        let AuthoredLayoutNode::Slot { meta, take, .. } = &group_children[0] else {
            panic!("expected master slot");
        };
        assert_eq!(meta.id.as_deref(), Some("master"));
        assert_eq!(meta.class, vec!["master-slot".to_owned()]);
        assert_eq!(*take, SlotTake::Count(1));
    }

    #[test]
    fn quickjs_authoring_layout_service_works_with_filesystem_loader() {
        let temp_dir = std::env::temp_dir();
        let module_path = temp_dir.join("spiders-runtime-service-test.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let runtime = QuickJsPreparedLayoutRuntime::with_loader(FsLayoutSourceLoader);
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                directory: "layouts/master-stack".into(),
                module: module_path.to_string_lossy().into_owned(),
                stylesheet_path: Some("layouts/master-stack/index.css".into()),
                runtime_cache_payload: None,
            }],
            ..Config::default()
        };

        let loaded = runtime
            .prepare_layout(&config, &workspace())
            .unwrap()
            .unwrap();
        let layout = runtime
            .evaluate_layout(
                &loaded,
                &state().layout_context(&workspace(), Some(loaded.selected.clone())),
            )
            .unwrap();

        assert_eq!(loaded.selected.name, "master-stack");
        assert!(matches!(layout, SourceLayoutNode::Workspace { .. }));

        let _ = fs::remove_file(module_path);
    }
}
