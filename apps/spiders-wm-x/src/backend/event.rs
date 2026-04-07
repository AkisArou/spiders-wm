use anyhow::{Context, Result};
use std::time::{Duration, Instant};
use tracing::info;
use xcb::{Xid, randr, x};

use super::ScreenDescriptor;

pub(crate) trait ManageEventHandler {
    fn on_map_request(&mut self, connection: &xcb::Connection, window: x::Window) -> Result<()>;
    fn on_configure_request(
        &mut self,
        connection: &xcb::Connection,
        event: &x::ConfigureRequestEvent,
    ) -> Result<()>;
    fn on_unmap_notify(&mut self, connection: &xcb::Connection, window: x::Window) -> Result<()>;
    fn on_destroy_notify(&mut self, connection: &xcb::Connection, window: x::Window) -> Result<()>;
    fn on_configure_notify(&mut self, event: &x::ConfigureNotifyEvent);
    fn on_property_notify(
        &mut self,
        connection: &xcb::Connection,
        window: x::Window,
        atom: x::Atom,
    ) -> Result<()>;
    fn on_focus_in(&mut self, connection: &xcb::Connection, window: x::Window) -> Result<()>;
    fn on_client_message(
        &mut self,
        connection: &xcb::Connection,
        window: x::Window,
        type_atom: x::Atom,
        data: &x::ClientMessageData,
    ) -> Result<()>;
    fn on_randr_notify(
        &mut self,
        connection: &xcb::Connection,
        event: &randr::NotifyEvent,
    ) -> Result<()>;
}

pub(crate) fn install_manage_root_mask(
    connection: &xcb::Connection,
    screen: &ScreenDescriptor,
) -> Result<()> {
    connection
        .send_and_check_request(&x::ChangeWindowAttributes {
            window: screen.root_window(),
            value_list: &[x::Cw::EventMask(manage_event_mask())],
        })
        .map_err(|error| ownership_error(error, screen))?;
    connection.flush().context("failed to flush X11 window manager root event mask")?;

    if randr::get_extension_data(connection).is_some() {
        connection
            .send_and_check_request(&randr::SelectInput {
                window: screen.root_window(),
                enable: randr::NotifyMask::SCREEN_CHANGE
                    | randr::NotifyMask::CRTC_CHANGE
                    | randr::NotifyMask::OUTPUT_CHANGE
                    | randr::NotifyMask::OUTPUT_PROPERTY
                    | randr::NotifyMask::RESOURCE_CHANGE,
            })
            .context("failed to register RandR root notifications")?;
        connection.flush().context("failed to flush RandR root notifications")?;
    }

    Ok(())
}

pub(crate) fn observe_connection_events(
    connection: &xcb::Connection,
    screen: &ScreenDescriptor,
    event_limit: Option<usize>,
    idle_timeout_ms: Option<u64>,
) -> Result<()> {
    connection
        .send_and_check_request(&x::ChangeWindowAttributes {
            window: screen.root_window(),
            value_list: &[x::Cw::EventMask(observation_event_mask())],
        })
        .context("failed to register root observation event mask")?;
    connection.flush().context("failed to flush root observation event mask")?;

    info!(
        root_window_id = screen.root_window_id,
        event_limit, idle_timeout_ms, "spiders-wm-x started X11 event observation"
    );

    let mut observed = 0_usize;
    let idle_timeout = idle_timeout_ms.map(Duration::from_millis);
    let mut last_event_at = Instant::now();

    loop {
        let Some(event) =
            connection.poll_for_event().context("failed while polling for X11 events")?
        else {
            if let Some(idle_timeout) = idle_timeout
                && last_event_at.elapsed() >= idle_timeout
            {
                info!(
                    observed,
                    idle_timeout_ms, "spiders-wm-x reached X11 observation idle timeout"
                );
                break;
            }

            std::thread::sleep(Duration::from_millis(25));
            continue;
        };

        observed += 1;
        last_event_at = Instant::now();

        info!(event_index = observed, event = ?event, "spiders-wm-x observed X11 event");

        if event_limit.is_some_and(|limit| observed >= limit) {
            info!(observed, "spiders-wm-x reached X11 observation event limit");
            break;
        }
    }

    Ok(())
}

pub(crate) fn run_manage_event_loop(
    connection: &xcb::Connection,
    screen: &ScreenDescriptor,
    handler: &mut impl ManageEventHandler,
) -> Result<()> {
    info!(root_window_id = screen.root_window_id, "spiders-wm-x entered X11 manage event loop");

    loop {
        let event =
            connection.wait_for_event().context("failed while waiting for X11 manage events")?;
        dispatch_manage_event(connection, handler, &event)?;
    }
}

