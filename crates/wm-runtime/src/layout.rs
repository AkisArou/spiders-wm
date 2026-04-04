use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use spiders_core::snapshot::WindowSnapshot;
use spiders_core::types::{ShellKind, WindowMode};
use spiders_core::wm::WindowGeometry;
use spiders_core::{OutputId, ResolvedLayoutNode, WindowId, WorkspaceId};
use spiders_css::style::FlexDirectionValue;
use spiders_scene::LayoutSnapshotNode;
use spiders_scene::ast::ValidatedLayoutTree;
use spiders_scene::pipeline::{LayoutPipelineError, compile_stylesheet, compute_layout_from_sheet};
use crate::{PreviewDiagnostic, PreviewSnapshotClasses, PreviewSnapshotNode};
use crate::PreviewWindow;

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
    stylesheet_source: &str,
    width: f32,
    height: f32,
) -> PreviewLayoutComputation {
    let window_snapshots = windows.iter().cloned().map(window_snapshot).collect::<Vec<_>>();

    match ValidatedLayoutTree::new(layout.clone()) {
        Ok(validated) => match validated.resolve(&window_snapshots) {
            Ok(resolved) => {
                let root = resolved.root;
                let claimed_ids = collect_claimed_window_ids(&root);
                match compile_stylesheet(stylesheet_source)
                    .and_then(|sheet| compute_layout_from_sheet(&root, &sheet, width, height))
                {
                    Ok(laid_out) => PreviewLayoutComputation {
                        snapshot_root: Some(snapshot_node(laid_out.snapshot())),
                        diagnostics: Vec::new(),
                        unclaimed_window_ids: unclaimed_window_ids(windows, &claimed_ids),
                    },
                    Err(error) => PreviewLayoutComputation {
                        snapshot_root: None,
                        diagnostics: vec![css_diagnostic(error)],
                        unclaimed_window_ids: unclaimed_window_ids(windows, &claimed_ids),
                    },
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
        | ResolvedLayoutNode::Group { children, .. } => {
            for child in children {
                collect_claimed_window_ids_inner(child, out);
            }
        }
        ResolvedLayoutNode::Window { window_id, .. } => {
            if let Some(window_id) = window_id.as_ref() {
                out.insert(window_id.as_str().to_owned());
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
            children: children.into_iter().map(snapshot_node).collect(),
        },
        LayoutSnapshotNode::Window {
            meta,
            rect,
            window_id,
            styles,
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
            children: Vec::new(),
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
