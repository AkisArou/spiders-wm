use std::collections::BTreeMap;
use std::path::PathBuf;

use leptos::prelude::*;
use spiders_config::model::Config;
use spiders_config::runtime::load_config_from_source_bundle;
use spiders_core::command::WmCommand;
use spiders_core::event::WmEvent;
use spiders_core::types::SpiderPlatform;
use spiders_core::LayoutId;
use spiders_runtime_js_browser::JavaScriptBrowserRuntimeProvider;
use spiders_scene::pipeline::SceneCache;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::browser_ipc;
use crate::bindings::{ParsedBindingEntry, ParsedBindingsState};
use crate::editor_files::{
    EditorFileId, initial_content, initial_editor_buffers, initial_open_editor_files, runtime_path,
};
use crate::layout_runtime::{EvaluatedPreviewLayout, source_bundle_sources};
use crate::session::PreviewSessionState;
use crate::workspace::initial_open_directories;
use spiders_wm_runtime::PreviewRenderAction;

#[derive(Clone, Copy)]
pub struct AppState {
    pub session: RwSignal<PreviewSessionState>,
    pub editor_buffers: RwSignal<BTreeMap<EditorFileId, String>>,
    pub active_file_id: RwSignal<Option<EditorFileId>>,
    pub open_file_ids: RwSignal<Vec<EditorFileId>>,
    pub directory_open_state: RwSignal<BTreeMap<String, bool>>,
    pub latest_preview_request_key: RwSignal<String>,
    pub latest_config_request_key: RwSignal<String>,
    pub preview_refresh_scheduled: RwSignal<bool>,
    pub preview_eval_request: RwSignal<u64>,
    pub preview_scene_cache: RwSignal<SceneCache>,
    pub loaded_config: RwSignal<Option<Config>>,
    pub loaded_preview_layout: RwSignal<Option<EvaluatedPreviewLayout>>,
    pub loaded_bindings: RwSignal<ParsedBindingsState>,
}

impl AppState {
    pub fn new() -> Self {
        let initial_buffers = initial_editor_buffers();
        let initial_environment = build_preview_environment(&initial_buffers, None);

        Self {
            session: RwSignal::new(PreviewSessionState::new(
                LayoutId::from("master-stack"),
                initial_environment.workspace_names,
                initial_environment.stylesheets_by_layout,
            )),
            editor_buffers: RwSignal::new(initial_buffers),
            active_file_id: RwSignal::new(Some(EditorFileId::LayoutTsx)),
            open_file_ids: RwSignal::new(initial_open_editor_files()),
            directory_open_state: RwSignal::new(initial_open_directories()),
            latest_preview_request_key: RwSignal::new(String::new()),
            latest_config_request_key: RwSignal::new(String::new()),
            preview_refresh_scheduled: RwSignal::new(false),
            preview_eval_request: RwSignal::new(1),
            preview_scene_cache: RwSignal::new(SceneCache::new()),
            loaded_config: RwSignal::new(None),
            loaded_preview_layout: RwSignal::new(None),
            loaded_bindings: RwSignal::new(default_bindings_state()),
        }
    }

    pub fn parsed_bindings(&self) -> ParsedBindingsState {
        self.loaded_bindings.get_untracked()
    }

    pub fn binding_entries(&self) -> Vec<ParsedBindingEntry> {
        self.parsed_bindings().entries
    }

    pub fn apply_loaded_config(&self, config: Config) {
        let buffers = self.editor_buffers.get_untracked();
        let next_environment = build_preview_environment(&buffers, Some(&config));
        self.loaded_bindings.set(bindings_state_from_config(&config));
        self.loaded_config.set(Some(config));
        self.preview_scene_cache.update(|cache| cache.clear());
        self.session.update(|state| {
            state.sync_inputs(
                next_environment.workspace_names,
                next_environment.stylesheets_by_layout,
            )
        });
        self.broadcast_preview_event(WmEvent::ConfigReloaded);
        self.request_preview_reevaluation();
    }

