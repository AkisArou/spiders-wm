use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spiders_core::runtime::layout_context::LayoutWindowContext;
use spiders_core::{LayoutNodeMeta, ResolvedLayoutNode, RuntimeContentKind, WindowId};
use spiders_css::{
    AppearanceValue, BorderStyleValue, BoxShadowValue, ColorValue, FontQuery, FontWeightValue,
    LengthPercentage, TextAlignValue, TextTransformValue,
};
use spiders_scene::ComputedStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecorationMode {
    ClientSide,
    CompositorTitlebar,
    NoTitlebar,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearancePlan {
    pub window_id: WindowId,
    pub decoration_mode: DecorationMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarPlan {
    pub window_id: WindowId,
    pub height: i32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub background: ColorValue,
    pub border_bottom_width: i32,
    pub border_bottom_color: ColorValue,
    pub title: String,
    pub text_color: ColorValue,
    pub text_align: TextAlignValue,
    pub text_transform: TextTransformValue,
    pub font: FontQuery,
    pub letter_spacing: i32,
    pub box_shadow: Option<Vec<BoxShadowValue>>,
    pub padding_top: i32,
    pub padding_right: i32,
    pub padding_bottom: i32,
    pub padding_left: i32,
    pub corner_radius_top_left: i32,
    pub corner_radius_top_right: i32,
    pub buttons: Vec<TitlebarButtonPlan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TitlebarButtonKind {
    Close,
    ToggleFullscreen,
    ToggleFloating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarButtonRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarButtonPlan {
    pub kind: TitlebarButtonKind,
    pub rect: TitlebarButtonRect,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarButtonColors {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarButtonsConfig {
    pub close: bool,
    pub fullscreen: bool,
    pub floating: bool,
}

impl Default for TitlebarButtonsConfig {
    fn default() -> Self {
        Self { close: true, fullscreen: true, floating: true }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TitlebarPlanInput {
    pub window_id: WindowId,
    pub title: String,
    pub focused: bool,
    pub titlebar_style: Option<ComputedStyle>,
    pub window_style: Option<ComputedStyle>,
    pub default_background_focused: ColorValue,
    pub default_background_unfocused: ColorValue,
    pub default_text_color_focused: ColorValue,
    pub default_text_color_unfocused: ColorValue,
    pub offset_x: i32,
    pub offset_y: i32,
    pub effective_opacity: f32,
    pub buttons: TitlebarButtonsConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitlebarPlanPreset {
    pub height: i32,
    pub border_bottom_width: i32,
    pub border_bottom_color_focused: ColorValue,
    pub border_bottom_color_unfocused: ColorValue,
    pub font: FontQuery,
    pub font_unfocused_weight: FontWeightValue,
    pub padding_left: i32,
    pub padding_right: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarWhen {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "appId")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floating: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fullscreen: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlebarRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<TitlebarWhen>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default)]
    pub children: Vec<TitlebarNode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedTitlebarContext {
    pub workspace: Option<String>,
    pub slot: Option<String>,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub floating: bool,
    pub fullscreen: bool,
}

impl From<&LayoutWindowContext> for ResolvedTitlebarContext {
    fn from(value: &LayoutWindowContext) -> Self {
        Self {
            workspace: None,
            slot: None,
            app_id: value.app_id.clone(),
            title: value.title.clone(),
            floating: value.floating,
            fullscreen: value.fullscreen,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTitlebarTree {
    pub meta: LayoutNodeMeta,
    pub children: Vec<ResolvedTitlebarNode>,
}

pub const TITLEBAR_ACTION_KEY: &str = "titlebar_action";
pub const TITLEBAR_ICON_ASSET_KEY: &str = "titlebar_icon_asset";
pub const TITLEBAR_ICON_CHILDREN_KEY: &str = "titlebar_icon_children";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTitlebarNode {
    Group {
        meta: LayoutNodeMeta,
        children: Vec<ResolvedTitlebarNode>,
    },
    WindowTitle {
        meta: LayoutNodeMeta,
        text: Option<String>,
    },
    WorkspaceName {
        meta: LayoutNodeMeta,
        text: Option<String>,
    },
    Text {
        meta: LayoutNodeMeta,
        text: String,
    },
    Badge {
        meta: LayoutNodeMeta,
        text: String,
    },
    Button {
        meta: LayoutNodeMeta,
        action: Option<TitlebarButtonAction>,
        children: Vec<ResolvedTitlebarNode>,
    },
    Icon {
        meta: LayoutNodeMeta,
        asset: Option<String>,
        children: Vec<TitlebarIconNode>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlebarButtonAction {
    Close,
    ToggleFullscreen,
    ToggleFloating,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TitlebarNode {
    Group {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<String>,
        #[serde(default)]
        children: Vec<TitlebarNode>,
    },
    WindowTitle {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fallback: Option<String>,
    },
    WorkspaceName {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<String>,
    },
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<String>,
        text: String,
    },
    Badge {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<String>,
        text: String,
    },
    Button {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", alias = "onClick")]
        on_click: Option<JsonValue>,
        #[serde(default)]
        children: Vec<TitlebarNode>,
    },
    Icon {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asset: Option<String>,
        #[serde(default)]
        children: Vec<TitlebarIconNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TitlebarIconNode {
    Svg {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view_box: Option<String>,
        #[serde(default)]
        children: Vec<TitlebarIconNode>,
    },
    Path {
        d: String,
    },
}

pub fn decode_titlebar_rules(value: &JsonValue) -> Result<Vec<TitlebarRule>, serde_json::Error> {
    if looks_like_normalized_titlebar_rules(value)
        && let Ok(rules) = serde_json::from_value(value.clone())
    {
        return Ok(rules);
    }

    decode_sdk_titlebar_rules(value)
}

fn looks_like_normalized_titlebar_rules(value: &JsonValue) -> bool {
    value
        .as_array()
        .is_some_and(|entries| entries.iter().all(looks_like_normalized_titlebar_rule))
}

fn looks_like_normalized_titlebar_rule(value: &JsonValue) -> bool {
    value.as_object().is_some_and(|object| {
        if object.contains_key("props") || matches!(object.get("type").and_then(JsonValue::as_str), Some("titlebar")) {
            return false;
        }

        object
            .get("children")
            .and_then(JsonValue::as_array)
            .is_some_and(|children| children.iter().all(looks_like_normalized_titlebar_node))
    })
}

fn looks_like_normalized_titlebar_node(value: &JsonValue) -> bool {
    value.as_object().is_some_and(|object| {
        matches!(
            object.get("type").and_then(JsonValue::as_str),
            Some("group" | "windowTitle" | "workspaceName" | "text" | "badge" | "button" | "icon")
        )
    })
}

fn decode_sdk_titlebar_rules(value: &JsonValue) -> Result<Vec<TitlebarRule>, serde_json::Error> {
    let Some(entries) = value.as_array() else {
        return Err(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "titlebars must be an array",
        )));
    };

    entries.iter().map(decode_sdk_titlebar_rule).collect()
}

fn decode_sdk_titlebar_rule(value: &JsonValue) -> Result<TitlebarRule, serde_json::Error> {
    let object = expect_object(value, "titlebar rule")?;
    let kind = expect_string(object.get("type"), "titlebar rule type")?;
    if kind != "titlebar" {
        return Err(invalid_data_error("titlebar rule must have type `titlebar`"));
    }
    let props = object
        .get("props")
        .map(|value| expect_object(value, "titlebar props"))
        .transpose()?
        .cloned()
        .unwrap_or_default();
    let children = decode_sdk_titlebar_children(object.get("children"))?;

    Ok(TitlebarRule {
        when: props.get("when").map(normalize_sdk_when_value).transpose()?,
        disabled: props.get("disabled").and_then(JsonValue::as_bool).unwrap_or(false),
        class: props.get("class").and_then(JsonValue::as_str).map(str::to_string),
        children,
    })
}

fn decode_sdk_titlebar_children(
    value: Option<&JsonValue>,
) -> Result<Vec<TitlebarNode>, serde_json::Error> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(entries) = value.as_array() else {
        return Err(invalid_data_error("titlebar children must be an array"));
    };

    entries.iter().filter(|entry| !entry.is_null()).map(decode_sdk_titlebar_node).collect()
}

fn decode_sdk_titlebar_node(value: &JsonValue) -> Result<TitlebarNode, serde_json::Error> {
    if let Some(text) = value.as_str() {
        return Ok(TitlebarNode::Text { class: None, text: text.to_string() });
    }
    if let Some(number) = value.as_i64() {
        return Ok(TitlebarNode::Text { class: None, text: number.to_string() });
    }

    let object = expect_object(value, "titlebar node")?;
    let kind = expect_string(object.get("type"), "titlebar node type")?;
    let props = object
        .get("props")
        .map(|value| expect_object(value, "titlebar node props"))
        .transpose()?
        .cloned()
        .unwrap_or_default();
    let class = props.get("class").and_then(JsonValue::as_str).map(str::to_string);

    match kind {
        "titlebar.group" => Ok(TitlebarNode::Group {
            class,
            children: decode_sdk_titlebar_children(object.get("children"))?,
        }),
        "titlebar.windowTitle" => Ok(TitlebarNode::WindowTitle {
            class,
            fallback: props.get("fallback").and_then(JsonValue::as_str).map(str::to_string),
        }),
        "titlebar.workspaceName" => Ok(TitlebarNode::WorkspaceName { class }),
        "titlebar.text" => {
            Ok(TitlebarNode::Text { class, text: decode_sdk_text_content(object.get("children"))? })
        }
        "titlebar.badge" => Ok(TitlebarNode::Badge {
            class,
            text: decode_sdk_text_content(object.get("children"))?,
        }),
        "titlebar.button" => Ok(TitlebarNode::Button {
            class,
            on_click: props.get("onClick").cloned(),
            children: decode_sdk_titlebar_children(object.get("children"))?,
        }),
        "titlebar.icon" => Ok(TitlebarNode::Icon {
            class,
            asset: props.get("asset").and_then(JsonValue::as_str).map(str::to_string),
            children: decode_sdk_icon_children(object.get("children"))?,
        }),
        other => Err(invalid_data_error(&format!("unsupported titlebar node type `{other}`"))),
    }
}

fn decode_sdk_icon_children(
    value: Option<&JsonValue>,
) -> Result<Vec<TitlebarIconNode>, serde_json::Error> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(entries) = value.as_array() else {
        return Err(invalid_data_error("titlebar icon children must be an array"));
    };

    entries.iter().filter(|entry| !entry.is_null()).map(decode_sdk_icon_node).collect()
}

fn decode_sdk_icon_node(value: &JsonValue) -> Result<TitlebarIconNode, serde_json::Error> {
    let object = expect_object(value, "titlebar icon node")?;
    let kind = expect_string(object.get("type"), "titlebar icon node type")?;
    let props = object
        .get("props")
        .map(|value| expect_object(value, "titlebar icon node props"))
        .transpose()?
        .cloned()
        .unwrap_or_default();

    match kind {
        "icon" | "icon.svg" => Ok(TitlebarIconNode::Svg {
            view_box: props.get("viewBox").and_then(JsonValue::as_str).map(str::to_string),
            children: decode_sdk_icon_children(object.get("children"))?,
        }),
        "path" | "icon.path" => Ok(TitlebarIconNode::Path {
            d: expect_string(props.get("d"), "icon path d")?.to_string(),
        }),
        other => Err(invalid_data_error(&format!("unsupported titlebar icon node type `{other}`"))),
    }
}

fn decode_sdk_text_content(value: Option<&JsonValue>) -> Result<String, serde_json::Error> {
    let Some(value) = value else {
        return Ok(String::new());
    };
    let Some(entries) = value.as_array() else {
        return Err(invalid_data_error("titlebar text children must be an array"));
    };

    let mut out = String::new();
    for entry in entries {
        if let Some(text) = entry.as_str() {
            out.push_str(text);
        } else if let Some(number) = entry.as_i64() {
            out.push_str(&number.to_string());
        } else if let Some(number) = entry.as_u64() {
            out.push_str(&number.to_string());
        } else if let Some(number) = entry.as_f64() {
            out.push_str(&number.to_string());
        } else if entry.is_null() {
            continue;
        } else {
            return Err(invalid_data_error("titlebar text children must be primitive text"));
        }
    }

    Ok(out)
}

fn normalize_sdk_when_value(value: &JsonValue) -> Result<TitlebarWhen, serde_json::Error> {
    let object = expect_object(value, "titlebar when")?;
    Ok(TitlebarWhen {
        workspace: object.get("workspace").and_then(JsonValue::as_str).map(str::to_string),
        slot: object.get("slot").and_then(JsonValue::as_str).map(str::to_string),
        app_id: object
            .get("app_id")
            .or_else(|| object.get("appId"))
            .and_then(JsonValue::as_str)
            .map(str::to_string),
        title: object.get("title").and_then(JsonValue::as_str).map(str::to_string),
        floating: object.get("floating").and_then(JsonValue::as_bool),
        fullscreen: object.get("fullscreen").and_then(JsonValue::as_bool),
    })
}

fn expect_object<'a>(
    value: &'a JsonValue,
    label: &str,
) -> Result<&'a serde_json::Map<String, JsonValue>, serde_json::Error> {
    value.as_object().ok_or_else(|| invalid_data_error(&format!("expected object for {label}")))
}

fn expect_string<'a>(
    value: Option<&'a JsonValue>,
    label: &str,
) -> Result<&'a str, serde_json::Error> {
    value
        .and_then(JsonValue::as_str)
        .ok_or_else(|| invalid_data_error(&format!("expected string for {label}")))
}

fn invalid_data_error(message: &str) -> serde_json::Error {
    serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, message.to_string()))
}

pub fn select_titlebar_rule<'a>(
    rules: &'a [TitlebarRule],
    context: &ResolvedTitlebarContext,
) -> Option<&'a TitlebarRule> {
    rules.iter().filter(|rule| titlebar_rule_matches(rule, context)).last()
}

pub fn resolve_titlebar_tree(
    rule: &TitlebarRule,
    context: &ResolvedTitlebarContext,
) -> ResolvedTitlebarTree {
    ResolvedTitlebarTree {
        meta: titlebar_meta(rule.class.as_deref(), "titlebar"),
        children: rule.children.iter().map(|node| resolve_titlebar_node(node, context)).collect(),
    }
}

pub fn titlebar_tree_to_runtime_node(tree: &ResolvedTitlebarTree) -> ResolvedLayoutNode {
    ResolvedLayoutNode::Content {
        meta: tree.meta.clone(),
        kind: RuntimeContentKind::Container,
        text: None,
        children: tree.children.iter().map(titlebar_runtime_node_from_resolved).collect(),
    }
}

pub fn titlebar_rule_matches(rule: &TitlebarRule, context: &ResolvedTitlebarContext) -> bool {
    let Some(when) = rule.when.as_ref() else {
        return true;
    };

    when.workspace.as_ref().is_none_or(|workspace| context.workspace.as_ref() == Some(workspace))
        && when.slot.as_ref().is_none_or(|slot| context.slot.as_ref() == Some(slot))
        && when.app_id.as_ref().is_none_or(|app_id| context.app_id.as_ref() == Some(app_id))
        && when.title.as_ref().is_none_or(|title| {
            context.title.as_ref().is_some_and(|window_title| window_title.contains(title))
        })
        && when.floating.is_none_or(|floating| context.floating == floating)
        && when.fullscreen.is_none_or(|fullscreen| context.fullscreen == fullscreen)
}

pub fn titlebar_icon_nodes_from_data(
    data: &BTreeMap<String, String>,
) -> Option<Vec<TitlebarIconNode>> {
    let encoded = data.get(TITLEBAR_ICON_CHILDREN_KEY)?;
    serde_json::from_str(encoded).ok()
}

pub fn titlebar_icon_asset_from_data(data: &BTreeMap<String, String>) -> Option<String> {
    data.get(TITLEBAR_ICON_ASSET_KEY).cloned().filter(|asset| !asset.trim().is_empty())
}

pub fn titlebar_button_action_from_data(
    data: &BTreeMap<String, String>,
) -> Option<TitlebarButtonAction> {
    data.get(TITLEBAR_ACTION_KEY)
        .map(String::as_str)
        .and_then(parse_button_action_name)
}

pub fn titlebar_icon_view_box(nodes: &[TitlebarIconNode]) -> Option<String> {
    for node in nodes {
        match node {
            TitlebarIconNode::Svg { view_box, children } => {
                if view_box.as_ref().is_some_and(|value| !value.trim().is_empty()) {
                    return view_box.clone();
                }
                if let Some(view_box) = titlebar_icon_view_box(children) {
                    return Some(view_box);
                }
            }
            TitlebarIconNode::Path { .. } => {}
        }
    }

    None
}

pub fn titlebar_icon_paths(nodes: &[TitlebarIconNode]) -> Vec<String> {
    let mut paths = Vec::new();
    collect_titlebar_icon_paths(nodes, &mut paths);
    paths
}

fn resolve_titlebar_node(
    node: &TitlebarNode,
    context: &ResolvedTitlebarContext,
) -> ResolvedTitlebarNode {
    match node {
        TitlebarNode::Group { class, children } => ResolvedTitlebarNode::Group {
            meta: titlebar_meta(class.as_deref(), "titlebar-group"),
            children: children.iter().map(|child| resolve_titlebar_node(child, context)).collect(),
        },
        TitlebarNode::WindowTitle { class, fallback } => ResolvedTitlebarNode::WindowTitle {
            meta: titlebar_meta(class.as_deref(), "titlebar-window-title"),
            text: context
                .title
                .as_ref()
                .filter(|title| !title.trim().is_empty())
                .cloned()
                .or_else(|| fallback.clone()),
        },
        TitlebarNode::WorkspaceName { class } => ResolvedTitlebarNode::WorkspaceName {
            meta: titlebar_meta(class.as_deref(), "titlebar-workspace-name"),
            text: context.workspace.clone(),
        },
        TitlebarNode::Text { class, text } => ResolvedTitlebarNode::Text {
            meta: titlebar_meta(class.as_deref(), "titlebar-text"),
            text: text.clone(),
        },
        TitlebarNode::Badge { class, text } => ResolvedTitlebarNode::Badge {
            meta: titlebar_meta(class.as_deref(), "titlebar-badge"),
            text: text.clone(),
        },
        TitlebarNode::Button { class, on_click, children } => ResolvedTitlebarNode::Button {
            meta: titlebar_meta(class.as_deref(), "titlebar-button"),
            action: on_click.as_ref().and_then(normalize_button_action),
            children: children.iter().map(|child| resolve_titlebar_node(child, context)).collect(),
        },
        TitlebarNode::Icon { class, asset, children } => ResolvedTitlebarNode::Icon {
            meta: titlebar_meta(class.as_deref(), "titlebar-icon"),
            asset: asset.clone().or_else(|| context.app_id.clone()),
            children: children.clone(),
        },
    }
}

fn titlebar_runtime_node_from_resolved(node: &ResolvedTitlebarNode) -> ResolvedLayoutNode {
    match node {
        ResolvedTitlebarNode::Group { meta, children } => ResolvedLayoutNode::Content {
            meta: meta.clone(),
            kind: RuntimeContentKind::Container,
            text: None,
            children: children.iter().map(titlebar_runtime_node_from_resolved).collect(),
        },
        ResolvedTitlebarNode::WindowTitle { meta, text } => ResolvedLayoutNode::Content {
            meta: meta.clone(),
            kind: RuntimeContentKind::Text,
            text: text.clone(),
            children: Vec::new(),
        },
        ResolvedTitlebarNode::WorkspaceName { meta, text } => ResolvedLayoutNode::Content {
            meta: meta.clone(),
            kind: RuntimeContentKind::Text,
            text: text.clone(),
            children: Vec::new(),
        },
        ResolvedTitlebarNode::Text { meta, text } | ResolvedTitlebarNode::Badge { meta, text } => {
            ResolvedLayoutNode::Content {
                meta: meta.clone(),
                kind: RuntimeContentKind::Text,
                text: Some(text.clone()),
                children: Vec::new(),
            }
        }
        ResolvedTitlebarNode::Button {
            meta,
            action,
            children,
        } => ResolvedLayoutNode::Content {
            meta: titlebar_button_meta(meta, *action),
            kind: RuntimeContentKind::Container,
            text: None,
            children: children.iter().map(titlebar_runtime_node_from_resolved).collect(),
        },
        ResolvedTitlebarNode::Icon {
            meta,
            asset,
            children,
        } => ResolvedLayoutNode::Content {
            meta: titlebar_icon_meta(meta, asset.as_deref(), children),
            kind: RuntimeContentKind::Container,
            text: None,
            children: Vec::new(),
        },
    }
}

fn titlebar_meta(class: Option<&str>, node_name: &str) -> LayoutNodeMeta {
    let mut meta = LayoutNodeMeta::default();
    meta.name = Some(node_name.to_string());
    if let Some(class) = class {
        meta.class = class.split_whitespace().map(str::to_string).collect();
    }
    meta
}

fn titlebar_button_meta(meta: &LayoutNodeMeta, action: Option<TitlebarButtonAction>) -> LayoutNodeMeta {
    let mut meta = meta.clone();
    if let Some(action) = action {
        meta.data.insert(TITLEBAR_ACTION_KEY.to_string(), titlebar_button_action_name(action).to_string());
    }
    meta
}

fn titlebar_icon_meta(
    meta: &LayoutNodeMeta,
    asset: Option<&str>,
    children: &[TitlebarIconNode],
) -> LayoutNodeMeta {
    let mut meta = meta.clone();
    if let Some(asset) = asset.filter(|asset| !asset.trim().is_empty()) {
        meta.data.insert(TITLEBAR_ICON_ASSET_KEY.to_string(), asset.to_string());
    }
    if !children.is_empty() && let Ok(serialized) = serde_json::to_string(children) {
        meta.data.insert(TITLEBAR_ICON_CHILDREN_KEY.to_string(), serialized);
    }
    meta
}

fn collect_titlebar_icon_paths(nodes: &[TitlebarIconNode], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            TitlebarIconNode::Svg { children, .. } => collect_titlebar_icon_paths(children, out),
            TitlebarIconNode::Path { d } => out.push(d.clone()),
        }
    }
}

fn titlebar_button_action_name(action: TitlebarButtonAction) -> &'static str {
    match action {
        TitlebarButtonAction::Close => "close",
        TitlebarButtonAction::ToggleFullscreen => "toggle-fullscreen",
        TitlebarButtonAction::ToggleFloating => "toggle-floating",
    }
}

