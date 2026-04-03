use clsx::clsx;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::app_state::AppState;
use crate::editor_host::download_directory;
use crate::workspace::{EditorFileTreeDirectory, EditorFileTreeNode, file_by_id};

use super::buffers::{editor_file_badge, is_file_dirty};
use super::download::{collect_directory_download_items, download_directory_title};

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
pub fn FileTreeDirectoryView(
    directory: EditorFileTreeDirectory,
    #[prop(optional)] is_root: bool,
    #[prop(optional)] depth: usize,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let directory_path = directory.path.to_string();
    let directory_name = directory.name.to_string();
    let default_open = directory.default_open;
    let download_title = download_directory_title(&directory);
    let can_download = directory.download_root_path.is_some();
    let download_directory_node = directory.clone();
    let child_nodes = directory.children.clone();
    let padding_left = format!("padding-left: {}px", depth * 14 + 8);

    view! {
        <div class=if is_root { "grid" } else { "grid" }>
            {(!is_root).then(|| {
                let directory_name_text = directory_name.clone();
                let row_padding = padding_left.clone();

                view! {
                    <div class="group/layout-subtree flex items-center gap-1">
                        <button
                            class="text-terminal-dim flex flex-1 items-center gap-1 px-2 py-1 text-left text-sm leading-5"
                            style=row_padding
                            on:click=move |_| app_state.toggle_directory(directory_path.clone(), default_open)
                        >
                            <span>
                                {move || if is_directory_open(app_state, directory.path, default_open, is_root) { "v" } else { ">" }}
                            </span>
                            <span class="min-w-0 flex-1 truncate">{directory_name_text}</span>
                        </button>

                        {can_download.then(|| {
                            let directory_for_download = download_directory_node.clone();
                            let directory_label = directory_for_download.name;

                            view! {
                                <button
                                    class="border-terminal-border bg-terminal-bg-panel text-terminal-dim hover:border-terminal-info hover:text-terminal-fg rounded-full border px-1.5 py-0 text-[10px] uppercase tracking-[0.16em]"
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
                                    "dl"
                                </button>
                            }
                        })}
                    </div>
                }
            })}

            <Show when=move || is_directory_open(app_state, directory.path, default_open, is_root)>
                <div class="grid">
                    {child_nodes
                        .clone()
                        .into_iter()
                        .map(|child| view! { <FileTreeNodeView node=child depth=depth + 1/> })
                        .collect_view()}
                </div>
            </Show>
        </div>
    }
}

#[component]
pub fn FileTreeNodeView(node: EditorFileTreeNode, #[prop(optional)] depth: usize) -> impl IntoView {
    let app_state = expect_context::<AppState>();

    view! {
        {match node {
            EditorFileTreeNode::Directory(directory) => {
                view! { <FileTreeDirectoryView directory=directory depth=depth/> }.into_any()
            }
            EditorFileTreeNode::File(file_id) => {
                let file = file_by_id(file_id);
                let badge = editor_file_badge(file.language).to_string();
                let label = file.label.to_string();
                let padding_left = format!("padding-left: {}px", depth * 14 + 8);

                view! {
                    <button
                        class=move || {
                            clsx!(
                                "flex w-full items-center gap-2 px-2 py-1 text-left text-sm leading-5",
                                (
                                    app_state.active_file_id.get() == Some(file_id),
                                    "bg-terminal-bg-hover text-terminal-fg-strong"
                                ),
                                (
                                    app_state.active_file_id.get() != Some(file_id),
                                    "text-terminal-muted hover:bg-terminal-bg-hover hover:text-terminal-fg"
                                )
                            )
                        }
                        style=padding_left
                        on:click=move |_| app_state.select_editor_file(file_id)
                    >
                        <span class="text-terminal-info shrink-0">
                            {badge}
                        </span>
                        <span class="min-w-0 flex-1 truncate">{label}</span>

                        <Show when=move || app_state.open_file_ids.get().contains(&file_id)>
                            <span class="text-terminal-dim shrink-0 text-xs">
                                "open"
                            </span>
                        </Show>

                        <Show when=move || is_file_dirty(&app_state.editor_buffers.get(), file_id)>
                            <span class="text-terminal-warn ml-auto shrink-0">"+"</span>
                        </Show>
                    </button>
                }
                    .into_any()
            }
        }}
    }
}
