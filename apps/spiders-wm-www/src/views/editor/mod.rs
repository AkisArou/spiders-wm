use std::collections::BTreeMap;

use clsx::clsx;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::app_state::AppState;
use crate::editor_host::{DirectoryDownloadItem, copy_text_to_clipboard, download_directory};
use crate::workspace::{
    EditorFile, EditorFileId, EditorFileTreeDirectory, EditorFileTreeNode, WORKSPACE_ROOT,
    file_by_id, initial_content, workspace_file_tree,
};

const EYEBROW_CLASS: &str = "text-[0.72rem] uppercase tracking-[0.18em] text-sky-200/70";
const CARD_CLASS: &str = "grid gap-4 rounded-[28px] border border-white/10 bg-[linear-gradient(180deg,rgba(11,24,37,0.92),rgba(7,15,23,0.92))] p-4 shadow-[0_28px_80px_rgba(0,0,0,0.34)] backdrop-blur-[18px] sm:p-5";
const PANEL_CLASS: &str = "rounded-[22px] border border-white/10 bg-white/[0.03] p-4";
const MEMORY_LIST_CLASS: &str = "grid gap-2 text-sm leading-6 text-slate-200";
const MEMORY_ITEM_CLASS: &str = "grid gap-1 border-b border-white/6 pb-2 last:border-b-0 last:pb-0";
const MEMORY_SCOPE_CLASS: &str = "text-[0.72rem] uppercase tracking-[0.16em] text-sky-200/60";
const MEMORY_VALUE_CLASS: &str = "text-sm text-slate-200";
const ACTION_BUTTON_CLASS: &str = "rounded-full border border-white/10 bg-white/[0.04] px-3 py-2 text-[0.72rem] uppercase tracking-[0.14em] text-slate-200 transition duration-150 hover:border-sky-300/30 hover:bg-white/[0.08] hover:text-white disabled:cursor-not-allowed disabled:border-white/5 disabled:bg-white/[0.02] disabled:text-slate-500";

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

fn is_file_dirty(buffers: &BTreeMap<EditorFileId, String>, file_id: EditorFileId) -> bool {
    buffers
        .get(&file_id)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(file_id))
        != initial_content(file_id)
}

fn active_file(app_state: AppState) -> Option<&'static EditorFile> {
    app_state.active_file_id.get().map(file_by_id)
}

fn active_file_path(app_state: AppState) -> String {
    active_file(app_state)
        .map(|file| file.path.to_string())
        .unwrap_or_else(|| "no file open".to_string())
}

