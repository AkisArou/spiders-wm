use spiders_shared::runtime::runtime_contract::AuthoringLayoutRuntime;
use spiders_shared::runtime::runtime_error::{RuntimeError, RuntimeRefreshSummary};

use crate::authoring_layout::AuthoringLayoutServiceError;
use crate::model::{Config, ConfigPaths};

pub(super) fn load_config_with_cache_update<R>(
    runtime: &R,
    paths: &ConfigPaths,
) -> Result<(Config, Option<RuntimeRefreshSummary>), AuthoringLayoutServiceError>
where
    R: AuthoringLayoutRuntime<Config = Config>,
{
    if paths.authored_config.exists() {
        let update =
            runtime.refresh_prepared_config(&paths.authored_config, &paths.prepared_config)?;
        Ok((
            runtime.load_prepared_config(&paths.prepared_config)?,
            Some(update),
        ))
    } else if paths.prepared_config.exists() {
        Ok((runtime.load_prepared_config(&paths.prepared_config)?, None))
    } else {
        Ok((runtime.load_authored_config(&paths.authored_config)?, None))
    }
}

pub(super) fn load_authored_config<R>(
    runtime: &R,
    paths: &ConfigPaths,
) -> Result<Config, AuthoringLayoutServiceError>
where
    R: AuthoringLayoutRuntime<Config = Config>,
{
    Ok(runtime.load_authored_config(&paths.authored_config)?)
}

pub(super) fn write_prepared_config<R>(
    runtime: &R,
    paths: &ConfigPaths,
) -> Result<RuntimeRefreshSummary, AuthoringLayoutServiceError>
where
    R: AuthoringLayoutRuntime<Config = Config>,
{
    Ok(runtime.rebuild_prepared_config(&paths.authored_config, &paths.prepared_config)?)
}

pub(super) fn reload_config<R>(
    runtime: &R,
    paths: Option<&ConfigPaths>,
) -> Result<Config, AuthoringLayoutServiceError>
where
    R: AuthoringLayoutRuntime<Config = Config>,
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
