#[derive(Debug)]
pub struct CliBootstrap {
    pub paths: spiders_config::model::ConfigPaths,
    pub service: spiders_config::authoring_layout::AuthoringLayoutService,
}

pub fn build_bootstrap(
    options: spiders_config::model::ConfigDiscoveryOptions,
) -> Result<CliBootstrap, spiders_config::model::LayoutConfigError> {
    let paths = spiders_config::model::ConfigPaths::discover(options)?;
    let js_provider = spiders_runtime_js_native::JavaScriptNativeRuntimeProvider;
    let service = spiders_config::runtime::build_authoring_layout_service(&paths, &[&js_provider])?;

    Ok(CliBootstrap { paths, service })
}
