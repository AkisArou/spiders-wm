use std::collections::BTreeSet;
use serde::{Deserialize, Serialize};
use spiders_config::model::Config;
use spiders_core::runtime::layout_context::LayoutWindowContext;
use spiders_core::runtime::prepared_layout::{PreparedStylesheet, PreparedStylesheets};
use spiders_core::snapshot::WindowSnapshot;
use spiders_core::types::{ShellKind, WindowMode};
use spiders_core::wm::WindowGeometry;
use spiders_core::{LayoutSpace, OutputId, ResolvedLayoutNode, WindowId, WorkspaceId};
use spiders_css::style::FlexDirectionValue;
use spiders_scene::{LayoutSnapshotNode, SceneRequest, SceneResponse};
use spiders_scene::ast::ValidatedLayoutTree;
use spiders_scene::pipeline::{LayoutPipelineError, SceneCache, compile_stylesheet, compute_layout_from_sheet};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use crate::{PreviewDiagnostic, PreviewSnapshotClasses, PreviewSnapshotNode};
use crate::PreviewWindow;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = "export function spidersPerfNow() { return Date.now(); }")]
extern "C" {
    fn spidersPerfNow() -> f64;
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = "export function spidersPerfLog(message) { console.log(message); }")]
extern "C" {
    fn spidersPerfLog(message: &str);
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    spidersPerfNow()
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    0.0
}

#[cfg(target_arch = "wasm32")]
fn perf_log(stage: &str, started_ms: f64, windows: usize) {
    let elapsed_ms = now_ms() - started_ms;
    spidersPerfLog(&format!(
        "[perf] wm-runtime.{stage} {:.2}ms windows={windows}",
        elapsed_ms
    ));
}

#[cfg(not(target_arch = "wasm32"))]
fn perf_log(_stage: &str, _started_ms: f64, _windows: usize) {}

pub const PREVIEW_OUTPUT_ID: &str = "preview-output";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLayoutComputation {
    pub snapshot_root: Option<PreviewSnapshotNode>,
    #[serde(default)]
    pub diagnostics: Vec<PreviewDiagnostic>,
    #[serde(default)]
    pub unclaimed_window_ids: Vec<String>,
}

pub fn compute_layout_preview_from_source_layout(
    layout: &spiders_core::SourceLayoutNode,
    windows: &[PreviewWindow],
    config: Option<&Config>,
    workspace_name: Option<&str>,
    stylesheet_source: &str,
    width: f32,
    height: f32,
) -> PreviewLayoutComputation {
    compute_layout_preview_from_source_layout_with_cache(
        layout,
        windows,
        config,
        workspace_name,
        stylesheet_source,
        width,
        height,
        None,
    )
}

