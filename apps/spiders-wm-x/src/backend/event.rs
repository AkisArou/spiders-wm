use anyhow::{Context, Result};
use calloop::generic::Generic;
use calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use spiders_ipc::IpcClientId;
use std::os::fd::OwnedFd;
use std::os::unix::io::AsFd;
use std::os::unix::net::UnixListener;
use std::time::{Duration, Instant};
use tracing::info;
use x11rb::connection::Connection;
use x11rb::protocol::randr::{self};
use x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ClientMessageData, ClientMessageEvent, ConfigureNotifyEvent,
    ConfigureRequestEvent, ConnectionExt as _, EventMask, FocusInEvent, KeyPressEvent,
    PropertyNotifyEvent, Window,
};
use x11rb::protocol::Event;
use x11rb::xcb_ffi::XCBConnection;

use super::ScreenDescriptor;

pub(crate) trait ManageEventHandler {
    fn should_exit(&self) -> bool {
        false
    }
    fn after_dispatch(&mut self, _connection: &XCBConnection) -> Result<()> {
        Ok(())
    }
    fn on_ipc_listener_ready(
        &mut self,
        _handle: &LoopHandle<'_, ManageLoopDispatchState>,
    ) -> Result<()> {
        Ok(())
    }
    fn on_ipc_client_ready(
        &mut self,
        _connection: &XCBConnection,
        _handle: &LoopHandle<'_, ManageLoopDispatchState>,
        _client_id: IpcClientId,
    ) -> Result<()> {
        Ok(())
    }
    fn on_map_request(&mut self, connection: &XCBConnection, window: Window) -> Result<()>;
    fn on_configure_request(
        &mut self,
        connection: &XCBConnection,
        event: &ConfigureRequestEvent,
    ) -> Result<()>;
    fn on_unmap_notify(&mut self, connection: &XCBConnection, window: Window) -> Result<()>;
    fn on_destroy_notify(&mut self, connection: &XCBConnection, window: Window) -> Result<()>;
    fn on_configure_notify(&mut self, event: &ConfigureNotifyEvent);
    fn on_property_notify(
        &mut self,
        connection: &XCBConnection,
        window: Window,
        atom: u32,
    ) -> Result<()>;
    fn on_focus_in(&mut self, connection: &XCBConnection, event: &FocusInEvent) -> Result<()>;
    fn on_key_press(&mut self, connection: &XCBConnection, event: &KeyPressEvent) -> Result<()>;
    fn on_client_message(
        &mut self,
        connection: &XCBConnection,
        window: Window,
        type_atom: u32,
        data: &ClientMessageData,
    ) -> Result<()>;
    #[allow(dead_code)]
    fn on_randr_notify(
        &mut self,
        connection: &XCBConnection,
        event: &randr::NotifyEvent,
    ) -> Result<()>;
    #[allow(dead_code)]
    fn on_randr_screen_change(
        &mut self,
        connection: &XCBConnection,
        event: &randr::ScreenChangeNotifyEvent,
    ) -> Result<()>;
}

#[derive(Default)]
pub(crate) struct ManageLoopDispatchState {
    ipc_listener_ready: bool,
    ipc_client_ready: Vec<IpcClientId>,
}

impl ManageLoopDispatchState {
    pub(crate) fn mark_ipc_listener_ready(&mut self) {
        self.ipc_listener_ready = true;
    }

    pub(crate) fn take_ipc_listener_ready(&mut self) -> bool {
        std::mem::take(&mut self.ipc_listener_ready)
    }

    pub(crate) fn mark_ipc_client_ready(&mut self, client_id: IpcClientId) {
        self.ipc_client_ready.push(client_id);
    }

    pub(crate) fn take_ipc_client_ready(&mut self) -> Vec<IpcClientId> {
        std::mem::take(&mut self.ipc_client_ready)
    }
}

pub(crate) fn register_ipc_listener_source(
    handle: &LoopHandle<'_, ManageLoopDispatchState>,
    listener: &UnixListener,
) -> Result<RegistrationToken> {
    let listener = listener.try_clone().context("failed to clone IPC listener for calloop")?;
    handle
        .insert_source(Generic::new(listener, Interest::READ, Mode::Level), |_, _, state| {
            state.mark_ipc_listener_ready();
            Ok(PostAction::Continue)
        })
        .context("failed to register IPC listener with calloop")
}

