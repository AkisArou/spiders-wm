use spiders_core::runtime::runtime_error::{RuntimeError, RuntimeRefreshSummary};

use crate::authoring_layout::AuthoringLayoutServiceError;
use crate::model::{Config, ConfigPaths};
use crate::runtime::AuthoringConfigRuntime;

pub(super) fn load_config_with_cache_update<C>(
    runtime: &C,
    paths: &ConfigPaths,
) -> Result<(Config, Option<RuntimeRefreshSummary>), AuthoringLayoutServiceError>
where
    C: AuthoringConfigRuntime + ?Sized,
{
    if paths.authored_config.exists() {
        let update =
            runtime.refresh_prepared_config(&paths.authored_config, &paths.prepared_config)?;
        Ok((runtime.load_prepared_config(&paths.prepared_config)?, Some(update)))
    } else if paths.prepared_config.exists() {
        Ok((runtime.load_prepared_config(&paths.prepared_config)?, None))
    } else {
        Ok((runtime.load_authored_config(&paths.authored_config)?, None))
    }
}

pub(super) fn load_authored_config<C>(
    runtime: &C,
    paths: &ConfigPaths,
) -> Result<Config, AuthoringLayoutServiceError>
where
    C: AuthoringConfigRuntime + ?Sized,
{
    Ok(runtime.load_authored_config(&paths.authored_config)?)
}

pub(super) fn write_prepared_config<C>(
    runtime: &C,
    paths: &ConfigPaths,
) -> Result<RuntimeRefreshSummary, AuthoringLayoutServiceError>
where
    C: AuthoringConfigRuntime + ?Sized,
{
    Ok(runtime.rebuild_prepared_config(&paths.authored_config, &paths.prepared_config)?)
}

pub(super) fn reload_config<C>(
    runtime: &C,
    paths: Option<&ConfigPaths>,
) -> Result<Config, AuthoringLayoutServiceError>
where
    C: AuthoringConfigRuntime + ?Sized,
{
    let Some(paths) = paths else {
        return Err(RuntimeError::Other {
            message: "prepared config reload requires configured paths".into(),
        }
        .into());
    };

    let _ = write_prepared_config(runtime, paths)?;
    Ok(runtime.load_prepared_config(&paths.prepared_config)?)
}
