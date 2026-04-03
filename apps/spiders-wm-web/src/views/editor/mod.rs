use std::collections::{BTreeMap, BTreeSet};

use dioxus::prelude::*;

use crate::bindings::ParsedBindingEntry;
use crate::editor_host::{copy_text_to_clipboard, download_directory, DirectoryDownloadItem};
use crate::session::PreviewSessionState;
use crate::workspace::{
    file_by_id, initial_content, workspace_file_tree, EditorFileId, EditorFileTreeDirectory,
    EditorFileTreeNode, EDITOR_FILES, WORKSPACE_ROOT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CopyFeedback {
    Idle,
    Copied,
    Failed,
}

fn editor_file_badge(language: &str) -> &'static str {
    match language {
        "css" => "css",
        _ if language.contains("react") => "tsx",
        _ => "ts",
    }
}

fn select_editor_file(
    mut open_file_ids: Signal<Vec<EditorFileId>>,
    mut active_file_id: Signal<Option<EditorFileId>>,
    file_id: EditorFileId,
) {
    open_file_ids.with_mut(|open_files| {
        if !open_files.contains(&file_id) {
            open_files.push(file_id);
        }
    });
    active_file_id.set(Some(file_id));
}

fn close_editor_file(
    mut open_file_ids: Signal<Vec<EditorFileId>>,
    mut active_file_id: Signal<Option<EditorFileId>>,
    file_id: EditorFileId,
) {
    open_file_ids.with_mut(|open_files| {
        let Some(index) = open_files
            .iter()
            .position(|open_file_id| *open_file_id == file_id)
        else {
            return;
        };

        open_files.remove(index);

        if active_file_id() == Some(file_id) {
            let next_index = index
                .saturating_sub(1)
                .min(open_files.len().saturating_sub(1));
            if let Some(next_file_id) = open_files.get(next_index).copied() {
                active_file_id.set(Some(next_file_id));
            } else {
                active_file_id.set(None);
            }
        }
    });
}

fn close_other_editor_files(
    mut open_file_ids: Signal<Vec<EditorFileId>>,
    mut active_file_id: Signal<Option<EditorFileId>>,
    file_id: EditorFileId,
) {
    open_file_ids.set(vec![file_id]);
    active_file_id.set(Some(file_id));
}

fn close_all_editor_files(
    mut open_file_ids: Signal<Vec<EditorFileId>>,
    mut active_file_id: Signal<Option<EditorFileId>>,
) {
    open_file_ids.set(Vec::new());
    active_file_id.set(None);
}

fn download_directory_title(directory: &EditorFileTreeDirectory) -> String {
    let Some(root_path) = directory.download_root_path else {
        return "Download directory".to_string();
    };

    let parent_path = root_path
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or_default();

    if parent_path.is_empty() {
        format!(
            "Choose the parent directory so {}/ is created there. If folder picking is unavailable, files will download individually instead.",
            directory.name,
        )
    } else {
        format!(
            "Choose {parent_path}/ so {}/ is created there and its files are copied inside it. If folder picking is unavailable, files will download individually instead.",
            directory.name,
        )
    }
}

fn collect_directory_download_items(
    directory: &EditorFileTreeDirectory,
    buffers: &BTreeMap<EditorFileId, String>,
) -> Vec<DirectoryDownloadItem> {
    let Some(root_path) = directory.download_root_path else {
        return Vec::new();
    };

    let mut items = Vec::new();
    collect_directory_download_items_recursive(directory, root_path, buffers, &mut items);
    items
}

fn collect_directory_download_items_recursive(
    directory: &EditorFileTreeDirectory,
    root_path: &str,
    buffers: &BTreeMap<EditorFileId, String>,
    items: &mut Vec<DirectoryDownloadItem>,
) {
    for child in &directory.children {
        match child {
            EditorFileTreeNode::Directory(child_directory) => {
                collect_directory_download_items_recursive(
                    child_directory,
                    root_path,
                    buffers,
                    items,
                );
            }
            EditorFileTreeNode::File(file_id) => {
                let file = file_by_id(*file_id);
                let relative_path = file
                    .path
                    .strip_prefix(root_path)
                    .unwrap_or(file.path)
                    .trim_start_matches('/');
                let content = buffers
                    .get(file_id)
                    .cloned()
                    .unwrap_or_else(|| initial_content(*file_id).to_string());

                items.push(DirectoryDownloadItem {
                    relative_path: relative_path.to_string(),
                    content,
                });
            }
        }
    }
}