fn normalize_button_action(value: &JsonValue) -> Option<TitlebarButtonAction> {
    if let Some(action) = value.as_str() {
        return parse_button_action_name(action);
    }

    let object = value.as_object()?;
    object
        .get("action")
        .and_then(JsonValue::as_str)
        .and_then(parse_button_action_name)
}

fn parse_button_action_name(name: &str) -> Option<TitlebarButtonAction> {
    match name {
        "close" | "close-window" | "closeFocusedWindow" => Some(TitlebarButtonAction::Close),
        "toggle-fullscreen" | "toggleFullscreen" => Some(TitlebarButtonAction::ToggleFullscreen),
        "toggle-floating" | "toggleFloating" => Some(TitlebarButtonAction::ToggleFloating),
        _ => None,
    }
}

impl Default for TitlebarPlanPreset {
    fn default() -> Self {
        Self {
            height: 24,
            border_bottom_width: 1,
            border_bottom_color_focused: ColorValue { red: 93, green: 173, blue: 226, alpha: 190 },
            border_bottom_color_unfocused: ColorValue { red: 94, green: 99, blue: 118, alpha: 150 },
            font: FontQuery {
                families: vec![
                    spiders_css::FontFamilyName::SystemUi,
                    spiders_css::FontFamilyName::SansSerif,
                ],
                weight: FontWeightValue::Bold,
                size_px: 12,
            },
            font_unfocused_weight: FontWeightValue::Normal,
            padding_left: 8,
            padding_right: 8,
        }
    }
}

