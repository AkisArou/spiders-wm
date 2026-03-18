use std::{
    collections::HashMap, // , ffi::OsString, sync::Arc
};

// use smithay::{
//     desktop::{PopupManager, Space, Window, WindowSurfaceType},
//     input::{Seat, SeatState},
//     reexports::{
//         calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction, generic::Generic},
//         wayland_server::{
//             Display, DisplayHandle, Resource,
//             backend::{ClientData, ClientId, DisconnectReason},
//             protocol::wl_surface::WlSurface,
//         },
//     },
//     utils::{Logical, Point, Serial},
//     wayland::{
//         compositor::{CompositorClientState, CompositorState},
//         output::OutputManagerState,
//         selection::data_device::DataDeviceState,
//         shell::xdg::XdgShellState,
//         shm::ShmState,
//         socket::ListeningSocketSource,
//     },
// };

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutputId(pub u64);

#[derive(Debug, Default)]
pub struct TopologyState {
    pub windows: HashMap<WindowId, WindowNode>,
    pub outputs: HashMap<OutputId, OutputNode>,
}

#[derive(Debug)]
pub struct WindowNode {
    pub alive: bool,
    pub mapped: bool,
    pub title: Option<String>,
    pub app_id: Option<String>,
}

#[derive(Debug)]
pub struct OutputNode {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug)]
pub struct WmState {
    pub active_workspace: WorkspaceId,
    pub focused_window: Option<WindowId>,
    pub focused_output: Option<OutputId>,
    pub workspaces: HashMap<WorkspaceId, WorkspaceState>,
    pub windows: HashMap<WindowId, ManagedWindowState>,
}

#[derive(Debug)]
pub struct WorkspaceState {
    pub name: String,
    pub output: Option<OutputId>,
    pub windows: Vec<WindowId>,
}

#[derive(Debug)]
pub struct ManagedWindowState {
    pub workspace: WorkspaceId,
    pub floating: bool,
    pub fullscreen: bool,
}

impl Default for WmState {
    fn default() -> Self {
        let ws = WorkspaceId(1);

        let mut workspaces = HashMap::new();

        workspaces.insert(
            ws,
            WorkspaceState {
                name: "1".into(),
                output: None,
                windows: Vec::new(),
            },
        );

        Self {
            active_workspace: ws,
            focused_window: None,
            focused_output: None,
            workspaces,
            windows: HashMap::new(),
        }
    }
}

// pub struct SpidersWm2 {
//     pub start_time: std::time::Instant,
//     pub socket_name: OsString,
//     pub display_handle: DisplayHandle,
//
//     pub space: Space<Window>,
//     pub loop_signal: LoopSignal,
//
//     pub compositor_state: CompositorState,
//     pub xdg_shell_state: XdgShellState,
//     pub shm_state: ShmState,
//     #[allow(dead_code)]
//     pub output_manager_state: OutputManagerState,
//     pub seat_state: SeatState<SpidersWm2>,
//     pub data_device_state: DataDeviceState,
//     pub popups: PopupManager,
//
//     pub seat: Seat<Self>,
// }
//
// impl SpidersWm2 {
//     pub fn new(event_loop: &mut EventLoop<Self>, display: Display<Self>) -> Self {
//         let start_time = std::time::Instant::now();
//         let display_handle = display.handle();
//
//         let compositor_state = CompositorState::new::<Self>(&display_handle);
//         let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
//         let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
//         let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
//         let data_device_state = DataDeviceState::new::<Self>(&display_handle);
//         let popups = PopupManager::default();
//
//         let mut seat_state = SeatState::new();
//         let mut seat: Seat<Self> = seat_state.new_wl_seat(&display_handle, "winit");
//         seat.add_keyboard(Default::default(), 200, 25)
//             .expect("failed to create keyboard");
//         seat.add_pointer();
//
//         let space = Space::default();
//         let socket_name = Self::init_wayland_listener(display, event_loop);
//         let loop_signal = event_loop.get_signal();
//
//         Self {
//             start_time,
//             socket_name,
//             display_handle,
//             space,
//             loop_signal,
//             compositor_state,
//             xdg_shell_state,
//             shm_state,
//             output_manager_state,
//             seat_state,
//             data_device_state,
//             popups,
//             seat,
//         }
//     }
//
//     fn init_wayland_listener(display: Display<Self>, event_loop: &mut EventLoop<Self>) -> OsString {
//         let listening_socket =
//             ListeningSocketSource::new_auto().expect("failed to create wayland socket");
//         let socket_name = listening_socket.socket_name().to_os_string();
//         let loop_handle = event_loop.handle();
//
//         loop_handle
//             .insert_source(listening_socket, move |client_stream, _, state| {
//                 state
//                     .display_handle
//                     .insert_client(client_stream, Arc::new(ClientState::default()))
//                     .expect("failed to insert client");
//             })
//             .expect("failed to add listening socket source");
//
//         loop_handle
//             .insert_source(
//                 Generic::new(display, Interest::READ, Mode::Level),
//                 |_, display, state| {
//                     // SAFETY: the display lives for the duration of the event loop source.
//                     unsafe {
//                         display.get_mut().dispatch_clients(state).unwrap();
//                     }
//                     Ok(PostAction::Continue)
//                 },
//             )
//             .expect("failed to add wayland display source");
//
//         socket_name
//     }
//
//     pub fn surface_under(
//         &self,
//         pos: Point<f64, Logical>,
//     ) -> Option<(WlSurface, Point<f64, Logical>)> {
//         self.space
//             .element_under(pos)
//             .and_then(|(window, location)| {
//                 window
//                     .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
//                     .map(|(surface, point)| (surface, (point + location).to_f64()))
//             })
//     }
//
//     pub fn focus_window(&mut self, window: Option<Window>, serial: Serial) {
//         let focused_surface = window.as_ref().and_then(|window| {
//             window
//                 .toplevel()
//                 .map(|toplevel| toplevel.wl_surface().clone())
//         });
//
//         if let Some(window) = window.as_ref() {
//             self.space.raise_element(window, true);
//         }
//
//         self.space.elements().for_each(|mapped| {
//             let is_focused = focused_surface.as_ref().is_some_and(|surface| {
//                 mapped
//                     .toplevel()
//                     .is_some_and(|toplevel| toplevel.wl_surface().id() == surface.id())
//             });
//
//             mapped.set_activated(is_focused);
//
//             if let Some(toplevel) = mapped.toplevel() {
//                 toplevel.send_pending_configure();
//             }
//         });
//
//         if let Some(keyboard) = self.seat.get_keyboard() {
//             keyboard.set_focus(self, focused_surface, serial);
//         }
//     }
// }
//
// #[derive(Default)]
// pub struct ClientState {
//     pub compositor_state: CompositorClientState,
// }
//
// impl ClientData for ClientState {
//     fn initialized(&self, _client_id: ClientId) {}
//
//     fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
// }
