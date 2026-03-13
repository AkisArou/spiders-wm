#[cfg(feature = "smithay-winit")]
mod imp {
    use std::collections::{HashMap, HashSet};

    use crate::backend::{BackendDiscoveryEvent, BackendSurfaceSnapshot};
    use smithay::backend::renderer::utils::on_commit_buffer_handler;
    use smithay::delegate_compositor;
    use smithay::delegate_seat;
    use smithay::delegate_shm;
    use smithay::delegate_xdg_shell;
    use smithay::input::keyboard::XkbConfig;
    use smithay::input::{Seat, SeatHandler, SeatState};
    use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
    use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
    use smithay::reexports::wayland_server::protocol::wl_buffer;
    use smithay::reexports::wayland_server::protocol::wl_seat;
    use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
    use smithay::reexports::wayland_server::Resource;
    use smithay::reexports::wayland_server::{BindError, Client, Display, DisplayHandle};
    use smithay::utils::Serial;
    use smithay::wayland::buffer::BufferHandler;
    use smithay::wayland::compositor::{
        get_parent, get_role, is_sync_subsurface, with_states, BufferAssignment,
    };
    use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
    use smithay::wayland::shell::xdg::{
        PopupSurface, PositionerState, ToplevelSurface, XdgPopupSurfaceData, XdgShellHandler,
        XdgShellState, XdgToplevelSurfaceData, XDG_POPUP_ROLE, XDG_TOPLEVEL_ROLE,
    };
    use smithay::wayland::shm::{ShmHandler, ShmState};
    use smithay::wayland::socket::ListeningSocketSource;
    use spiders_shared::ids::WindowId;

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
        pub xdg_shell_state: XdgShellState,
        pub seat_state: SeatState<Self>,
        pub seat: Seat<Self>,
        pub seat_name: String,
        next_window_serial: u64,
        toplevel_window_ids: HashMap<String, WindowId>,
        tracked_surfaces: HashSet<String>,
        pending_discovery_events: Vec<BackendDiscoveryEvent>,
    }

    impl SpidersSmithayState {
        pub fn new(
            display: &Display<Self>,
            seat_name: impl Into<String>,
        ) -> Result<Self, SmithayStateError> {
            let display_handle = display.handle();
            let compositor_state = CompositorState::new::<Self>(&display_handle);
            let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
            let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
            let mut seat_state = SeatState::new();
            let seat_name = seat_name.into();
            let mut seat = seat_state.new_wl_seat(&display_handle, seat_name.clone());
            seat.add_keyboard(XkbConfig::default(), 200, 25)?;
            seat.add_pointer();

            Ok(Self {
                display_handle,
                compositor_state,
                shm_state,
                xdg_shell_state,
                seat_state,
                seat,
                seat_name,
                next_window_serial: 1,
                toplevel_window_ids: HashMap::new(),
                tracked_surfaces: HashSet::new(),
                pending_discovery_events: Vec::new(),
            })
        }

        pub fn bind_auto_socket_source(&self) -> Result<ListeningSocketSource, SmithayStateError> {
            ListeningSocketSource::new_auto().map_err(Into::into)
        }

        pub fn take_discovery_events(&mut self) -> Vec<BackendDiscoveryEvent> {
            std::mem::take(&mut self.pending_discovery_events)
        }

        fn track_surface_snapshot(&mut self, snapshot: BackendSurfaceSnapshot) {
            let surface_id = match &snapshot {
                BackendSurfaceSnapshot::Window { surface_id, .. }
                | BackendSurfaceSnapshot::Popup { surface_id, .. }
                | BackendSurfaceSnapshot::Layer { surface_id, .. }
                | BackendSurfaceSnapshot::Unmanaged { surface_id } => surface_id.clone(),
            };

            if !self.tracked_surfaces.insert(surface_id) {
                return;
            }

            self.pending_discovery_events
                .push(snapshot_into_discovery_event(snapshot));
        }

        fn track_surface_loss_by_id(&mut self, surface_id: String) {
            if self.tracked_surfaces.remove(&surface_id) {
                self.pending_discovery_events
                    .push(BackendDiscoveryEvent::SurfaceLost { surface_id });
            }
        }

        fn track_toplevel_surface(&mut self, surface: &WlSurface) {
            let surface_id = smithay_surface_id(surface);
            let window_id = self.window_id_for_surface(&surface_id);
            self.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                window_id,
                surface_id,
                output_id: None,
            });
        }

        fn track_popup_surface(&mut self, surface: &PopupSurface) {
            let wl_surface = surface.wl_surface();
            let surface_id = smithay_surface_id(wl_surface);
            let parent_surface_id = surface
                .get_parent_surface()
                .map(|parent| smithay_surface_id(&parent))
                .unwrap_or_else(|| format!("parent-{surface_id}"));

            self.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id,
                output_id: None,
                parent_surface_id,
            });
        }

        fn track_toplevel_surface_loss(&mut self, surface: &ToplevelSurface) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            self.toplevel_window_ids.remove(&surface_id);
            self.track_surface_loss_by_id(surface_id);
        }

        fn track_popup_surface_loss(&mut self, surface: &PopupSurface) {
            self.track_surface_loss_by_id(smithay_surface_id(surface.wl_surface()));
        }

        fn track_committed_surface(&mut self, surface: &WlSurface) {
            if is_sync_subsurface(surface) {
                return;
            }

            let root = root_surface(surface);
            let role = get_role(&root);
            let surface_id = smithay_surface_id(&root);

            match role {
                Some(XDG_TOPLEVEL_ROLE) => {
                    if surface_has_buffer(&root) {
                        self.track_toplevel_surface(&root);
                    } else if self.tracked_surfaces.contains(&surface_id) {
                        self.toplevel_window_ids.remove(&surface_id);
                        self.track_surface_loss_by_id(surface_id);
                    }
                }
                Some(XDG_POPUP_ROLE) => {
                    if surface_has_buffer(&root) {
                        self.track_popup_surface_by_root(&root);
                    } else if self.tracked_surfaces.contains(&surface_id) {
                        self.track_surface_loss_by_id(surface_id);
                    }
                }
                _ => {
                    if surface_has_buffer(&root) {
                        self.track_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                            surface_id,
                        });
                    }
                }
            }
        }

        fn track_popup_surface_by_root(&mut self, surface: &WlSurface) {
            let surface_id = smithay_surface_id(surface);
            let parent_surface_id = with_states(surface, |states| {
                states
                    .data_map
                    .get::<XdgPopupSurfaceData>()
                    .and_then(|data| data.lock().ok())
                    .and_then(|data| data.parent.clone())
                    .map(|parent| smithay_surface_id(&parent))
            })
            .unwrap_or_else(|| format!("parent-{surface_id}"));

            self.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id,
                output_id: None,
                parent_surface_id,
            });
        }

        fn window_id_for_surface(&mut self, surface_id: &str) -> WindowId {
            if let Some(window_id) = self.toplevel_window_ids.get(surface_id) {
                return window_id.clone();
            }

            let window_id = WindowId::from(format!("smithay-window-{}", self.next_window_serial));
            self.next_window_serial += 1;
            self.toplevel_window_ids
                .insert(surface_id.to_owned(), window_id.clone());
            window_id
        }
    }

    fn smithay_surface_id(surface: &WlSurface) -> String {
        format!("wl-surface-{}", surface.id().protocol_id())
    }

    fn root_surface(surface: &WlSurface) -> WlSurface {
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        root
    }

    fn surface_has_buffer(surface: &WlSurface) -> bool {
        with_states(surface, |states| {
            let mut attributes = states
                .cached_state
                .get::<smithay::wayland::compositor::SurfaceAttributes>();
            let pending = matches!(
                attributes.pending().buffer,
                Some(BufferAssignment::NewBuffer(_))
            );
            let current = matches!(
                attributes.current().buffer,
                Some(BufferAssignment::NewBuffer(_))
            );
            pending || current
        })
    }

    fn snapshot_into_discovery_event(snapshot: BackendSurfaceSnapshot) -> BackendDiscoveryEvent {
        match snapshot {
            BackendSurfaceSnapshot::Window {
                surface_id,
                window_id,
                output_id,
            } => BackendDiscoveryEvent::WindowSurfaceDiscovered {
                surface_id,
                window_id,
                output_id,
            },
            BackendSurfaceSnapshot::Popup {
                surface_id,
                output_id,
                parent_surface_id,
            } => BackendDiscoveryEvent::PopupSurfaceDiscovered {
                surface_id,
                output_id,
                parent_surface_id,
            },
            BackendSurfaceSnapshot::Layer {
                surface_id,
                output_id,
            } => BackendDiscoveryEvent::LayerSurfaceDiscovered {
                surface_id,
                output_id,
            },
            BackendSurfaceSnapshot::Unmanaged { surface_id } => {
                BackendDiscoveryEvent::UnmanagedSurfaceDiscovered { surface_id }
            }
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

        fn commit(&mut self, surface: &WlSurface) {
            on_commit_buffer_handler::<Self>(surface);
            self.track_committed_surface(surface);

            let root = root_surface(surface);
            if get_role(&root) == Some(XDG_TOPLEVEL_ROLE) {
                let needs_initial_configure = with_states(&root, |states| {
                    states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .and_then(|data| data.lock().ok())
                        .map(|data| !data.initial_configure_sent)
                        .unwrap_or(false)
                });

                if needs_initial_configure {
                    self.xdg_shell_state
                        .toplevel_surfaces()
                        .iter()
                        .filter(|surface| surface.wl_surface() == &root)
                        .for_each(|surface| {
                            let _ = surface.send_configure();
                        });
                }
            }
        }
    }

    impl ShmHandler for SpidersSmithayState {
        fn shm_state(&self) -> &ShmState {
            &self.shm_state
        }
    }

    impl XdgShellHandler for SpidersSmithayState {
        fn xdg_shell_state(&mut self) -> &mut XdgShellState {
            &mut self.xdg_shell_state
        }

        fn new_toplevel(&mut self, surface: ToplevelSurface) {
            self.track_toplevel_surface(surface.wl_surface());
            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Activated);
            });
            surface.send_configure();
        }

        fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
            self.track_popup_surface(&surface);
        }

        fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}

        fn reposition_request(
            &mut self,
            _surface: PopupSurface,
            _positioner: PositionerState,
            _token: u32,
        ) {
        }

        fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
            self.track_toplevel_surface_loss(&surface);
        }

        fn popup_destroyed(&mut self, surface: PopupSurface) {
            self.track_popup_surface_loss(&surface);
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
    delegate_xdg_shell!(SpidersSmithayState);

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn smithay_state_initializes_seat_capabilities() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            assert!(state.seat.get_keyboard().is_some());
            assert!(state.seat.get_pointer().is_some());
            assert_eq!(state.seat_name, "test-seat");
        }

        #[test]
        fn smithay_state_binds_socket_source() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            let socket = state.bind_auto_socket_source().unwrap();

            assert!(!socket.socket_name().is_empty());
        }

        #[test]
        fn smithay_state_tracks_surface_events_by_id() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-11".into(),
                window_id: WindowId::from("smithay-window-1"),
                output_id: None,
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-11".into(),
                window_id: WindowId::from("smithay-window-1"),
                output_id: None,
            });
            state.track_surface_loss_by_id("wl-surface-11".into());

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 2);
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::WindowSurfaceDiscovered { surface_id, .. }
                    if surface_id == "wl-surface-11"
            ));
            assert!(matches!(
                &events[1],
                BackendDiscoveryEvent::SurfaceLost { surface_id }
                    if surface_id == "wl-surface-11"
            ));
        }

        #[test]
        fn smithay_state_assigns_stable_window_ids_per_surface() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let first = state.window_id_for_surface("wl-surface-21");
            let second = state.window_id_for_surface("wl-surface-21");
            let third = state.window_id_for_surface("wl-surface-22");

            assert_eq!(first, second);
            assert_eq!(first, WindowId::from("smithay-window-1"));
            assert_eq!(third, WindowId::from("smithay-window-2"));
        }

        #[test]
        fn smithay_state_releases_window_id_mapping_when_surface_is_lost() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let first = state.window_id_for_surface("wl-surface-31");
            state.toplevel_window_ids.remove("wl-surface-31");
            let second = state.window_id_for_surface("wl-surface-31");

            assert_eq!(first, WindowId::from("smithay-window-1"));
            assert_eq!(second, WindowId::from("smithay-window-2"));
        }

        #[test]
        fn smithay_state_tracks_unmanaged_surface_snapshots() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-surface-90".into(),
            });

            let events = state.take_discovery_events();
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::UnmanagedSurfaceDiscovered { surface_id }
                    if surface_id == "wl-surface-90"
            ));
        }
    }
}

#[cfg(feature = "smithay-winit")]
pub use imp::{SmithayClientState, SmithayStateError, SpidersSmithayState};
