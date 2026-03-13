#[derive(Debug)]
pub struct CliBootstrap {
    pub paths: spiders_config::model::ConfigPaths,
    pub service: spiders_config::service::ConfigRuntimeService<
        spiders_config::loader::RuntimeProjectLayoutSourceLoader,
        spiders_config::runtime::BoaLayoutRuntime<
            spiders_config::loader::RuntimeProjectLayoutSourceLoader,
        >,
    >,
}

pub fn build_bootstrap(
    options: spiders_config::model::ConfigDiscoveryOptions,
) -> Result<CliBootstrap, spiders_config::model::LayoutConfigError> {
    let paths = spiders_config::model::ConfigPaths::discover(options)?;
    let resolver = spiders_config::loader::RuntimePathResolver::new(
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
    let loader = spiders_config::loader::RuntimeProjectLayoutSourceLoader::new(resolver);
    let runtime = spiders_config::runtime::BoaLayoutRuntime::with_loader(loader.clone());
    let service = spiders_config::service::ConfigRuntimeService::new(loader, runtime);

    Ok(CliBootstrap { paths, service })
}
