#[derive(Debug)]
pub struct CliBootstrap {
    pub paths: spiders_config::model::ConfigPaths,
    pub service: spiders_config::authoring_layout::AuthoringLayoutService<
        spiders_runtime_js::runtime::BoaPreparedLayoutRuntime<
            spiders_runtime_js::loader::RuntimeProjectLayoutSourceLoader,
        >,
    >,
}

pub fn build_bootstrap(
    options: spiders_config::model::ConfigDiscoveryOptions,
) -> Result<CliBootstrap, spiders_config::model::LayoutConfigError> {
    let paths = spiders_config::model::ConfigPaths::discover(options)?;
    let resolver = spiders_runtime_js::loader::RuntimePathResolver::new(
        paths
            .authored_config
            .parent()
            .and_then(|dir| dir.parent())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from(".")),
        paths
            .runtime_config
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from(".")),
    );
    let loader = spiders_runtime_js::loader::RuntimeProjectLayoutSourceLoader::new(resolver);
    let runtime = spiders_runtime_js::runtime::BoaPreparedLayoutRuntime::with_loader(loader.clone());
    let service = spiders_config::authoring_layout::AuthoringLayoutService::new(runtime);

    Ok(CliBootstrap { paths, service })
}
