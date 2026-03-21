use anyhow::Result;
use spiders_config::model::ConfigDiscoveryOptions;
use spiders_river::SpidersRiver;

fn main() -> Result<()> {
    let runtime = SpidersRiver::discover(ConfigDiscoveryOptions::default())?;
    let mut connection = runtime.connect()?;

    while connection.is_running() {
        connection.blocking_dispatch()?;
    }

    Ok(())
}
