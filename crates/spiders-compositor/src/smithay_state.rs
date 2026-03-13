#[cfg(feature = "smithay-winit")]
mod imp {
    use smithay::delegate_compositor;
    use smithay::delegate_seat;
    use smithay::delegate_shm;
    use smithay::input::keyboard::XkbConfig;
    use smithay::input::{Seat, SeatHandler, SeatState};
    use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
    use smithay::reexports::wayland_server::protocol::wl_buffer;
    use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
    use smithay::reexports::wayland_server::{BindError, Client, Display, DisplayHandle};
    use smithay::wayland::buffer::BufferHandler;
    use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
    use smithay::wayland::shm::{ShmHandler, ShmState};
    use smithay::wayland::socket::ListeningSocketSource;

    #[derive(Debug, thiserror::Error)]
    pub enum SmithayStateError {
        #[error(transparent)]
        Keyboard(#[from] smithay::input::keyboard::Error),
        #[error(transparent)]
        SocketBind(#[from] BindError),
    }

    #[derive(Debug, Default)]
    pub struct SmithayClientState {
        pub compositor_state: CompositorClientState,
    }

    impl ClientData for SmithayClientState {
        fn initialized(&self, _client_id: ClientId) {}

        fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
    }

    #[derive(Debug)]
    pub struct SpidersSmithayState {
        pub display_handle: DisplayHandle,
        pub compositor_state: CompositorState,
        pub shm_state: ShmState,
        pub seat_state: SeatState<Self>,
        pub seat: Seat<Self>,
        pub seat_name: String,
    }

    impl SpidersSmithayState {
        pub fn new(
            display: &Display<Self>,
            seat_name: impl Into<String>,
        ) -> Result<Self, SmithayStateError> {
            let display_handle = display.handle();
            let compositor_state = CompositorState::new::<Self>(&display_handle);
            let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
            let mut seat_state = SeatState::new();
            let seat_name = seat_name.into();
            let mut seat = seat_state.new_wl_seat(&display_handle, seat_name.clone());
            seat.add_keyboard(XkbConfig::default(), 200, 25)?;
            seat.add_pointer();

            Ok(Self {
                display_handle,
                compositor_state,
                shm_state,
                seat_state,
                seat,
                seat_name,
            })
        }

        pub fn bind_auto_socket_source(&self) -> Result<ListeningSocketSource, SmithayStateError> {
            ListeningSocketSource::new_auto().map_err(Into::into)
        }
    }

    impl BufferHandler for SpidersSmithayState {
        fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
    }

    impl CompositorHandler for SpidersSmithayState {
        fn compositor_state(&mut self) -> &mut CompositorState {
            &mut self.compositor_state
        }

        fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
            &client
                .get_data::<SmithayClientState>()
                .unwrap()
                .compositor_state
        }

        fn commit(&mut self, _surface: &WlSurface) {}
    }

    impl ShmHandler for SpidersSmithayState {
        fn shm_state(&self) -> &ShmState {
            &self.shm_state
        }
    }

    impl SeatHandler for SpidersSmithayState {
        type KeyboardFocus = WlSurface;
        type PointerFocus = WlSurface;
        type TouchFocus = WlSurface;

        fn seat_state(&mut self) -> &mut SeatState<Self> {
            &mut self.seat_state
        }

        fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}

        fn cursor_image(
            &mut self,
            _seat: &Seat<Self>,
            _image: smithay::input::pointer::CursorImageStatus,
        ) {
        }
    }

    delegate_compositor!(SpidersSmithayState);
    delegate_shm!(SpidersSmithayState);
    delegate_seat!(SpidersSmithayState);
}

#[cfg(feature = "smithay-winit")]
pub use imp::{SmithayClientState, SmithayStateError, SpidersSmithayState};