fn active_file_language(app_state: AppState) -> String {
    active_file(app_state)
        .map(|file| file.language.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn active_file_badge(app_state: AppState) -> String {
    active_file(app_state)
        .map(|file| editor_file_badge(file.language).to_string())
        .unwrap_or_else(|| "--".to_string())
}

fn active_file_is_dirty(app_state: AppState) -> bool {
    let Some(file) = active_file(app_state) else {
        return false;
    };

    let buffers = app_state.editor_buffers.get();
    is_file_dirty(&buffers, file.id)
}

fn active_buffer_text(app_state: AppState) -> String {
    let Some(file) = active_file(app_state) else {
        return String::new();
    };

    app_state
        .editor_buffers
        .get()
        .get(&file.id)
        .cloned()
        .unwrap_or_else(|| initial_content(file.id).to_string())
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

fn is_directory_open(
    app_state: AppState,
    path: &'static str,
    default_open: bool,
    is_root: bool,
) -> bool {
    if is_root {
        true
    } else {
        app_state
            .directory_open_state
            .get()
            .get(path)
            .copied()
            .unwrap_or(default_open)
    }
}

#[component]
fn FileTreeDirectoryView(
    directory: EditorFileTreeDirectory,
    #[prop(optional)] is_root: bool,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let directory_path = directory.path.to_string();
    let directory_name = directory.name.to_string();
    let default_open = directory.default_open;
    let download_title = download_directory_title(&directory);
    let can_download = directory.download_root_path.is_some();
    let download_directory_node = directory.clone();
    let child_nodes = directory.children.clone();

    view! {
        <div class=if is_root { "grid gap-3" } else { "grid gap-2" }>
            {(!is_root).then(|| {
                let directory_name_text = directory_name.clone();

                view! {
                    <div class="flex items-center gap-2">
                        <button
                            class="flex flex-1 items-center gap-2 rounded-[14px] border border-white/8 bg-white/[0.03] px-3 py-2 text-left text-sm text-slate-200 transition duration-150 hover:border-white/12 hover:bg-white/[0.05]"
                            on:click=move |_| app_state.toggle_directory(directory_path.clone(), default_open)
                        >
                            <span class="w-3 text-[0.72rem] uppercase tracking-[0.14em] text-sky-200/60">
                                {move || if is_directory_open(app_state, directory.path, default_open, is_root) { "v" } else { ">" }}
                            </span>
                            <span class="truncate text-sm font-medium text-white">{directory_name_text}</span>
                        </button>

                        {can_download.then(|| {
                            let directory_for_download = download_directory_node.clone();
                            let directory_label = directory_for_download.name;

                            view! {
                                <button
                                    class="rounded-full border border-white/10 bg-white/[0.03] px-3 py-2 text-[0.68rem] uppercase tracking-[0.16em] text-sky-100/80 transition duration-150 hover:border-sky-300/30 hover:bg-sky-300/[0.08] hover:text-white"
                                    title=download_title.clone()
                                    on:click=move |event| {
                                        event.stop_propagation();

                                        let items = collect_directory_download_items(
                                            &directory_for_download,
                                            &app_state.editor_buffers.get_untracked(),
                                        );
                                        if items.is_empty() {
                                            return;
                                        }

                                        spawn_local(async move {
                                            let _ = download_directory(directory_label, &items).await;
                                        });
                                    }
                                >
                                    "download"
                                </button>
                            }
                        })}
                    </div>
                }
            })}

            <Show when=move || is_directory_open(app_state, directory.path, default_open, is_root)>
                <div class=if is_root {
                    "grid gap-2"
                } else {
                    "ml-4 grid gap-2 border-l border-white/8 pl-3"
                }>
                    {child_nodes
                        .clone()
                        .into_iter()
                        .map(|child| view! { <FileTreeNodeView node=child/> })
                        .collect_view()}
                </div>
            </Show>
        </div>
    }
}

#[component]
fn FileTreeNodeView(node: EditorFileTreeNode) -> impl IntoView {
    let app_state = expect_context::<AppState>();

    view! {
        {match node {
            EditorFileTreeNode::Directory(directory) => {
                view! { <FileTreeDirectoryView directory=directory/> }.into_any()
            }
            EditorFileTreeNode::File(file_id) => {
                let file = file_by_id(file_id);
                let badge = editor_file_badge(file.language).to_string();
                let label = file.label.to_string();
                let path = file.path.to_string();

                view! {
                    <button
                        class=move || {
                            clsx!(
                                "group flex w-full items-start gap-3 rounded-[16px] border px-3 py-2 text-left transition duration-150",
                                (
                                    app_state.active_file_id.get() == Some(file_id),
                                    "border-sky-300/40 bg-sky-300/[0.09] text-white shadow-[0_10px_30px_rgba(2,132,199,0.12)]"
                                ),
                                (
                                    app_state.active_file_id.get() != Some(file_id),
                                    "border-white/5 bg-white/[0.02] text-slate-300 hover:border-white/12 hover:bg-white/[0.05]"
                                )
                            )
                        }
                        on:click=move |_| app_state.select_editor_file(file_id)
                    >
                        <span class="inline-flex min-w-[2.5rem] items-center justify-center rounded-full bg-white/[0.08] px-2 py-1 text-[0.68rem] uppercase tracking-[0.14em] text-sky-100/80">
                            {badge}
                        </span>
                        <span class="grid min-w-0 flex-1 gap-1">
                            <span class="truncate text-sm font-medium text-white">{label}</span>
                            <span class="truncate text-[0.76rem] text-slate-400">{path}</span>
                        </span>

                        <Show when=move || app_state.open_file_ids.get().contains(&file_id)>
                            <span class="rounded-full border border-emerald-400/20 bg-emerald-400/10 px-2 py-1 text-[0.62rem] uppercase tracking-[0.12em] text-emerald-200">
                                "open"
                            </span>
                        </Show>

                        <Show when=move || is_file_dirty(&app_state.editor_buffers.get(), file_id)>
                            <span class="text-sm font-semibold text-amber-200">"*"</span>
                        </Show>
                    </button>
                }
                    .into_any()
            }
        }}
    }
}

#[component]
pub fn EditorView() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let file_tree = workspace_file_tree();
    let copy_feedback = RwSignal::new(CopyFeedback::Idle);

    view! {
        <section class="grid gap-4 xl:grid-cols-[minmax(0,1.6fr)_20rem]">
            <article class=CARD_CLASS>
                <div class="grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(16rem,24rem)] lg:items-end">
                    <div>
                        <p class=EYEBROW_CLASS>"Workspace source"</p>
                        <h2 class="mt-2 text-[1.5rem] font-semibold tracking-[-0.03em] text-white">
                            {move || active_file_path(app_state)}
                        </h2>
                    </div>
                    <p class="max-w-[34rem] text-left text-sm leading-6 text-slate-300 lg:justify-self-end lg:text-right">
                        "These buffers mirror the playground workspace bundle. Config, bindings, and CSS edits apply at runtime; TSX buffers are preserved in state for the Monaco/runtime-eval step."
                    </p>
                </div>

                <div class="grid min-h-[640px] gap-4 lg:grid-cols-[minmax(240px,280px)_minmax(0,1fr)]">
                    <aside class="order-2 grid content-start gap-3 rounded-[22px] border border-white/10 bg-[radial-gradient(circle_at_top,rgba(125,211,252,0.05),transparent_50%),linear-gradient(180deg,rgba(5,10,15,0.88),rgba(9,17,26,0.96))] p-4 lg:order-1">
                        <p class=EYEBROW_CLASS>"workspace://files"</p>
                        <h3 class="text-lg font-semibold tracking-[-0.03em] text-white">{WORKSPACE_ROOT}</h3>
                        <FileTreeDirectoryView directory=file_tree is_root=true/>
                    </aside>

                    <div class="order-1 flex min-h-[640px] flex-col overflow-hidden rounded-[22px] border border-white/10 bg-[linear-gradient(180deg,rgba(4,8,12,0.96),rgba(9,17,26,0.98))] lg:order-2">
                        <div class="flex flex-wrap items-start justify-between gap-3 border-b border-white/8 px-3 py-3">
                            <div class="flex min-w-0 flex-1 gap-2 overflow-x-auto pb-1">
                            <Show
                                when=move || !app_state.open_file_ids.get().is_empty()
                                fallback=move || {
                                    view! {
                                        <span class="inline-flex items-center px-2 py-2 text-sm text-slate-500">
                                            "no files open"
                                        </span>
                                    }
                                }
                            >
                                <>
                                    {move || {
                                        app_state
                                            .open_file_ids
                                            .get()
                                            .into_iter()
                                            .map(|file_id| {
                                                let file = file_by_id(file_id);
                                                let badge = editor_file_badge(file.language).to_string();
                                                let label = file.label.to_string();

                                                view! {
                                                    <div
                                                        class=move || {
                                                            clsx!(
                                                                "inline-flex min-w-0 cursor-pointer items-center gap-2 rounded-[16px] border px-3 py-2 text-sm transition duration-150",
                                                                (
                                                                    app_state.active_file_id.get() == Some(file_id),
                                                                    "border-sky-300/40 bg-sky-300/[0.09] text-white"
                                                                ),
                                                                (
                                                                    app_state.active_file_id.get() != Some(file_id),
                                                                    "border-white/10 bg-white/[0.035] text-slate-300 hover:border-white/16 hover:bg-white/[0.07] hover:text-white"
                                                                )
                                                            )
                                                        }
                                                        on:click=move |_| app_state.select_editor_file(file_id)
                                                    >
                                                        <span class="inline-flex min-w-[2.35rem] items-center justify-center rounded-full bg-white/[0.08] px-2 py-1 text-[0.68rem] uppercase tracking-[0.12em] text-sky-100/80">
                                                            {badge}
                                                        </span>
                                                        <span class="truncate">{label}</span>

                                                        <Show when=move || is_file_dirty(&app_state.editor_buffers.get(), file_id)>
                                                            <span class="text-sm font-semibold text-amber-200">"*"</span>
                                                        </Show>

                                                        <button
                                                            class="inline-flex h-5 w-5 items-center justify-center rounded-full bg-white/[0.05] text-slate-300 transition duration-150 hover:bg-white/[0.12] hover:text-white"
                                                            on:click=move |event| {
                                                                event.stop_propagation();
                                                                app_state.close_editor_file(file_id);
                                                            }
                                                        >
                                                            "x"
                                                        </button>
                                                    </div>
                                                }
                                            })
                                            .collect_view()
                                    }}
                                </>
                            </Show>
                            </div>

                            <div class="flex flex-wrap items-center justify-end gap-2">
                                <button
                                    class=ACTION_BUTTON_CLASS
                                    disabled=move || app_state.active_file_id.get().is_none()
                                    on:click=move |_| {
                                        let Some(_) = app_state.active_file_id.get_untracked() else {
                                            return;
                                        };

                                        let contents = active_buffer_text(app_state);
                                        copy_feedback.set(CopyFeedback::Idle);

                                        spawn_local(async move {
                                            let feedback = match copy_text_to_clipboard(&contents).await {
                                                Ok(()) => CopyFeedback::Copied,
                                                Err(_) => CopyFeedback::Failed,
                                            };
                                            copy_feedback.set(feedback);
                                        });
                                    }
                                >
                                    {move || match copy_feedback.get() {
                                        CopyFeedback::Idle => "copy",
                                        CopyFeedback::Copied => "copied",
                                        CopyFeedback::Failed => "copy failed",
                                    }}
                                </button>

                                <button
                                    class=ACTION_BUTTON_CLASS
                                    disabled=move || {
                                        app_state.active_file_id.get().is_none()
                                            || app_state.open_file_ids.get().len() <= 1
                                    }
                                    on:click=move |_| {
                                        let Some(file_id) = app_state.active_file_id.get_untracked() else {
                                            return;
                                        };

                                        app_state.close_other_editor_files(file_id);
                                        copy_feedback.set(CopyFeedback::Idle);
                                    }
                                >
                                    "close others"
                                </button>

                                <button
                                    class=ACTION_BUTTON_CLASS
                                    disabled=move || app_state.open_file_ids.get().is_empty()
                                    on:click=move |_| {
                                        app_state.close_all_editor_files();
                                        copy_feedback.set(CopyFeedback::Idle);
                                    }
                                >
                                    "close all"
                                </button>
                            </div>
                        </div>

                        <div class="flex flex-wrap items-center gap-2 border-b border-white/8 bg-black/20 px-4 py-3 text-[0.72rem] uppercase tracking-[0.16em] text-slate-400">
                            <span class="rounded-full border border-sky-300/20 bg-sky-300/[0.08] px-2.5 py-1 text-sky-100/85">
                                {move || active_file_badge(app_state)}
                            </span>
                            <span class="min-w-0 flex-1 truncate text-slate-300 normal-case tracking-normal">
                                {move || active_file_path(app_state)}
                            </span>
                            <span>{move || active_file_language(app_state)}</span>
                            <Show when=move || active_file_is_dirty(app_state)>
                                <span class="rounded-full border border-amber-300/20 bg-amber-300/10 px-2.5 py-1 text-amber-100">
                                    "modified"
                                </span>
                            </Show>
                        </div>

                        <Show
                            when=move || app_state.active_file_id.get().is_some()
                            fallback=move || {
                                view! {
                                    <div class="grid flex-1 place-items-center bg-[linear-gradient(180deg,rgba(255,255,255,0.015),transparent)] px-4 text-sm uppercase tracking-[0.18em] text-slate-500">
                                        "no file open"
                                    </div>
                                }
                            }
                        >
                            <textarea
                                class="min-h-[30rem] flex-1 resize-none bg-[linear-gradient(180deg,rgba(255,255,255,0.015),transparent)] px-4 py-4 font-mono text-[0.94rem] leading-[1.55] text-white outline-none"
                                prop:value=move || active_buffer_text(app_state)
                                prop:spellcheck=false
                                on:input=move |event| {
                                    let Some(file_id) = app_state.active_file_id.get_untracked() else {
                                        return;
                                    };

                                    app_state.update_buffer(file_id, event_target_value(&event));
                                }
                            />
                        </Show>
                    </div>
                </div>
            </article>

            <article class=CARD_CLASS>
                <div class=PANEL_CLASS>
                    <p class=EYEBROW_CLASS>"File metadata"</p>
                    <ul class=MEMORY_LIST_CLASS>
                        <li class=MEMORY_ITEM_CLASS>
                            <span class=MEMORY_SCOPE_CLASS>"language"</span>
                            <span class=MEMORY_VALUE_CLASS>{move || active_file_language(app_state)}</span>
                        </li>
                        <li class=MEMORY_ITEM_CLASS>
                            <span class=MEMORY_SCOPE_CLASS>"workspace root"</span>
                            <span class=MEMORY_VALUE_CLASS>{WORKSPACE_ROOT}</span>
                        </li>
                        <li class=MEMORY_ITEM_CLASS>
                            <span class=MEMORY_SCOPE_CLASS>"preview layout"</span>
                            <span class=MEMORY_VALUE_CLASS>
                                {move || app_state.session.get().selected_layout_name().to_string()}
                            </span>
                        </li>
                    </ul>
                </div>

                <div class=PANEL_CLASS>
                    <p class=EYEBROW_CLASS>"Bindings"</p>
                    <ul class=MEMORY_LIST_CLASS>
                        {move || {
                            app_state
                                .binding_entries()
                                .into_iter()
                                .map(|entry| {
                                    view! {
                                        <li class=MEMORY_ITEM_CLASS>
                                            <span class=MEMORY_SCOPE_CLASS>{entry.chord}</span>
                                            <span class=MEMORY_VALUE_CLASS>{entry.command_label}</span>
                                        </li>
                                    }
                                })
                                .collect_view()
                        }}
                    </ul>
                </div>

                <div class=PANEL_CLASS>
                    <p class=EYEBROW_CLASS>"Runtime notes"</p>
                    <ul class=MEMORY_LIST_CLASS>
                        <li class=MEMORY_ITEM_CLASS>
                            <span class=MEMORY_SCOPE_CLASS>"Applied live"</span>
                            <span class=MEMORY_VALUE_CLASS>
                                "config.ts, bindings.ts, root css, layout css, active layout tsx"
                            </span>
                        </li>
                        <li class=MEMORY_ITEM_CLASS>
                            <span class=MEMORY_SCOPE_CLASS>"Buffered next"</span>
                            <span class=MEMORY_VALUE_CLASS>
                                "additional imported source files are the next gap once the workspace bundle expands beyond the current playground set"
                            </span>
                        </li>
                    </ul>
                </div>
            </article>
        </section>
    }
}
