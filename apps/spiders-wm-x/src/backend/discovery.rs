use anyhow::Result;
use spiders_core::signal::WmSignal;
use spiders_core::window_id;
use spiders_wm_runtime::{PreviewRenderAction, WmHost, WmRuntime};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, GetPropertyReply, MapState, Window};

use super::atoms::Atoms;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredWindow {
    pub(crate) window: Window,
    pub(crate) window_id: spiders_core::WindowId,
    pub(crate) title: Option<String>,
    pub(crate) app_id: Option<String>,
    pub(crate) class: Option<String>,
    pub(crate) instance: Option<String>,
    pub(crate) mapped: bool,
}

pub(crate) fn discover_windows<C: Connection>(
    connection: &C,
    root_window: Window,
    atoms: &Atoms,
) -> Result<Vec<DiscoveredWindow>> {
    let tree = connection.query_tree(root_window)?.reply()?;
    let mut windows = Vec::new();

    for window in tree.children {
        if let Some(discovered) = discover_window(connection, atoms, window)? {
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

pub(crate) fn discover_window_for_event<C: Connection>(
    connection: &C,
    atoms: &Atoms,
    window: Window,
) -> Result<Option<DiscoveredWindow>> {
    discover_window(connection, atoms, window)
}

fn discover_window<C: Connection>(
    connection: &C,
    atoms: &Atoms,
    window: Window,
) -> Result<Option<DiscoveredWindow>> {
    let attributes = match connection.get_window_attributes(window)?.reply() {
        Ok(attributes) => attributes,
        Err(_) => return Ok(None),
    };

    if attributes.override_redirect {
        return Ok(None);
    }

    let title = read_window_title(connection, atoms, window)?;
    let (instance, class) = read_wm_class(connection, atoms, window)?;
    let app_id = class.clone().or_else(|| instance.clone());
    let mapped = attributes.map_state == MapState::VIEWABLE;

    Ok(Some(DiscoveredWindow {
        window,
        window_id: window_id(window),
        title,
        app_id,
        class,
        instance,
        mapped,
    }))
}

fn read_window_title<C: Connection>(
    connection: &C,
    atoms: &Atoms,
    window: Window,
) -> Result<Option<String>> {
    if atoms.net_wm_name != u32::from(AtomEnum::NONE)
        && let Some(title) =
            read_property_string(connection, window, atoms.net_wm_name, atoms.utf8_string)?
        && !title.is_empty()
    {
        return Ok(Some(title));
    }

    if atoms.wm_name != u32::from(AtomEnum::NONE)
        && let Some(title) =
            read_property_string(connection, window, atoms.wm_name, AtomEnum::STRING.into())?
        && !title.is_empty()
    {
        return Ok(Some(title));
    }

    Ok(None)
}

fn read_wm_class<C: Connection>(
    connection: &C,
    atoms: &Atoms,
    window: Window,
) -> Result<(Option<String>, Option<String>)> {
    if atoms.wm_class == u32::from(AtomEnum::NONE) {
        return Ok((None, None));
    }

    let Some(raw) =
        read_property_bytes(connection, window, atoms.wm_class, AtomEnum::STRING.into())?
    else {
        return Ok((None, None));
    };

    let mut parts = raw.split(|byte| *byte == 0).filter(|part| !part.is_empty());
    let instance = parts.next().map(decode_lossy_property);
    let class = parts.next().map(decode_lossy_property);
    Ok((instance, class))
}

fn read_property_string<C: Connection>(
    connection: &C,
    window: Window,
    property: Atom,
    property_type: Atom,
) -> Result<Option<String>> {
    Ok(read_property_bytes(connection, window, property, property_type)?
        .map(|bytes| decode_lossy_property(&bytes)))
}

fn read_property_bytes<C: Connection>(
    connection: &C,
    window: Window,
    property: Atom,
    property_type: Atom,
) -> Result<Option<Vec<u8>>> {
    let reply = match connection
        .get_property(false, window, property, property_type, 0, 1024)?
        .reply()
    {
        Ok(reply) => reply,
        Err(_) => return Ok(None),
    };

    if property_value_u8(&reply).is_empty() {
        return Ok(None);
    }

    Ok(Some(property_value_u8(&reply).to_vec()))
}

fn property_value_u8(reply: &GetPropertyReply) -> &[u8] {
    &reply.value
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
