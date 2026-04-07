use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::randr::{self};
use x11rb::protocol::xproto::ConnectionExt as _;

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

pub(crate) fn discover_outputs<C: Connection>(
    connection: &C,
    screen: &ScreenDescriptor,
) -> Result<Vec<DiscoveredOutput>> {
    let reply = match randr::get_monitors(connection, screen.root_window, true)?.reply() {
        Ok(reply) => reply,
        Err(_) => return Ok(vec![fallback_output(screen)]),
    };

    let outputs = reply
        .monitors
        .into_iter()
        .filter(|monitor| monitor.width > 0 && monitor.height > 0)
        .map(|monitor| discovered_output_from_monitor(connection, monitor))
        .collect::<Result<Vec<_>>>()?;

    if outputs.is_empty() { Ok(vec![fallback_output(screen)]) } else { Ok(outputs) }
}

fn discovered_output_from_monitor<C: Connection>(
    connection: &C,
    monitor: randr::MonitorInfo,
) -> Result<DiscoveredOutput> {
    let name = atom_name(connection, monitor.name)?;
    let name = if name.is_empty() {
        format!("monitor-{}x{}-{}-{}", monitor.width, monitor.height, monitor.x, monitor.y)
    } else {
        name
    };

    Ok(DiscoveredOutput {
        output_id: spiders_core::OutputId::from(format!("x11-output-{name}")),
        name,
        x: i32::from(monitor.x),
        y: i32::from(monitor.y),
        width: u32::from(monitor.width),
        height: u32::from(monitor.height),
        primary: monitor.primary,
    })
}

fn atom_name<C: Connection>(connection: &C, atom: u32) -> Result<String> {
    let reply = connection.get_atom_name(atom)?.reply()?;
    Ok(String::from_utf8_lossy(&reply.name).into_owned())
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