    pub fn apply_config_error(&self) {
        self.loaded_config.set(None);
        self.loaded_preview_layout.set(None);
        self.loaded_bindings.set(default_bindings_state());
        self.preview_scene_cache.update(|cache| cache.clear());
        let buffers = self.editor_buffers.get_untracked();
        let next_environment = build_preview_environment(&buffers, None);
        self.session.update(|state| {
            state.sync_inputs(
                next_environment.workspace_names,
                next_environment.stylesheets_by_layout,
            )
        });
        self.broadcast_preview_event(WmEvent::ConfigReloaded);
        self.request_preview_reevaluation();
    }

    pub fn update_buffer(&self, file_id: EditorFileId, next_value: String) {
        self.editor_buffers.update(|buffers| {
            buffers.insert(file_id, next_value);
        });
        self.request_preview_reevaluation();
    }

    pub fn apply_loaded_preview_layout(&self, layout: EvaluatedPreviewLayout) {
        self.loaded_preview_layout.set(Some(layout));
    }

    pub fn apply_preview_render_action(&self, action: PreviewRenderAction) {
        match action {
            PreviewRenderAction::None => {}
            PreviewRenderAction::RefreshFromLoadedLayout => {
                self.refresh_preview_from_loaded_state()
            }
            PreviewRenderAction::RefreshFromLoadedLayoutAndReevaluate => {
                self.refresh_preview_from_loaded_state();
                if self.should_reevaluate_loaded_preview_layout() {
                    self.request_preview_reevaluation();
                }
            }
        }
    }

    fn should_reevaluate_loaded_preview_layout(&self) -> bool {
        self.loaded_preview_layout
            .get_untracked()
            .map(|layout| {
                let dependencies = layout.dependencies;
                dependencies.uses_window_count
                    || dependencies.uses_window_order
                    || dependencies.uses_window_focus
                    || dependencies.uses_visible_window_ids
                    || dependencies.uses_workspace_name
                    || dependencies.uses_workspace_names
                    || dependencies.uses_selected_layout_name
                    || dependencies.uses_layout_adjustments
            })
            .unwrap_or(true)
    }

    pub fn request_preview_reevaluation(&self) {
        self.preview_eval_request.update(|value| *value += 1);
    }

    pub fn mutate_session(
        &self,
        mutate: impl FnOnce(&mut PreviewSessionState) -> PreviewRenderAction,
    ) -> PreviewRenderAction {
        let mut state = self.session.get_untracked();
        let action = mutate(&mut state);
        self.session.set(state);
        action
    }

    pub fn broadcast_preview_event(&self, event: WmEvent) {
        browser_ipc::broadcast_event(event);
    }

    pub fn apply_preview_failure_state(&self) {
        self.loaded_preview_layout.set(None);
    }

    pub fn refresh_preview_from_loaded_state(&self) {
        if self.preview_refresh_scheduled.get_untracked() {
            return;
        }

        self.preview_refresh_scheduled.set(true);

        let schedule = self.clone();
        let Some(window) = web_sys::window() else {
            schedule.run_preview_refresh();
            return;
        };

        let callback = Closure::once(move |_timestamp: f64| {
            schedule.run_preview_refresh();
        });

        if window.request_animation_frame(callback.as_ref().unchecked_ref()).is_ok() {
            callback.forget();
        } else {
            self.preview_refresh_scheduled.set(false);
            self.run_preview_refresh();
        }
    }

