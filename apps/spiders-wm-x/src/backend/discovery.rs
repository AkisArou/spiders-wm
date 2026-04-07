use anyhow::{Context, Result};
use spiders_core::signal::WmSignal;
use spiders_core::window_id;
use spiders_wm_runtime::{PreviewRenderAction, WmHost, WmRuntime};
use xcb::{Xid, x};

use super::ScreenDescriptor;
use super::atoms::Atoms;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredWindow {
    pub(crate) window: x::Window,
    pub(crate) window_id: spiders_core::WindowId,
    pub(crate) title: Option<String>,
    pub(crate) app_id: Option<String>,
    pub(crate) class: Option<String>,
    pub(crate) instance: Option<String>,
    pub(crate) mapped: bool,
}

pub(crate) fn discover_windows(
    connection: &xcb::Connection,
    screen: &ScreenDescriptor,
    atoms: &Atoms,
) -> Result<Vec<DiscoveredWindow>> {
    let tree = connection
        .wait_for_reply(connection.send_request(&x::QueryTree { window: screen.root_window() }))
        .context("failed to query the X11 root window tree")?;

    let mut windows = Vec::new();
    for window in tree.children() {
        if let Some(discovered) = discover_window(connection, atoms, *window)? {
            windows.push(discovered);
        }
    }

    Ok(windows)
}

pub(crate) fn sync_discovered_windows(
    runtime: &mut WmRuntime<'_>,
    discovered_windows: &[DiscoveredWindow],
) {
    for discovered in discovered_windows {
        runtime.place_new_window(discovered.window_id.clone());

        let mut host = NoopHost;
        let _ = runtime.handle_signal(
            &mut host,
            WmSignal::WindowIdentityChanged {
                window_id: discovered.window_id.clone(),
                title: discovered.title.clone(),
                app_id: discovered.app_id.clone(),
                class: discovered.class.clone(),
                instance: discovered.instance.clone(),
            },
        );
        let _ = runtime.handle_signal(
            &mut host,
            WmSignal::WindowMappedChanged {
                window_id: discovered.window_id.clone(),
                mapped: discovered.mapped,
            },
        );
    }

    let _ = runtime.take_events();
}

fn discover_window(
    connection: &xcb::Connection,
    atoms: &Atoms,
    window: x::Window,
) -> Result<Option<DiscoveredWindow>> {
    let attributes = match connection
        .wait_for_reply(connection.send_request(&x::GetWindowAttributes { window }))
    {
        Ok(attributes) => attributes,
        Err(_) => return Ok(None),
    };

    if attributes.override_redirect() {
        return Ok(None);
    }

    let title = read_window_title(connection, atoms, window)?;
    let (instance, class) = read_wm_class(connection, atoms, window)?;
    let app_id = class.clone().or_else(|| instance.clone());
    let mapped = attributes.map_state() == x::MapState::Viewable;

    Ok(Some(DiscoveredWindow {
        window,
        window_id: window_id(window.resource_id()),
        title,
        app_id,
        class,
        instance,
        mapped,
    }))
}

pub(crate) fn discover_window_for_event(
    connection: &xcb::Connection,
    atoms: &Atoms,
    window: x::Window,
) -> Result<Option<DiscoveredWindow>> {
    discover_window(connection, atoms, window)
}

fn read_window_title(
    connection: &xcb::Connection,
    atoms: &Atoms,
    window: x::Window,
) -> Result<Option<String>> {
    if atoms.net_wm_name != x::ATOM_NONE
        && let Some(title) =
            read_property_string(connection, window, atoms.net_wm_name, atoms.utf8_string)?
        && !title.is_empty()
    {
        return Ok(Some(title));
    }

    if atoms.wm_name != x::ATOM_NONE
        && let Some(title) =
            read_property_string(connection, window, atoms.wm_name, x::ATOM_STRING)?
        && !title.is_empty()
    {
        return Ok(Some(title));
    }

    Ok(None)
}

fn read_wm_class(
    connection: &xcb::Connection,
    atoms: &Atoms,
    window: x::Window,
) -> Result<(Option<String>, Option<String>)> {
    if atoms.wm_class == x::ATOM_NONE {
        return Ok((None, None));
    }

    let Some(raw) = read_property_bytes(connection, window, atoms.wm_class, x::ATOM_STRING)? else {
        return Ok((None, None));
    };

    let mut parts = raw.split(|byte| *byte == 0).filter(|part| !part.is_empty());
    let instance = parts.next().map(decode_lossy_property);
    let class = parts.next().map(decode_lossy_property);

    Ok((instance, class))
}

fn read_property_string(
    connection: &xcb::Connection,
    window: x::Window,
    property: x::Atom,
    property_type: x::Atom,
) -> Result<Option<String>> {
    Ok(read_property_bytes(connection, window, property, property_type)?
        .map(|bytes| decode_lossy_property(&bytes)))
}

fn read_property_bytes(
    connection: &xcb::Connection,
    window: x::Window,
    property: x::Atom,
    property_type: x::Atom,
) -> Result<Option<Vec<u8>>> {
    let reply = match connection.wait_for_reply(connection.send_request(&x::GetProperty {
        delete: false,
        window,
        property,
        r#type: property_type,
        long_offset: 0,
        long_length: 1024,
    })) {
        Ok(reply) => reply,
        Err(_) => return Ok(None),
    };

    if reply.value::<u8>().is_empty() {
        return Ok(None);
    }

    Ok(Some(reply.value::<u8>().to_vec()))
}

fn decode_lossy_property(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string()
}

struct NoopHost;

impl WmHost for NoopHost {
    fn on_effect(&mut self, _effect: spiders_core::effect::WmHostEffect) -> PreviewRenderAction {
        PreviewRenderAction::None
    }
}
