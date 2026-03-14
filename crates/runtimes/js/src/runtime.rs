use std::collections::BTreeMap;

use boa_engine::{Context as JsContext, JsValue, Source};
use serde::Deserialize;
use spiders_config::model::Config;
use spiders_layout::ast::{
    AuthoredLayoutNode, AuthoredNodeMeta, LayoutValidationError, ValidatedLayoutTree,
};
use spiders_shared::layout::{SlotTake, SourceLayoutNode};
use spiders_shared::runtime::{
    LayoutModuleContract, LayoutRuntime, LayoutSourceLoader, RuntimeError,
};
use spiders_shared::wm::{LayoutEvaluationContext, LoadedLayout, SelectedLayout};

use crate::loader::InlineLayoutSourceLoader;

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

#[derive(Debug, Clone, PartialEq)]
struct EvaluatedLayoutModule {
    export: JsValue,
}

#[derive(Debug, Deserialize, Clone)]
struct JsAuthoredNodeMeta {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    class: Vec<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    data: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum JsAuthoredLayoutNode {
    Workspace {
        #[serde(flatten)]
        meta: JsAuthoredNodeMeta,
        #[serde(default)]
        children: Vec<JsAuthoredLayoutNode>,
    },
    Group {
        #[serde(flatten)]
        meta: JsAuthoredNodeMeta,
        #[serde(default)]
        children: Vec<JsAuthoredLayoutNode>,
    },
    Window {
        #[serde(flatten)]
        meta: JsAuthoredNodeMeta,
        #[serde(default, rename = "match")]
        match_expr: Option<String>,
    },
    Slot {
        #[serde(flatten)]
        meta: JsAuthoredNodeMeta,
        #[serde(default, rename = "match")]
        match_expr: Option<String>,
        #[serde(default)]
        take: SlotTake,
    },
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LayoutRuntimeError {
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
pub struct StubLayoutRuntime;

#[derive(Debug)]
pub struct BoaLayoutRuntime<L = InlineLayoutSourceLoader> {
    contract: LayoutModuleContract,
    loader: L,
}

impl Default for BoaLayoutRuntime<InlineLayoutSourceLoader> {
    fn default() -> Self {
        Self {
            contract: LayoutModuleContract::default(),
            loader: InlineLayoutSourceLoader,
        }
    }
}

impl BoaLayoutRuntime<InlineLayoutSourceLoader> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<L> BoaLayoutRuntime<L> {
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
    ) -> Result<SourceLayoutNode, LayoutRuntimeError> {
        let mut js = JsContext::default();
        let module = self.evaluate_module(selected_layout, source, &mut js)?;
        let context_value = self.context_to_js_value(context, &mut js)?;

        self.call_layout_export(selected_layout, &module, &context_value, &mut js)
    }

    pub fn normalize_authored_layout(
        &self,
        root: AuthoredLayoutNode,
    ) -> Result<SourceLayoutNode, LayoutRuntimeError> {
        Ok(ValidatedLayoutTree::from_authored(root)?.root)
    }

    fn evaluate_module(
        &self,
        selected_layout: &SelectedLayout,
        source: &str,
        js: &mut JsContext,
    ) -> Result<EvaluatedLayoutModule, LayoutRuntimeError> {
        let wrapped = format!("({source})");
        let export = js.eval(Source::from_bytes(&wrapped)).map_err(|error| {
            LayoutRuntimeError::JavaScript {
                message: error.to_string(),
            }
        })?;

        if export.is_null_or_undefined() {
            return Err(LayoutRuntimeError::MissingExport {
                name: selected_layout.name.clone(),
                export: self.contract.export_name.clone(),
            });
        }

        Ok(EvaluatedLayoutModule { export })
    }

    fn context_to_js_value(
        &self,
        context: &LayoutEvaluationContext,
        js: &mut JsContext,
    ) -> Result<JsValue, LayoutRuntimeError> {
        let value =
            serde_json::to_value(context).map_err(|error| LayoutRuntimeError::JavaScript {
                message: error.to_string(),
            })?;

        JsValue::from_json(&value, js).map_err(|error| LayoutRuntimeError::JavaScript {
            message: error.to_string(),
        })
    }

    fn call_layout_export(
        &self,
        selected_layout: &SelectedLayout,
        module: &EvaluatedLayoutModule,
        context_value: &JsValue,
        js: &mut JsContext,
    ) -> Result<SourceLayoutNode, LayoutRuntimeError> {
        let callable =
            module
                .export
                .as_callable()
                .ok_or_else(|| LayoutRuntimeError::NonCallableExport {
                    name: selected_layout.name.clone(),
                    export: self.contract.export_name.clone(),
                })?;

        let value = callable
            .call(
                &JsValue::undefined(),
                std::slice::from_ref(context_value),
                js,
            )
            .map_err(|error| LayoutRuntimeError::JavaScript {
                message: error.to_string(),
            })?;

        self.convert_js_value_to_authored_layout(selected_layout, &value, js)
    }

    fn convert_js_value_to_authored_layout(
        &self,
        selected_layout: &SelectedLayout,
        value: &JsValue,
        js: &mut JsContext,
    ) -> Result<SourceLayoutNode, LayoutRuntimeError> {
        let json = value
            .to_json(js)
            .map_err(|error| LayoutRuntimeError::JavaScript {
                message: error.to_string(),
            })?
            .ok_or_else(|| LayoutRuntimeError::ValueConversion {
                name: selected_layout.name.clone(),
                message: "layout function returned undefined".into(),
            })?;

        let authored =
            decode_authored_layout_node(&json, &DecodePath::root()).map_err(|message| {
                LayoutRuntimeError::ValueConversion {
                    name: selected_layout.name.clone(),
                    message,
                }
            })?;

        self.normalize_authored_layout(authored)
    }
}

impl<L: LayoutSourceLoader<Config>> BoaLayoutRuntime<L> {
    pub fn selected_layout(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, RuntimeError> {
        config
            .resolve_selected_layout(workspace)
            .map_err(|error| RuntimeError::Config {
                message: error.to_string(),
            })
    }

    pub fn load_selected_layout(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<LoadedLayout>, RuntimeError> {
        self.loader.load_runtime_source(config, workspace)
    }
}

impl LayoutRuntime for StubLayoutRuntime {
    type Config = Config;

    fn selected_layout(
        &self,
        config: &Self::Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, RuntimeError> {
        config
            .resolve_selected_layout(workspace)
            .map_err(|error| RuntimeError::Config {
                message: error.to_string(),
            })
    }

    fn load_selected_layout(
        &self,
        config: &Self::Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<LoadedLayout>, RuntimeError> {
        Ok(self
            .selected_layout(config, workspace)?
            .map(|selected| LoadedLayout {
                selected,
                runtime_source: String::new(),
            }))
    }

    fn build_context(
        &self,
        state: &spiders_shared::wm::StateSnapshot,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
        selected_layout: Option<SelectedLayout>,
    ) -> LayoutEvaluationContext {
        state.layout_context(workspace, selected_layout)
    }

    fn evaluate_layout(
        &self,
        loaded_layout: &LoadedLayout,
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

impl<L: LayoutSourceLoader<Config>> LayoutRuntime for BoaLayoutRuntime<L> {
    type Config = Config;

    fn selected_layout(
        &self,
        config: &Self::Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, RuntimeError> {
        BoaLayoutRuntime::selected_layout(self, config, workspace)
    }

    fn load_selected_layout(
        &self,
        config: &Self::Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<LoadedLayout>, RuntimeError> {
        BoaLayoutRuntime::load_selected_layout(self, config, workspace)
    }

    fn build_context(
        &self,
        state: &spiders_shared::wm::StateSnapshot,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
        selected_layout: Option<SelectedLayout>,
    ) -> LayoutEvaluationContext {
        state.layout_context(workspace, selected_layout)
    }

    fn evaluate_layout(
        &self,
        loaded_layout: &LoadedLayout,
        context: &LayoutEvaluationContext,
    ) -> Result<SourceLayoutNode, RuntimeError> {
        self.evaluate_module_source(
            &loaded_layout.selected,
            context,
            &loaded_layout.runtime_source,
        )
        .map_err(|error| RuntimeError::Other {
            message: error.to_string(),
        })
    }

    fn contract(&self) -> LayoutModuleContract {
        self.contract.clone()
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
        JsAuthoredLayoutNode::Workspace { meta, children } => AuthoredLayoutNode::Workspace {
            meta: decode_meta(meta),
            children: decode_children(children, &path.field("children"))?,
        },
        JsAuthoredLayoutNode::Group { meta, children } => AuthoredLayoutNode::Group {
            meta: decode_meta(meta),
            children: decode_children(children, &path.field("children"))?,
        },
        JsAuthoredLayoutNode::Window { meta, match_expr } => AuthoredLayoutNode::Window {
            meta: decode_meta(meta),
            match_expr,
        },
        JsAuthoredLayoutNode::Slot {
            meta,
            match_expr,
            take,
        } => AuthoredLayoutNode::Slot {
            meta: decode_meta(meta),
            match_expr,
            take,
        },
    })
}

fn decode_meta(meta: JsAuthoredNodeMeta) -> AuthoredNodeMeta {
    AuthoredNodeMeta {
        id: meta.id,
        class: meta.class,
        name: meta.name,
        data: meta.data,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use spiders_config::model::{Config, LayoutDefinition};
    use spiders_shared::ids::{OutputId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, StateSnapshot, WorkspaceSnapshot,
    };

    use super::*;

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
            tag_names: vec!["1".into()],
        }
    }

    #[test]
    fn boa_runtime_exposes_default_export_contract() {
        let runtime = BoaLayoutRuntime::new();
        assert_eq!(runtime.contract().export_name, "default");
    }

    #[test]
    fn boa_runtime_decodes_js_layout_object_into_normalized_tree() {
        let runtime = BoaLayoutRuntime::new();
        let layout = runtime
            .evaluate_module_source(
                &SelectedLayout {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: String::new(),
                    effects_stylesheet: String::new(),
                },
                &state().layout_context(&workspace(), None),
                "ctx => ({ type: 'workspace', children: [{ type: 'window', match: 'app_id=\"firefox\"' }] })",
            )
            .unwrap();

        assert!(matches!(layout, SourceLayoutNode::Workspace { .. }));
    }

    #[test]
    fn boa_runtime_service_works_with_filesystem_loader() {
        let temp_dir = std::env::temp_dir();
        let module_path = temp_dir.join("spiders-runtime-service-test.js");
        fs::write(
            &module_path,
            "ctx => ({ type: 'workspace', children: [{ type: 'window', id: 'main' }] })",
        )
        .unwrap();

        let runtime = BoaLayoutRuntime::with_loader(FsLayoutSourceLoader);
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: module_path.to_string_lossy().into_owned(),
                stylesheet: String::new(),
                effects_stylesheet: String::new(),
                runtime_source: None,
            }],
            ..Config::default()
        };

        let loaded = runtime
            .load_selected_layout(&config, &workspace())
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