pub(crate) fn register_ipc_client_source(
    handle: &LoopHandle<'_, ManageLoopDispatchState>,
    client_id: IpcClientId,
    stream: &std::os::unix::net::UnixStream,
) -> Result<RegistrationToken> {
    let stream = stream.try_clone().context("failed to clone IPC client stream for calloop")?;
    handle
        .insert_source(Generic::new(stream, Interest::READ, Mode::Level), move |_, _, state| {
            state.mark_ipc_client_ready(client_id);
            Ok(PostAction::Continue)
        })
        .context("failed to register IPC client with calloop")
}

pub(crate) fn install_manage_root_mask<C: Connection>(
    connection: &C,
    screen: &ScreenDescriptor,
) -> Result<()> {
    connection
        .change_window_attributes(
            screen.root_window,
            &ChangeWindowAttributesAux::new().event_mask(manage_event_mask()),
        )?
        .check()
        .map_err(|error| ownership_error(error, screen))?;
    connection.flush().context("failed to flush X11 window manager root event mask")?;

    let _ = randr::query_version(connection, 1, 5)?.reply();
    let randr_registered = match randr::select_input(
        connection,
        screen.root_window,
        randr::NotifyMask::SCREEN_CHANGE
            | randr::NotifyMask::CRTC_CHANGE
            | randr::NotifyMask::OUTPUT_CHANGE
            | randr::NotifyMask::OUTPUT_PROPERTY
            | randr::NotifyMask::RESOURCE_CHANGE,
    ) {
        Ok(cookie) => cookie.check().is_ok(),
        Err(_) => false,
    };
    if randr_registered {
        connection.flush().context("failed to flush RandR root notifications")?;
    }

    Ok(())
}

