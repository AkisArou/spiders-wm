use std::collections::HashMap;
use std::path::{Path, PathBuf};

use spiders_runtime_js_core::compile::AppBuildPlan;
use spiders_runtime_js_core::graph::{
    AppKind, DiscoveredApp, ModuleGraphBuilder, discover_project_apps,
};
use tower_lsp::lsp_types::Url;

use crate::project::ProjectIndex;

#[derive(Debug, Default)]
pub struct WorkspaceState {
    project_index: ProjectIndex,
    open_documents: HashMap<PathBuf, String>,
}

impl WorkspaceState {
    pub fn upsert_document(&mut self, uri: &Url, source: &str) {
        let Ok(path) = uri.to_file_path() else {
            return;
        };

        self.open_documents.insert(path.clone(), source.to_string());

        if let Some(config_entry) = discover_config_entry_for_path(&path) {
            self.rebuild_project_from_config(&config_entry);
        }
    }

    pub fn remove_document(&mut self, uri: &Url) {
        let Ok(path) = uri.to_file_path() else {
            return;
        };

        self.open_documents.remove(&path);

        if let Some(config_entry) = discover_config_entry_for_path(&path) {
            self.rebuild_project_from_config(&config_entry);
        }
    }

    pub fn project_index(&self) -> &ProjectIndex {
        &self.project_index
    }

    fn rebuild_project_from_config(&mut self, config_entry: &Path) {
        let Ok(project) = discover_project_apps(config_entry) else {
            return;
        };

        let mut next_index = ProjectIndex::default();
        let graph_builder = ModuleGraphBuilder::new();

        for app in std::iter::once(&project.config_app).chain(project.layout_apps.iter()) {
            let Ok(graph) = graph_builder.build(app) else {
                continue;
            };
            let plan = AppBuildPlan::from_graph(&graph);
            let script_sources = collect_script_sources(&plan, &self.open_documents);
            let stylesheet_sources = collect_stylesheet_sources(&plan, &self.open_documents);
            let scope_id = app_scope_id(app);
            next_index.index_app_scope(scope_id, script_sources, stylesheet_sources);
        }

        self.project_index = next_index;
    }
}

fn collect_script_sources(
    plan: &AppBuildPlan,
    open_documents: &HashMap<PathBuf, String>,
) -> Vec<(PathBuf, String)> {
    plan.script_modules
        .iter()
        .filter_map(|path| read_source(path, open_documents).map(|source| (path.clone(), source)))
        .collect()
}

fn collect_stylesheet_sources(
    plan: &AppBuildPlan,
    open_documents: &HashMap<PathBuf, String>,
) -> Vec<(PathBuf, String)> {
    plan.stylesheet_modules
        .iter()
        .filter_map(|path| read_source(path, open_documents).map(|source| (path.clone(), source)))
        .collect()
}

fn read_source(path: &Path, open_documents: &HashMap<PathBuf, String>) -> Option<String> {
    if let Some(source) = open_documents.get(path) {
        return Some(source.clone());
    }
    std::fs::read_to_string(path).ok()
}

fn app_scope_id(app: &DiscoveredApp) -> PathBuf {
    match app.kind {
        AppKind::Config => app.root_dir.join("index.css"),
        AppKind::Layout => app.entry_path.clone(),
    }
}

fn discover_config_entry_for_path(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() { path.to_path_buf() } else { path.parent()?.to_path_buf() };

    loop {
        if let Some(config) = config_entry_in_dir(&current) {
            return Some(config);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn config_entry_in_dir(dir: &Path) -> Option<PathBuf> {
    ["config.tsx", "config.ts", "config.jsx", "config.js"]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_nearest_config_entry() {
        let root =
            std::env::temp_dir().join(format!("spiders-css-lsp-workspace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("layouts/master-stack/components")).unwrap();
        std::fs::write(root.join("config.ts"), "export default {};").unwrap();

        let discovered =
            discover_config_entry_for_path(&root.join("layouts/master-stack/components/Foo.tsx"))
                .unwrap();

        assert_eq!(discovered, root.join("config.ts"));
        let _ = std::fs::remove_dir_all(root);
    }
}
