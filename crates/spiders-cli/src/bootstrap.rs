#[derive(Debug)]
pub struct CliBootstrap {
    pub paths: spiders_config::model::ConfigPaths,
    pub service: spiders_config::authoring_layout::AuthoringLayoutService<
        spiders_runtime_js::DefaultLayoutRuntime,
    >,
}

pub fn build_bootstrap(
    options: spiders_config::model::ConfigDiscoveryOptions,
) -> Result<CliBootstrap, spiders_config::model::LayoutConfigError> {
    let paths = spiders_config::model::ConfigPaths::discover(options)?;
    let service = spiders_runtime_js::build_authoring_layout_service(&paths);

    Ok(CliBootstrap { paths, service })
}
