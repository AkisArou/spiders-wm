mod config_decode;

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::path::{Path, PathBuf};

use js_sys::Promise;
use serde_json::Value;
use spiders_config::model::{Config, LayoutConfigError, RuntimeKind};
use spiders_config::runtime::{
    SourceBundle, SourceBundleConfigRuntime, SourceBundlePreparedLayoutRuntime,
    SourceBundleRuntimeBundle, SourceBundleRuntimeProvider,
};
use spiders_core::runtime::layout_context::LayoutEvaluationContext;
use spiders_core::runtime::prepared_layout::{PreparedLayout, PreparedStylesheet, PreparedStylesheets};
use spiders_core::snapshot::{StateSnapshot, WorkspaceSnapshot};
use spiders_core::SourceLayoutNode;
use spiders_runtime_js_core::{
    JavaScriptModuleGraph, compile_source_bundle_to_module_graph, decode_js_layout_value,
    decode_runtime_graph_payload, encode_runtime_graph_payload,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::config_decode::{decode_config_value, validate_layout_selection};

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

export async function evaluateModuleGraph(moduleGraph, exportName, arg) {
    const moduleMap = new Map(
        (moduleGraph.modules ?? []).map((module) => [module.specifier, module]),
    );
    const cache = new Map();
    const entryUrl = moduleUrlFor(moduleGraph.entry, moduleMap, cache, new Set());
    const namespace = await import(entryUrl);
    const exported = exportName === "default" ? namespace.default : namespace[exportName];

    if (typeof exported !== "function") {
        throw new Error(`Module ${moduleGraph.entry} does not export callable ${exportName}`);
    }

    return exported(arg);
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = evaluateModuleGraph)]
    fn evaluate_module_graph(
        module_graph: JsValue,
        export_name: &str,
        arg: JsValue,
    ) -> Result<Promise, JsValue>;
}

pub async fn evaluate_layout_module_graph(
    module_graph: &JavaScriptModuleGraph,
    context: &LayoutEvaluationContext,
) -> Result<JsValue, String> {
    let graph_value = serde_wasm_bindgen::to_value(module_graph).map_err(|error| error.to_string())?;
    let context_value = serde_wasm_bindgen::to_value(context).map_err(|error| error.to_string())?;
    let promise = evaluate_module_graph(graph_value, "default", context_value).map_err(js_error_to_string)?;

    JsFuture::from(promise).await.map_err(js_error_to_string)
}

pub async fn load_config_from_source_bundle(
    root_dir: &Path,
    entry_path: &Path,
    sources: &BTreeMap<PathBuf, String>,
) -> Result<Config, String> {
    let graph = compile_source_bundle_to_module_graph(root_dir, entry_path, sources)
        .map_err(|error| error.to_string())?;
    let graph_value = serde_wasm_bindgen::to_value(&graph).map_err(|error| error.to_string())?;
    let arg = JsValue::NULL;
    let promise = evaluate_module_graph(graph_value, "default", arg).map_err(js_error_to_string)?;
    let value = JsFuture::from(promise).await.map_err(js_error_to_string)?;
    let config_value: Value = serde_wasm_bindgen::from_value(value).map_err(|error| error.to_string())?;
    let mut config = decode_config_value(entry_path, &config_value).map_err(|error| error.to_string())?;
    config.global_stylesheet_path = sources
        .contains_key(&root_dir.join("index.css"))
        .then(|| root_dir.join("index.css").to_string_lossy().into_owned());
    config.layouts = discover_layout_definitions(root_dir, sources)?;
    validate_layout_selection(entry_path, &config.layout_selection, &config.layouts)
        .map_err(|error| error.to_string())?;
    Ok(config)
}

pub fn compile_module_graph_from_source_bundle(
    root_dir: &Path,
    entry_path: &Path,
    sources: &BTreeMap<PathBuf, String>,
) -> Result<JavaScriptModuleGraph, String> {
    compile_source_bundle_to_module_graph(root_dir, entry_path, sources).map_err(|error| error.to_string())
}