    fn run_preview_refresh(&self) {
        self.preview_refresh_scheduled.set(false);
        let loaded_preview_layout = self.loaded_preview_layout.get_untracked();
        let mut scene_cache = self.preview_scene_cache.get_untracked();
        self.session.update(|state| {
            if let Some(layout) = loaded_preview_layout.as_ref() {
                state.apply_layout_source(
                    layout.layout.clone(),
                    Some(&layout.config),
                    Some(&mut scene_cache),
                );
            }
        });
        self.preview_scene_cache.set(scene_cache);
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
            let Some(index) = open_files.iter().position(|open_file_id| *open_file_id == file_id)
            else {
                return;
            };

            open_files.remove(index);

            if self.active_file_id.get_untracked() == Some(file_id) {
                let next_index = index.saturating_sub(1).min(open_files.len().saturating_sub(1));
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

struct PreviewInputs {
    workspace_names: Vec<String>,
    stylesheets_by_layout: BTreeMap<LayoutId, String>,
}

fn build_preview_environment(
    buffers: &BTreeMap<EditorFileId, String>,
    config: Option<&Config>,
) -> PreviewInputs {
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

    let workspace_names = config
        .map(|config| config.workspaces.clone())
        .filter(|workspaces: &Vec<String>| !workspaces.is_empty())
        .unwrap_or_else(|| vec!["1".to_string(), "2".to_string(), "3".to_string()]);

    PreviewInputs {
        workspace_names,
        stylesheets_by_layout: BTreeMap::from([
            (LayoutId::from("master-stack"), format!("{root_css}\n\n{master_css}")),
            (LayoutId::from("focus-repro"), format!("{root_css}\n\n{focus_css}")),
        ]),
    }
}

fn default_bindings_state() -> ParsedBindingsState {
    ParsedBindingsState { source: String::new(), mod_key: "super".to_string(), entries: Vec::new() }
}

fn bindings_state_from_config(config: &Config) -> ParsedBindingsState {
    let mod_key = config
        .bindings
        .iter()
        .find_map(|binding| binding.trigger.split('+').next().map(str::to_string))
        .unwrap_or_else(|| "super".to_string());

    ParsedBindingsState {
        source: String::new(),
        mod_key: mod_key.clone(),
        entries: config
            .bindings
            .iter()
            .map(|binding| ParsedBindingEntry {
                bind: binding.trigger.split('+').map(str::to_string).collect(),
                chord: binding.trigger.clone(),
                command: Some(binding.command.clone()),
                command_label: display_command(&binding.command),
            })
            .collect(),
    }
}

fn display_command(command: &WmCommand) -> String {
    match command {
        WmCommand::Spawn { command } => format!("spawn {command}"),
        WmCommand::ReloadConfig => "reload config".to_string(),
        WmCommand::SetLayout { name } => format!("set layout {name}"),
        WmCommand::CycleLayout { .. } => "cycle layout".to_string(),
        WmCommand::ViewWorkspace { workspace } => format!("view workspace {workspace}"),
        WmCommand::ToggleAssignFocusedWindowToWorkspace { workspace } => {
            format!("toggle workspace {workspace}")
        }
        WmCommand::AssignFocusedWindowToWorkspace { workspace } => {
            format!("assign workspace {workspace}")
        }
        WmCommand::FocusDirection { direction } => format!("focus {direction:?}"),
        WmCommand::SwapDirection { direction } => format!("swap {direction:?}"),
        WmCommand::ResizeDirection { direction } => format!("resize {direction:?}"),
        WmCommand::ResizeTiledDirection { direction } => format!("resize tiled {direction:?}"),
        WmCommand::ToggleFloating => "toggle floating".to_string(),
        WmCommand::ToggleFullscreen => "toggle fullscreen".to_string(),
        WmCommand::CloseFocusedWindow => "close focused window".to_string(),
        other => format!("{other:?}"),
    }
}

pub async fn load_config_from_buffers(
    buffers: &BTreeMap<EditorFileId, String>,
) -> Result<Config, String> {
    let root_dir = PathBuf::from(crate::editor_files::WORKSPACE_FS_ROOT);
    let entry_path = PathBuf::from(runtime_path(EditorFileId::Config));
    let sources = source_bundle_sources(buffers);
    let js_provider = JavaScriptBrowserRuntimeProvider::new(SpiderPlatform::Web);
    load_config_from_source_bundle(
        &root_dir,
        &entry_path,
        &sources,
        &[&js_provider],
    )
    .await
    .map_err(|error| error.to_string())
}
