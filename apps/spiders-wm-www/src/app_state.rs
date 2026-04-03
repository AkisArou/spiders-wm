use std::collections::BTreeMap;

use leptos::prelude::*;

use crate::bindings::{ParsedBindingEntry, ParsedBindingsState, parse_bindings_source};
use crate::session::{PreviewEnvironment, PreviewLayoutId, PreviewSessionState};
use crate::workspace::{
    EditorFileId, initial_content, initial_editor_buffers, initial_open_directories,
    initial_open_editor_files, parse_workspace_names,
};

#[derive(Clone, Copy)]
pub struct AppState {
    pub session: RwSignal<PreviewSessionState>,
    pub editor_buffers: RwSignal<BTreeMap<EditorFileId, String>>,
    pub active_file_id: RwSignal<Option<EditorFileId>>,
    pub open_file_ids: RwSignal<Vec<EditorFileId>>,
    pub directory_open_state: RwSignal<BTreeMap<String, bool>>,
    pub latest_preview_request_key: RwSignal<String>,
}

impl AppState {
    pub fn new() -> Self {
        let initial_buffers = initial_editor_buffers();
        let initial_environment = build_preview_environment(&initial_buffers);

        Self {
            session: RwSignal::new(PreviewSessionState::new(
                PreviewLayoutId::MasterStack,
                initial_environment,
            )),
            editor_buffers: RwSignal::new(initial_buffers),
            active_file_id: RwSignal::new(Some(EditorFileId::LayoutTsx)),
            open_file_ids: RwSignal::new(initial_open_editor_files()),
            directory_open_state: RwSignal::new(initial_open_directories()),
            latest_preview_request_key: RwSignal::new(String::new()),
        }
    }

    pub fn parsed_bindings(&self) -> ParsedBindingsState {
        let buffers = self.editor_buffers.get();
        parse_bindings_source(binding_source(&buffers))
    }

    pub fn binding_entries(&self) -> Vec<ParsedBindingEntry> {
        self.parsed_bindings().entries
    }

    pub fn update_buffer(&self, file_id: EditorFileId, next_value: String) {
        self.editor_buffers.update(|buffers| {
            buffers.insert(file_id, next_value);
            let next_environment = build_preview_environment(buffers);
            self.session
                .update(|state| state.sync_environment(next_environment));
        });
    }

    pub fn select_editor_file(&self, file_id: EditorFileId) {
        self.open_file_ids.update(|open_files| {
            if !open_files.contains(&file_id) {
                open_files.push(file_id);
            }
        });
        self.active_file_id.set(Some(file_id));
    }

    pub fn close_editor_file(&self, file_id: EditorFileId) {
        self.open_file_ids.update(|open_files| {
            let Some(index) = open_files
                .iter()
                .position(|open_file_id| *open_file_id == file_id)
            else {
                return;
            };

            open_files.remove(index);

            if self.active_file_id.get_untracked() == Some(file_id) {
                let next_index = index
                    .saturating_sub(1)
                    .min(open_files.len().saturating_sub(1));
                let next_file_id = open_files.get(next_index).copied();
                self.active_file_id.set(next_file_id);
            }
        });
    }

    pub fn close_other_editor_files(&self, file_id: EditorFileId) {
        self.open_file_ids.set(vec![file_id]);
        self.active_file_id.set(Some(file_id));
    }

    pub fn close_all_editor_files(&self) {
        self.open_file_ids.set(Vec::new());
        self.active_file_id.set(None);
    }

    pub fn toggle_directory(&self, path: String, default_open: bool) {
        self.directory_open_state.update(|state| {
            let next_value = !state.get(&path).copied().unwrap_or(default_open);
            state.insert(path, next_value);
        });
    }
}

fn build_preview_environment(buffers: &BTreeMap<EditorFileId, String>) -> PreviewEnvironment {
    let root_css = buffers
        .get(&EditorFileId::RootCss)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(EditorFileId::RootCss));
    let master_css = buffers
        .get(&EditorFileId::LayoutCss)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(EditorFileId::LayoutCss));
    let focus_css = buffers
        .get(&EditorFileId::FocusReproLayoutCss)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(EditorFileId::FocusReproLayoutCss));
    let config_source = buffers
        .get(&EditorFileId::Config)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(EditorFileId::Config));

    PreviewEnvironment {
        workspace_names: parse_workspace_names(config_source),
        stylesheets: BTreeMap::from([
            (
                PreviewLayoutId::MasterStack,
                format!("{root_css}\n\n{master_css}"),
            ),
            (
                PreviewLayoutId::FocusRepro,
                format!("{root_css}\n\n{focus_css}"),
            ),
        ]),
    }
}

fn binding_source(buffers: &BTreeMap<EditorFileId, String>) -> &str {
    buffers
        .get(&EditorFileId::ConfigBindings)
        .map(String::as_str)
        .unwrap_or_else(|| initial_content(EditorFileId::ConfigBindings))
}