fn js_error_to_string(error: JsValue) -> String {
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

fn discover_layout_definitions(
    root_dir: &Path,
    sources: &BTreeMap<PathBuf, String>,
) -> Result<Vec<spiders_config::model::LayoutDefinition>, String> {
    let mut layout_entries = sources
        .keys()
        .filter_map(|path| discover_layout_entry(root_dir, sources, path))
        .collect::<Vec<_>>();
    layout_entries.sort_by(|left, right| left.name.cmp(&right.name));
    layout_entries.dedup_by(|left, right| left.name == right.name);

    layout_entries
        .into_iter()
        .map(|layout| {
            let runtime_graph = compile_source_bundle_to_module_graph(root_dir, &layout.entry_path, sources)?;
            Ok(spiders_config::model::LayoutDefinition {
                name: layout.name,
                directory: layout
                    .entry_path
                    .parent()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                module: runtime_graph.entry.clone(),
                stylesheet_path: layout
                    .stylesheet_path
                    .map(|path| path.to_string_lossy().into_owned()),
                runtime_cache_payload: Some(encode_runtime_graph_payload(&runtime_graph)),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredLayoutApp {
    name: String,
    entry_path: PathBuf,
    stylesheet_path: Option<PathBuf>,
}

fn discover_layout_entry(
    root_dir: &Path,
    sources: &BTreeMap<PathBuf, String>,
    path: &Path,
) -> Option<DiscoveredLayoutApp> {
    let relative = path.strip_prefix(root_dir).ok()?;
    let components = relative.iter().map(|segment| segment.to_str()).collect::<Option<Vec<_>>>()?;
    if components.len() != 3 || components[0] != "layouts" || components[2] == "index.css" {
        return None;
    }

    if !matches!(components[2], "index.ts" | "index.tsx" | "index.js" | "index.jsx") {
        return None;
    }

    let stylesheet_path = root_dir.join("layouts").join(components[1]).join("index.css");

    Some(DiscoveredLayoutApp {
        name: components[1].to_string(),
        entry_path: path.to_path_buf(),
        stylesheet_path: sources.contains_key(&stylesheet_path).then_some(stylesheet_path),
    })
}

#[derive(Debug, Default, Clone, Copy)]
pub struct JavaScriptBrowserRuntimeProvider;

#[derive(Debug, Default)]
pub struct JavaScriptBrowserConfigRuntime;

#[derive(Debug, Default)]
pub struct JavaScriptBrowserPreparedLayoutRuntime;

impl SourceBundleRuntimeProvider for JavaScriptBrowserRuntimeProvider {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::JavaScript
    }

    fn build_source_bundle_runtime_bundle(
        &self,
    ) -> Result<SourceBundleRuntimeBundle, LayoutConfigError> {
        Ok(SourceBundleRuntimeBundle {
            config_runtime: Box::new(JavaScriptBrowserConfigRuntime),
            layout_runtime: Box::new(JavaScriptBrowserPreparedLayoutRuntime),
        })
    }
}

impl SourceBundleConfigRuntime for JavaScriptBrowserConfigRuntime {
    fn load_config<'a>(
        &'a self,
        root_dir: &'a Path,
        entry_path: &'a Path,
        sources: &'a SourceBundle,
    ) -> Pin<Box<dyn Future<Output = Result<Config, LayoutConfigError>> + 'a>> {
        Box::pin(async move {
            load_config_from_source_bundle(root_dir, entry_path, sources).await.map_err(|message| {
                LayoutConfigError::EvaluateAuthoredConfig {
                    path: entry_path.to_path_buf(),
                    message,
                }
            })
        })
    }
}

impl SourceBundlePreparedLayoutRuntime for JavaScriptBrowserPreparedLayoutRuntime {
    fn prepare_layout<'a>(
        &'a self,
        root_dir: &'a Path,
        sources: &'a SourceBundle,
        config: &'a Config,
        workspace: &'a WorkspaceSnapshot,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PreparedLayout>, LayoutConfigError>> + 'a>> {
        Box::pin(async move {
            let Some(layout) = config.selected_layout(workspace) else {
                return Ok(None);
            };

            let runtime_graph = decode_runtime_graph_payload(
                layout.runtime_cache_payload.as_ref().ok_or_else(|| {
                    LayoutConfigError::DecodeAuthoredConfig {
                        path: root_dir.join(&layout.module),
                        message: format!("layout `{}` is missing runtime cache payload", layout.name),
                    }
                })?,
            )
            .map_err(|error| LayoutConfigError::DecodeAuthoredConfig {
                path: root_dir.join(&layout.module),
                message: error.to_string(),
            })?;

            Ok(Some(PreparedLayout {
                selected: config.resolve_selected_layout(workspace)?.expect("selected layout exists"),
                runtime_payload: encode_runtime_graph_payload(&runtime_graph),
                stylesheets: PreparedStylesheets {
                    global: load_stylesheet_asset(
                        config.global_stylesheet_path.as_deref(),
                        root_dir,
                        sources,
                    ),
                    layout: load_stylesheet_asset(layout.stylesheet_path.as_deref(), root_dir, sources),
                },
            }))
        })
    }

    fn build_context(
        &self,
        state: &StateSnapshot,
        workspace: &WorkspaceSnapshot,
        artifact: Option<&PreparedLayout>,
    ) -> LayoutEvaluationContext {
        state.layout_context(workspace, artifact.map(|artifact| artifact.selected.clone()))
    }

    fn evaluate_layout<'a>(
        &'a self,
        _root_dir: &'a Path,
        _sources: &'a SourceBundle,
        artifact: &'a PreparedLayout,
        context: &'a LayoutEvaluationContext,
    ) -> Pin<Box<dyn Future<Output = Result<SourceLayoutNode, LayoutConfigError>> + 'a>> {
        Box::pin(async move {
            let runtime_graph = decode_runtime_graph_payload(&artifact.runtime_payload).map_err(|error| {
                LayoutConfigError::DecodeAuthoredConfig {
                    path: PathBuf::from(&artifact.selected.module),
                    message: error.to_string(),
                }
            })?;
            let value = evaluate_layout_module_graph(&runtime_graph, context)
                .await
                .map_err(|message| LayoutConfigError::EvaluateAuthoredConfig {
                    path: PathBuf::from(&artifact.selected.module),
                    message,
                })?;
            let json: Value =
                serde_wasm_bindgen::from_value(value).map_err(|error| LayoutConfigError::DecodeAuthoredConfig {
                    path: PathBuf::from(&artifact.selected.module),
                    message: error.to_string(),
                })?;

            decode_js_layout_value(&json).map_err(|message| LayoutConfigError::DecodeAuthoredConfig {
                path: PathBuf::from(&artifact.selected.module),
                message,
            })
        })
    }
}

fn load_stylesheet_asset(
    path: Option<&str>,
    root_dir: &Path,
    sources: &SourceBundle,
) -> Option<PreparedStylesheet> {
    let path = path?;
    let source_path = PathBuf::from(path);
    let resolved = if source_path.is_absolute() {
        source_path
    } else {
        root_dir.join(&source_path)
    };
    let source = sources.get(&resolved).cloned().unwrap_or_default();
    Some(PreparedStylesheet { path: path.to_string(), source })
}
