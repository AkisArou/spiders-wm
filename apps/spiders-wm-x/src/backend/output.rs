use anyhow::{Context, Result};
use xcb::{randr, x};

use super::ScreenDescriptor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredOutput {
    pub(crate) output_id: spiders_core::OutputId,
    pub(crate) name: String,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) primary: bool,
}

pub(crate) fn discover_outputs(
    connection: &xcb::Connection,
    screen: &ScreenDescriptor,
) -> Result<Vec<DiscoveredOutput>> {
    if randr::get_extension_data(connection).is_none() {
        return Ok(vec![fallback_output(screen)]);
    }

    let reply: randr::GetMonitorsReply = match connection.wait_for_reply(
        connection
            .send_request(&randr::GetMonitors { window: screen.root_window(), get_active: true }),
    ) {
        Ok(reply) => reply,
        Err(_) => return Ok(vec![fallback_output(screen)]),
    };

    let outputs = reply
        .monitors()
        .filter(|monitor: &&randr::MonitorInfo| monitor.width() > 0 && monitor.height() > 0)
        .map(|monitor| discovered_output_from_monitor(connection, monitor))
        .collect::<Result<Vec<_>>>()?;

    if outputs.is_empty() { Ok(vec![fallback_output(screen)]) } else { Ok(outputs) }
}

fn discovered_output_from_monitor(
    connection: &xcb::Connection,
    monitor: &randr::MonitorInfo,
) -> Result<DiscoveredOutput> {
    let atom_name = connection
        .wait_for_reply(connection.send_request(&x::GetAtomName { atom: monitor.name() }))
        .context("failed to read RandR monitor atom name")?;
    let name = atom_name.name().to_utf8().into_owned();
    let name = if name.is_empty() {
        format!("monitor-{}x{}-{}-{}", monitor.width(), monitor.height(), monitor.x(), monitor.y())
    } else {
        name
    };

    Ok(DiscoveredOutput {
        output_id: spiders_core::OutputId::from(format!("x11-output-{name}")),
        name,
        x: i32::from(monitor.x()),
        y: i32::from(monitor.y()),
        width: u32::from(monitor.width()),
        height: u32::from(monitor.height()),
        primary: monitor.primary(),
    })
}

fn fallback_output(screen: &ScreenDescriptor) -> DiscoveredOutput {
    DiscoveredOutput {
        output_id: spiders_core::OutputId::from(format!("x11-screen-{}", screen.index)),
        name: format!("screen-{}", screen.index),
        x: 0,
        y: 0,
        width: u32::from(screen.width),
        height: u32::from(screen.height),
        primary: true,
    }
}
