use std::{ffi::OsString, sync::Arc, time::Instant};

use smithay::{
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{Seat, SeatState},
    reexports::{
        calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction, generic::Generic},
        wayland_server::{
            Display, DisplayHandle, Resource, backend::ClientData, protocol::wl_surface::WlSurface,
        },
    },
    utils::{Logical, Point, Serial},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::app::AppState;

#[derive(Debug)]
pub struct SpidersWm2 {
    pub app: AppState,
    pub runtime: RuntimeState,
}

#[derive(Debug)]
pub struct RuntimeState {
    pub start_time: Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub loop_signal: LoopSignal,
    pub smithay: SmithayState,
}

#[derive(Debug)]
pub struct SmithayState {
    pub space: Space<Window>,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<SpidersWm2>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,
    pub seat: Seat<SpidersWm2>,
}

impl SpidersWm2 {
    pub fn new(event_loop: &mut EventLoop<Self>, display: Display<Self>) -> Self {
        let runtime = RuntimeState::new(event_loop, display);

        Self {
            app: AppState::default(),
            runtime,
        }
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.runtime
            .smithay
            .space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(surface, point)| (surface, (point + location).to_f64()))
            })
    }

    pub fn focus_window_surface(&mut self, surface: Option<WlSurface>, serial: Serial) {
        let window_to_raise = surface
            .as_ref()
            .and_then(|target_surface| {
                self.runtime.smithay.space.elements().find(|window| {
                    window
                        .toplevel()
                        .is_some_and(|toplevel| toplevel.wl_surface().id() == target_surface.id())
                })
            })
            .cloned();

        if let Some(window) = window_to_raise {
            self.runtime.smithay.space.raise_element(&window, true);
        }

        self.runtime.smithay.space.elements().for_each(|mapped| {
            let is_focused = surface.as_ref().is_some_and(|target_surface| {
                mapped
                    .toplevel()
                    .is_some_and(|toplevel| toplevel.wl_surface().id() == target_surface.id())
            });

            mapped.set_activated(is_focused);

            if let Some(toplevel) = mapped.toplevel() {
                toplevel.send_pending_configure();
            }
        });
    }
}

impl RuntimeState {
    fn new(event_loop: &mut EventLoop<SpidersWm2>, display: Display<SpidersWm2>) -> Self {
        let start_time = Instant::now();
        let display_handle = display.handle();
        let socket_name = Self::init_wayland_listener(display, event_loop);
        let loop_signal = event_loop.get_signal();
        let smithay = SmithayState::new(&display_handle);

        Self {
            start_time,
            socket_name,
            display_handle,
            loop_signal,
            smithay,
        }
    }

    fn init_wayland_listener(
        display: Display<SpidersWm2>,
        event_loop: &mut EventLoop<SpidersWm2>,
    ) -> OsString {
        let listening_socket =
            ListeningSocketSource::new_auto().expect("failed to create wayland socket");
        let socket_name = listening_socket.socket_name().to_os_string();

        let loop_handle = event_loop.handle();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                state
                    .runtime
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .expect("failed to insert client");
            })
            .expect("failed to add listening socket source");

        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| unsafe {
                    display.get_mut().dispatch_clients(state).unwrap();

                    Ok(PostAction::Continue)
                },
            )
            .expect("failed to add wayland display source");

        socket_name
    }
}

impl SmithayState {
    fn new(display_handle: &DisplayHandle) -> Self {
        let compositor_state = CompositorState::new::<SpidersWm2>(display_handle);
        let xdg_shell_state = XdgShellState::new::<SpidersWm2>(display_handle);
        let shm_state = ShmState::new::<SpidersWm2>(display_handle, vec![]);
        let output_manager_state =
            OutputManagerState::new_with_xdg_output::<SpidersWm2>(display_handle);
        let data_device_state = DataDeviceState::new::<SpidersWm2>(display_handle);

        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(display_handle, "winit");

        seat.add_keyboard(Default::default(), 200, 25)
            .expect("failed to create keyboard");
        seat.add_pointer();

        Self {
            space: Space::default(),
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups: PopupManager::default(),
            seat,
        }
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: smithay::reexports::wayland_server::backend::ClientId) {}

    fn disconnected(
        &self,
        _client_id: smithay::reexports::wayland_server::backend::ClientId,
        _reason: smithay::reexports::wayland_server::backend::DisconnectReason,
    ) {
    }
}
