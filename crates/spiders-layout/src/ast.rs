use std::collections::{BTreeMap, BTreeSet};

use spiders_shared::layout::{
    LayoutNodeType, ResolvedLayoutNode, SlotTake, SourceLayoutNode, WindowMatch,
};
use spiders_shared::wm::WindowSnapshot;
use thiserror::Error;

use crate::matching::{matches_window, parse_window_match, MatchParseError};

#[derive(Debug, Clone)]
pub struct ValidatedLayoutTree {
    pub root: SourceLayoutNode,
}

#[derive(Debug, Clone)]
pub struct ResolvedLayoutTree {
    pub root: ResolvedLayoutNode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthoredNodeMeta {
    pub id: Option<String>,
    pub class: Vec<String>,
    pub name: Option<String>,
    pub data: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthoredLayoutNode {
    Workspace {
        meta: AuthoredNodeMeta,
        children: Vec<AuthoredLayoutNode>,
    },
    Group {
        meta: AuthoredNodeMeta,
        children: Vec<AuthoredLayoutNode>,
    },
    Window {
        meta: AuthoredNodeMeta,
        match_expr: Option<String>,
    },
    Slot {
        meta: AuthoredNodeMeta,
        match_expr: Option<String>,
        take: SlotTake,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LayoutResolveError {
    #[error("layout must be validated before resolution")]
    InvalidRoot,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LayoutValidationError {
    #[error("layout root must be a workspace node")]
    RootMustBeWorkspace,
    #[error("node id `{id}` is duplicated")]
    DuplicateId { id: String },
    #[error("node type `{child:?}` is not allowed under `{parent:?}`")]
    InvalidChild {
        parent: LayoutNodeType,
        child: LayoutNodeType,
    },
    #[error("slot `take` must be a positive integer or `remaining`")]
    InvalidSlotTake,
    #[error("`match` must contain at least one clause when provided")]
    EmptyMatch,
    #[error("failed to parse `match`: {source}")]
    InvalidMatch {
        #[from]
        source: MatchParseError,
    },
}

impl ValidatedLayoutTree {
    pub fn from_authored(root: AuthoredLayoutNode) -> Result<Self, LayoutValidationError> {
        Self::new(normalize_authored_node(root)?)
    }

    pub fn new(root: SourceLayoutNode) -> Result<Self, LayoutValidationError> {
        if !matches!(root, SourceLayoutNode::Workspace { .. }) {
            return Err(LayoutValidationError::RootMustBeWorkspace);
        }

        let mut ids = BTreeSet::new();
        validate_node(&root, None, &mut ids)?;

        Ok(Self { root })
    }

    pub fn resolve(
        &self,
        windows: &[WindowSnapshot],
    ) -> Result<ResolvedLayoutTree, LayoutResolveError> {
        let SourceLayoutNode::Workspace { meta, children } = &self.root else {
            return Err(LayoutResolveError::InvalidRoot);
        };

        let mut claimed = BTreeSet::new();
        let resolved_children = children
            .iter()
            .flat_map(|child| resolve_node(child, windows, &mut claimed))
            .collect();

        Ok(ResolvedLayoutTree {
            root: ResolvedLayoutNode::Workspace {
                meta: meta.clone(),
                children: resolved_children,
            },
        })
    }
}

fn normalize_authored_node(
    node: AuthoredLayoutNode,
) -> Result<SourceLayoutNode, LayoutValidationError> {
    Ok(match node {
        AuthoredLayoutNode::Workspace { meta, children } => SourceLayoutNode::Workspace {
            meta: normalize_meta(meta),
            children: children
                .into_iter()
                .map(normalize_authored_node)
                .collect::<Result<Vec<_>, _>>()?,
        },
        AuthoredLayoutNode::Group { meta, children } => SourceLayoutNode::Group {
            meta: normalize_meta(meta),
            children: children
                .into_iter()
                .map(normalize_authored_node)
                .collect::<Result<Vec<_>, _>>()?,
        },
        AuthoredLayoutNode::Window { meta, match_expr } => SourceLayoutNode::Window {
            meta: normalize_meta(meta),
            window_match: normalize_match(match_expr)?,
        },
        AuthoredLayoutNode::Slot {
            meta,
            match_expr,
            take,
        } => SourceLayoutNode::Slot {
            meta: normalize_meta(meta),
            window_match: normalize_match(match_expr)?,
            take,
        },
    })
}

fn normalize_meta(meta: AuthoredNodeMeta) -> spiders_shared::layout::LayoutNodeMeta {
    spiders_shared::layout::LayoutNodeMeta {
        id: meta.id,
        class: meta.class,
        name: meta.name,
        data: meta.data,
    }
}

fn normalize_match(
    match_expr: Option<String>,
) -> Result<Option<WindowMatch>, LayoutValidationError> {
    match match_expr {
        Some(match_expr) => Ok(Some(parse_window_match(&match_expr)?)),
        None => Ok(None),
    }
}

fn resolved_window_meta(
    meta: &spiders_shared::layout::LayoutNodeMeta,
    window: Option<&WindowSnapshot>,
) -> spiders_shared::layout::LayoutNodeMeta {
    let mut resolved = meta.clone();

    let Some(window) = window else {
        return resolved;
    };

    let mut insert = |key: &str, value: Option<&str>| {
        if let Some(value) = value {
            resolved.data.insert(key.to_owned(), value.to_owned());
        }
    };

    insert("app_id", window.app_id.as_deref());
    insert("title", window.title.as_deref());
    insert("class", window.class.as_deref());
    insert("instance", window.instance.as_deref());
    insert("role", window.role.as_deref());
    insert("window_type", window.window_type.as_deref());
    resolved.data.insert(
        "shell".to_owned(),
        match window.shell {
            spiders_shared::wm::ShellKind::XdgToplevel => "xdg_toplevel",
            spiders_shared::wm::ShellKind::X11 => "x11",
            spiders_shared::wm::ShellKind::Unknown => "unknown",
        }
        .to_owned(),
    );

    resolved
}

fn resolve_node(
    node: &SourceLayoutNode,
    windows: &[WindowSnapshot],
    claimed: &mut BTreeSet<String>,
) -> Vec<ResolvedLayoutNode> {
    match node {
        SourceLayoutNode::Workspace { meta, children } => vec![ResolvedLayoutNode::Workspace {
            meta: meta.clone(),
            children: children
                .iter()
                .flat_map(|child| resolve_node(child, windows, claimed))
                .collect(),
        }],
        SourceLayoutNode::Group { meta, children } => vec![ResolvedLayoutNode::Group {
            meta: meta.clone(),
            children: children
                .iter()
                .flat_map(|child| resolve_node(child, windows, claimed))
                .collect(),
        }],
        SourceLayoutNode::Window { meta, window_match } => {
            let claimed_window = windows
                .iter()
                .find(|window| can_claim_window(window_match.as_ref(), window, claimed))
                .inspect(|window| {
                    claimed.insert(window.id.to_string());
                });

            vec![ResolvedLayoutNode::Window {
                meta: resolved_window_meta(meta, claimed_window),
                window_id: claimed_window.map(|window| window.id.clone()),
            }]
        }
        SourceLayoutNode::Slot {
            meta,
            window_match,
            take,
        } => {
            let matching_ids: Vec<_> = windows
                .iter()
                .filter(|window| can_claim_window(window_match.as_ref(), window, claimed))
                .map(|window| window.id.clone())
                .collect();

            let limit = match take {
                SlotTake::Count(count) => *count as usize,
                SlotTake::Remaining(_) => matching_ids.len(),
            };

            matching_ids
                .into_iter()
                .take(limit)
                .map(|window_id| {
                    let window = windows.iter().find(|window| window.id == window_id);
                    claimed.insert(window_id.to_string());

                    ResolvedLayoutNode::Window {
                        meta: resolved_window_meta(meta, window),
                        window_id: Some(window_id),
                    }
                })
                .collect()
        }
    }
}

fn can_claim_window(
    window_match: Option<&WindowMatch>,
    window: &WindowSnapshot,
    claimed: &BTreeSet<String>,
) -> bool {
    !claimed.contains(window.id.as_str())
        && window.mapped
        && window_match.is_none_or(|window_match| matches_window(window_match, window))
}

fn validate_node(
    node: &SourceLayoutNode,
    parent: Option<LayoutNodeType>,
    ids: &mut BTreeSet<String>,
) -> Result<(), LayoutValidationError> {
    let node_type = node.node_type();

    if let Some(parent) = parent {
        let child_allowed = matches!(
            (parent, node_type),
            (LayoutNodeType::Workspace, LayoutNodeType::Group)
                | (LayoutNodeType::Workspace, LayoutNodeType::Window)
                | (LayoutNodeType::Workspace, LayoutNodeType::Slot)
                | (LayoutNodeType::Group, LayoutNodeType::Group)
                | (LayoutNodeType::Group, LayoutNodeType::Window)
                | (LayoutNodeType::Group, LayoutNodeType::Slot)
        );

        if !child_allowed {
            return Err(LayoutValidationError::InvalidChild {
                parent,
                child: node_type,
            });
        }
    }

    if let Some(id) = &node.meta().id {
        if !ids.insert(id.clone()) {
            return Err(LayoutValidationError::DuplicateId { id: id.clone() });
        }
    }

    match node {
        SourceLayoutNode::Window { window_match, .. } => validate_match(window_match.as_ref())?,
        SourceLayoutNode::Slot {
            window_match, take, ..
        } => {
            validate_match(window_match.as_ref())?;

            if matches!(take, SlotTake::Count(0)) {
                return Err(LayoutValidationError::InvalidSlotTake);
            }
        }
        SourceLayoutNode::Workspace { children, .. } | SourceLayoutNode::Group { children, .. } => {
            for child in children {
                validate_node(child, Some(node_type), ids)?;
            }
        }
    }

    Ok(())
}

fn validate_match(window_match: Option<&WindowMatch>) -> Result<(), LayoutValidationError> {
    if matches!(window_match, Some(WindowMatch { clauses }) if clauses.is_empty()) {
        return Err(LayoutValidationError::EmptyMatch);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::WindowId;
    use spiders_shared::layout::{LayoutNodeMeta, MatchClause, MatchKey};
    use spiders_shared::wm::ShellKind;

    use super::*;

    fn window(id: &str, app_id: &str, title: &str) -> WindowSnapshot {
        WindowSnapshot {
            id: WindowId::from(id),
            shell: ShellKind::XdgToplevel,
            app_id: Some(app_id.into()),
            title: Some(title.into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            floating: false,
            floating_rect: None,
            fullscreen: false,
            focused: false,
            urgent: false,
            output_id: None,
            workspace_id: None,
            tags: vec![],
        }
    }

    #[test]
    fn rejects_non_workspace_root() {
        let tree = SourceLayoutNode::Group {
            meta: LayoutNodeMeta::default(),
            children: vec![],
        };

        let error = ValidatedLayoutTree::new(tree).unwrap_err();

        assert_eq!(error, LayoutValidationError::RootMustBeWorkspace);
    }

    #[test]
    fn rejects_duplicate_ids() {
        let tree = SourceLayoutNode::Workspace {
            meta: LayoutNodeMeta {
                id: Some("root".into()),
                ..LayoutNodeMeta::default()
            },
            children: vec![
                SourceLayoutNode::Group {
                    meta: LayoutNodeMeta {
                        id: Some("dup".into()),
                        ..LayoutNodeMeta::default()
                    },
                    children: vec![],
                },
                SourceLayoutNode::Window {
                    meta: LayoutNodeMeta {
                        id: Some("dup".into()),
                        ..LayoutNodeMeta::default()
                    },
                    window_match: None,
                },
            ],
        };

        let error = ValidatedLayoutTree::new(tree).unwrap_err();

        assert_eq!(
            error,
            LayoutValidationError::DuplicateId { id: "dup".into() }
        );
    }

    #[test]
    fn rejects_nested_workspace_nodes() {
        let tree = SourceLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![SourceLayoutNode::Workspace {
                meta: LayoutNodeMeta::default(),
                children: vec![],
            }],
        };

        let error = ValidatedLayoutTree::new(tree).unwrap_err();

        assert_eq!(
            error,
            LayoutValidationError::InvalidChild {
                parent: LayoutNodeType::Workspace,
                child: LayoutNodeType::Workspace,
            }
        );
    }

    #[test]
    fn rejects_zero_slot_take() {
        let tree = SourceLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![SourceLayoutNode::Slot {
                meta: LayoutNodeMeta::default(),
                window_match: None,
                take: SlotTake::Count(0),
            }],
        };

        let error = ValidatedLayoutTree::new(tree).unwrap_err();

        assert_eq!(error, LayoutValidationError::InvalidSlotTake);
    }

    #[test]
    fn accepts_non_empty_match_clauses() {
        let tree = SourceLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![SourceLayoutNode::Window {
                meta: LayoutNodeMeta::default(),
                window_match: Some(WindowMatch {
                    clauses: vec![MatchClause {
                        key: MatchKey::AppId,
                        value: "firefox".into(),
                    }],
                }),
            }],
        };

        let validated = ValidatedLayoutTree::new(tree);

        assert!(validated.is_ok());
    }

    #[test]
    fn normalizes_authored_match_expression_before_validation() {
        let tree = ValidatedLayoutTree::from_authored(AuthoredLayoutNode::Workspace {
            meta: AuthoredNodeMeta::default(),
            children: vec![AuthoredLayoutNode::Window {
                meta: AuthoredNodeMeta::default(),
                match_expr: Some("app_id=\"firefox\" title=\"Mozilla Firefox\"".into()),
            }],
        })
        .unwrap();

        assert_eq!(
            tree.root,
            SourceLayoutNode::Workspace {
                meta: LayoutNodeMeta::default(),
                children: vec![SourceLayoutNode::Window {
                    meta: LayoutNodeMeta::default(),
                    window_match: Some(WindowMatch {
                        clauses: vec![
                            MatchClause {
                                key: MatchKey::AppId,
                                value: "firefox".into(),
                            },
                            MatchClause {
                                key: MatchKey::Title,
                                value: "Mozilla Firefox".into(),
                            },
                        ],
                    }),
                }],
            }
        );
    }

    #[test]
    fn authored_invalid_match_bubbles_up_as_validation_error() {
        let error = ValidatedLayoutTree::from_authored(AuthoredLayoutNode::Workspace {
            meta: AuthoredNodeMeta::default(),
            children: vec![AuthoredLayoutNode::Window {
                meta: AuthoredNodeMeta::default(),
                match_expr: Some("app_id=firefox".into()),
            }],
        })
        .unwrap_err();

        assert_eq!(
            error,
            LayoutValidationError::InvalidMatch {
                source: MatchParseError::ExpectedQuotedValue {
                    key: "app_id".into(),
                },
            }
        );
    }

    #[test]
    fn resolve_keeps_unmatched_window_node_as_empty_runtime_leaf() {
        let tree = ValidatedLayoutTree::new(SourceLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![SourceLayoutNode::Window {
                meta: LayoutNodeMeta {
                    id: Some("main".into()),
                    ..LayoutNodeMeta::default()
                },
                window_match: Some(WindowMatch {
                    clauses: vec![MatchClause {
                        key: MatchKey::AppId,
                        value: "firefox".into(),
                    }],
                }),
            }],
        })
        .unwrap();

        let resolved = tree
            .resolve(&[window("w1", "alacritty", "Terminal")])
            .unwrap();

        assert_eq!(
            resolved.root,
            ResolvedLayoutNode::Workspace {
                meta: LayoutNodeMeta::default(),
                children: vec![ResolvedLayoutNode::Window {
                    meta: LayoutNodeMeta {
                        id: Some("main".into()),
                        ..LayoutNodeMeta::default()
                    },
                    window_id: None,
                }],
            }
        );
    }

    #[test]
    fn resolve_slot_expands_into_multiple_runtime_windows_in_order() {
        let tree = ValidatedLayoutTree::new(SourceLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![SourceLayoutNode::Slot {
                meta: LayoutNodeMeta {
                    class: vec!["stack".into()],
                    ..LayoutNodeMeta::default()
                },
                window_match: Some(WindowMatch {
                    clauses: vec![MatchClause {
                        key: MatchKey::AppId,
                        value: "firefox".into(),
                    }],
                }),
                take: SlotTake::Count(2),
            }],
        })
        .unwrap();

        let resolved = tree
            .resolve(&[
                window("w1", "firefox", "one"),
                window("w2", "firefox", "two"),
                window("w3", "firefox", "three"),
            ])
            .unwrap();

        assert_eq!(
            resolved.root,
            ResolvedLayoutNode::Workspace {
                meta: LayoutNodeMeta::default(),
                children: vec![
                    ResolvedLayoutNode::Window {
                        meta: LayoutNodeMeta {
                            class: vec!["stack".into()],
                            data: [
                                ("app_id".into(), "firefox".into()),
                                ("shell".into(), "xdg_toplevel".into()),
                                ("title".into(), "one".into()),
                            ]
                            .into_iter()
                            .collect(),
                            ..LayoutNodeMeta::default()
                        },
                        window_id: Some(WindowId::from("w1")),
                    },
                    ResolvedLayoutNode::Window {
                        meta: LayoutNodeMeta {
                            class: vec!["stack".into()],
                            data: [
                                ("app_id".into(), "firefox".into()),
                                ("shell".into(), "xdg_toplevel".into()),
                                ("title".into(), "two".into()),
                            ]
                            .into_iter()
                            .collect(),
                            ..LayoutNodeMeta::default()
                        },
                        window_id: Some(WindowId::from("w2")),
                    },
                ],
            }
        );
    }

    #[test]
    fn resolve_later_nodes_only_see_unclaimed_windows() {
        let tree = ValidatedLayoutTree::new(SourceLayoutNode::Workspace {
            meta: LayoutNodeMeta::default(),
            children: vec![
                SourceLayoutNode::Window {
                    meta: LayoutNodeMeta {
                        id: Some("primary".into()),
                        ..LayoutNodeMeta::default()
                    },
                    window_match: Some(WindowMatch {
                        clauses: vec![MatchClause {
                            key: MatchKey::AppId,
                            value: "firefox".into(),
                        }],
                    }),
                },
                SourceLayoutNode::Slot {
                    meta: LayoutNodeMeta {
                        id: Some("rest".into()),
                        ..LayoutNodeMeta::default()
                    },
                    window_match: Some(WindowMatch {
                        clauses: vec![MatchClause {
                            key: MatchKey::AppId,
                            value: "firefox".into(),
                        }],
                    }),
                    take: SlotTake::Remaining(spiders_shared::layout::RemainingTake::Remaining),
                },
            ],
        })
        .unwrap();

        let resolved = tree
            .resolve(&[
                window("w1", "firefox", "one"),
                window("w2", "firefox", "two"),
                window("w3", "alacritty", "three"),
            ])
            .unwrap();

        assert_eq!(
            resolved.root,
            ResolvedLayoutNode::Workspace {
                meta: LayoutNodeMeta::default(),
                children: vec![
                    ResolvedLayoutNode::Window {
                        meta: LayoutNodeMeta {
                            id: Some("primary".into()),
                            data: [
                                ("app_id".into(), "firefox".into()),
                                ("shell".into(), "xdg_toplevel".into()),
                                ("title".into(), "one".into()),
                            ]
                            .into_iter()
                            .collect(),
                            ..LayoutNodeMeta::default()
                        },
                        window_id: Some(WindowId::from("w1")),
                    },
                    ResolvedLayoutNode::Window {
                        meta: LayoutNodeMeta {
                            id: Some("rest".into()),
                            data: [
                                ("app_id".into(), "firefox".into()),
                                ("shell".into(), "xdg_toplevel".into()),
                                ("title".into(), "two".into()),
                            ]
                            .into_iter()
                            .collect(),
                            ..LayoutNodeMeta::default()
                        },
                        window_id: Some(WindowId::from("w2")),
                    },
                ],
            }
        );
    }
}