#[component]
fn FileTreeDirectoryView(
    directory: EditorFileTreeDirectory,
    active_file_id: Signal<Option<EditorFileId>>,
    open_file_ids: Signal<Vec<EditorFileId>>,
    editor_buffers: Signal<BTreeMap<EditorFileId, String>>,
    directory_open_state: Signal<BTreeMap<String, bool>>,
    dirty_file_ids: BTreeSet<EditorFileId>,
    is_root: bool,
) -> Element {
    let is_open = if is_root {
        true
    } else {
        directory_open_state()
            .get(directory.path)
            .copied()
            .unwrap_or(directory.default_open)
    };
    let directory_path = directory.path.to_string();
    let directory_name = directory.name.to_string();
    let default_open = directory.default_open;
    let download_title = download_directory_title(&directory);
    let can_download = directory.download_root_path.is_some();
    let download_directory_node = directory.clone();

    rsx! {
        div { class: if is_root { "file-tree-root" } else { "file-tree-directory" },
            if !is_root {
                div { class: "file-tree-directory-row",
                    button {
                        class: "file-tree-directory-toggle",
                        onclick: move |_| {
                            directory_open_state
                                .with_mut(|state| {
                                    let next_value = !state
                                        .get(&directory_path)
                                        .copied()
                                        .unwrap_or(default_open);
                                    state.insert(directory_path.clone(), next_value);
                                });
                        },
                        span { class: "file-tree-chevron",
                            if is_open {
                                "v"
                            } else {
                                ">"
                            }
                        }
                        span { class: "file-tree-directory-name", "{directory_name}" }
                    }

                    if can_download {
                        button {
                            class: "file-tree-directory-download",
                            title: download_title.clone(),
                            onclick: move |event| {
                                event.stop_propagation();
                                let directory = download_directory_node.clone();
                                let items = collect_directory_download_items(&directory, &editor_buffers());

                                if items.is_empty() {
                                    return;
                                }

                                spawn(async move {
                                    let _ = download_directory(directory.name, &items).await;
                                });
                            },
                            "download"
                        }
                    }
                }
            }

            if is_open {
                div { class: if is_root { "file-tree-list" } else { "file-tree-children" },
                    for child in directory.children {
                        FileTreeNodeView {
                            node: child,
                            active_file_id,
                            open_file_ids,
                            editor_buffers,
                            directory_open_state,
                            dirty_file_ids: dirty_file_ids.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FileTreeNodeView(
    node: EditorFileTreeNode,
    active_file_id: Signal<Option<EditorFileId>>,
    open_file_ids: Signal<Vec<EditorFileId>>,
    editor_buffers: Signal<BTreeMap<EditorFileId, String>>,
    directory_open_state: Signal<BTreeMap<String, bool>>,
    dirty_file_ids: BTreeSet<EditorFileId>,
) -> Element {
    match node {
        EditorFileTreeNode::Directory(directory) => rsx! {
            FileTreeDirectoryView {
                directory,
                active_file_id,
                open_file_ids,
                editor_buffers,
                directory_open_state,
                dirty_file_ids,
                is_root: false,
            }
        },
        EditorFileTreeNode::File(file_id) => {
            let file = file_by_id(file_id);
            let is_active = active_file_id() == Some(file_id);
            let is_open = open_file_ids().contains(&file_id);
            let is_dirty = dirty_file_ids.contains(&file_id);
            let badge = editor_file_badge(file.language);
            let path = file.path.to_string();
            let label = file.label.to_string();

            rsx! {
                button {
                    class: if is_active { "file-tree-file is-active" } else { "file-tree-file" },
                    onclick: move |_| select_editor_file(open_file_ids, active_file_id, file_id),
                    span { class: "file-tree-file-badge", "{badge}" }
                    span { class: "file-tree-file-copy",
                        span { class: "file-tree-file-label", "{label}" }
                        span { class: "file-tree-file-path", "{path}" }
                    }
                    if is_open {
                        span { class: "file-tree-file-state", "open" }
                    }
                    if is_dirty {
                        span { class: "file-tree-dirty-dot", "*" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn EditorView(
    session: Signal<PreviewSessionState>,
    editor_buffers: Signal<BTreeMap<EditorFileId, String>>,
    active_file_id: Signal<Option<EditorFileId>>,
    open_file_ids: Signal<Vec<EditorFileId>>,
    directory_open_state: Signal<BTreeMap<String, bool>>,
    binding_entries: Vec<ParsedBindingEntry>,
    on_update_buffer: EventHandler<(EditorFileId, String)>,
) -> Element {
    let mut copy_feedback = use_signal(|| CopyFeedback::Idle);
    let buffers_snapshot = editor_buffers();
    let file_tree = workspace_file_tree();
    let selected_layout_name = session().selected_layout_name();
    let active_file = active_file_id().map(file_by_id);
    let dirty_file_ids = EDITOR_FILES
        .iter()
        .filter(|file| {
            buffers_snapshot
                .get(&file.id)
                .map(String::as_str)
                .unwrap_or_else(|| initial_content(file.id))
                != initial_content(file.id)
        })
        .map(|file| file.id)
        .collect::<BTreeSet<_>>();
    let active_file_path = active_file
        .map(|file| file.path)
        .unwrap_or("no file open")
        .to_string();
    let active_file_language = active_file
        .map(|file| file.language)
        .unwrap_or("none")
        .to_string();
    let active_file_badge = active_file
        .map(|file| editor_file_badge(file.language))
        .unwrap_or("--")
        .to_string();
    let active_file_is_dirty = active_file
        .map(|file| dirty_file_ids.contains(&file.id))
        .unwrap_or(false);
    let active_buffer = active_file.map(|file| {
        buffers_snapshot
            .get(&file.id)
            .cloned()
            .unwrap_or_else(|| initial_content(file.id).to_string())
    });
    let has_active_file = active_file.is_some();
    let active_buffer_value = active_buffer.clone().unwrap_or_default();
    let open_editor_file_ids = open_file_ids();
    let open_editor_file_count = open_editor_file_ids.len();

    let open_editor_tabs = open_editor_file_ids.iter().copied().map(|file_id| {
        let file = file_by_id(file_id);
        let is_active = active_file_id() == Some(file_id);
        let is_dirty = dirty_file_ids.contains(&file_id);
        let badge = editor_file_badge(file.language);
        let label = file.label.to_string();

        rsx! {
            div {
                class: if is_active { "editor-tab is-active" } else { "editor-tab" },
                onclick: move |_| select_editor_file(open_file_ids, active_file_id, file_id),
                span { class: "editor-tab-badge", "{badge}" }
                span { class: "editor-tab-label", "{label}" }
                if is_dirty {
                    span { class: "editor-tab-dirty", "*" }
                }
                button {
                    class: "editor-tab-close",
                    onclick: move |event| {
                        event.stop_propagation();
                        close_editor_file(open_file_ids, active_file_id, file_id);
                    },
                    "x"
                }
            }
        }
    });

    rsx! {
        section { class: "studio-grid",
            article { class: "canvas-card",
                div { class: "canvas-header",
                    div {
                        p { class: "eyebrow", "Workspace source" }
                        h2 { "{active_file_path}" }
                    }
                    p { class: "scenario-summary",
                        "These buffers mirror the playground workspace bundle. Config, bindings, and CSS edits apply at runtime; TSX buffers are preserved in state for the Monaco/runtime-eval step."
                    }
                }

                div { class: "editor-workbench",
                    aside { class: "editor-sidebar",
                        p { class: "eyebrow", "workspace://files" }
                        h3 { class: "editor-sidebar-title", "{WORKSPACE_ROOT}" }
                        FileTreeDirectoryView {
                            directory: file_tree,
                            active_file_id,
                            open_file_ids,
                            editor_buffers,
                            directory_open_state,
                            dirty_file_ids: dirty_file_ids.clone(),
                            is_root: true,
                        }
                    }

                    div { class: "editor-main",
                        div { class: "editor-tabbar",
                            if open_editor_file_count == 0 {
                                span { class: "editor-tabbar-empty", "no files open" }
                            } else {
                                {open_editor_tabs}
                            }

                            div { class: "editor-tabbar-actions",
                                button {
                                    class: "editor-action-button",
                                    disabled: !has_active_file,
                                    onclick: move |_| {
                                        if active_file_id().is_none() {
                                            return;
                                        }

                                        let contents = active_buffer.clone().unwrap_or_default();
                                        copy_feedback.set(CopyFeedback::Idle);

                                        spawn(async move {
                                            let feedback = match copy_text_to_clipboard(&contents).await {
                                                Ok(()) => CopyFeedback::Copied,
                                                Err(_) => CopyFeedback::Failed,
                                            };
                                            copy_feedback.set(feedback);
                                        });
                                    },
                                    if copy_feedback() == CopyFeedback::Copied {
                                        "copied"
                                    } else if copy_feedback() == CopyFeedback::Failed {
                                        "copy failed"
                                    } else {
                                        "copy"
                                    }
                                }

                                button {
                                    class: "editor-action-button",
                                    disabled: active_file_id().is_none() || open_editor_file_count <= 1,
                                    onclick: move |_| {
                                        let Some(file_id) = active_file_id() else {
                                            return;
                                        };

                                        close_other_editor_files(open_file_ids, active_file_id, file_id);
                                        copy_feedback.set(CopyFeedback::Idle);
                                    },
                                    "close others"
                                }

                                button {
                                    class: "editor-action-button",
                                    disabled: open_editor_file_count == 0,
                                    onclick: move |_| {
                                        close_all_editor_files(open_file_ids, active_file_id);
                                        copy_feedback.set(CopyFeedback::Idle);
                                    },
                                    "close all"
                                }
                            }
                        }

                        div { class: "editor-statusbar",
                            span { class: "editor-status-pill", "{active_file_badge}" }
                            span { class: "editor-status-path", "{active_file_path}" }
                            span { class: "editor-status-language", "{active_file_language}" }
                            if active_file_is_dirty {
                                span { class: "editor-status-dirty", "modified" }
                            }
                        }

                        if has_active_file {
                            textarea {
                                class: "editor-textarea",
                                value: active_buffer_value,
                                spellcheck: false,
                                oninput: move |event| {
                                    let Some(current_file_id) = active_file_id() else {
                                        return;
                                    };

                                    on_update_buffer.call((current_file_id, event.value()));
                                },
                            }
                        } else {
                            div { class: "editor-empty-state", "no file open" }
                        }
                    }
                }
            }

            article { class: "inspector-card",
                div { class: "inspector-panel",
                    p { class: "eyebrow", "File metadata" }
                    ul { class: "memory-list",
                        li {
                            span { class: "memory-scope", "language" }
                            span { class: "memory-window", "{active_file_language}" }
                        }
                        li {
                            span { class: "memory-scope", "workspace root" }
                            span { class: "memory-window", "{WORKSPACE_ROOT}" }
                        }
                        li {
                            span { class: "memory-scope", "preview layout" }
                            span { class: "memory-window", "{selected_layout_name}" }
                        }
                    }
                }

                div { class: "inspector-panel",
                    p { class: "eyebrow", "Bindings" }
                    ul { class: "memory-list",
                        for entry in binding_entries.iter().cloned() {
                            li {
                                span { class: "memory-scope", "{entry.chord}" }
                                span { class: "memory-window", "{entry.command_label}" }
                            }
                        }
                    }
                }

                div { class: "inspector-panel",
                    p { class: "eyebrow", "Runtime notes" }
                    ul { class: "memory-list",
                        li {
                            span { class: "memory-scope", "Applied live" }
                            span { class: "memory-window",
                                "config.ts, bindings.ts, root css, layout css, active layout tsx"
                            }
                        }
                        li {
                            span { class: "memory-scope", "Buffered next" }
                            span { class: "memory-window",
                                "additional imported source files are the next gap once the workspace bundle expands beyond the current playground set"
                            }
                        }
                    }
                }
            }
        }
    }
}
