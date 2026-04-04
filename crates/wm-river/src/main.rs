use anyhow::Result;
use spiders_config::model::config_discovery_options_from_env;
use spiders_wm_river::SpidersWm;

fn main() -> Result<()> {
    spiders_logging::init("spiders_wm_river");

    let runtime = SpidersWm::discover(config_discovery_options_from_env())?;
    let mut connection = runtime.connect()?;

    while connection.is_running() {
        connection.blocking_dispatch()?;
    }

    Ok(())
}