pub(crate) fn observe_connection_events<C: Connection>(
    connection: &C,
    screen: &ScreenDescriptor,
    event_limit: Option<usize>,
    idle_timeout_ms: Option<u64>,
) -> Result<()> {
    connection
        .change_window_attributes(
            screen.root_window,
            &ChangeWindowAttributesAux::new().event_mask(observation_event_mask()),
        )?
        .check()
        .context("failed to register root observation event mask")?;
    connection.flush().context("failed to flush root observation event mask")?;

    info!(
        root_window_id = screen.root_window,
        event_limit,
        idle_timeout_ms,
        "spiders-wm-x started X11 event observation"
    );

    let mut observed = 0_usize;
    let idle_timeout = idle_timeout_ms.map(Duration::from_millis);
    let mut last_event_at = Instant::now();

    loop {
        let Some(event) = connection
            .poll_for_event()
            .context("failed while polling for X11 events")?
        else {
            if let Some(idle_timeout) = idle_timeout
                && last_event_at.elapsed() >= idle_timeout
            {
                info!(
                    observed,
                    idle_timeout_ms,
                    "spiders-wm-x reached X11 observation idle timeout"
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
    connection: &XCBConnection,
    screen: &ScreenDescriptor,
    ipc_listener: Option<&UnixListener>,
    handler: &mut impl ManageEventHandler,
) -> Result<()> {
    info!(root_window_id = screen.root_window, "spiders-wm-x entered X11 manage event loop");

    let poll_fd = connection
        .as_fd()
        .try_clone_to_owned()
        .context("failed to duplicate X11 connection fd for calloop")?;
    let mut event_loop =
        EventLoop::<ManageLoopDispatchState>::try_new().context("failed to create X11 calloop event loop")?;
    event_loop
        .handle()
        .insert_source(
            Generic::new(poll_fd, Interest::READ, Mode::Level),
            |_, _: &mut calloop::generic::NoIoDrop<OwnedFd>, _| Ok(PostAction::Continue),
        )
        .context("failed to register X11 connection with calloop")?;

    if let Some(listener) = ipc_listener {
        register_ipc_listener_source(&event_loop.handle(), listener)?;
    }

    let mut dispatch_state = ManageLoopDispatchState::default();

    loop {
        drain_manage_events(connection, handler)?;
        if handler.should_exit() {
            info!(root_window_id = screen.root_window, "spiders-wm-x leaving X11 manage event loop");
            break;
        }
        event_loop
            .dispatch(Some(Duration::from_millis(50)), &mut dispatch_state)
            .context("failed while dispatching X11 calloop events")?;
        if dispatch_state.take_ipc_listener_ready() {
            handler.on_ipc_listener_ready(&event_loop.handle())?;
        }
        for client_id in dispatch_state.take_ipc_client_ready() {
            handler.on_ipc_client_ready(connection, &event_loop.handle(), client_id)?;
        }
        handler.after_dispatch(connection)?;
        if handler.should_exit() {
            info!(root_window_id = screen.root_window, "spiders-wm-x leaving X11 manage event loop");
            break;
        }
    }

    Ok(())
}

fn drain_manage_events(connection: &XCBConnection, handler: &mut impl ManageEventHandler) -> Result<()> {
    while let Some(event) = connection
        .poll_for_event()
        .context("failed while polling for X11 manage events")?
    {
        dispatch_manage_event(connection, handler, &event)?;
    }

    Ok(())
}

fn observation_event_mask() -> EventMask {
    EventMask::STRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_NOTIFY | EventMask::PROPERTY_CHANGE
}

fn manage_event_mask() -> EventMask {
    EventMask::SUBSTRUCTURE_REDIRECT
        | EventMask::SUBSTRUCTURE_NOTIFY
        | EventMask::STRUCTURE_NOTIFY
        | EventMask::PROPERTY_CHANGE
        | EventMask::FOCUS_CHANGE
        | EventMask::KEY_PRESS
}

fn dispatch_manage_event(
    connection: &XCBConnection,
    handler: &mut impl ManageEventHandler,
    event: &Event,
) -> Result<()> {
    match event {
        Event::MapRequest(ev) => {
            info!(window = ev.window, parent = ev.parent, "received X11 map request");
            handler.on_map_request(connection, ev.window)?;
        }
        Event::ConfigureRequest(ev) => {
            info!(window = ev.window, parent = ev.parent, value_mask = ?ev.value_mask, "received X11 configure request");
            handler.on_configure_request(connection, ev)?;
        }
        Event::UnmapNotify(ev) => {
            info!(window = ev.window, event = ev.event, "received X11 unmap notify");
            handler.on_unmap_notify(connection, ev.window)?;
        }
        Event::DestroyNotify(ev) => {
            info!(window = ev.window, event = ev.event, "received X11 destroy notify");
            handler.on_destroy_notify(connection, ev.window)?;
        }
        Event::PropertyNotify(PropertyNotifyEvent { window, atom, state, .. }) => {
            info!(window = *window, atom = *atom, state = ?state, "received X11 property notify");
            handler.on_property_notify(connection, *window, *atom)?;
        }
        Event::ClientMessage(ClientMessageEvent { window, type_, format, data, .. }) => {
            info!(window = *window, type_atom = *type_, format = *format, "received X11 client message");
            handler.on_client_message(connection, *window, *type_, data)?;
        }
        Event::ConfigureNotify(ev) => {
            info!(window = ev.window, event = ev.event, width = ev.width, height = ev.height, x = ev.x, y = ev.y, "received X11 configure notify");
            handler.on_configure_notify(ev);
        }
        Event::MapNotify(ev) => {
            info!(window = ev.window, event = ev.event, override_redirect = ev.override_redirect, "received X11 map notify");
        }
        Event::FocusIn(ev) => {
            info!(event = ev.event, mode = ?ev.mode, detail = ?ev.detail, "received X11 focus in");
            handler.on_focus_in(connection, ev)?;
        }
        Event::KeyPress(ev) => {
            info!(event = ev.event, detail = ev.detail, state = ?ev.state, "received X11 key press");
            handler.on_key_press(connection, ev)?;
        }
        Event::RandrNotify(ev) => {
            info!(event = ?ev, "received X11 RandR notify");
            handler.on_randr_notify(connection, ev)?;
        }
        Event::RandrScreenChangeNotify(ev) => {
            info!(root = ev.root, width = ev.width, height = ev.height, "received X11 RandR screen change notify");
            handler.on_randr_screen_change(connection, ev)?;
        }
        Event::FocusOut(ev) => {
            info!(event = ev.event, mode = ?ev.mode, detail = ?ev.detail, "received X11 focus out");
        }
        other => {
            info!(event = ?other, "received X11 event outside current manage handlers");
        }
    }

    Ok(())
}

fn ownership_error(
    error: x11rb::errors::ReplyError,
    screen: &ScreenDescriptor,
) -> anyhow::Error {
    anyhow::anyhow!(
        "failed to acquire X11 WM ownership on screen {} root {}: {:?}. another window manager is likely already running on this screen",
        screen.index,
        screen.root_window,
        error
    )
}
