use anyhow::Result;
use spiders_config::model::ConfigDiscoveryOptions;
use spiders_river::SpidersRiver;

fn main() -> Result<()> {
    spiders_logging::init("spiders_river");

    let runtime = SpidersRiver::discover(ConfigDiscoveryOptions::from_env())?;
    let mut connection = runtime.connect()?;

    while connection.is_running() {
        connection.blocking_dispatch()?;
    }

    Ok(())
}
