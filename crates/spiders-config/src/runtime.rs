use std::collections::BTreeMap;

use boa_engine::{Context as JsContext, JsValue, Source};
use serde::Deserialize;
use spiders_layout::ast::{
    AuthoredLayoutNode, AuthoredNodeMeta, LayoutValidationError, ValidatedLayoutTree,
};
use spiders_shared::layout::{SlotTake, SourceLayoutNode};
use spiders_shared::wm::{LayoutEvaluationContext, SelectedLayout};

use crate::model::{Config, LayoutConfigError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutModuleContract {
    pub export_name: String,
}

impl Default for LayoutModuleContract {
    fn default() -> Self {
        Self {
            export_name: "default".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedLayoutModule {
    pub export: JsValue,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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
    #[error(transparent)]
    Config(#[from] LayoutConfigError),
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

pub trait LayoutRuntime {
    fn selected_layout(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, LayoutRuntimeError>;

    fn build_context(
        &self,
        state: &spiders_shared::wm::StateSnapshot,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
        selected_layout: Option<SelectedLayout>,
    ) -> LayoutEvaluationContext;

    fn evaluate_layout(
        &self,
        selected_layout: &SelectedLayout,
        context: &LayoutEvaluationContext,
    ) -> Result<SourceLayoutNode, LayoutRuntimeError>;

    fn contract(&self) -> LayoutModuleContract;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StubLayoutRuntime;

#[derive(Debug, Default)]
pub struct BoaLayoutRuntime {
    contract: LayoutModuleContract,
}

impl BoaLayoutRuntime {
    pub fn new() -> Self {
        Self::default()
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

        let authored: JsAuthoredLayoutNode =
            serde_json::from_value(json).map_err(|error| LayoutRuntimeError::ValueConversion {
                name: selected_layout.name.clone(),
                message: error.to_string(),
            })?;

        self.normalize_authored_layout(convert_authored_layout_node(authored))
    }
}

impl LayoutRuntime for StubLayoutRuntime {
    fn selected_layout(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, LayoutRuntimeError> {
        Ok(config.resolve_selected_layout(workspace)?)
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
        selected_layout: &SelectedLayout,
        _context: &LayoutEvaluationContext,
    ) -> Result<SourceLayoutNode, LayoutRuntimeError> {
        Err(LayoutRuntimeError::NotImplemented {
            name: selected_layout.name.clone(),
        })
    }

    fn contract(&self) -> LayoutModuleContract {
        LayoutModuleContract::default()
    }
}

impl LayoutRuntime for BoaLayoutRuntime {
    fn selected_layout(
        &self,
        config: &Config,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> Result<Option<SelectedLayout>, LayoutRuntimeError> {
        Ok(config.resolve_selected_layout(workspace)?)
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
        selected_layout: &SelectedLayout,
        context: &LayoutEvaluationContext,
    ) -> Result<SourceLayoutNode, LayoutRuntimeError> {
        self.evaluate_module_source(selected_layout, context, &selected_layout.module)
    }

    fn contract(&self) -> LayoutModuleContract {
        self.contract.clone()
    }
}

fn convert_authored_layout_node(node: JsAuthoredLayoutNode) -> AuthoredLayoutNode {
    match node {
        JsAuthoredLayoutNode::Workspace { meta, children } => AuthoredLayoutNode::Workspace {
            meta: convert_meta(meta),
            children: children
                .into_iter()
                .map(convert_authored_layout_node)
                .collect(),
        },
        JsAuthoredLayoutNode::Group { meta, children } => AuthoredLayoutNode::Group {
            meta: convert_meta(meta),
            children: children
                .into_iter()
                .map(convert_authored_layout_node)
                .collect(),
        },
        JsAuthoredLayoutNode::Window { meta, match_expr } => AuthoredLayoutNode::Window {
            meta: convert_meta(meta),
            match_expr,
        },
        JsAuthoredLayoutNode::Slot {
            meta,
            match_expr,
            take,
        } => AuthoredLayoutNode::Slot {
            meta: convert_meta(meta),
            match_expr,
            take,
        },
    }
}

fn convert_meta(meta: JsAuthoredNodeMeta) -> AuthoredNodeMeta {
    AuthoredNodeMeta {
        id: meta.id,
        class: meta.class,
        name: meta.name,
        data: meta.data,
    }
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::{OutputId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, StateSnapshot, WorkspaceSnapshot,
    };

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

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
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
    fn stub_runtime_selects_shared_layout_payload_and_builds_context() {
        let runtime = StubLayoutRuntime;
        let config = Config {
            layouts: vec![LayoutDefinition {
                name: "master-stack".into(),
                module: "layouts/master-stack.js".into(),
                stylesheet: "workspace { display: flex; }".into(),
            }],
            ..Config::default()
        };
        let workspace = workspace();
        let selected = runtime.selected_layout(&config, &workspace).unwrap();
        let context = runtime.build_context(&state(), &workspace, selected.clone());

        assert_eq!(selected.unwrap().module, "layouts/master-stack.js");
        assert_eq!(context.space.width, 1920.0);
        assert_eq!(context.workspace.id, WorkspaceId::from("ws-1"));
    }

    #[test]
    fn stub_runtime_reports_unimplemented_layout_evaluation() {
        let runtime = StubLayoutRuntime;
        let error = runtime
            .evaluate_layout(
                &SelectedLayout {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: "workspace { display: flex; }".into(),
                },
                &state().layout_context(&workspace(), None),
            )
            .unwrap_err();

        assert_eq!(
            error,
            LayoutRuntimeError::NotImplemented {
                name: "master-stack".into(),
            }
        );
    }

    #[test]
    fn boa_runtime_exposes_default_export_contract() {
        let runtime = BoaLayoutRuntime::new();

        assert_eq!(runtime.contract().export_name, "default");
    }

    #[test]
    fn boa_runtime_normalizes_authored_layout_nodes() {
        let runtime = BoaLayoutRuntime::new();
        let normalized = runtime
            .normalize_authored_layout(AuthoredLayoutNode::Workspace {
                meta: Default::default(),
                children: vec![AuthoredLayoutNode::Window {
                    meta: Default::default(),
                    match_expr: Some("app_id=\"firefox\"".into()),
                }],
            })
            .unwrap();

        assert!(matches!(normalized, SourceLayoutNode::Workspace { .. }));
    }

    #[test]
    fn boa_runtime_reports_missing_export_for_undefined_result() {
        let runtime = BoaLayoutRuntime::new();
        let error = runtime
            .evaluate_module_source(
                &SelectedLayout {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: String::new(),
                },
                &state().layout_context(&workspace(), None),
                "undefined",
            )
            .unwrap_err();

        assert_eq!(
            error,
            LayoutRuntimeError::MissingExport {
                name: "master-stack".into(),
                export: "default".into(),
            }
        );
    }

    #[test]
    fn boa_runtime_rejects_non_callable_default_export() {
        let runtime = BoaLayoutRuntime::new();
        let error = runtime
            .evaluate_module_source(
                &SelectedLayout {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: String::new(),
                },
                &state().layout_context(&workspace(), None),
                "({ type: 'workspace', children: [] })",
            )
            .unwrap_err();

        assert_eq!(
            error,
            LayoutRuntimeError::NonCallableExport {
                name: "master-stack".into(),
                export: "default".into(),
            }
        );
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
                },
                &state().layout_context(&workspace(), None),
                "ctx => ({ type: 'workspace', children: [{ type: 'window', match: 'app_id=\"firefox\"' }] })",
            )
            .unwrap();

        assert!(matches!(layout, SourceLayoutNode::Workspace { .. }));
    }

    #[test]
    fn boa_runtime_decodes_slot_take_and_data_metadata() {
        let runtime = BoaLayoutRuntime::new();
        let layout = runtime
            .evaluate_module_source(
                &SelectedLayout {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: String::new(),
                },
                &state().layout_context(&workspace(), None),
                "ctx => ({ type: 'workspace', children: [{ type: 'slot', id: 'rest', class: ['stack'], data: { role: 'secondary' }, take: 2 }] })",
            )
            .unwrap();

        match layout {
            SourceLayoutNode::Workspace { children, .. } => match &children[0] {
                SourceLayoutNode::Slot { meta, take, .. } => {
                    assert_eq!(meta.id.as_deref(), Some("rest"));
                    assert_eq!(meta.class, vec!["stack".to_string()]);
                    assert_eq!(meta.data.get("role").map(String::as_str), Some("secondary"));
                    assert_eq!(take, &SlotTake::Count(2));
                }
                other => panic!("expected slot node, got {other:?}"),
            },
            other => panic!("expected workspace node, got {other:?}"),
        }
    }

    #[test]
    fn boa_runtime_reports_decode_errors_with_context() {
        let runtime = BoaLayoutRuntime::new();
        let error = runtime
            .evaluate_module_source(
                &SelectedLayout {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: String::new(),
                },
                &state().layout_context(&workspace(), None),
                "ctx => ({ children: [] })",
            )
            .unwrap_err();

        assert!(matches!(
            error,
            LayoutRuntimeError::ValueConversion { name, .. } if name == "master-stack"
        ));
    }
}