pub fn compute_layout_preview_from_source_layout_with_cache(
    layout: &spiders_core::SourceLayoutNode,
    windows: &[PreviewWindow],
    config: Option<&Config>,
    workspace_name: Option<&str>,
    stylesheet_source: &str,
    width: f32,
    height: f32,
    mut scene_cache: Option<&mut SceneCache>,
) -> PreviewLayoutComputation {
    let total_started = now_ms();
    let window_snapshots = windows.iter().cloned().map(window_snapshot).collect::<Vec<_>>();
    let validate_started = now_ms();

    match ValidatedLayoutTree::new(layout.clone()) {
        Ok(validated) => match validated.resolve(&window_snapshots) {
            Ok(resolved) => {
                perf_log("layout.validate_and_resolve", validate_started, windows.len());
                let titlebar_started = now_ms();
                let root = attach_titlebar_content(
                    resolved.root,
                    &window_snapshots,
                    config,
                    workspace_name,
                );
                perf_log("layout.attach_titlebar_content", titlebar_started, windows.len());
                let claimed_started = now_ms();
                let claimed_ids = collect_claimed_window_ids(&root);
                perf_log("layout.collect_claimed_window_ids", claimed_started, windows.len());
                let style_started = now_ms();
                let layout_result = if let Some(cache) = scene_cache.as_deref_mut() {
                    let request = SceneRequest {
                        workspace_id: workspace_name.map(WorkspaceId::from).unwrap_or_default(),
                        output_id: Some(OutputId::from(PREVIEW_OUTPUT_ID)),
                        layout_name: workspace_name.map(ToOwned::to_owned),
                        root: root.clone(),
                        stylesheets: PreparedStylesheets {
                            global: None,
                            layout: Some(PreparedStylesheet {
                                path: workspace_name
                                    .map(|name| format!("preview://{name}.css"))
                                    .unwrap_or_else(|| "preview://layout.css".to_string()),
                                source: stylesheet_source.to_string(),
                            }),
                        },
                        space: LayoutSpace { width, height },
                    };
                    let precompile_started = now_ms();
                    match cache.precompile_layout(
                        request.layout_name.as_deref().unwrap_or("__default__"),
                        stylesheet_source,
                    ) {
                        Ok(()) => {
                            perf_log("layout.precompile_stylesheet", precompile_started, windows.len());
                            let compute_started = now_ms();
                            let response = cache.compute_layout_from_request(&request);
                            perf_log("layout.compute_from_cached_sheet", compute_started, windows.len());
                            response
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    compile_stylesheet(stylesheet_source)
                        .and_then(|sheet| compute_layout_from_sheet(&root, &sheet, width, height))
                        .map(|laid_out| SceneResponse { root: laid_out.snapshot() })
                };

                match layout_result {
                    Ok(response) => {
                        perf_log("layout.compile_and_compute", style_started, windows.len());
                        let snapshot_started = now_ms();
                        let result = PreviewLayoutComputation {
                            snapshot_root: Some(snapshot_node(response.root)),
                            diagnostics: Vec::new(),
                            unclaimed_window_ids: unclaimed_window_ids(windows, &claimed_ids),
                        };
                        perf_log("layout.snapshot_build", snapshot_started, windows.len());
                        perf_log("layout.total", total_started, windows.len());
                        result
                    }
                    Err(error) => {
                        perf_log("layout.compile_and_compute", style_started, windows.len());
                        perf_log("layout.total", total_started, windows.len());
                        PreviewLayoutComputation {
                            snapshot_root: None,
                            diagnostics: vec![css_diagnostic(error)],
                            unclaimed_window_ids: unclaimed_window_ids(windows, &claimed_ids),
                        }
                    }
                }
            }
            Err(error) => PreviewLayoutComputation {
                snapshot_root: None,
                diagnostics: vec![layout_diagnostic(error.to_string())],
                unclaimed_window_ids: windows.iter().map(|window| window.id.clone()).collect(),
            },
        },
        Err(error) => PreviewLayoutComputation {
            snapshot_root: None,
            diagnostics: vec![layout_diagnostic(error.to_string())],
            unclaimed_window_ids: windows.iter().map(|window| window.id.clone()).collect(),
        },
    }
}

fn attach_titlebar_content(
    node: ResolvedLayoutNode,
    windows: &[WindowSnapshot],
    config: Option<&Config>,
    workspace_name: Option<&str>,
) -> ResolvedLayoutNode {
    match node {
        ResolvedLayoutNode::Workspace { meta, children } => ResolvedLayoutNode::Workspace {
            meta,
            children: children
                .into_iter()
                .map(|child| attach_titlebar_content(child, windows, config, workspace_name))
                .collect(),
        },
        ResolvedLayoutNode::Group { meta, children } => ResolvedLayoutNode::Group {
            meta,
            children: children
                .into_iter()
                .map(|child| attach_titlebar_content(child, windows, config, workspace_name))
                .collect(),
        },
        ResolvedLayoutNode::Content {
            meta,
            kind,
            text,
            children,
        } => ResolvedLayoutNode::Content {
            meta,
            kind,
            text,
            children: children
                .into_iter()
                .map(|child| attach_titlebar_content(child, windows, config, workspace_name))
                .collect(),
        },
        ResolvedLayoutNode::Window {
            meta,
            window_id,
            mut children,
        } => {
            children = children
                .into_iter()
                .map(|child| attach_titlebar_content(child, windows, config, workspace_name))
                .collect();

            if let (Some(config), Some(window_id)) = (config, window_id.as_ref())
                && let Some(window) = windows.iter().find(|window| &window.id == window_id)
                && let Ok(Some(titlebar)) =
                    config.resolve_titlebar_runtime_node(&layout_window_context(window), workspace_name, None)
            {
                children.insert(0, titlebar);
            }

            ResolvedLayoutNode::Window {
                meta,
                window_id,
                children,
            }
        }
    }
}

fn layout_window_context(window: &WindowSnapshot) -> LayoutWindowContext {
    LayoutWindowContext {
        id: window.id.clone(),
        app_id: window.app_id.clone(),
        title: window.title.clone(),
        class: window.class.clone(),
        instance: window.instance.clone(),
        role: window.role.clone(),
        shell: Some(match window.shell {
            ShellKind::X11 => "x11",
            ShellKind::XdgToplevel => "xdg_toplevel",
            ShellKind::Unknown => "unknown",
        }
        .to_string()),
        window_type: window.window_type.clone(),
        floating: window.mode.is_floating(),
        fullscreen: window.mode.is_fullscreen(),
        focused: window.focused,
    }
}

fn window_snapshot(window: PreviewWindow) -> WindowSnapshot {
    let mode = if window.fullscreen {
        WindowMode::Fullscreen
    } else if window.floating {
        WindowMode::Floating { rect: None }
    } else {
        WindowMode::Tiled
    };

    WindowSnapshot {
        id: window.id.into(),
        shell: match window.shell.as_deref() {
            Some("x11") => ShellKind::X11,
            Some("xdg-toplevel") | Some("xdg_toplevel") => ShellKind::XdgToplevel,
            _ => ShellKind::Unknown,
        },
        app_id: window.app_id,
        title: window.title,
        class: window.class,
        instance: window.instance,
        role: window.role,
        window_type: window.window_type,
        mapped: true,
        mode,
        focused: window.focused,
        urgent: false,
        closing: false,
        output_id: None,
        workspace_id: None,
        workspaces: Vec::new(),
    }
}

pub fn preview_window_snapshot(window: &PreviewWindow, workspace_name: Option<&str>) -> WindowSnapshot {
    let mut snapshot = window_snapshot(window.clone());
    snapshot.output_id = Some(OutputId::from(PREVIEW_OUTPUT_ID));
    snapshot.workspace_id = workspace_name.map(WorkspaceId::from);
    snapshot.workspaces = workspace_name.into_iter().map(ToOwned::to_owned).collect();
    snapshot
}

pub fn collect_snapshot_geometries(
    node: &PreviewSnapshotNode,
    out: &mut std::collections::BTreeMap<WindowId, WindowGeometry>,
) {
    if node.node_type == "window"
        && let (Some(window_id), Some(rect)) = (node.window_id.as_ref(), node.rect)
    {
        out.insert(
            window_id.clone(),
            WindowGeometry {
                x: rect.x.round() as i32,
                y: rect.y.round() as i32,
                width: rect.width.round() as i32,
                height: rect.height.round() as i32,
            },
        );
    }

    for child in &node.children {
        collect_snapshot_geometries(child, out);
    }
}

pub fn empty_window_geometry() -> WindowGeometry {
    WindowGeometry { x: 0, y: 0, width: 0, height: 0 }
}

fn collect_claimed_window_ids(root: &ResolvedLayoutNode) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    collect_claimed_window_ids_inner(root, &mut ids);
    ids
}

fn collect_claimed_window_ids_inner(root: &ResolvedLayoutNode, out: &mut BTreeSet<String>) {
    match root {
        ResolvedLayoutNode::Workspace { children, .. }
        | ResolvedLayoutNode::Group { children, .. }
        | ResolvedLayoutNode::Content { children, .. } => {
            for child in children {
                collect_claimed_window_ids_inner(child, out);
            }
        }
        ResolvedLayoutNode::Window { window_id, children, .. } => {
            if let Some(window_id) = window_id.as_ref() {
                out.insert(window_id.as_str().to_owned());
            }
            for child in children {
                collect_claimed_window_ids_inner(child, out);
            }
        }
    }
}

fn snapshot_node(node: LayoutSnapshotNode) -> PreviewSnapshotNode {
    match node {
        LayoutSnapshotNode::Workspace { meta, rect, children, styles } => PreviewSnapshotNode {
            node_type: "workspace".to_string(),
            id: meta.id,
            class_name: snapshot_classes(meta.class),
            rect: Some(rect),
            window_id: None,
            axis: layout_axis(styles.as_ref()).map(str::to_string),
            reverse: layout_reverse(styles.as_ref()),
            layout_style: None,
            titlebar_style: None,
            text: None,
            data: meta.data,
            children: children.into_iter().map(snapshot_node).collect(),
        },
        LayoutSnapshotNode::Group { meta, rect, children, styles } => PreviewSnapshotNode {
            node_type: "group".to_string(),
            id: meta.id,
            class_name: snapshot_classes(meta.class),
            rect: Some(rect),
            window_id: None,
            axis: layout_axis(styles.as_ref()).map(str::to_string),
            reverse: layout_reverse(styles.as_ref()),
            layout_style: None,
            titlebar_style: None,
            text: None,
            data: meta.data,
            children: children.into_iter().map(snapshot_node).collect(),
        },
        LayoutSnapshotNode::Content {
            meta,
            rect,
            text,
            children,
            styles,
        } => PreviewSnapshotNode {
            node_type: meta.name.clone().unwrap_or_else(|| "content".to_string()),
            id: meta.id,
            class_name: snapshot_classes(meta.class),
            rect: Some(rect),
            window_id: None,
            axis: None,
            reverse: false,
            layout_style: styles.as_ref().map(|styles| styles.layout.clone()),
            titlebar_style: None,
            text,
            data: meta.data,
            children: children.into_iter().map(snapshot_node).collect(),
        },
        LayoutSnapshotNode::Window {
            meta,
            rect,
            window_id,
            styles,
            children,
        } => PreviewSnapshotNode {
            node_type: "window".to_string(),
            id: meta.id,
            class_name: snapshot_classes(meta.class),
            rect: Some(rect),
            window_id,
            axis: None,
            reverse: false,
            layout_style: styles.as_ref().map(|styles| styles.layout.clone()),
            titlebar_style: styles.as_ref().and_then(|styles| styles.titlebar.clone()),
            text: None,
            data: meta.data,
            children: children.into_iter().map(snapshot_node).collect(),
        },
    }
}

fn snapshot_classes(classes: Vec<String>) -> Option<PreviewSnapshotClasses> {
    match classes.as_slice() {
        [] => None,
        [single] => Some(PreviewSnapshotClasses::One(single.clone())),
        _ => Some(PreviewSnapshotClasses::Many(classes)),
    }
}

fn layout_axis(styles: Option<&spiders_scene::SceneNodeStyle>) -> Option<&'static str> {
    match styles?.layout.flex_direction? {
        FlexDirectionValue::Row | FlexDirectionValue::RowReverse => Some("horizontal"),
        FlexDirectionValue::Column | FlexDirectionValue::ColumnReverse => Some("vertical"),
    }
}

