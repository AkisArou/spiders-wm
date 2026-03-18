use crate::{app::AppState, state::WorkspaceId, wm};

use std::{collections::HashSet, ffi::OsString, sync::Arc, time::Instant};

use smithay::{
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{Seat, SeatState},
    reexports::{
        calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction, generic::Generic},
        wayland_server::{
            Display, DisplayHandle, Resource, backend::ClientData, protocol::wl_surface::WlSurface,
        },
        winit::window,
    },
    utils::{IsAlive, Logical, Point, SERIAL_COUNTER, Serial},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

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

        if let Some(keyboard) = self.runtime.smithay.seat.get_keyboard() {
            keyboard.set_focus(self, surface, serial);
        }
    }

    pub fn unmap_window_surface(&mut self, surface: &WlSurface) {
        let removed_window_id = self.app.bindings.window_for_surface(&surface.id());

        let was_focused = removed_window_id
            .is_some_and(|window_id| self.app.wm.focused_window == Some(window_id));

        if let Some(window_id) = self.app.bindings.unbind_surface(&surface.id()) {
            wm::remove_window(&mut self.app.topology, &mut self.app.wm, window_id);
        }

        let window_to_unmap = self
            .runtime
            .smithay
            .space
            .elements()
            .find(|window| {
                window
                    .toplevel()
                    .is_some_and(|toplevel| toplevel.wl_surface() == surface)
            })
            .cloned();

        if let Some(window) = window_to_unmap {
            self.runtime.smithay.space.unmap_elem(&window);
        }

        if was_focused {
            let next_surface = wm::next_focus_in_active_workspace(&self.app.wm)
                .and_then(|window_id| self.app.bindings.surface_for_window(&window_id));

            self.focus_window_surface(next_surface, SERIAL_COUNTER.next_serial());
        }
    }

    pub fn refresh_active_workspace(&mut self) {
        let visible: HashSet<_> = wm::active_workspace_windows(&self.app.wm)
            .into_iter()
            .collect();

        for window_id in self.app.bindings.known_windows() {
            let Some(window) = self.app.bindings.element_for_window(&window_id) else {
                continue;
            };

            if visible.contains(&window_id) {
                if self
                    .runtime
                    .smithay
                    .space
                    .element_location(&window)
                    .is_none()
                {
                    self.runtime
                        .smithay
                        .space
                        .map_element(window, (0, 0), false);
                }
            } else {
                self.runtime.smithay.space.unmap_elem(&window);
            }
        }

        let focused_surface = self
            .app
            .wm
            .focused_window
            .and_then(|window_id| self.app.bindings.surface_for_window(&window_id));

        self.focus_window_surface(focused_surface, SERIAL_COUNTER.next_serial());
    }

    pub fn switch_workspace(&mut self, workspace_id: WorkspaceId) {
        wm::switch_to_workspace(&mut self.app.wm, workspace_id);
        self.refresh_active_workspace();
    }

    pub fn cleanup_dead_windows(&mut self) {
        let dead_surfaces = self
            .app
            .bindings
            .known_windows()
            .into_iter()
            .filter_map(|window_id| self.app.bindings.surface_for_window(&window_id))
            .filter(|surface| !surface.alive())
            .collect::<Vec<_>>();

        for surface in dead_surfaces {
            self.unmap_window_surface(&surface);
        }
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
