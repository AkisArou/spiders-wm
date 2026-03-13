#[cfg(feature = "smithay-winit")]
mod imp {
    use std::collections::HashMap;

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

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithaySurfaceRoleCounts {
        pub toplevel: usize,
        pub popup: usize,
        pub unmanaged: usize,
        pub layer: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownToplevelSurface {
        pub surface_id: String,
        pub window_id: WindowId,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownPopupSurface {
        pub surface_id: String,
        pub parent: SmithayPopupParentSnapshot,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SmithayPopupParentSnapshot {
        Resolved {
            surface_id: String,
            window_id: Option<WindowId>,
        },
        Unresolved,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownUnmanagedSurface {
        pub surface_id: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownLayerSurface {
        pub surface_id: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownSurfacesSnapshot {
        pub all: Vec<SmithayKnownSurface>,
        pub toplevels: Vec<SmithayKnownToplevelSurface>,
        pub popups: Vec<SmithayKnownPopupSurface>,
        pub unmanaged: Vec<SmithayKnownUnmanagedSurface>,
        pub layers: Vec<SmithayKnownLayerSurface>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SmithayKnownSurface {
        Toplevel(SmithayKnownToplevelSurface),
        Popup(SmithayKnownPopupSurface),
        Layer(SmithayKnownLayerSurface),
        Unmanaged(SmithayKnownUnmanagedSurface),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayStateSnapshot {
        pub seat_name: String,
        pub tracked_surface_count: usize,
        pub tracked_toplevel_count: usize,
        pub pending_discovery_event_count: usize,
        pub role_counts: SmithaySurfaceRoleCounts,
        pub known_surfaces: SmithayKnownSurfacesSnapshot,
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
        tracked_surfaces: HashMap<String, SmithayTrackedSurfaceKind>,
        popup_parent_links: HashMap<String, SmithayPopupParentLink>,
        pending_discovery_events: Vec<BackendDiscoveryEvent>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SmithayTrackedSurfaceKind {
        Toplevel,
        Popup,
        Layer,
        Unmanaged,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SmithayPopupParentLink {
        parent: SmithayPopupParentSnapshot,
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
                tracked_surfaces: HashMap::new(),
                popup_parent_links: HashMap::new(),
                pending_discovery_events: Vec::new(),
            })
        }

        pub fn bind_auto_socket_source(&self) -> Result<ListeningSocketSource, SmithayStateError> {
            ListeningSocketSource::new_auto().map_err(Into::into)
        }

        pub fn take_discovery_events(&mut self) -> Vec<BackendDiscoveryEvent> {
            std::mem::take(&mut self.pending_discovery_events)
        }

        #[cfg(test)]
        pub(crate) fn track_test_surface_snapshot(&mut self, snapshot: BackendSurfaceSnapshot) {
            if let BackendSurfaceSnapshot::Window {
                surface_id,
                window_id,
                ..
            } = &snapshot
            {
                self.toplevel_window_ids
                    .insert(surface_id.clone(), window_id.clone());
            }

            self.track_surface_snapshot(snapshot);
        }

        pub fn snapshot(&self) -> SmithayStateSnapshot {
            let role_counts = self.role_counts();
            let known_surfaces = self.known_surfaces_snapshot();
            SmithayStateSnapshot {
                seat_name: self.seat_name.clone(),
                tracked_surface_count: self.tracked_surfaces.len(),
                tracked_toplevel_count: self.toplevel_window_ids.len(),
                pending_discovery_event_count: self.pending_discovery_events.len(),
                role_counts,
                known_surfaces,
            }
        }

        fn track_surface_snapshot(&mut self, snapshot: BackendSurfaceSnapshot) {
            let (surface_id, kind) = match &snapshot {
                BackendSurfaceSnapshot::Window { surface_id, .. } => {
                    (surface_id.clone(), SmithayTrackedSurfaceKind::Toplevel)
                }
                BackendSurfaceSnapshot::Popup { surface_id, .. } => {
                    (surface_id.clone(), SmithayTrackedSurfaceKind::Popup)
                }
                BackendSurfaceSnapshot::Layer { surface_id, .. } => {
                    (surface_id.clone(), SmithayTrackedSurfaceKind::Layer)
                }
                BackendSurfaceSnapshot::Unmanaged { surface_id } => {
                    (surface_id.clone(), SmithayTrackedSurfaceKind::Unmanaged)
                }
            };

            if self.tracked_surfaces.contains_key(&surface_id) {
                return;
            }

            self.tracked_surfaces.insert(surface_id, kind);

            self.pending_discovery_events
                .push(snapshot_into_discovery_event(snapshot));
        }

        fn track_surface_loss_by_id(&mut self, surface_id: String) {
            if self.tracked_surfaces.remove(&surface_id).is_some() {
                self.popup_parent_links.remove(&surface_id);
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
            let parent = surface
                .get_parent_surface()
                .map(|parent| {
                    let parent_surface_id = smithay_surface_id(&parent);
                    SmithayPopupParentSnapshot::Resolved {
                        window_id: self.toplevel_window_ids.get(&parent_surface_id).cloned(),
                        surface_id: parent_surface_id,
                    }
                })
                .unwrap_or(SmithayPopupParentSnapshot::Unresolved);

            self.popup_parent_links.insert(
                surface_id.clone(),
                SmithayPopupParentLink {
                    parent: parent.clone(),
                },
            );

            let parent_surface_id = popup_parent_surface_id(&parent, &surface_id);

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
                    } else if self.tracked_surfaces.contains_key(&surface_id) {
                        self.toplevel_window_ids.remove(&surface_id);
                        self.track_surface_loss_by_id(surface_id);
                    }
                }
                Some(XDG_POPUP_ROLE) => {
                    if surface_has_buffer(&root) {
                        self.track_popup_surface_by_root(&root);
                    } else if self.tracked_surfaces.contains_key(&surface_id) {
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
            let parent = with_states(surface, |states| {
                states
                    .data_map
                    .get::<XdgPopupSurfaceData>()
                    .and_then(|data| {
                        data.lock().ok().and_then(|data| {
                            data.parent.clone().map(|parent| {
                                let parent_surface_id = smithay_surface_id(&parent);
                                SmithayPopupParentSnapshot::Resolved {
                                    window_id: self
                                        .toplevel_window_ids
                                        .get(&parent_surface_id)
                                        .cloned(),
                                    surface_id: parent_surface_id,
                                }
                            })
                        })
                    })
            })
            .unwrap_or(SmithayPopupParentSnapshot::Unresolved);

            self.popup_parent_links.insert(
                surface_id.clone(),
                SmithayPopupParentLink {
                    parent: parent.clone(),
                },
            );

            let parent_surface_id = popup_parent_surface_id(&parent, &surface_id);

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

        fn role_counts(&self) -> SmithaySurfaceRoleCounts {
            let mut counts = SmithaySurfaceRoleCounts {
                toplevel: 0,
                popup: 0,
                unmanaged: 0,
                layer: 0,
            };

            for kind in self.tracked_surfaces.values() {
                match kind {
                    SmithayTrackedSurfaceKind::Toplevel => counts.toplevel += 1,
                    SmithayTrackedSurfaceKind::Popup => counts.popup += 1,
                    SmithayTrackedSurfaceKind::Unmanaged => counts.unmanaged += 1,
                    SmithayTrackedSurfaceKind::Layer => counts.layer += 1,
                }
            }

            counts
        }

        fn known_surfaces_snapshot(&self) -> SmithayKnownSurfacesSnapshot {
            let mut toplevels = self
                .toplevel_window_ids
                .iter()
                .map(|(surface_id, window_id)| SmithayKnownToplevelSurface {
                    surface_id: surface_id.clone(),
                    window_id: window_id.clone(),
                })
                .collect::<Vec<_>>();
            toplevels.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));

            let mut popups = self
                .tracked_surfaces
                .iter()
                .filter_map(|(surface_id, kind)| {
                    (*kind == SmithayTrackedSurfaceKind::Popup).then(|| {
                        let parent = self.popup_parent_links.get(surface_id);
                        SmithayKnownPopupSurface {
                            surface_id: surface_id.clone(),
                            parent: parent
                                .map(|parent| parent.parent.clone())
                                .unwrap_or(SmithayPopupParentSnapshot::Unresolved),
                        }
                    })
                })
                .collect::<Vec<_>>();
            popups.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));

            let mut unmanaged = self
                .tracked_surfaces
                .iter()
                .filter_map(|(surface_id, kind)| {
                    (*kind == SmithayTrackedSurfaceKind::Unmanaged).then(|| {
                        SmithayKnownUnmanagedSurface {
                            surface_id: surface_id.clone(),
                        }
                    })
                })
                .collect::<Vec<_>>();
            unmanaged.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));

            let mut layers = self
                .tracked_surfaces
                .iter()
                .filter_map(|(surface_id, kind)| {
                    (*kind == SmithayTrackedSurfaceKind::Layer).then(|| SmithayKnownLayerSurface {
                        surface_id: surface_id.clone(),
                    })
                })
                .collect::<Vec<_>>();
            layers.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));

            let mut all =
                Vec::with_capacity(toplevels.len() + popups.len() + unmanaged.len() + layers.len());
            all.extend(toplevels.iter().cloned().map(SmithayKnownSurface::Toplevel));
            all.extend(popups.iter().cloned().map(SmithayKnownSurface::Popup));
            all.extend(layers.iter().cloned().map(SmithayKnownSurface::Layer));
            all.extend(
                unmanaged
                    .iter()
                    .cloned()
                    .map(SmithayKnownSurface::Unmanaged),
            );
            all.sort_by(|left, right| {
                known_surface_sort_key(left).cmp(&known_surface_sort_key(right))
            });

            SmithayKnownSurfacesSnapshot {
                all,
                toplevels,
                popups,
                unmanaged,
                layers,
            }
        }
    }

    fn known_surface_sort_key(surface: &SmithayKnownSurface) -> (&'static str, &str) {
        match surface {
            SmithayKnownSurface::Toplevel(surface) => ("toplevel", &surface.surface_id),
            SmithayKnownSurface::Popup(surface) => ("popup", &surface.surface_id),
            SmithayKnownSurface::Layer(surface) => ("layer", &surface.surface_id),
            SmithayKnownSurface::Unmanaged(surface) => ("unmanaged", &surface.surface_id),
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

    fn popup_parent_surface_id(parent: &SmithayPopupParentSnapshot, surface_id: &str) -> String {
        match parent {
            SmithayPopupParentSnapshot::Resolved { surface_id, .. } => surface_id.clone(),
            SmithayPopupParentSnapshot::Unresolved => format!("unresolved-parent-{surface_id}"),
        }
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

        #[test]
        fn smithay_state_snapshot_reports_tracked_counts() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let before = state.snapshot();
            assert_eq!(before.seat_name, "test-seat");
            assert_eq!(before.tracked_surface_count, 0);
            assert_eq!(before.tracked_toplevel_count, 0);
            assert_eq!(before.pending_discovery_event_count, 0);
            assert_eq!(before.role_counts.toplevel, 0);
            assert_eq!(before.role_counts.popup, 0);
            assert_eq!(before.role_counts.unmanaged, 0);
            assert_eq!(before.role_counts.layer, 0);
            assert!(before.known_surfaces.all.is_empty());
            assert!(before.known_surfaces.toplevels.is_empty());
            assert!(before.known_surfaces.popups.is_empty());
            assert!(before.known_surfaces.unmanaged.is_empty());
            assert!(before.known_surfaces.layers.is_empty());

            let window_id = state.window_id_for_surface("wl-surface-101");
            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-101".into(),
                window_id,
                output_id: None,
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-surface-102".into(),
            });

            let after = state.snapshot();
            assert_eq!(after.tracked_surface_count, 2);
            assert_eq!(after.tracked_toplevel_count, 1);
            assert_eq!(after.pending_discovery_event_count, 2);
            assert_eq!(after.role_counts.toplevel, 1);
            assert_eq!(after.role_counts.popup, 0);
            assert_eq!(after.role_counts.unmanaged, 1);
            assert_eq!(after.role_counts.layer, 0);
            assert_eq!(after.known_surfaces.all.len(), 2);
            assert_eq!(after.known_surfaces.toplevels.len(), 1);
            assert_eq!(after.known_surfaces.unmanaged.len(), 1);

            let _ = state.take_discovery_events();
            let drained = state.snapshot();
            assert_eq!(drained.pending_discovery_event_count, 0);
            assert_eq!(drained.tracked_surface_count, 2);
        }

        #[test]
        fn smithay_state_snapshot_reports_role_breakdown() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let window_id = state.window_id_for_surface("wl-surface-201");
            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-201".into(),
                window_id,
                output_id: None,
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-surface-202".into(),
                output_id: None,
                parent_surface_id: "wl-surface-201".into(),
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-surface-203".into(),
                output_id: "out-1".into(),
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-surface-204".into(),
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.tracked_surface_count, 4);
            assert_eq!(snapshot.role_counts.toplevel, 1);
            assert_eq!(snapshot.role_counts.popup, 1);
            assert_eq!(snapshot.role_counts.layer, 1);
            assert_eq!(snapshot.role_counts.unmanaged, 1);
            assert_eq!(snapshot.known_surfaces.all.len(), 4);
            assert_eq!(snapshot.known_surfaces.toplevels.len(), 1);
            assert_eq!(snapshot.known_surfaces.popups.len(), 1);
            assert_eq!(snapshot.known_surfaces.layers.len(), 1);
            assert_eq!(snapshot.known_surfaces.unmanaged.len(), 1);
        }

        #[test]
        fn smithay_state_snapshot_reports_popup_parent_window_identity() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let parent_window_id = state.window_id_for_surface("wl-surface-301");
            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-301".into(),
                window_id: parent_window_id.clone(),
                output_id: None,
            });
            state.popup_parent_links.insert(
                "wl-surface-302".into(),
                SmithayPopupParentLink {
                    parent: SmithayPopupParentSnapshot::Resolved {
                        surface_id: "wl-surface-301".into(),
                        window_id: Some(parent_window_id.clone()),
                    },
                },
            );
            state.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-surface-302".into(),
                output_id: None,
                parent_surface_id: "wl-surface-301".into(),
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.popups.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.popups[0].parent,
                SmithayPopupParentSnapshot::Resolved {
                    surface_id: "wl-surface-301".into(),
                    window_id: Some(parent_window_id),
                }
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_unresolved_popup_parent() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.popup_parent_links.insert(
                "wl-surface-401".into(),
                SmithayPopupParentLink {
                    parent: SmithayPopupParentSnapshot::Unresolved,
                },
            );
            state.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-surface-401".into(),
                output_id: None,
                parent_surface_id: "unresolved-parent-wl-surface-401".into(),
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.popups.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.popups[0].parent,
                SmithayPopupParentSnapshot::Unresolved
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_unified_known_surface_order() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let window_id = state.window_id_for_surface("wl-surface-501");
            state.track_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-surface-504".into(),
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-surface-502".into(),
                output_id: None,
                parent_surface_id: "unresolved-parent-wl-surface-502".into(),
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-surface-503".into(),
                output_id: "out-1".into(),
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-501".into(),
                window_id,
                output_id: None,
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.all.len(), 4);
            assert!(matches!(
                &snapshot.known_surfaces.all[0],
                SmithayKnownSurface::Layer(surface) if surface.surface_id == "wl-surface-503"
            ));
            assert!(matches!(
                &snapshot.known_surfaces.all[1],
                SmithayKnownSurface::Popup(surface) if surface.surface_id == "wl-surface-502"
            ));
            assert!(matches!(
                &snapshot.known_surfaces.all[2],
                SmithayKnownSurface::Toplevel(surface) if surface.surface_id == "wl-surface-501"
            ));
            assert!(matches!(
                &snapshot.known_surfaces.all[3],
                SmithayKnownSurface::Unmanaged(surface) if surface.surface_id == "wl-surface-504"
            ));
        }
    }
}

#[cfg(feature = "smithay-winit")]
pub use imp::{
    SmithayClientState, SmithayKnownLayerSurface, SmithayKnownPopupSurface, SmithayKnownSurface,
    SmithayKnownSurfacesSnapshot, SmithayKnownToplevelSurface, SmithayKnownUnmanagedSurface,
    SmithayPopupParentSnapshot, SmithayStateError, SmithayStateSnapshot, SmithaySurfaceRoleCounts,
    SpidersSmithayState,
};
