use std::collections::BTreeMap;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;

use spiders_config::model::Config;
use spiders_config::authoring_layout::SourceBundleAuthoringLayoutService;
use spiders_config::runtime::build_source_bundle_authoring_layout_service;
use spiders_core::runtime::layout_context::LayoutEvaluationDependencies;
use spiders_core::LayoutId;
use spiders_runtime_js_browser::JavaScriptBrowserRuntimeProvider;
use spiders_wm_runtime::{PreviewSession, build_preview_layout_context};

use crate::editor_files::{EDITOR_FILES, EditorFileId, WORKSPACE_FS_ROOT, runtime_path};
use crate::session::PreviewSessionState;

#[derive(Debug, Clone)]
pub struct PreviewRenderRequest {
    pub active_layout: LayoutId,
    pub runtime_state: PreviewSession,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub buffers: BTreeMap<EditorFileId, String>,
}

impl PreviewRenderRequest {
    pub fn from_state(
        buffers: &BTreeMap<EditorFileId, String>,
        session: &PreviewSessionState,
    ) -> Self {
        Self {
            active_layout: session.active_layout().clone(),
            runtime_state: session.runtime_state().clone(),
            canvas_width: session.canvas_width() as u32,
            canvas_height: session.canvas_height() as u32,
            buffers: buffers.clone(),
        }
    }
}

#[derive(Clone)]
pub struct EvaluatedPreviewLayout {
    pub config: Config,
    pub layout: spiders_core::SourceLayoutNode,
    pub dependencies: LayoutEvaluationDependencies,
}

pub async fn evaluate_layout_source(
    request: &PreviewRenderRequest,
) -> Result<EvaluatedPreviewLayout, String> {
    let config_entry_path = PathBuf::from(runtime_path(EditorFileId::Config));
    let root_dir = PathBuf::from(WORKSPACE_FS_ROOT);
    let sources = source_bundle_sources(&request.buffers);
    let maybe_service = LAYOUT_SERVICE.with_borrow_mut(|slot| slot.take());
    let mut service = match maybe_service {
        Some(service) => service,
        None => build_layout_service(&config_entry_path).map_err(|error| error.to_string())?,
    };

    let result = async {
        let config = service
            .load_config(&root_dir, &config_entry_path, &sources)
            .await
            .map_err(|error| error.to_string())?;
        let state = build_preview_state_snapshot(request);
        let workspace = state
            .current_workspace()
            .cloned()
            .ok_or_else(|| "preview workspace is unavailable".to_string())?;
        let evaluation = service
            .evaluate_prepared_for_workspace(&root_dir, &sources, &config, &state, &workspace)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "no selected layout for preview workspace".to_string())?;
        Ok(EvaluatedPreviewLayout {
            config,
            layout: evaluation.layout,
            dependencies: evaluation.dependencies,
        })
    }
    .await;

    LAYOUT_SERVICE.with_borrow_mut(|slot| {
        *slot = Some(service);
    });

    result
}

fn build_layout_service(
    entry_path: &std::path::Path,
) -> Result<SourceBundleAuthoringLayoutService, spiders_config::model::LayoutConfigError> {
    build_source_bundle_authoring_layout_service(entry_path, &[&JavaScriptBrowserRuntimeProvider])
}

thread_local! {
    static LAYOUT_SERVICE: RefCell<Option<SourceBundleAuthoringLayoutService>> = const { RefCell::new(None) };
}

fn build_preview_state_snapshot(request: &PreviewRenderRequest) -> spiders_core::snapshot::StateSnapshot {
    let context = build_preview_layout_context(
        &request.runtime_state,
        Some(request.active_layout.as_str().to_string()),
        "DP-1",
        request.canvas_width,
        request.canvas_height,
    );
    let workspace_id = context.workspace_id.clone();
    let output_id = spiders_core::OutputId::from(spiders_wm_runtime::PREVIEW_OUTPUT_ID);

    spiders_core::snapshot::StateSnapshot {
        focused_window_id: context.state.as_ref().and_then(|state| state.focused_window_id.clone()),
        current_output_id: Some(output_id.clone()),
        current_workspace_id: Some(workspace_id.clone()),
        outputs: vec![spiders_core::snapshot::OutputSnapshot {
            id: output_id,
            name: context.monitor.name.clone(),
            logical_x: 0,
            logical_y: 0,
            logical_width: request.canvas_width,
            logical_height: request.canvas_height,
            scale: context.monitor.scale.unwrap_or(1),
            transform: spiders_core::types::OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(workspace_id.clone()),
        }],
        workspaces: vec![spiders_core::snapshot::WorkspaceSnapshot {
            id: workspace_id,
            name: context.workspace.name.clone(),
            output_id: Some(spiders_core::OutputId::from(spiders_wm_runtime::PREVIEW_OUTPUT_ID)),
            active_workspaces: context.workspace.workspaces.clone(),
            focused: true,
            visible: true,
            effective_layout: Some(spiders_core::types::LayoutRef {
                name: request.active_layout.as_str().to_string(),
            }),
        }],
        windows: request
            .runtime_state
            .windows
            .iter()
            .filter(|window| window.workspace_name == request.runtime_state.active_workspace_name)
            .map(|window| spiders_wm_runtime::preview_window_snapshot(window, Some(window.workspace_name.as_str())))
            .collect(),
        visible_window_ids: request
            .runtime_state
            .windows
            .iter()
            .filter(|window| window.workspace_name == request.runtime_state.active_workspace_name)
            .map(|window| spiders_core::WindowId::from(window.id.as_str()))
            .collect(),
        workspace_names: request.runtime_state.workspace_names.clone(),
    }
}

pub fn source_bundle_sources(
    buffers: &BTreeMap<EditorFileId, String>,
) -> BTreeMap<PathBuf, String> {
    let mut sources = fixture_source_bundle();

    for file in EDITOR_FILES {
        if let Some(source) = buffers.get(&file.id) {
            sources.insert(PathBuf::from(runtime_path(file.id)), source.clone());
        }
    }

    sources
}

fn fixture_source_bundle() -> BTreeMap<PathBuf, String> {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/spiders-wm");
    let runtime_root = PathBuf::from(WORKSPACE_FS_ROOT);
    let mut sources = BTreeMap::new();
    collect_fixture_sources(&fixture_root, &runtime_root, &mut sources);
    sources
}

fn collect_fixture_sources(
    fixture_root: &std::path::Path,
    runtime_root: &std::path::Path,
    sources: &mut BTreeMap<PathBuf, String>,
) {
    let Ok(entries) = fs::read_dir(fixture_root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_fixture_sources(&path, runtime_root, sources);
            continue;
        }

        let Ok(relative) = path.strip_prefix(fixture_root) else {
            continue;
        };
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };

        sources.insert(runtime_root.join(relative), source);
    }
}
