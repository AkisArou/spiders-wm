pub mod authored;
mod module_graph_runtime;
pub mod runtime;

use spiders_config::model::{ConfigPaths, LayoutConfigError, RuntimeKind};
use spiders_config::runtime::{RuntimeBundle, RuntimeProvider};
use spiders_core::types::SpiderPlatform;

pub use module_graph_runtime::{
    ModuleGraphRuntimeError, call_entry_export_with_json_arg,
    call_entry_export_with_json_arg_with_platform, evaluate_entry_export_to_json,
    evaluate_entry_export_to_json_with_platform, format_js_error,
};
pub use runtime::QuickJsPreparedLayoutRuntime;

pub type DefaultLayoutRuntime = runtime::QuickJsPreparedLayoutRuntime<
    spiders_runtime_js_core::loader::RuntimeProjectLayoutSourceLoader,
>;

pub fn build_default_runtime(paths: &ConfigPaths, platform: SpiderPlatform) -> DefaultLayoutRuntime {
    let resolver = spiders_runtime_js_core::loader::RuntimePathResolver::new(
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
    let loader = spiders_runtime_js_core::loader::RuntimeProjectLayoutSourceLoader::new(resolver);
    runtime::QuickJsPreparedLayoutRuntime::with_loader_for_platform(loader, platform)
}

#[derive(Debug, Clone, Copy)]
pub struct JavaScriptNativeRuntimeProvider {
    platform: SpiderPlatform,
}

impl JavaScriptNativeRuntimeProvider {
    pub const fn new(platform: SpiderPlatform) -> Self {
        Self { platform }
    }
}

impl Default for JavaScriptNativeRuntimeProvider {
    fn default() -> Self {
        Self::new(SpiderPlatform::Wayland)
    }
}

impl RuntimeProvider for JavaScriptNativeRuntimeProvider {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::JavaScript
    }

    fn build_runtime_bundle(
        &self,
        paths: &ConfigPaths,
    ) -> Result<RuntimeBundle, LayoutConfigError> {
        let runtime = build_default_runtime(paths, self.platform);
        Ok(RuntimeBundle {
            config_runtime: Box::new(runtime.clone()),
            layout_runtime: Box::new(runtime),
        })
    }
}
