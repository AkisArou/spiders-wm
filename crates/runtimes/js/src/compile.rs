use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::mem;
use std::path::Path;
use std::path::PathBuf;

use oxc::allocator::Allocator;
use oxc::ast::ast::{
    BindingPattern, Declaration, ImportDeclarationSpecifier, ModuleExportName, Statement,
};
use oxc::codegen::CodegenReturn;
use oxc::diagnostics::OxcDiagnostic;
use oxc::parser::Parser;
use oxc::span::GetSpan;
use oxc::span::SourceType;
use oxc::transformer::{JsxRuntime, TransformOptions};
use oxc::CompilerInterface;

use crate::graph::{ImportedModuleKind, ModuleGraph, ModuleId, ModuleKind, ModuleRecord};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppBuildPlan {
    pub script_modules: Vec<PathBuf>,
    pub stylesheet_modules: Vec<PathBuf>,
    pub virtual_modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledApp {
    pub scripts: Vec<CompiledScriptModule>,
    pub stylesheet: String,
    pub virtual_modules: Vec<CompiledVirtualModule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledScriptModule {
    pub path: PathBuf,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledVirtualModule {
    pub specifier: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledApp {
    pub javascript: String,
    pub stylesheet: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CompileError {
    #[error("failed to read script module `{path}`")]
    ReadScript { path: PathBuf },
    #[error("failed to infer source type for `{path}`")]
    UnsupportedSourceType { path: PathBuf },
    #[error("failed to transpile `{path}`")]
    Transpile { path: PathBuf },
    #[error("failed to read stylesheet `{path}`")]
    ReadStylesheet { path: PathBuf },
    #[error("unsupported virtual module `{specifier}`")]
    UnsupportedVirtualModule { specifier: String },
    #[error("failed to read virtual module source `{path}`")]
    ReadVirtualModule { path: PathBuf },
    #[error("compiled output for module `{module}` is unavailable")]
    MissingCompiledModule { module: String },
    #[error("bundler failed to parse module `{module}`")]
    ParseBundledModule { module: String },
    #[error("bundler could not resolve import `{specifier}` in `{module}`")]
    MissingResolvedImport { module: String, specifier: String },
}

impl AppBuildPlan {
    pub fn from_graph(graph: &ModuleGraph) -> Self {
        let mut script_modules = Vec::new();
        let mut stylesheet_modules = Vec::new();
        let mut virtual_modules = Vec::new();
        let mut seen_stylesheets = BTreeSet::new();
        let mut needs_jsx_runtime = false;

        if let Some(stylesheet_path) = graph.app.stylesheet_path.as_ref() {
            if seen_stylesheets.insert(stylesheet_path.clone()) {
                stylesheet_modules.push(stylesheet_path.clone());
            }
        }

        for module_id in &graph.order {
            let Some(module) = graph.modules.get(module_id) else {
                continue;
            };

            match (&module.id, module.kind) {
                (ModuleId::File(path), ModuleKind::Script) => {
                    if matches!(
                        path.extension().and_then(|extension| extension.to_str()),
                        Some("tsx" | "jsx")
                    ) {
                        needs_jsx_runtime = true;
                    }
                    script_modules.push(path.clone())
                }
                (ModuleId::File(path), ModuleKind::Stylesheet) => {
                    if seen_stylesheets.insert(path.clone()) {
                        stylesheet_modules.push(path.clone());
                    }
                }
                (ModuleId::Virtual(name), ModuleKind::Virtual) => {
                    virtual_modules.push(name.clone())
                }
                _ => {}
            }
        }

        if needs_jsx_runtime
            && !virtual_modules
                .iter()
                .any(|name| name == "spider-wm/jsx-runtime")
        {
            virtual_modules.push("spider-wm/jsx-runtime".into());
        }

        Self {
            script_modules,
            stylesheet_modules,
            virtual_modules,
        }
    }
}

struct AppScriptCompiler {
    printed: String,
    errors: Vec<OxcDiagnostic>,
    transform: TransformOptions,
}

impl Default for AppScriptCompiler {
    fn default() -> Self {
        let mut transform = TransformOptions::default();
        transform.jsx.runtime = JsxRuntime::Classic;
        transform.jsx.pragma = Some("sp".into());
        transform.jsx.pragma_frag = Some("Fragment".into());

        Self {
            printed: String::new(),
            errors: Vec::new(),
            transform,
        }
    }
}

impl AppScriptCompiler {
    fn execute(
        &mut self,
        source_text: &str,
        source_type: SourceType,
        source_path: &std::path::Path,
    ) -> Result<String, Vec<OxcDiagnostic>> {
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

pub fn compile_app(plan: &AppBuildPlan) -> Result<CompiledApp, CompileError> {
    let mut scripts = Vec::new();
    for path in &plan.script_modules {
        let source = std::fs::read_to_string(path)
            .map_err(|_| CompileError::ReadScript { path: path.clone() })?;
        let source_type = SourceType::from_path(path)
            .map_err(|_| CompileError::UnsupportedSourceType { path: path.clone() })?;
        let mut compiler = AppScriptCompiler::default();
        let injected_source = if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("tsx" | "jsx")
        ) {
            format!("import {{ sp, Fragment }} from \"spider-wm/jsx-runtime\";\n{source}")
        } else {
            source.clone()
        };
        let code = compiler
            .execute(&injected_source, source_type, path)
            .map_err(|_| CompileError::Transpile { path: path.clone() })?;
        scripts.push(CompiledScriptModule {
            path: path.clone(),
            code,
        });
    }

    let mut stylesheet_chunks = Vec::new();
    for path in &plan.stylesheet_modules {
        let source = std::fs::read_to_string(path)
            .map_err(|_| CompileError::ReadStylesheet { path: path.clone() })?;
        stylesheet_chunks.push(source);
    }

    let virtual_modules = plan
        .virtual_modules
        .iter()
        .map(|specifier| {
            Ok(CompiledVirtualModule {
                specifier: specifier.clone(),
                code: read_virtual_module_source(specifier)?,
            })
        })
        .collect::<Result<Vec<_>, CompileError>>()?;

    Ok(CompiledApp {
        scripts,
        stylesheet: stylesheet_chunks.join("\n"),
        virtual_modules,
    })
}

pub fn bundle_app(graph: &ModuleGraph, compiled: &CompiledApp) -> Result<BundledApp, CompileError> {
    let compiled_scripts = compiled
        .scripts
        .iter()
        .map(|module| (ModuleId::File(module.path.clone()), module.code.clone()))
        .collect::<BTreeMap<_, _>>();
    let compiled_virtuals = compiled
        .virtual_modules
        .iter()
        .map(|module| {
            (
                ModuleId::Virtual(module.specifier.clone()),
                module.code.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut module_factories = Vec::new();
    for module_id in &graph.order {
        let Some(record) = graph.modules.get(module_id) else {
            continue;
        };
        if !matches!(record.kind, ModuleKind::Script | ModuleKind::Virtual) {
            continue;
        }

        let code = match module_id {
            ModuleId::File(_) => compiled_scripts.get(module_id),
            ModuleId::Virtual(_) => compiled_virtuals.get(module_id),
        }
        .ok_or_else(|| CompileError::MissingCompiledModule {
            module: module_key(&graph.app.root_dir, module_id),
        })?;

        let rewritten = rewrite_module_code(code, module_id, record, &graph.app.root_dir)?;
        let key = serde_json::to_string(&module_key(&graph.app.root_dir, module_id)).unwrap();
        module_factories.push(format!(
            "{key}: (module, exports, __require) => {{\n{rewritten}\n}}"
        ));
    }

    for (module_id, code) in &compiled_virtuals {
        if graph.modules.contains_key(module_id) {
            continue;
        }
        let record = ModuleRecord {
            id: module_id.clone(),
            kind: ModuleKind::Virtual,
            imports: Vec::new(),
            resolved_imports: Vec::new(),
        };
        let rewritten = rewrite_module_code(code, module_id, &record, &graph.app.root_dir)?;
        let key = serde_json::to_string(&module_key(&graph.app.root_dir, module_id)).unwrap();
        module_factories.push(format!(
            "{key}: (module, exports, __require) => {{\n{rewritten}\n}}"
        ));
    }

    let entry_key = serde_json::to_string(&module_key(
        &graph.app.root_dir,
        &ModuleId::File(graph.app.entry_path.clone()),
    ))
    .unwrap();

    let javascript = format!(
        "(() => {{\nconst __modules = {{\n{}\n}};\nconst __cache = Object.create(null);\nconst __require = (id) => {{\n  if (Object.prototype.hasOwnProperty.call(__cache, id)) {{\n    return __cache[id].exports;\n  }}\n  const factory = __modules[id];\n  if (!factory) {{\n    throw new Error(`unknown bundled module ${{id}}`);\n  }}\n  const module = {{ exports: {{}} }};\n  __cache[id] = module;\n  factory(module, module.exports, __require);\n  return module.exports;\n}};\nreturn __require({entry_key}).default;\n}})()",
        module_factories.join(",\n")
    );

    Ok(BundledApp {
        javascript,
        stylesheet: compiled.stylesheet.clone(),
    })
}

fn read_virtual_module_source(specifier: &str) -> Result<String, CompileError> {
    let relative_path = match specifier {
        "spider-wm/actions" => PathBuf::from("sdk/actions.js"),
        "spider-wm/config" => PathBuf::from("src/virtual/config.js"),
        "spider-wm/jsx-runtime" => PathBuf::from("sdk/jsx-runtime.js"),
        "spider-wm/layout" => PathBuf::from("src/virtual/layout.js"),
        "spider-wm/api" => PathBuf::from("src/virtual/api.js"),
        _ => {
            return Err(CompileError::UnsupportedVirtualModule {
                specifier: specifier.into(),
            });
        }
    };

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    std::fs::read_to_string(&path).map_err(|_| CompileError::ReadVirtualModule { path })
}

fn rewrite_module_code(
    source: &str,
    module_id: &ModuleId,
    record: &ModuleRecord,
    root_dir: &Path,
) -> Result<String, CompileError> {
    let allocator = Allocator::default();
    let source_type = match module_id {
        ModuleId::File(path) => {
            SourceType::from_path(path).map_err(|_| CompileError::ParseBundledModule {
                module: module_key(root_dir, module_id),
            })?
        }
        ModuleId::Virtual(_) => SourceType::from_path(Path::new("virtual.js")).unwrap(),
    };
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return Err(CompileError::ParseBundledModule {
            module: module_key(root_dir, module_id),
        });
    }

    let mut out = String::new();
    let mut cursor = 0usize;
    let mut temp_index = 0usize;

    for statement in &parsed.program.body {
        let span = statement.span();
        let start = span.start as usize;
        let end = span.end as usize;
        out.push_str(&source[cursor..start]);

        match statement {
            Statement::ImportDeclaration(decl) => {
                out.push_str(&rewrite_import_declaration(
                    decl,
                    record,
                    root_dir,
                    module_id,
                    &mut temp_index,
                )?);
            }
            Statement::ExportDefaultDeclaration(decl) => {
                out.push_str(&rewrite_export_default_declaration(decl, source));
            }
            Statement::ExportNamedDeclaration(decl) => {
                out.push_str(&rewrite_export_named_declaration(
                    decl,
                    source,
                    record,
                    root_dir,
                    module_id,
                    &mut temp_index,
                )?);
            }
            Statement::ExportAllDeclaration(decl) => {
                out.push_str(&rewrite_export_all_declaration(
                    decl,
                    record,
                    root_dir,
                    module_id,
                    &mut temp_index,
                )?);
            }
            _ => out.push_str(&source[start..end]),
        }

        cursor = end;
    }

    out.push_str(&source[cursor..]);
    Ok(out)
}

fn rewrite_import_declaration(
    decl: &oxc::ast::ast::ImportDeclaration<'_>,
    record: &ModuleRecord,
    root_dir: &Path,
    module_id: &ModuleId,
    temp_index: &mut usize,
) -> Result<String, CompileError> {
    let Some(resolved) = resolve_import_module(record, decl.source.value.as_str()) else {
        return Err(CompileError::MissingResolvedImport {
            module: module_key(root_dir, module_id),
            specifier: decl.source.value.to_string(),
        });
    };
    if matches!(resolved.kind, ImportedModuleKind::Stylesheet) {
        return Ok(String::new());
    }

    let required_key = serde_json::to_string(&module_key(root_dir, &resolved.module_id)).unwrap();
    let Some(specifiers) = decl.specifiers.as_ref() else {
        return Ok(format!("__require({required_key});"));
    };
    if specifiers.is_empty() {
        return Ok(format!("__require({required_key});"));
    }

    let temp_name = next_temp(temp_index, "import");
    let mut lines = vec![format!("const {temp_name} = __require({required_key});")];
    for specifier in specifiers {
        match specifier {
            ImportDeclarationSpecifier::ImportDefaultSpecifier(spec) => {
                lines.push(format!(
                    "const {} = {temp_name}.default;",
                    spec.local.name.as_str()
                ));
            }
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(spec) => {
                lines.push(format!("const {} = {temp_name};", spec.local.name.as_str()));
            }
            ImportDeclarationSpecifier::ImportSpecifier(spec) => {
                lines.push(format!(
                    "const {} = {temp_name}{};",
                    spec.local.name.as_str(),
                    module_export_accessor(&spec.imported)
                ));
            }
        }
    }

    Ok(lines.join("\n"))
}

fn rewrite_export_default_declaration(
    decl: &oxc::ast::ast::ExportDefaultDeclaration<'_>,
    source: &str,
) -> String {
    let declaration_source = span_slice(source, decl.declaration.span());
    format!("module.exports.default = {declaration_source};")
}

fn rewrite_export_named_declaration(
    decl: &oxc::ast::ast::ExportNamedDeclaration<'_>,
    source: &str,
    record: &ModuleRecord,
    root_dir: &Path,
    module_id: &ModuleId,
    temp_index: &mut usize,
) -> Result<String, CompileError> {
    if let Some(declaration) = &decl.declaration {
        let declaration_source = span_slice(source, declaration.span());
        let mut lines = vec![declaration_source.to_owned()];
        for binding in declaration_binding_names(declaration) {
            lines.push(format!("module.exports.{binding} = {binding};"));
        }
        return Ok(lines.join("\n"));
    }

    if let Some(source_module) = &decl.source {
        let Some(resolved) = resolve_import_module(record, source_module.value.as_str()) else {
            return Err(CompileError::MissingResolvedImport {
                module: module_key(root_dir, module_id),
                specifier: source_module.value.to_string(),
            });
        };
        let required_key =
            serde_json::to_string(&module_key(root_dir, &resolved.module_id)).unwrap();
        let temp_name = next_temp(temp_index, "reexport");
        let mut lines = vec![format!("const {temp_name} = __require({required_key});")];
        for specifier in &decl.specifiers {
            let exported = module_exports_accessor(&specifier.exported);
            let local = module_export_accessor(&specifier.local);
            lines.push(format!("module.exports{exported} = {temp_name}{local};"));
        }
        return Ok(lines.join("\n"));
    }

    let mut lines = Vec::new();
    for specifier in &decl.specifiers {
        let exported = module_exports_accessor(&specifier.exported);
        let local = module_export_name(&specifier.local);
        lines.push(format!("module.exports{exported} = {local};"));
    }
    Ok(lines.join("\n"))
}

fn rewrite_export_all_declaration(
    decl: &oxc::ast::ast::ExportAllDeclaration<'_>,
    record: &ModuleRecord,
    root_dir: &Path,
    module_id: &ModuleId,
    temp_index: &mut usize,
) -> Result<String, CompileError> {
    let Some(resolved) = resolve_import_module(record, decl.source.value.as_str()) else {
        return Err(CompileError::MissingResolvedImport {
            module: module_key(root_dir, module_id),
            specifier: decl.source.value.to_string(),
        });
    };
    let required_key = serde_json::to_string(&module_key(root_dir, &resolved.module_id)).unwrap();
    let temp_name = next_temp(temp_index, "export_all");
    if let Some(exported) = &decl.exported {
        let exported_name = module_exports_accessor(exported);
        return Ok(format!(
            "module.exports{exported_name} = __require({required_key});"
        ));
    }

    Ok(format!(
        "const {temp_name} = __require({required_key});\nfor (const key in {temp_name}) {{\n  if (key !== \"default\") {{\n    module.exports[key] = {temp_name}[key];\n  }}\n}}"
    ))
}

fn resolve_import_module(
    record: &ModuleRecord,
    specifier: &str,
) -> Option<crate::graph::ResolvedImport> {
    record
        .resolved_imports
        .iter()
        .find(|import| import.specifier == specifier)
        .cloned()
        .or_else(|| {
            specifier
                .starts_with("spider-wm/")
                .then(|| crate::graph::ResolvedImport {
                    specifier: specifier.to_owned(),
                    kind: ImportedModuleKind::Virtual,
                    module_id: ModuleId::Virtual(specifier.to_owned()),
                })
        })
}

fn declaration_binding_names(declaration: &Declaration<'_>) -> Vec<String> {
    match declaration {
        Declaration::VariableDeclaration(decl) => decl
            .declarations
            .iter()
            .flat_map(|declarator| binding_pattern_names(&declarator.id))
            .collect(),
        Declaration::FunctionDeclaration(decl) => decl
            .id
            .as_ref()
            .map(|id| vec![id.name.as_str().to_owned()])
            .unwrap_or_default(),
        Declaration::ClassDeclaration(decl) => decl
            .id
            .as_ref()
            .map(|id| vec![id.name.as_str().to_owned()])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn binding_pattern_names(pattern: &BindingPattern<'_>) -> Vec<String> {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => vec![identifier.name.as_str().to_owned()],
        BindingPattern::AssignmentPattern(pattern) => binding_pattern_names(&pattern.left),
        BindingPattern::ObjectPattern(pattern) => {
            let mut names = pattern
                .properties
                .iter()
                .flat_map(|property| binding_pattern_names(&property.value))
                .collect::<Vec<_>>();
            if let Some(rest) = &pattern.rest {
                names.extend(binding_pattern_names(&rest.argument));
            }
            names
        }
        BindingPattern::ArrayPattern(pattern) => {
            let mut names = pattern
                .elements
                .iter()
                .flatten()
                .flat_map(binding_pattern_names)
                .collect::<Vec<_>>();
            if let Some(rest) = &pattern.rest {
                names.extend(binding_pattern_names(&rest.argument));
            }
            names
        }
    }
}

fn module_export_accessor(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(name) => format!(".{}", name.name.as_str()),
        ModuleExportName::IdentifierReference(name) => format!(".{}", name.name.as_str()),
        ModuleExportName::StringLiteral(name) => {
            format!("[{}]", serde_json::to_string(name.value.as_str()).unwrap())
        }
    }
}

fn module_export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(name) => name.name.as_str().to_owned(),
        ModuleExportName::IdentifierReference(name) => name.name.as_str().to_owned(),
        ModuleExportName::StringLiteral(name) => {
            serde_json::to_string(name.value.as_str()).unwrap()
        }
    }
}

fn module_exports_accessor(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(name) => format!(".{}", name.name.as_str()),
        ModuleExportName::IdentifierReference(name) => format!(".{}", name.name.as_str()),
        ModuleExportName::StringLiteral(name) => {
            format!("[{}]", serde_json::to_string(name.value.as_str()).unwrap())
        }
    }
}

fn module_key(root_dir: &Path, module_id: &ModuleId) -> String {
    match module_id {
        ModuleId::File(path) => path
            .strip_prefix(root_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/"),
        ModuleId::Virtual(specifier) => specifier.clone(),
    }
}

fn span_slice(source: &str, span: oxc::span::Span) -> &str {
    &source[span.start as usize..span.end as usize]
}

fn next_temp(index: &mut usize, prefix: &str) -> String {
    let current = *index;
    *index += 1;
    format!("__{prefix}_{current}")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::graph::{discover_project_apps, ModuleGraphBuilder};

    use super::*;

    fn unique_root(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("spiders-config-compile-{name}-{unique}"))
    }

    #[test]
    fn build_plan_keeps_layout_entry_and_collects_component_css() {
        let root = unique_root("layout-plan");
        fs::create_dir_all(root.join("layouts/master-stack")).unwrap();
        fs::create_dir_all(root.join("components")).unwrap();
        fs::write(root.join("config.ts"), "export default {};").unwrap();
        fs::write(
            root.join("layouts/master-stack/index.tsx"),
            r#"
                import "./index.css";
                import { StackGroup } from "../../components/StackGroup";
                export default function layout() { return StackGroup; }
            "#,
        )
        .unwrap();
        fs::write(root.join("layouts/master-stack/index.css"), "workspace {}").unwrap();
        fs::write(
            root.join("components/StackGroup.tsx"),
            "import './StackGroup.css'; export const StackGroup = () => null;",
        )
        .unwrap();
        fs::write(root.join("components/StackGroup.css"), ".stack {}").unwrap();

        let project = discover_project_apps(root.join("config.ts")).unwrap();
        let graph = ModuleGraphBuilder::new()
            .build(&project.layout_apps[0])
            .unwrap();
        let plan = AppBuildPlan::from_graph(&graph);

        assert!(plan
            .script_modules
            .contains(&root.join("layouts/master-stack/index.tsx")));
        assert!(plan
            .script_modules
            .contains(&root.join("components/StackGroup.tsx")));
        assert!(plan
            .stylesheet_modules
            .contains(&root.join("layouts/master-stack/index.css")));
        assert!(plan
            .stylesheet_modules
            .contains(&root.join("components/StackGroup.css")));
    }

    #[test]
    fn compile_app_transpiles_scripts_and_concatenates_stylesheets() {
        let root = unique_root("compile");
        fs::create_dir_all(root.join("layouts/master-stack")).unwrap();
        fs::write(root.join("config.ts"), "export default {};").unwrap();
        fs::write(
            root.join("layouts/master-stack/index.ts"),
            "const value: number = 1; export default value;",
        )
        .unwrap();
        fs::write(root.join("layouts/master-stack/index.css"), "workspace {}").unwrap();

        let project = discover_project_apps(root.join("config.ts")).unwrap();
        let graph = ModuleGraphBuilder::new()
            .build(&project.layout_apps[0])
            .unwrap();
        let plan = AppBuildPlan::from_graph(&graph);
        let compiled = compile_app(&plan).unwrap();

        assert_eq!(compiled.scripts.len(), 1);
        assert!(compiled.scripts[0].code.contains("const value"));
        assert!(!compiled.scripts[0].code.contains(": number"));
        assert!(compiled.stylesheet.contains("workspace {}"));
    }

    #[test]
    fn compile_app_materializes_actions_virtual_module() {
        let root = unique_root("virtual-actions");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("config.ts"),
            r#"
                import { spawn } from "spider-wm/actions";
                export default { binding: spawn("foot") };
            "#,
        )
        .unwrap();

        let project = discover_project_apps(root.join("config.ts")).unwrap();
        let graph = ModuleGraphBuilder::new()
            .build(&project.config_app)
            .unwrap();
        let plan = AppBuildPlan::from_graph(&graph);
        let compiled = compile_app(&plan).unwrap();

        assert_eq!(compiled.virtual_modules.len(), 1);
        assert_eq!(compiled.virtual_modules[0].specifier, "spider-wm/actions");
        assert!(compiled.virtual_modules[0]
            .code
            .contains("export const spawn"));
    }

    #[test]
    fn compile_app_uses_spider_wm_jsx_runtime_for_tsx_modules() {
        let root = unique_root("tsx-runtime");
        fs::create_dir_all(root.join("layouts/master-stack")).unwrap();
        fs::write(root.join("config.ts"), "export default {};").unwrap();
        fs::write(
            root.join("layouts/master-stack/index.tsx"),
            r#"
                export default function layout() {
                    return <workspace id="root" />;
                }
            "#,
        )
        .unwrap();

        let project = discover_project_apps(root.join("config.ts")).unwrap();
        let graph = ModuleGraphBuilder::new()
            .build(&project.layout_apps[0])
            .unwrap();
        let plan = AppBuildPlan::from_graph(&graph);
        let compiled = compile_app(&plan).unwrap();

        assert!(compiled.scripts[0].code.contains("spider-wm/jsx-runtime"));
        assert!(!compiled.scripts[0].code.contains("<workspace"));
    }

    #[test]
    fn bundle_app_emits_executable_config_entry() {
        let root = unique_root("bundle-config");
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(
            root.join("config.ts"),
            r#"
                import { bindings } from "./config/bindings";
                export default { bindings };
            "#,
        )
        .unwrap();
        fs::write(
            root.join("config/bindings.ts"),
            r#"
                import { spawn } from "spider-wm/actions";
                export const bindings = { action: spawn("foot") };
            "#,
        )
        .unwrap();

        let project = discover_project_apps(root.join("config.ts")).unwrap();
        let graph = ModuleGraphBuilder::new()
            .build(&project.config_app)
            .unwrap();
        let plan = AppBuildPlan::from_graph(&graph);
        let compiled = compile_app(&plan).unwrap();
        let bundled = bundle_app(&graph, &compiled).unwrap();

        assert!(bundled
            .javascript
            .contains("return __require(\"config.ts\").default"));
        assert!(bundled.javascript.contains("spawn"));
        assert!(bundled.javascript.contains("module.exports.default"));
    }

    #[test]
    fn bundle_app_rewrites_layout_imports_exports_and_css_side_effects() {
        let root = unique_root("bundle-layout");
        fs::create_dir_all(root.join("layouts/master-stack")).unwrap();
        fs::create_dir_all(root.join("components")).unwrap();
        fs::write(root.join("config.ts"), "export default {};").unwrap();
        fs::write(
            root.join("layouts/master-stack/index.tsx"),
            r#"
                import "./index.css";
                import { StackGroup } from "../../components/StackGroup";
                export default function layout() {
                    return StackGroup();
                }
            "#,
        )
        .unwrap();
        fs::write(root.join("layouts/master-stack/index.css"), ".layout {}").unwrap();
        fs::write(
            root.join("components/StackGroup.ts"),
            r#"
                import "./StackGroup.css";
                export function StackGroup() {
                    return { type: "group", children: [] };
                }
            "#,
        )
        .unwrap();
        fs::write(root.join("components/StackGroup.css"), ".stack {}").unwrap();

        let project = discover_project_apps(root.join("config.ts")).unwrap();
        let graph = ModuleGraphBuilder::new()
            .build(&project.layout_apps[0])
            .unwrap();
        let plan = AppBuildPlan::from_graph(&graph);
        let compiled = compile_app(&plan).unwrap();
        let bundled = bundle_app(&graph, &compiled).unwrap();

        assert!(bundled
            .javascript
            .contains("const __import_0 = __require(\"components/StackGroup.ts\")"));
        assert!(bundled
            .javascript
            .contains("module.exports.StackGroup = StackGroup;"));
        assert!(!bundled.javascript.contains("./index.css"));
        assert!(bundled.stylesheet.contains(".layout {}"));
        assert!(bundled.stylesheet.contains(".stack {}"));
    }
}