fn layout_reverse(styles: Option<&spiders_scene::SceneNodeStyle>) -> bool {
    matches!(
        styles.and_then(|styles| styles.layout.flex_direction),
        Some(FlexDirectionValue::RowReverse | FlexDirectionValue::ColumnReverse)
    )
}

fn unclaimed_window_ids(
    windows: &[PreviewWindow],
    claimed_ids: &BTreeSet<String>,
) -> Vec<String> {
    windows
        .iter()
        .filter(|window| !claimed_ids.contains(window.id.as_str()))
        .map(|window| window.id.clone())
        .collect()
}

fn layout_diagnostic(message: String) -> PreviewDiagnostic {
    PreviewDiagnostic { source: "layout".to_string(), level: "error".to_string(), message }
}

fn css_diagnostic(error: LayoutPipelineError) -> PreviewDiagnostic {
    PreviewDiagnostic {
        source: "css".to_string(),
        level: "error".to_string(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_core::{LayoutNodeMeta, SourceLayoutNode};
    use spiders_titlebar_core::TITLEBAR_ACTION_KEY;

    fn preview_window(id: &str, app_id: &str, title: &str) -> PreviewWindow {
        PreviewWindow {
            id: id.to_string(),
            app_id: Some(app_id.to_string()),
            title: Some(title.to_string()),
            class: Some(app_id.to_string()),
            instance: Some(app_id.to_string()),
            role: None,
            shell: Some("xdg_toplevel".to_string()),
            window_type: None,
            floating: false,
            fullscreen: false,
            focused: true,
            workspace_name: "1".to_string(),
        }
    }

    fn first_titlebar_button(node: &PreviewSnapshotNode) -> Option<&PreviewSnapshotNode> {
        if node.node_type == "titlebar-button" {
            return Some(node);
        }

        node.children.iter().find_map(first_titlebar_button)
    }

    #[test]
    fn preview_snapshot_preserves_titlebar_button_action_metadata() {
        let layout = SourceLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![SourceLayoutNode::Window {
                meta: LayoutNodeMeta::default(),
                window_match: None,
            }],
        };
        let config = Config {
            titlebars: vec![serde_json::json!({
                "class": "default-titlebar",
                "children": [
                    {
                        "type": "button",
                        "class": "close-button",
                        "onClick": { "action": "close" },
                        "children": [
                            {
                                "type": "text",
                                "text": "x"
                            }
                        ]
                    }
                ]
            })],
            ..Config::default()
        };

        let result = compute_layout_preview_from_source_layout(
            &layout,
            &[preview_window("win-1", "foot", "Terminal 1")],
            Some(&config),
            Some("1"),
            "workspace { display: flex; width: 100%; height: 100%; } window { flex: 1; } titlebar { display: flex; height: 28px; } .close-button { width: 18px; height: 18px; }",
            1200.0,
            800.0,
        );

        let root = result.snapshot_root.expect("preview snapshot should exist");
        let button = first_titlebar_button(&root).expect("titlebar button should exist");

        assert_eq!(button.data.get(TITLEBAR_ACTION_KEY).map(String::as_str), Some("close"));
    }
}
