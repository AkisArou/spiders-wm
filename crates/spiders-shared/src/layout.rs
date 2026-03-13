use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{OutputId, WindowId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutSpace {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutNodeType {
    Workspace,
    Group,
    Window,
    Slot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeLayoutNodeType {
    Workspace,
    Group,
    Window,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutNodeMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub class: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub data: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKey {
    AppId,
    Title,
    Class,
    Instance,
    Role,
    Shell,
    WindowType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchClause {
    pub key: MatchKey,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowMatch {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clauses: Vec<MatchClause>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemainingTake {
    #[serde(rename = "remaining")]
    Remaining,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SlotTake {
    Count(u32),
    Remaining(RemainingTake),
}

impl Default for SlotTake {
    fn default() -> Self {
        Self::Remaining(RemainingTake::Remaining)
    }
}

impl SlotTake {
    pub fn is_remaining(&self) -> bool {
        matches!(self, Self::Remaining(RemainingTake::Remaining))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SourceLayoutNode {
    Workspace {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        children: Vec<SourceLayoutNode>,
    },
    Group {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        children: Vec<SourceLayoutNode>,
    },
    Window {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "match")]
        window_match: Option<WindowMatch>,
    },
    Slot {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "match")]
        window_match: Option<WindowMatch>,
        #[serde(default, skip_serializing_if = "SlotTake::is_remaining")]
        take: SlotTake,
    },
}

impl SourceLayoutNode {
    pub fn node_type(&self) -> LayoutNodeType {
        match self {
            Self::Workspace { .. } => LayoutNodeType::Workspace,
            Self::Group { .. } => LayoutNodeType::Group,
            Self::Window { .. } => LayoutNodeType::Window,
            Self::Slot { .. } => LayoutNodeType::Slot,
        }
    }

    pub fn meta(&self) -> &LayoutNodeMeta {
        match self {
            Self::Workspace { meta, .. }
            | Self::Group { meta, .. }
            | Self::Window { meta, .. }
            | Self::Slot { meta, .. } => meta,
        }
    }

    pub fn children(&self) -> &[SourceLayoutNode] {
        match self {
            Self::Workspace { children, .. } | Self::Group { children, .. } => children,
            Self::Window { .. } | Self::Slot { .. } => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ResolvedLayoutNode {
    Workspace {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        children: Vec<ResolvedLayoutNode>,
    },
    Group {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        children: Vec<ResolvedLayoutNode>,
    },
    Window {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_id: Option<WindowId>,
    },
}

impl ResolvedLayoutNode {
    pub fn node_type(&self) -> RuntimeLayoutNodeType {
        match self {
            Self::Workspace { .. } => RuntimeLayoutNodeType::Workspace,
            Self::Group { .. } => RuntimeLayoutNodeType::Group,
            Self::Window { .. } => RuntimeLayoutNodeType::Window,
        }
    }

    pub fn meta(&self) -> &LayoutNodeMeta {
        match self {
            Self::Workspace { meta, .. } | Self::Group { meta, .. } | Self::Window { meta, .. } => {
                meta
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LayoutSnapshotNode {
    Workspace {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        rect: LayoutRect,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        children: Vec<LayoutSnapshotNode>,
    },
    Group {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        rect: LayoutRect,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        children: Vec<LayoutSnapshotNode>,
    },
    Window {
        #[serde(flatten)]
        meta: LayoutNodeMeta,
        rect: LayoutRect,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_id: Option<WindowId>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutRequest {
    pub workspace_id: WorkspaceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_id: Option<OutputId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_name: Option<String>,
    pub root: ResolvedLayoutNode,
    pub stylesheet: String,
    pub space: LayoutSpace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutResponse {
    pub root: LayoutSnapshotNode,
}

impl LayoutSnapshotNode {
    pub fn rect(&self) -> LayoutRect {
        match self {
            Self::Workspace { rect, .. } | Self::Group { rect, .. } | Self::Window { rect, .. } => {
                *rect
            }
        }
    }

    pub fn meta(&self) -> &LayoutNodeMeta {
        match self {
            Self::Workspace { meta, .. } | Self::Group { meta, .. } | Self::Window { meta, .. } => {
                meta
            }
        }
    }

    pub fn children(&self) -> &[LayoutSnapshotNode] {
        match self {
            Self::Workspace { children, .. } | Self::Group { children, .. } => children,
            Self::Window { .. } => &[],
        }
    }

    pub fn find_by_node_id(&self, node_id: &str) -> Option<&LayoutSnapshotNode> {
        if self.meta().id.as_deref() == Some(node_id) {
            return Some(self);
        }

        self.children()
            .iter()
            .find_map(|child| child.find_by_node_id(node_id))
    }

    pub fn find_by_window_id(&self, window_id: &WindowId) -> Option<&LayoutSnapshotNode> {
        if matches!(self, Self::Window { window_id: Some(id), .. } if id == window_id) {
            return Some(self);
        }

        self.children()
            .iter()
            .find_map(|child| child.find_by_window_id(window_id))
    }

    pub fn collect_windows<'a>(&'a self, windows: &mut Vec<&'a LayoutSnapshotNode>) {
        if matches!(self, Self::Window { .. }) {
            windows.push(self);
            return;
        }

        for child in self.children() {
            child.collect_windows(windows);
        }
    }

    pub fn window_nodes(&self) -> Vec<&LayoutSnapshotNode> {
        let mut windows = Vec::new();
        self.collect_windows(&mut windows);
        windows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_take_defaults_to_remaining_keyword() {
        let value = serde_json::to_value(SlotTake::default()).unwrap();

        assert_eq!(value, serde_json::Value::String("remaining".to_owned()));
    }

    #[test]
    fn layout_snapshot_helpers_find_nodes_recursively() {
        let snapshot = LayoutSnapshotNode::Workspace {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            },
            children: vec![LayoutSnapshotNode::Group {
                meta: LayoutNodeMeta {
                    id: Some("stack".into()),
                    ..LayoutNodeMeta::default()
                },
                rect: LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 800.0,
                    height: 600.0,
                },
                children: vec![LayoutSnapshotNode::Window {
                    meta: LayoutNodeMeta {
                        id: Some("main".into()),
                        ..LayoutNodeMeta::default()
                    },
                    rect: LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: 400.0,
                        height: 600.0,
                    },
                    window_id: Some(WindowId::from("w1")),
                }],
            }],
        };

        assert_eq!(
            snapshot
                .find_by_node_id("stack")
                .map(LayoutSnapshotNode::rect),
            Some(LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            })
        );
        assert_eq!(
            snapshot
                .find_by_window_id(&WindowId::from("w1"))
                .map(LayoutSnapshotNode::rect),
            Some(LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 600.0,
            })
        );
        assert_eq!(snapshot.window_nodes().len(), 1);
    }
}
