use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::mem;
use std::path::{Path, PathBuf};

use oxc::CompilerInterface;
use oxc::allocator::Allocator;
use oxc::ast::ast::Statement;
use oxc::codegen::CodegenReturn;
use oxc::diagnostics::OxcDiagnostic;
use oxc::parser::Parser;
use oxc::span::{GetSpan, SourceType};
use oxc::transformer::{JsxRuntime, TransformOptions};
use serde::Serialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::session::{PreviewLayoutId, PreviewSessionState};
use crate::workspace::{EDITOR_FILES, EditorFileId, WORKSPACE_FS_ROOT, runtime_path};

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewRenderRequest {
    pub active_layout: PreviewLayoutId,
    pub active_workspace_name: String,
    pub workspace_names: Vec<String>,
    pub windows: Vec<PreviewRequestWindow>,
    pub last_action: String,
    pub buffers: BTreeMap<EditorFileId, String>,
}

impl PreviewRenderRequest {
    pub fn from_state(
        buffers: &BTreeMap<EditorFileId, String>,
        session: &PreviewSessionState,
    ) -> Self {
        Self {
            active_layout: session.active_layout,
            active_workspace_name: session.active_workspace_name.clone(),
            workspace_names: session.workspace_names.clone(),
            windows: session
                .visible_windows()
                .into_iter()
                .map(|window| PreviewRequestWindow {
                    id: window.id.as_str().to_string(),
                    app_id: window.app_id,
                    title: window.title,
                    class: window.class,
                    instance: window.instance,
                    role: window.role,
                    shell: window.shell,
                    window_type: window.window_type,
                    floating: window.floating,
                    fullscreen: window.fullscreen,
                    focused: window.focused,
                })
                .collect(),
            last_action: session.last_action.clone(),
            buffers: buffers.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewRequestWindow {
    pub id: String,
    #[serde(rename = "app_id")]
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub class: Option<String>,
    pub instance: Option<String>,
    pub role: Option<String>,
    pub shell: Option<String>,
    #[serde(rename = "window_type")]
    pub window_type: Option<String>,
    pub floating: bool,
    pub fullscreen: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RuntimeLayoutContext {
    monitor: RuntimeMonitor,
    workspace: RuntimeWorkspace,
    windows: Vec<PreviewRequestWindow>,
    state: RuntimeState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RuntimeMonitor {
    name: &'static str,
    width: i32,
    height: i32,
    scale: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RuntimeWorkspace {
    name: String,
    workspaces: Vec<String>,
    #[serde(rename = "windowCount")]
    window_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RuntimeState {
    prototype: bool,
    #[serde(rename = "lastAction")]
    last_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct JavaScriptModule {
    specifier: String,
    source: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    resolved_imports: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct JavaScriptModuleGraph {
    entry: String,
    modules: Vec<JavaScriptModule>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SourceModuleId {
    File(PathBuf),
    Virtual(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportedModuleKind {
    Script,
    Stylesheet,
    Virtual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportedModule {
    specifier: String,
    kind: ImportedModuleKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedImport {
    specifier: String,
    kind: ImportedModuleKind,
    module_id: SourceModuleId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceModuleKind {
    Script,
    Stylesheet,
    Virtual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceModuleRecord {
    id: SourceModuleId,
    kind: SourceModuleKind,
    imports: Vec<ImportedModule>,
    resolved_imports: Vec<ResolvedImport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceModuleGraph {
    root_dir: PathBuf,
    entry_path: PathBuf,
    modules: BTreeMap<SourceModuleId, SourceModuleRecord>,
    order: Vec<SourceModuleId>,
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
    let context = RuntimeLayoutContext {
        monitor: RuntimeMonitor {
            name: "DP-1",
            width: 3440,
            height: 1440,
            scale: 1,
        },
        workspace: RuntimeWorkspace {
            name: request.active_workspace_name.clone(),
            workspaces: request.workspace_names.clone(),
            window_count: request.windows.len(),
        },
        windows: request.windows.clone(),
        state: RuntimeState {
            prototype: true,
            last_action: request.last_action.clone(),
        },
    };
    let context_value = serde_wasm_bindgen::to_value(&context).map_err(|error| error.to_string())?;
    let promise = evaluate_layout_module_graph(graph_value, context_value)
        .map_err(js_error_to_string)?;

    JsFuture::from(promise)
        .await
        .map_err(js_error_to_string)
}

fn compile_request_module_graph(
    request: &PreviewRenderRequest,
) -> Result<JavaScriptModuleGraph, String> {
    let entry_path = PathBuf::from(runtime_path(layout_entry_file_id(request.active_layout)));
    let root_dir = PathBuf::from(WORKSPACE_FS_ROOT);
    let sources = source_bundle_sources(&request.buffers);
    let graph = build_source_graph(&root_dir, &entry_path, &sources)?;
    compiled_module_graph(&graph, &sources)
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

fn build_source_graph(
    root_dir: &Path,
    entry_path: &Path,
    sources: &BTreeMap<PathBuf, String>,
) -> Result<SourceModuleGraph, String> {
    let mut modules = BTreeMap::new();
    let mut order = Vec::new();
    let mut pending = VecDeque::from([SourceModuleId::File(entry_path.to_path_buf())]);
    let mut visited = BTreeSet::new();

    while let Some(module_id) = pending.pop_front() {
        if !visited.insert(module_id.clone()) {
            continue;
        }

        let mut record = load_module_record(&module_id, sources)?;
        for import in &record.imports {
            let resolved = resolve_import(&module_id, import, sources)?;
            record.resolved_imports.push(ResolvedImport {
                specifier: import.specifier.clone(),
                kind: import.kind,
                module_id: resolved.clone(),
            });
            if !visited.contains(&resolved) {
                pending.push_back(resolved);
            }
        }

        order.push(module_id.clone());
        modules.insert(module_id, record);
    }

    Ok(SourceModuleGraph {
        root_dir: root_dir.to_path_buf(),
        entry_path: entry_path.to_path_buf(),
        modules,
        order,
    })
}

fn load_module_record(
    module_id: &SourceModuleId,
    sources: &BTreeMap<PathBuf, String>,
) -> Result<SourceModuleRecord, String> {
    match module_id {
        SourceModuleId::Virtual(specifier) => Ok(SourceModuleRecord {
            id: SourceModuleId::Virtual(specifier.clone()),
            kind: SourceModuleKind::Virtual,
            imports: Vec::new(),
            resolved_imports: Vec::new(),
        }),
        SourceModuleId::File(path) => {
            let source = sources
                .get(path)
                .ok_or_else(|| format!("missing source file {}", path.display()))?;
            let kind = if path.extension().and_then(|ext| ext.to_str()) == Some("css") {
                SourceModuleKind::Stylesheet
            } else {
                SourceModuleKind::Script
            };
            let imports = if kind == SourceModuleKind::Script {
                parse_imports(path, source)?
            } else {
                Vec::new()
            };

            Ok(SourceModuleRecord {
                id: SourceModuleId::File(path.clone()),
                kind,
                imports,
                resolved_imports: Vec::new(),
            })
        }
    }
}

fn parse_imports(path: &Path, source: &str) -> Result<Vec<ImportedModule>, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path)
        .map_err(|_| format!("failed to infer source type for {}", path.display()))?;
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return Err(format!("module {} has parse errors", path.display()));
    }

    let mut imports = Vec::new();
    for statement in &parsed.program.body {
        match statement {
            Statement::ImportDeclaration(decl) => {
                imports.push(classify_import_specifier(decl.source.value.as_str()));
            }
            Statement::ExportNamedDeclaration(decl) => {
                if let Some(source) = &decl.source {
                    imports.push(classify_import_specifier(source.value.as_str()));
                }
            }
            Statement::ExportAllDeclaration(decl) => {
                imports.push(classify_import_specifier(decl.source.value.as_str()));
            }
            _ => {}
        }
    }

    Ok(imports)
}

fn classify_import_specifier(specifier: &str) -> ImportedModule {
    let kind = if is_virtual_sdk_specifier(specifier) {
        ImportedModuleKind::Virtual
    } else if specifier.ends_with(".css") {
        ImportedModuleKind::Stylesheet
    } else {
        ImportedModuleKind::Script
    };

    ImportedModule {
        specifier: specifier.to_string(),
        kind,
    }
}

fn resolve_import(
    from: &SourceModuleId,
    import: &ImportedModule,
    sources: &BTreeMap<PathBuf, String>,
) -> Result<SourceModuleId, String> {
    if matches!(import.kind, ImportedModuleKind::Virtual) {
        return Ok(SourceModuleId::Virtual(import.specifier.clone()));
    }

    let from_path = match from {
        SourceModuleId::File(path) => path,
        SourceModuleId::Virtual(_) => {
            return Ok(SourceModuleId::Virtual(import.specifier.clone()));
        }
    };

    if !import.specifier.starts_with('.') && !import.specifier.starts_with('/') {
        return Err(format!(
            "unsupported external import {} from {}",
            import.specifier,
            from_path.display()
        ));
    }

    let resolved_path = resolve_source_path(
        from_path.parent().unwrap_or_else(|| Path::new(WORKSPACE_FS_ROOT)),
        &import.specifier,
        sources,
    )?;

    Ok(SourceModuleId::File(resolved_path))
}

fn resolve_source_path(
    from_dir: &Path,
    specifier: &str,
    sources: &BTreeMap<PathBuf, String>,
) -> Result<PathBuf, String> {
    let base = if specifier.starts_with('/') {
        PathBuf::from(specifier)
    } else {
        normalize_path(&from_dir.join(specifier))
    };

    for candidate in resolution_candidates(&base) {
        if sources.contains_key(&candidate) {
            return Ok(candidate);
        }
    }

    Err(format!("failed to resolve {} from {}", specifier, from_dir.display()))
}

fn resolution_candidates(base: &Path) -> Vec<PathBuf> {
    const EXTENSIONS: [&str; 6] = ["ts", "tsx", "js", "jsx", "json", "css"];

    let mut candidates = Vec::new();
    let base = normalize_path(base);
    let has_extension = base.extension().is_some();

    candidates.push(base.clone());
    if !has_extension {
        for extension in EXTENSIONS {
            candidates.push(base.with_extension(extension));
        }
        for extension in EXTENSIONS {
            candidates.push(base.join(format!("index.{extension}")));
        }
    }

    candidates
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn compiled_module_graph(
    graph: &SourceModuleGraph,
    sources: &BTreeMap<PathBuf, String>,
) -> Result<JavaScriptModuleGraph, String> {
    let mut modules = Vec::new();
    let mut compiled_scripts = BTreeMap::new();

    for module_id in &graph.order {
        let SourceModuleId::File(path) = module_id else {
            continue;
        };

        if path.extension().and_then(|extension| extension.to_str()) == Some("css") {
            continue;
        }

        let source = sources
            .get(path)
            .ok_or_else(|| format!("source for {} is unavailable", path.display()))?;
        let compiled = compile_script(path, source)?;
        compiled_scripts.insert(path.clone(), compiled);
    }

    for module_id in &graph.order {
        let Some(record) = graph.modules.get(module_id) else {
            continue;
        };
        if !matches!(record.kind, SourceModuleKind::Script | SourceModuleKind::Virtual) {
            continue;
        }

        let source = match module_id {
            SourceModuleId::File(path) => compiled_scripts
                .get(path)
                .cloned()
                .ok_or_else(|| format!("compiled output for {} is unavailable", path.display()))?,
            SourceModuleId::Virtual(specifier) => read_virtual_module_source(specifier)?,
        };
        let mut resolved_imports = record
            .resolved_imports
            .iter()
            .filter(|import| !matches!(import.kind, ImportedModuleKind::Stylesheet))
            .map(|import| {
                (
                    import.specifier.clone(),
                    module_key(&graph.root_dir, &import.module_id),
                )
            })
            .collect::<BTreeMap<_, _>>();

        if matches!(module_id, SourceModuleId::File(path) if matches!(path.extension().and_then(|extension| extension.to_str()), Some("tsx" | "jsx"))) {
            resolved_imports.insert(
                "@spiders-wm/sdk/jsx-runtime".to_string(),
                "@spiders-wm/sdk/jsx-runtime".to_string(),
            );
        }

        modules.push(JavaScriptModule {
            specifier: module_key(&graph.root_dir, module_id),
            source,
            resolved_imports,
        });
    }

    if !modules
        .iter()
        .any(|module| module.specifier == "@spiders-wm/sdk/jsx-runtime")
    {
        modules.push(JavaScriptModule {
            specifier: "@spiders-wm/sdk/jsx-runtime".to_string(),
            source: read_virtual_module_source("@spiders-wm/sdk/jsx-runtime")?,
            resolved_imports: BTreeMap::new(),
        });
    }

    Ok(JavaScriptModuleGraph {
        entry: module_key(&graph.root_dir, &SourceModuleId::File(graph.entry_path.clone())),
        modules,
    })
}

fn compile_script(path: &Path, source: &str) -> Result<String, String> {
    let source_type = SourceType::from_path(path)
        .map_err(|_| format!("failed to infer source type for {}", path.display()))?;
    let injected_source = if matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("tsx" | "jsx")
    ) {
        format!("import {{ sp, Fragment }} from \"@spiders-wm/sdk/jsx-runtime\";\n{source}")
    } else {
        source.to_string()
    };
    let mut compiler = AppScriptCompiler::default();
    let compiled = compiler
        .execute(&injected_source, source_type, path)
        .map_err(|_| format!("failed to transpile {}", path.display()))?;

    strip_stylesheet_imports(path, &compiled)
}

#[derive(Default)]
struct AppScriptCompiler {
    printed: String,
    errors: Vec<OxcDiagnostic>,
    transform: TransformOptions,
}

impl AppScriptCompiler {
    fn execute(
        &mut self,
        source_text: &str,
        source_type: SourceType,
        source_path: &Path,
    ) -> Result<String, Vec<OxcDiagnostic>> {
        if self.transform.jsx.pragma.is_none() {
            self.transform.jsx.runtime = JsxRuntime::Classic;
            self.transform.jsx.pragma = Some("sp".into());
            self.transform.jsx.pragma_frag = Some("Fragment".into());
        }
        self.compile(source_text, source_type, source_path);
        if self.errors.is_empty() {
            Ok(mem::take(&mut self.printed))
        } else {
            Err(mem::take(&mut self.errors))
        }
    }
}

impl CompilerInterface for AppScriptCompiler {
    fn handle_errors(&mut self, errors: Vec<OxcDiagnostic>) {
        self.errors.extend(errors);
    }

    fn transform_options(&self) -> Option<&TransformOptions> {
        Some(&self.transform)
    }

    fn after_codegen(&mut self, ret: CodegenReturn) {
        self.printed = ret.code;
    }
}

fn strip_stylesheet_imports(path: &Path, source: &str) -> Result<String, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path)
        .map_err(|_| format!("failed to infer source type for {}", path.display()))?;
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return Err(format!("module {} has parse errors", path.display()));
    }

    let mut out = String::new();
    let mut cursor = 0usize;
    for statement in &parsed.program.body {
        let span = statement.span();
        let start = span.start as usize;
        let end = span.end as usize;
        out.push_str(&source[cursor..start]);
        match statement {
            Statement::ImportDeclaration(decl) if decl.source.value.as_str().ends_with(".css") => {
            }
            _ => out.push_str(&source[start..end]),
        }
        cursor = end;
    }
    out.push_str(&source[cursor..]);

    Ok(out)
}

fn read_virtual_module_source(specifier: &str) -> Result<String, String> {
    match specifier {
        "@spiders-wm/sdk/commands" => {
            Ok(include_str!("../../../packages/spiders-wm-sdk/src/commands.js").to_string())
        }
        "@spiders-wm/sdk/config" => Ok(
            include_str!("../../../crates/runtimes/js/src/virtual/config.js").to_string(),
        ),
        "@spiders-wm/sdk/jsx-runtime" => {
            Ok(include_str!("../../../packages/spiders-wm-sdk/src/jsx-runtime.js").to_string())
        }
        "@spiders-wm/sdk/layout" => Ok(
            include_str!("../../../crates/runtimes/js/src/virtual/layout.js").to_string(),
        ),
        "@spiders-wm/sdk/api" => Ok(
            include_str!("../../../crates/runtimes/js/src/virtual/api.js").to_string(),
        ),
        _ => Err(format!("unsupported virtual module {specifier}")),
    }
}

fn module_key(root_dir: &Path, module_id: &SourceModuleId) -> String {
    match module_id {
        SourceModuleId::File(path) => path
            .strip_prefix(root_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/"),
        SourceModuleId::Virtual(specifier) => specifier.clone(),
    }
}

fn is_virtual_sdk_specifier(specifier: &str) -> bool {
    specifier.starts_with("@spiders-wm/sdk/")
}

fn layout_entry_file_id(layout: PreviewLayoutId) -> EditorFileId {
    match layout {
        PreviewLayoutId::MasterStack => EditorFileId::LayoutTsx,
        PreviewLayoutId::FocusRepro => EditorFileId::FocusReproLayoutTsx,
    }
}

fn js_error_to_string(error: JsValue) -> String {
    error
        .as_string()
        .unwrap_or_else(|| format!("{error:?}"))
}