pub fn build_titlebar_plan(input: &TitlebarPlanInput) -> TitlebarPlan {
    let titlebar_style = input.titlebar_style.as_ref();
    let window_style = input.window_style.as_ref();
    let background = apply_opacity(
        titlebar_background(
            titlebar_style,
            input.focused,
            input.default_background_focused,
            input.default_background_unfocused,
        ),
        input.effective_opacity,
    );
    let text_color = apply_opacity(
        titlebar_text_color(
            titlebar_style,
            input.focused,
            input.default_text_color_focused,
            input.default_text_color_unfocused,
        ),
        input.effective_opacity,
    );
    let text_transform =
        titlebar_style.and_then(|style| style.text_transform).unwrap_or(TextTransformValue::None);

    TitlebarPlan {
        window_id: input.window_id.clone(),
        title: apply_titlebar_text_transform(text_transform, input.title.clone()),
        height: titlebar_height_to_px(titlebar_style),
        offset_x: input.offset_x,
        offset_y: input.offset_y,
        background,
        border_bottom_width: titlebar_bottom_border_width(titlebar_style),
        border_bottom_color: apply_opacity(
            titlebar_bottom_border_color(titlebar_style, background),
            input.effective_opacity,
        ),
        text_color,
        text_align: titlebar_text_align(titlebar_style),
        text_transform,
        font: titlebar_font_query(titlebar_style),
        letter_spacing: titlebar_letter_spacing(titlebar_style),
        box_shadow: titlebar_box_shadow(titlebar_style, window_style),
        padding_top: titlebar_padding(titlebar_style).0,
        padding_right: titlebar_padding(titlebar_style).1,
        padding_bottom: titlebar_padding(titlebar_style).2,
        padding_left: titlebar_padding(titlebar_style).3,
        corner_radius_top_left: titlebar_corner_radii(titlebar_style, window_style).0,
        corner_radius_top_right: titlebar_corner_radii(titlebar_style, window_style).1,
        buttons: build_default_titlebar_buttons(
            titlebar_height_to_px(titlebar_style),
            &input.buttons,
        ),
    }
}

