use anyhow::Result;
use spiders_config::model::ConfigDiscoveryOptions;
use spiders_wm_river::SpidersWm;

fn main() -> Result<()> {
    spiders_logging::init("spiders_wm_river");

    let runtime = SpidersWm::discover(ConfigDiscoveryOptions::from_env())?;
    let mut connection = runtime.connect()?;

    while connection.is_running() {
        connection.blocking_dispatch()?;
    }

    Ok(())
}