fn observation_event_mask() -> x::EventMask {
    x::EventMask::STRUCTURE_NOTIFY
        | x::EventMask::SUBSTRUCTURE_NOTIFY
        | x::EventMask::PROPERTY_CHANGE
}

fn manage_event_mask() -> x::EventMask {
    x::EventMask::SUBSTRUCTURE_REDIRECT
        | x::EventMask::SUBSTRUCTURE_NOTIFY
        | x::EventMask::STRUCTURE_NOTIFY
        | x::EventMask::PROPERTY_CHANGE
        | x::EventMask::FOCUS_CHANGE
}

fn dispatch_manage_event(
    connection: &xcb::Connection,
    handler: &mut impl ManageEventHandler,
    event: &xcb::Event,
) -> Result<()> {
    match event {
        xcb::Event::X(x::Event::MapRequest(ev)) => {
            info!(
                window = ev.window().resource_id(),
                parent = ev.parent().resource_id(),
                "received X11 map request"
            );
            handler.on_map_request(connection, ev.window())?;
        }
        xcb::Event::X(x::Event::ConfigureRequest(ev)) => {
            info!(window = ev.window().resource_id(), parent = ev.parent().resource_id(), value_mask = ?ev.value_mask(), "received X11 configure request");
            handler.on_configure_request(connection, ev)?;
        }
        xcb::Event::X(x::Event::UnmapNotify(ev)) => {
            info!(
                window = ev.window().resource_id(),
                event = ev.event().resource_id(),
                "received X11 unmap notify"
            );
            handler.on_unmap_notify(connection, ev.window())?;
        }
        xcb::Event::X(x::Event::DestroyNotify(ev)) => {
            info!(
                window = ev.window().resource_id(),
                event = ev.event().resource_id(),
                "received X11 destroy notify"
            );
            handler.on_destroy_notify(connection, ev.window())?;
        }
        xcb::Event::X(x::Event::PropertyNotify(ev)) => {
            info!(window = ev.window().resource_id(), atom = ev.atom().resource_id(), state = ?ev.state(), "received X11 property notify");
            handler.on_property_notify(connection, ev.window(), ev.atom())?;
        }
        xcb::Event::X(x::Event::ClientMessage(ev)) => {
            info!(
                window = ev.window().resource_id(),
                type_atom = ev.r#type().resource_id(),
                format = ev.format(),
                "received X11 client message"
            );
            handler.on_client_message(connection, ev.window(), ev.r#type(), &ev.data())?;
        }
        xcb::Event::X(x::Event::ConfigureNotify(ev)) => {
            info!(
                window = ev.window().resource_id(),
                event = ev.event().resource_id(),
                width = ev.width(),
                height = ev.height(),
                x = ev.x(),
                y = ev.y(),
                "received X11 configure notify"
            );
            handler.on_configure_notify(ev);
        }
        xcb::Event::X(x::Event::MapNotify(ev)) => {
            info!(
                window = ev.window().resource_id(),
                event = ev.event().resource_id(),
                override_redirect = ev.override_redirect(),
                "received X11 map notify"
            );
        }
        xcb::Event::X(x::Event::FocusIn(ev)) => {
            info!(event = ev.event().resource_id(), mode = ?ev.mode(), detail = ?ev.detail(), "received X11 focus in");
            handler.on_focus_in(connection, ev.event())?;
        }
        xcb::Event::X(x::Event::FocusOut(ev)) => {
            info!(event = ev.event().resource_id(), mode = ?ev.mode(), detail = ?ev.detail(), "received X11 focus out");
        }
        xcb::Event::RandR(randr::Event::Notify(ev)) => {
            info!(sub_code = ?ev.sub_code(), event = ?ev, "received RandR notify event");
            handler.on_randr_notify(connection, ev)?;
        }
        xcb::Event::RandR(randr::Event::ScreenChangeNotify(ev)) => {
            info!(
                root = ev.root().resource_id(),
                width = ev.width(),
                height = ev.height(),
                "received RandR screen change notify"
            );
        }
        other => {
            info!(event = ?other, "received X11 event outside current manage handlers");
        }
    }

    Ok(())
}

fn ownership_error(error: xcb::ProtocolError, screen: &ScreenDescriptor) -> anyhow::Error {
    anyhow::anyhow!(
        "failed to acquire X11 WM ownership on screen {} root {}: {:?}. another window manager is likely already running on this screen",
        screen.index,
        screen.root_window_id,
        error
    )
}