pub fn apply_titlebar_plan_preset(
    mut plan: TitlebarPlan,
    focused: bool,
    preset: &TitlebarPlanPreset,
) -> TitlebarPlan {
    plan.height = preset.height;
    plan.border_bottom_width = preset.border_bottom_width;
    plan.border_bottom_color = if focused {
        preset.border_bottom_color_focused
    } else {
        preset.border_bottom_color_unfocused
    };
    plan.font = FontQuery {
        weight: if focused { preset.font.weight } else { preset.font_unfocused_weight },
        ..preset.font.clone()
    };
    plan.padding_left = preset.padding_left;
    plan.padding_right = preset.padding_right;
    plan.buttons = build_default_titlebar_buttons(plan.height, &TitlebarButtonsConfig::default());
    plan
}

pub fn build_titlebar_plan_with_preset(
    input: &TitlebarPlanInput,
    preset: &TitlebarPlanPreset,
) -> TitlebarPlan {
    apply_titlebar_plan_preset(build_titlebar_plan(input), input.focused, preset)
}

fn build_default_titlebar_buttons(
    height: i32,
    config: &TitlebarButtonsConfig,
) -> Vec<TitlebarButtonPlan> {
    let size = (height - 8).clamp(10, 18);
    let top = ((height - size) / 2).max(0);
    let gap = 6;
    let start_x = 8;
    let mut entries = Vec::new();
    if config.close {
        entries.push((TitlebarButtonKind::Close, "close"));
    }
    if config.fullscreen {
        entries.push((TitlebarButtonKind::ToggleFullscreen, "fullscreen"));
    }
    if config.floating {
        entries.push((TitlebarButtonKind::ToggleFloating, "floating"));
    }

    entries
        .into_iter()
        .enumerate()
        .map(|(index, (kind, label))| TitlebarButtonPlan {
            kind,
            rect: TitlebarButtonRect {
                x: start_x + index as i32 * (size + gap),
                y: top,
                width: size,
                height: size,
            },
            label: label.to_string(),
        })
        .collect()
}

