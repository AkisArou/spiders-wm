pub mod authored;
pub mod compile;
pub mod graph;
pub mod loader;
mod module_graph_runtime;
pub mod runtime;

pub type DefaultLayoutRuntime =
    runtime::QuickJsPreparedLayoutRuntime<loader::RuntimeProjectLayoutSourceLoader>;

pub fn build_authoring_layout_service(
    paths: &spiders_config::model::ConfigPaths,
) -> spiders_config::authoring_layout::AuthoringLayoutService<DefaultLayoutRuntime> {
    let resolver = loader::RuntimePathResolver::new(
        paths
            .authored_config
            .parent()
            .and_then(|dir| dir.parent())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from(".")),
        paths
            .prepared_config
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from(".")),
    );
    let loader = loader::RuntimeProjectLayoutSourceLoader::new(resolver);
    let runtime = runtime::QuickJsPreparedLayoutRuntime::with_loader(loader);
    spiders_config::authoring_layout::AuthoringLayoutService::with_paths(runtime, paths.clone())
}

pub fn crate_ready() -> bool {
    true
}
