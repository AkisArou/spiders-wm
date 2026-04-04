use std::collections::BTreeMap;
use std::path::PathBuf;

use spiders_core::LayoutId;
use spiders_wm_runtime::{PreviewSession, build_preview_layout_context, compile_source_bundle_to_module_graph};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::editor_files::{EDITOR_FILES, EditorFileId, WORKSPACE_FS_ROOT, runtime_path};
use crate::session::PreviewSessionState;

#[derive(Debug, Clone, PartialEq)]
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

#[wasm_bindgen(inline_js = r#"
const importFromPattern = /(\bfrom\s*)(["'])([^"']+)(\2)/g;
const importOnlyPattern = /(\bimport\s*)(["'])([^"']+)(\2)/g;
const dynamicImportPattern = /(\bimport\s*\(\s*)(["'])([^"']+)(\2)(\s*\))/g;

function rewriteSource(source, resolvedImports, moduleMap, toUrl) {
    const resolveSpecifier = (specifier) => {
        const resolved = resolvedImports[specifier];
        if (resolved) {
            return resolved;
        }

        return moduleMap.has(specifier) ? specifier : null;
    };

    const replaceImportFrom = (_, prefix, quote, specifier, closingQuote) => {
        const resolved = resolveSpecifier(specifier);
        if (!resolved) {
            return `${prefix}${quote}${specifier}${closingQuote}`;
        }

        return `${prefix}${quote}${toUrl(resolved)}${closingQuote}`;
    };

    const replaceDynamicImport = (_, prefix, quote, specifier, closingQuote, suffix) => {
        const resolved = resolveSpecifier(specifier);
        if (!resolved) {
            return `${prefix}${quote}${specifier}${closingQuote}${suffix}`;
        }

        return `${prefix}${quote}${toUrl(resolved)}${closingQuote}${suffix}`;
    };

    return source
        .replace(importFromPattern, replaceImportFrom)
        .replace(importOnlyPattern, replaceImportFrom)
        .replace(dynamicImportPattern, replaceDynamicImport);
}

function moduleUrlFor(specifier, moduleMap, cache, visiting) {
    if (cache.has(specifier)) {
        return cache.get(specifier);
    }

    if (visiting.has(specifier)) {
        throw new Error(`Circular module graph dependency detected at ${specifier}`);
    }

    const module = moduleMap.get(specifier);
    if (!module) {
        throw new Error(`Missing module ${specifier}`);
    }

    visiting.add(specifier);
    const resolvedImports = module.resolved_imports ?? {};
    const rewritten = rewriteSource(module.source, resolvedImports, moduleMap, (nextSpecifier) =>
        moduleUrlFor(nextSpecifier, moduleMap, cache, visiting),
    );
    const url = `data:text/javascript;charset=utf-8,${encodeURIComponent(rewritten)}`;
    cache.set(specifier, url);
    visiting.delete(specifier);
    return url;
}

export async function evaluateLayoutModuleGraph(moduleGraph, context) {
    const moduleMap = new Map(
        (moduleGraph.modules ?? []).map((module) => [module.specifier, module]),
    );
    const cache = new Map();
    const entryUrl = moduleUrlFor(moduleGraph.entry, moduleMap, cache, new Set());
    const namespace = await import(entryUrl);

    if (typeof namespace.default !== "function") {
        throw new Error(`Module ${moduleGraph.entry} does not export a default layout function`);
    }

    return namespace.default(context);
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = evaluateLayoutModuleGraph)]
    fn evaluate_layout_module_graph(
        module_graph: JsValue,
        context: JsValue,
    ) -> Result<js_sys::Promise, JsValue>;
}

pub async fn evaluate_layout_renderable(
    request: &PreviewRenderRequest,
) -> Result<JsValue, String> {
    let graph = compile_request_module_graph(request)?;
    let graph_value = serde_wasm_bindgen::to_value(&graph).map_err(|error| error.to_string())?;
    let context = build_preview_layout_context(
        &request.runtime_state,
        Some(request.active_layout.as_str().to_string()),
        "DP-1",
        request.canvas_width,
        request.canvas_height,
    );
    let context_value = serde_wasm_bindgen::to_value(&context).map_err(|error| error.to_string())?;
    let promise = evaluate_layout_module_graph(graph_value, context_value)
        .map_err(js_error_to_string)?;

    JsFuture::from(promise)
        .await
        .map_err(js_error_to_string)
}

fn compile_request_module_graph(
    request: &PreviewRenderRequest,
) -> Result<spiders_wm_runtime::JavaScriptModuleGraph, String> {
    let entry_path = PathBuf::from(runtime_path(layout_entry_file_id(&request.active_layout)));
    let root_dir = PathBuf::from(WORKSPACE_FS_ROOT);
    let sources = source_bundle_sources(&request.buffers);
    compile_source_bundle_to_module_graph(&root_dir, &entry_path, &sources)
}

fn source_bundle_sources(
    buffers: &BTreeMap<EditorFileId, String>,
) -> BTreeMap<PathBuf, String> {
    EDITOR_FILES
        .iter()
        .map(|file| {
            let path = PathBuf::from(runtime_path(file.id));
            let source = buffers.get(&file.id).cloned().unwrap_or_default();
            (path, source)
        })
        .collect()
}

fn layout_entry_file_id(layout: &LayoutId) -> EditorFileId {
    match layout.as_str() {
        "focus-repro" => EditorFileId::FocusReproLayoutTsx,
        _ => EditorFileId::LayoutTsx,
    }
}

fn js_error_to_string(error: JsValue) -> String {
    error
        .as_string()
        .unwrap_or_else(|| format!("{error:?}"))
}