pub fn decoration_mode_for_window(
    appearance: AppearanceValue,
    has_titlebar_style: bool,
    supports_compositor_titlebar: bool,
    is_fullscreen: bool,
) -> DecorationMode {
    if is_fullscreen {
        return DecorationMode::NoTitlebar;
    }

    match appearance {
        AppearanceValue::Auto if has_titlebar_style && supports_compositor_titlebar => {
            DecorationMode::CompositorTitlebar
        }
        AppearanceValue::Auto => DecorationMode::ClientSide,
        AppearanceValue::None => DecorationMode::NoTitlebar,
    }
}

pub fn titlebar_text_from_window(title: Option<&str>, app_id: Option<&str>) -> String {
    title
        .filter(|title| !title.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| app_id.filter(|app_id| !app_id.trim().is_empty()).map(str::to_owned))
        .unwrap_or_default()
}

pub fn titlebar_button_colors(kind: TitlebarButtonKind) -> TitlebarButtonColors {
    match kind {
        TitlebarButtonKind::Close => {
            TitlebarButtonColors { red: 235, green: 87, blue: 87, alpha: 220 }
        }
        TitlebarButtonKind::ToggleFullscreen => {
            TitlebarButtonColors { red: 242, green: 201, blue: 76, alpha: 220 }
        }
        TitlebarButtonKind::ToggleFloating => {
            TitlebarButtonColors { red: 111, green: 207, blue: 151, alpha: 220 }
        }
    }
}

pub fn titlebar_text_left_inset(plan: &TitlebarPlan) -> i32 {
    let buttons_right =
        plan.buttons.iter().map(|button| button.rect.x + button.rect.width).max().unwrap_or(0);

    if buttons_right > 0 { plan.padding_left.max(buttons_right + 8) } else { plan.padding_left }
}

pub fn titlebar_text_right_inset(plan: &TitlebarPlan, trailing_width: i32) -> i32 {
    if trailing_width > 0 { plan.padding_right.max(trailing_width + 8) } else { plan.padding_right }
}

fn titlebar_font_family(style: Option<&ComputedStyle>) -> Option<spiders_css::FontFamilyValue> {
    style
        .and_then(|style| style.font_family.as_ref())
        .cloned()
        .filter(|families| !families.is_empty())
}

fn titlebar_font_weight(style: Option<&ComputedStyle>) -> FontWeightValue {
    style.and_then(|style| style.font_weight).unwrap_or(FontWeightValue::Normal)
}

fn titlebar_font_size(style: Option<&ComputedStyle>) -> i32 {
    match style.and_then(|style| style.font_size) {
        Some(LengthPercentage::Px(value)) | Some(LengthPercentage::Percent(value)) => {
            value.round() as i32
        }
        None => 14,
    }
    .clamp(8, 48)
}

fn titlebar_font_query(style: Option<&ComputedStyle>) -> FontQuery {
    FontQuery {
        families: titlebar_font_family(style).unwrap_or_default(),
        weight: titlebar_font_weight(style),
        size_px: titlebar_font_size(style),
    }
}

fn titlebar_letter_spacing(style: Option<&ComputedStyle>) -> i32 {
    style.and_then(|style| style.letter_spacing).unwrap_or(0.0).round() as i32
}

fn titlebar_box_shadow(
    titlebar_style: Option<&ComputedStyle>,
    window_style: Option<&ComputedStyle>,
) -> Option<Vec<BoxShadowValue>> {
    titlebar_style
        .and_then(|style| style.box_shadow.as_ref())
        .or_else(|| window_style.and_then(|style| style.box_shadow.as_ref()))
        .cloned()
        .filter(|shadow| !shadow.is_empty())
}

fn titlebar_padding(style: Option<&ComputedStyle>) -> (i32, i32, i32, i32) {
    let Some(padding) = style.and_then(|style| style.padding) else {
        return (0, 0, 0, 0);
    };

    (
        border_length_to_px(padding.top),
        border_length_to_px(padding.right),
        border_length_to_px(padding.bottom),
        border_length_to_px(padding.left),
    )
}

fn titlebar_corner_radii(
    titlebar_style: Option<&ComputedStyle>,
    window_style: Option<&ComputedStyle>,
) -> (i32, i32) {
    let radius = titlebar_style
        .and_then(|style| style.border_radius)
        .or_else(|| window_style.and_then(|style| style.border_radius));
    let Some(radius) = radius else {
        return (0, 0);
    };

    (radius.top_left, radius.top_right)
}

fn apply_titlebar_text_transform(transform: TextTransformValue, text: String) -> String {
    match transform {
        TextTransformValue::None => text,
        TextTransformValue::Uppercase => text.to_uppercase(),
        TextTransformValue::Lowercase => text.to_lowercase(),
        TextTransformValue::Capitalize => {
            let mut result = String::with_capacity(text.len());
            let mut at_word_start = true;

            for character in text.chars() {
                if at_word_start && character.is_alphanumeric() {
                    result.extend(character.to_uppercase());
                    at_word_start = false;
                } else {
                    result.push(character);
                    if !character.is_alphanumeric() {
                        at_word_start = true;
                    }
                }
            }

            result
        }
    }
}

fn titlebar_bottom_border_width(style: Option<&ComputedStyle>) -> i32 {
    if matches!(
        style.and_then(|style| style.border_style).map(|border| border.bottom),
        Some(BorderStyleValue::None)
    ) {
        return 0;
    }

    style
        .and_then(|style| style.border)
        .map(|border| border_length_to_px(border.bottom))
        .unwrap_or(0)
}

fn titlebar_bottom_border_color(
    style: Option<&ComputedStyle>,
    background: ColorValue,
) -> ColorValue {
    if let Some(color) =
        style.and_then(|style| style.border_side_colors).and_then(|colors| colors.bottom)
    {
        return color;
    }

    style.and_then(|style| style.border_color).unwrap_or(background)
}

fn titlebar_background(
    style: Option<&ComputedStyle>,
    focused: bool,
    focused_default: ColorValue,
    unfocused_default: ColorValue,
) -> ColorValue {
    style.and_then(|style| style.background).unwrap_or(if focused {
        focused_default
    } else {
        unfocused_default
    })
}

fn titlebar_text_color(
    style: Option<&ComputedStyle>,
    focused: bool,
    focused_default: ColorValue,
    unfocused_default: ColorValue,
) -> ColorValue {
    style.and_then(|style| style.color).unwrap_or(if focused {
        focused_default
    } else {
        unfocused_default
    })
}

fn titlebar_text_align(style: Option<&ComputedStyle>) -> TextAlignValue {
    style.and_then(|style| style.text_align).unwrap_or(TextAlignValue::Left)
}

fn titlebar_height_to_px(style: Option<&ComputedStyle>) -> i32 {
    match style.and_then(|style| style.height) {
        Some(spiders_css::SizeValue::Auto) => 28,
        Some(spiders_css::SizeValue::LengthPercentage(LengthPercentage::Px(value)))
        | Some(spiders_css::SizeValue::LengthPercentage(LengthPercentage::Percent(value))) => {
            value.round() as i32
        }
        _ => 28,
    }
    .clamp(16, 64)
}

fn border_length_to_px(length: LengthPercentage) -> i32 {
    match length {
        LengthPercentage::Px(value) | LengthPercentage::Percent(value) => value.round() as i32,
    }
    .max(0)
}

fn apply_opacity(color: ColorValue, opacity: f32) -> ColorValue {
    let alpha = (f32::from(color.alpha) * opacity.clamp(0.0, 1.0)).round().clamp(0.0, 255.0) as u8;
    ColorValue { alpha, ..color }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> TitlebarPlanInput {
        TitlebarPlanInput {
            window_id: WindowId::from("window-1"),
            title: "Example".to_string(),
            focused: true,
            titlebar_style: None,
            window_style: None,
            default_background_focused: ColorValue { red: 1, green: 2, blue: 3, alpha: 255 },
            default_background_unfocused: ColorValue { red: 4, green: 5, blue: 6, alpha: 255 },
            default_text_color_focused: ColorValue { red: 10, green: 11, blue: 12, alpha: 255 },
            default_text_color_unfocused: ColorValue { red: 13, green: 14, blue: 15, alpha: 255 },
            offset_x: 0,
            offset_y: 0,
            effective_opacity: 1.0,
            buttons: TitlebarButtonsConfig::default(),
        }
    }

    #[test]
    fn default_button_layout_uses_shared_order_and_spacing() {
        let plan = build_titlebar_plan(&base_input());

        assert_eq!(plan.height, 28);
        assert_eq!(plan.buttons.len(), 3);

        assert_eq!(plan.buttons[0].kind, TitlebarButtonKind::Close);
        assert_eq!(plan.buttons[1].kind, TitlebarButtonKind::ToggleFullscreen);
        assert_eq!(plan.buttons[2].kind, TitlebarButtonKind::ToggleFloating);

        assert_eq!(plan.buttons[0].rect, TitlebarButtonRect { x: 8, y: 5, width: 18, height: 18 });
        assert_eq!(plan.buttons[1].rect, TitlebarButtonRect { x: 32, y: 5, width: 18, height: 18 });
        assert_eq!(plan.buttons[2].rect, TitlebarButtonRect { x: 56, y: 5, width: 18, height: 18 });
    }

    #[test]
    fn text_left_inset_uses_button_bounds_when_buttons_exist() {
        let mut plan = build_titlebar_plan(&base_input());
        plan.padding_left = 12;

        assert_eq!(titlebar_text_left_inset(&plan), 82);
    }

    #[test]
    fn text_left_inset_respects_padding_when_buttons_are_disabled() {
        let mut input = base_input();
        input.buttons = TitlebarButtonsConfig { close: false, fullscreen: false, floating: false };

        let mut plan = build_titlebar_plan(&input);
        plan.padding_left = 12;

        assert_eq!(plan.buttons.len(), 0);
        assert_eq!(titlebar_text_left_inset(&plan), 12);
    }

    #[test]
    fn text_right_inset_respects_padding_when_no_trailing_content_exists() {
        let mut plan = build_titlebar_plan(&base_input());
        plan.padding_right = 10;

        assert_eq!(titlebar_text_right_inset(&plan, 0), 10);
    }

    #[test]
    fn text_right_inset_reserves_space_for_trailing_content() {
        let mut plan = build_titlebar_plan(&base_input());
        plan.padding_right = 10;

        assert_eq!(titlebar_text_right_inset(&plan, 48), 56);
    }

    #[test]
    fn shared_button_colors_match_expected_palette() {
        assert_eq!(
            titlebar_button_colors(TitlebarButtonKind::Close),
            TitlebarButtonColors { red: 235, green: 87, blue: 87, alpha: 220 }
        );
        assert_eq!(
            titlebar_button_colors(TitlebarButtonKind::ToggleFullscreen),
            TitlebarButtonColors { red: 242, green: 201, blue: 76, alpha: 220 }
        );
        assert_eq!(
            titlebar_button_colors(TitlebarButtonKind::ToggleFloating),
            TitlebarButtonColors { red: 111, green: 207, blue: 151, alpha: 220 }
        );
    }

    #[test]
    fn preview_preset_applies_shared_preview_defaults() {
        let mut input = base_input();
        input.focused = false;
        let plan = build_titlebar_plan_with_preset(&input, &TitlebarPlanPreset::default());

        assert_eq!(plan.height, 24);
        assert_eq!(plan.border_bottom_width, 1);
        assert_eq!(
            plan.border_bottom_color,
            ColorValue { red: 94, green: 99, blue: 118, alpha: 150 }
        );
        assert_eq!(plan.font.size_px, 12);
        assert_eq!(plan.font.weight, FontWeightValue::Normal);
        assert_eq!(plan.padding_left, 8);
        assert_eq!(plan.padding_right, 8);
        assert_eq!(plan.buttons[0].rect, TitlebarButtonRect { x: 8, y: 4, width: 16, height: 16 });
    }

    #[test]
    fn decode_titlebar_rules_parses_structured_rule_tree() {
        let value = serde_json::json!([
            {
                "class": "default-titlebar",
                "children": [
                    {
                        "type": "group",
                        "class": "left",
                        "children": [
                            { "type": "workspaceName", "class": "workspace-name" },
                            { "type": "windowTitle", "class": "window-title" }
                        ]
                    },
                    {
                        "type": "button",
                        "class": "close-button",
                        "on_click": { "action": "close" },
                        "children": [
                            { "type": "icon", "asset": "close" }
                        ]
                    }
                ]
            },
            {
                "when": { "app_id": "foot", "workspace": "code" },
                "disabled": true,
                "children": []
            }
        ]);

        let rules = decode_titlebar_rules(&value).expect("rules should decode");

        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].class.as_deref(), Some("default-titlebar"));
        assert!(!rules[0].disabled);
        assert_eq!(rules[1].when.as_ref().and_then(|when| when.app_id.as_deref()), Some("foot"));
        assert_eq!(rules[1].when.as_ref().and_then(|when| when.workspace.as_deref()), Some("code"));
        assert!(rules[1].disabled);
    }

    #[test]
    fn decode_titlebar_rules_parses_sdk_jsx_node_shape() {
        let value = serde_json::json!([
            {
                "type": "titlebar",
                "props": {
                    "class": "default-titlebar"
                },
                "children": [
                    {
                        "type": "titlebar.group",
                        "props": { "class": "left" },
                        "children": [
                            {
                                "type": "titlebar.workspaceName",
                                "props": { "class": "workspace-name" },
                                "children": []
                            }
                        ]
                    },
                    {
                        "type": "titlebar.button",
                        "props": {
                            "class": "close-button",
                            "onClick": { "action": "close" }
                        },
                        "children": [
                            {
                                "type": "titlebar.icon",
                                "props": { "asset": "close" },
                                "children": []
                            }
                        ]
                    }
                ]
            },
            {
                "type": "titlebar",
                "props": {
                    "when": { "app_id": "foot" },
                    "disabled": true
                },
                "children": []
            }
        ]);

        let rules = decode_titlebar_rules(&value).expect("sdk jsx nodes should decode");

        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].class.as_deref(), Some("default-titlebar"));
        assert!(matches!(rules[0].children[0], TitlebarNode::Group { .. }));
        assert!(matches!(rules[0].children[1], TitlebarNode::Button { .. }));
        assert_eq!(rules[1].when.as_ref().and_then(|when| when.app_id.as_deref()), Some("foot"));
        assert!(rules[1].disabled);
    }

    #[test]
    fn decode_titlebar_rules_normalizes_sdk_camel_case_when_keys() {
        let value = serde_json::json!([
            {
                "type": "titlebar",
                "props": {
                    "when": {
                        "appId": "foot",
                        "workspace": "code"
                    },
                    "disabled": true
                },
                "children": []
            }
        ]);

        let rules = decode_titlebar_rules(&value).expect("sdk jsx nodes should decode");

        assert_eq!(rules[0].when.as_ref().and_then(|when| when.app_id.as_deref()), Some("foot"));
        assert_eq!(rules[0].when.as_ref().and_then(|when| when.workspace.as_deref()), Some("code"));
        assert!(rules[0].disabled);
    }

    #[test]
    fn decode_titlebar_rules_accepts_normalized_on_click_alias() {
        let value = serde_json::json!([
            {
                "class": "default-titlebar",
                "children": [
                    {
                        "type": "button",
                        "class": "close-button",
                        "onClick": { "action": "close" },
                        "children": []
                    }
                ]
            }
        ]);

        let rules = decode_titlebar_rules(&value).expect("normalized titlebar rules should decode");

        match &rules[0].children[0] {
            TitlebarNode::Button { on_click, .. } => {
                assert_eq!(
                    on_click.as_ref().and_then(|value| value.get("action")).and_then(JsonValue::as_str),
                    Some("close")
                );
            }
            other => panic!("expected button node, got {other:?}"),
        }
    }

    #[test]
    fn decode_titlebar_rules_rejects_missing_node_type() {
        let value = serde_json::json!([
            {
                "children": [
                    { "class": "broken" }
                ]
            }
        ]);

        assert!(decode_titlebar_rules(&value).is_err());
    }

    #[test]
    fn select_titlebar_rule_uses_last_matching_rule() {
        let rules = vec![
            TitlebarRule {
                when: None,
                disabled: false,
                class: Some("default-titlebar".into()),
                children: Vec::new(),
            },
            TitlebarRule {
                when: Some(TitlebarWhen {
                    workspace: Some("code".into()),
                    ..TitlebarWhen::default()
                }),
                disabled: false,
                class: Some("workspace-titlebar".into()),
                children: Vec::new(),
            },
            TitlebarRule {
                when: Some(TitlebarWhen {
                    workspace: Some("code".into()),
                    app_id: Some("foot".into()),
                    ..TitlebarWhen::default()
                }),
                disabled: true,
                class: Some("foot-titlebar".into()),
                children: Vec::new(),
            },
        ];

        let selected = select_titlebar_rule(
            &rules,
            &ResolvedTitlebarContext {
                workspace: Some("code".into()),
                app_id: Some("foot".into()),
                ..ResolvedTitlebarContext::default()
            },
        )
        .expect("a rule should match");

        assert_eq!(selected.class.as_deref(), Some("foot-titlebar"));
        assert!(selected.disabled);
    }

    #[test]
    fn titlebar_rule_matches_all_supported_fields() {
        let rule = TitlebarRule {
            when: Some(TitlebarWhen {
                workspace: Some("code".into()),
                slot: Some("main".into()),
                app_id: Some("firefox".into()),
                title: Some("Mozilla".into()),
                floating: Some(false),
                fullscreen: Some(true),
            }),
            disabled: false,
            class: None,
            children: Vec::new(),
        };

        assert!(titlebar_rule_matches(
            &rule,
            &ResolvedTitlebarContext {
                workspace: Some("code".into()),
                slot: Some("main".into()),
                app_id: Some("firefox".into()),
                title: Some("Mozilla Firefox".into()),
                floating: false,
                fullscreen: true,
            }
        ));

        assert!(!titlebar_rule_matches(
            &rule,
            &ResolvedTitlebarContext {
                workspace: Some("code".into()),
                slot: Some("main".into()),
                app_id: Some("firefox".into()),
                title: Some("Mozilla Firefox".into()),
                floating: true,
                fullscreen: true,
            }
        ));
    }

    #[test]
    fn resolve_titlebar_tree_preserves_classes_and_structure() {
        let rule = TitlebarRule {
            when: None,
            disabled: false,
            class: Some("default-titlebar root".into()),
            children: vec![TitlebarNode::Group {
                class: Some("left cluster".into()),
                children: vec![TitlebarNode::Text {
                    class: Some("label strong".into()),
                    text: "DEV".into(),
                }],
            }],
        };

        let tree = resolve_titlebar_tree(&rule, &ResolvedTitlebarContext::default());

        assert_eq!(tree.meta.name.as_deref(), Some("titlebar"));
        assert_eq!(tree.meta.class, vec!["default-titlebar", "root"]);
        match &tree.children[0] {
            ResolvedTitlebarNode::Group { meta, children } => {
                assert_eq!(meta.name.as_deref(), Some("titlebar-group"));
                assert_eq!(meta.class, vec!["left", "cluster"]);
                assert!(matches!(
                    &children[0],
                    ResolvedTitlebarNode::Text { text, .. } if text == "DEV"
                ));
            }
            other => panic!("expected group node, got {other:?}"),
        }
    }

    #[test]
    fn resolve_titlebar_tree_uses_context_for_dynamic_icon_asset_fallback() {
        let rule = TitlebarRule {
            when: None,
            disabled: false,
            class: None,
            children: vec![TitlebarNode::Icon { class: None, asset: None, children: Vec::new() }],
        };

        let tree = resolve_titlebar_tree(
            &rule,
            &ResolvedTitlebarContext {
                app_id: Some("firefox".into()),
                ..ResolvedTitlebarContext::default()
            },
        );

        match &tree.children[0] {
            ResolvedTitlebarNode::Icon { asset, .. } => {
                assert_eq!(asset.as_deref(), Some("firefox"));
            }
            other => panic!("expected icon node, got {other:?}"),
        }
    }

    #[test]
    fn titlebar_tree_maps_into_generic_runtime_content_nodes() {
        let tree = resolve_titlebar_tree(
            &TitlebarRule {
                when: None,
                disabled: false,
                class: Some("default-titlebar".into()),
                children: vec![
                    TitlebarNode::Group {
                        class: Some("left".into()),
                        children: vec![TitlebarNode::Text {
                            class: Some("label".into()),
                            text: "DEV".into(),
                        }],
                    },
                    TitlebarNode::WindowTitle {
                        class: Some("window-title".into()),
                        fallback: Some("fallback".into()),
                    },
                ],
            },
            &ResolvedTitlebarContext::default(),
        );

        let runtime = titlebar_tree_to_runtime_node(&tree);

        match runtime {
            ResolvedLayoutNode::Content { meta, kind, children, .. } => {
                assert_eq!(meta.name.as_deref(), Some("titlebar"));
                assert_eq!(kind, RuntimeContentKind::Container);
                assert_eq!(children.len(), 2);
                assert!(matches!(
                    &children[0],
                    ResolvedLayoutNode::Content { kind: RuntimeContentKind::Container, .. }
                ));
                assert!(matches!(
                    &children[1],
                    ResolvedLayoutNode::Content {
                        kind: RuntimeContentKind::Text,
                        text: Some(text),
                        ..
                    } if text == "fallback"
                ));
            }
            other => panic!("expected runtime content root, got {other:?}"),
        }
    }

    #[test]
    fn resolve_titlebar_tree_fills_dynamic_window_and_workspace_text() {
        let tree = resolve_titlebar_tree(
            &TitlebarRule {
                when: None,
                disabled: false,
                class: None,
                children: vec![
                    TitlebarNode::WorkspaceName {
                        class: Some("workspace-name".into()),
                    },
                    TitlebarNode::WindowTitle {
                        class: Some("window-title".into()),
                        fallback: Some("fallback".into()),
                    },
                ],
            },
            &ResolvedTitlebarContext {
                workspace: Some("code".into()),
                title: Some("Mozilla Firefox".into()),
                ..ResolvedTitlebarContext::default()
            },
        );

        assert!(matches!(
            &tree.children[0],
            ResolvedTitlebarNode::WorkspaceName { text: Some(text), .. } if text == "code"
        ));
        assert!(matches!(
            &tree.children[1],
            ResolvedTitlebarNode::WindowTitle { text: Some(text), .. } if text == "Mozilla Firefox"
        ));
    }

    #[test]
    fn titlebar_button_action_maps_into_runtime_meta_data() {
        let tree = resolve_titlebar_tree(
            &TitlebarRule {
                when: None,
                disabled: false,
                class: None,
                children: vec![TitlebarNode::Button {
                    class: Some("close-button".into()),
                    on_click: Some(serde_json::json!({ "action": "close" })),
                    children: vec![TitlebarNode::Text { class: None, text: "x".into() }],
                }],
            },
            &ResolvedTitlebarContext::default(),
        );

        let runtime = titlebar_tree_to_runtime_node(&tree);

        match runtime {
            ResolvedLayoutNode::Content { children, .. } => match &children[0] {
                ResolvedLayoutNode::Content { meta, .. } => {
                    assert_eq!(meta.data.get(TITLEBAR_ACTION_KEY).map(String::as_str), Some("close"));
                }
                other => panic!("expected content button node, got {other:?}"),
            },
            other => panic!("expected runtime content root, got {other:?}"),
        }
    }

    #[test]
    fn titlebar_icon_maps_vector_payload_into_runtime_meta_data() {
        let tree = resolve_titlebar_tree(
            &TitlebarRule {
                when: None,
                disabled: false,
                class: None,
                children: vec![TitlebarNode::Icon {
                    class: Some("close-icon".into()),
                    asset: Some("close".into()),
                    children: vec![TitlebarIconNode::Svg {
                        view_box: Some("0 0 16 16".into()),
                        children: vec![
                            TitlebarIconNode::Path { d: "M2 2 L14 14".into() },
                            TitlebarIconNode::Path { d: "M14 2 L2 14".into() },
                        ],
                    }],
                }],
            },
            &ResolvedTitlebarContext::default(),
        );

        let runtime = titlebar_tree_to_runtime_node(&tree);

        match runtime {
            ResolvedLayoutNode::Content { children, .. } => match &children[0] {
                ResolvedLayoutNode::Content { meta, kind, text, .. } => {
                    assert_eq!(*kind, RuntimeContentKind::Container);
                    assert_eq!(text, &None);
                    assert_eq!(titlebar_icon_asset_from_data(&meta.data).as_deref(), Some("close"));
                    let nodes = titlebar_icon_nodes_from_data(&meta.data).expect("icon metadata missing");
                    assert_eq!(titlebar_icon_view_box(&nodes).as_deref(), Some("0 0 16 16"));
                    assert_eq!(titlebar_icon_paths(&nodes).len(), 2);
                }
                other => panic!("expected content icon node, got {other:?}"),
            },
            other => panic!("expected runtime content root, got {other:?}"),
        }
    }
}
