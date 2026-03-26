use std::ffi::OsString;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::desktop::{PopupManager, Space, Window, WindowSurfaceType};
use smithay::input::{Seat, SeatState};
use smithay::reexports::calloop::{
    EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction, generic::Generic,
};
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, Display, DisplayHandle};
use smithay::utils::{Logical, Point, Size};
use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::DataDeviceHandler;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::{error, info, trace};

use crate::frame_sync::{FrameSyncState, Transaction, WindowFrameSyncState, plan_tiled_slot, plan_tiled_slots};
use crate::model::{WindowId, wm::WmModel};
use crate::runtime::{RuntimeCommand, WmRuntime};

pub struct SpidersWm {
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub event_loop: LoopHandle<'static, Self>,
    pub loop_signal: LoopSignal,
    pub blocker_cleared_tx: Sender<Client>,
    pub blocker_cleared_rx: Receiver<Client>,

    pub space: Space<Window>,
    pub popups: PopupManager,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    pub seat_state: SeatState<Self>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<Self>,
    pub backend: Option<WinitGraphicsBackend<GlesRenderer>>,

    pub focused_surface: Option<WlSurface>,

    pub(crate) managed_windows: Vec<ManagedWindow>,
    pub(crate) frame_sync: FrameSyncState,
    pub(crate) model: WmModel,
    pub(crate) next_window_id: u64,
}

pub(crate) struct ManagedWindow {
    pub(crate) id: WindowId,
    pub(crate) window: Window,
    pub(crate) mapped: bool,
    pub(crate) frame_sync: WindowFrameSyncState,
}

impl SpidersWm {
    pub fn new(event_loop: &mut EventLoop<'static, Self>, display: Display<Self>) -> Self {
        let start_time = std::time::Instant::now();
        let display_handle = display.handle();
        let compositor_state = CompositorState::new::<Self>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
        let dmabuf_state = DmabufState::new();
        let _output_manager_state =
            OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);
        let popups = PopupManager::default();
        let (blocker_cleared_tx, blocker_cleared_rx) = mpsc::channel();

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&display_handle, "winit");
        seat.add_keyboard(Default::default(), 200, 25)
            .expect("failed to create keyboard");
        seat.add_pointer();

        let socket_name = Self::init_wayland_listener(display, event_loop);
        let mut model = WmModel::default();
        {
            let mut runtime = WmRuntime::new(&mut model);
            let _ = runtime.execute(RuntimeCommand::EnsureDefaultWorkspace {
                name: "1".to_string(),
            });
            let _ = runtime.execute(RuntimeCommand::SelectWorkspace {
                workspace_id: "1".into(),
            });
            let _ = runtime.execute(RuntimeCommand::SelectNextWorkspace);
            let _ = runtime.execute(RuntimeCommand::EnsureSeat {
                seat_id: "winit".into(),
            });
        }

        Self {
            start_time,
            socket_name,
            display_handle,
            event_loop: event_loop.handle(),
            loop_signal: event_loop.get_signal(),
            blocker_cleared_tx,
            blocker_cleared_rx,
            space: Space::default(),
            popups,
            compositor_state,
            xdg_shell_state,
            shm_state,
            dmabuf_state,
            dmabuf_global: None,
            seat_state,
            data_device_state,
            seat,
            backend: None,
            focused_surface: None,
            managed_windows: Vec::new(),
            frame_sync: FrameSyncState::default(),
            model,
            next_window_id: 1,
        }
    }

    fn init_wayland_listener(
        display: Display<Self>,
        event_loop: &mut EventLoop<'static, Self>,
    ) -> OsString {
        let listening_socket =
            ListeningSocketSource::new_auto().expect("failed to create Wayland socket");
        let socket_name = listening_socket.socket_name().to_os_string();
        let loop_handle = event_loop.handle();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .expect("failed to insert Wayland client");
            })
            .expect("failed to register Wayland listening socket");

        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    unsafe {
                        display
                            .get_mut()
                            .dispatch_clients(state)
                            .expect("failed to dispatch clients");
                    }
                    Ok(PostAction::Continue)
                },
            )
            .expect("failed to register Wayland display source");

        socket_name
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(surface, point)| (surface, (point + location).to_f64()))
            })
    }

    pub fn window_id_for_surface(&self, surface: &WlSurface) -> Option<WindowId> {
        self.managed_window_for_surface(surface).map(|record| record.id)
    }

    pub fn managed_window_for_surface(&self, surface: &WlSurface) -> Option<&ManagedWindow> {
        self.managed_windows
            .iter()
            .find(|record| record.wl_surface() == *surface)
    }

    pub fn managed_window_mut_for_surface(&mut self, surface: &WlSurface) -> Option<&mut ManagedWindow> {
        self.managed_windows
            .iter_mut()
            .find(|record| record.wl_surface() == *surface)
    }

    pub fn managed_window_position_for_surface(&self, surface: &WlSurface) -> Option<usize> {
        self.managed_windows
            .iter()
            .position(|record| record.wl_surface() == *surface)
    }

    pub fn surface_for_window_id(&self, window_id: WindowId) -> Option<WlSurface> {
        self.managed_windows
            .iter()
            .find(|record| record.id == window_id)
            .map(ManagedWindow::wl_surface)
    }

    pub fn window_id_under(&self, pos: Point<f64, Logical>) -> Option<WindowId> {
        self.space
            .element_under(pos)
            .and_then(|(window, _)| window.toplevel().map(|toplevel| toplevel.wl_surface().clone()))
            .and_then(|surface| self.window_id_for_surface(&surface))
    }

    pub fn visible_managed_window_positions(&self) -> Vec<usize> {
        self.managed_windows
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                self.model
                    .window_is_on_current_workspace(record.id)
                    .then_some(index)
            })
            .collect()
    }

    pub fn spawn_foot(&self) {
            const FALLBACK_TERMINALS: &[&str] = &[
                "foot",
                "footclient",
                "weston-terminal",
                "alacritty",
                "kitty",
                "wezterm",
                "gnome-terminal",
                "konsole",
                "xfce4-terminal",
                "terminator",
                "xterm",
                "st",
                "urxvt",
            ];

            let override_terminal = std::env::var("SPIDERS_WM_TERMINAL").ok();
            let candidates: Vec<&str> = override_terminal
                .as_deref()
                .into_iter()
                .chain(FALLBACK_TERMINALS.iter().copied())
                .collect();

            for terminal in candidates {
                let mut command = Command::new(terminal);
                command.env("WAYLAND_DISPLAY", &self.socket_name);

                match command.spawn() {
                    Ok(_) => {
                        info!(terminal, "spawned terminal for Alt+Enter");
                        return;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(err) => {
                        error!(terminal, %err, "failed to spawn terminal");
                        return;
                    }
                }
            }

            error!(
                "Alt+Enter requested a terminal, but no supported terminal binary was found in PATH; set SPIDERS_WM_TERMINAL to override"
            );
    }

    pub(crate) fn flush_queued_relayout(&mut self) {
        if !self
            .frame_sync
            .take_ready_relayout(self.managed_windows.iter().map(|record| &record.frame_sync))
        {
            return;
        }

        self.start_relayout(None);
    }

    

    pub fn notify_blocker_cleared(&mut self) {
        let display_handle = self.display_handle.clone();
        while let Ok(client) = self.blocker_cleared_rx.try_recv() {
            trace!("calling blocker_cleared");
            self.client_compositor_state(&client)
                .blocker_cleared(self, &display_handle);
        }
    }

    pub fn schedule_relayout(&mut self) {
        self.schedule_relayout_with_transaction(None);
    }

    pub fn schedule_relayout_with_transaction(&mut self, transaction: Option<Transaction>) {
        if transaction.is_none()
            && self
                .frame_sync
                .should_defer_relayout(self.managed_windows.iter().map(|record| &record.frame_sync))
        {
            return;
        }

        self.start_relayout(transaction);
    }

    pub fn planned_layout_for_surface(
        &self,
        surface: &WlSurface,
    ) -> Option<(Point<i32, Logical>, Size<i32, Logical>)> {
        let output = self.space.outputs().next()?;
        let output_geometry = self.space.output_geometry(output)?;
        let visible_positions = self.visible_managed_window_positions();
        let index = visible_positions
            .iter()
            .position(|managed_index| self.managed_windows[*managed_index].wl_surface() == *surface)?;

        let slot = plan_tiled_slot(output_geometry, visible_positions.len(), index)?;
        Some((slot.location, slot.size))
    }

    fn start_relayout(&mut self, transaction: Option<Transaction>) {
        let output = self
            .space
            .outputs()
            .next()
            .expect("output must exist before relayout");
        let output_geometry = self
            .space
            .output_geometry(output)
            .expect("output geometry missing during relayout");

        let visible_positions = self.visible_managed_window_positions();
        for record in &self.managed_windows {
            if !self.model.window_is_on_current_workspace(record.id) {
                self.space.unmap_elem(&record.window);
            }
        }

        if visible_positions.is_empty() {
            return;
        }

        let transaction = transaction.unwrap_or_else(Transaction::new);
        let slots = plan_tiled_slots(output_geometry, visible_positions.len());

        for (slot_index, managed_index) in visible_positions.into_iter().enumerate() {
            let slot = slots[slot_index];
            let current_location = self
                .space
                .element_location(&self.managed_windows[managed_index].window);
            let toplevel = self.managed_windows[managed_index].window.toplevel().cloned();

            if let Some(toplevel) = toplevel {
                let record = &mut self.managed_windows[managed_index];
                let mut needs_configure = !record.mapped;
                toplevel.with_pending_state(|state| {
                    if state.size != Some(slot.size) {
                        needs_configure = true;
                    }
                    state.size = Some(slot.size);
                });

                let action = record.frame_sync.plan_relayout(
                    &record.window,
                    record.mapped,
                    current_location,
                    slot.location,
                    slot.size,
                    needs_configure,
                    &transaction,
                );

                if needs_configure {
                    if action.unmap_window {
                        self.space.unmap_elem(&record.window);
                    }
                    let serial = toplevel.send_configure();
                    record.frame_sync.register_sent_configure(serial);
                }

                if let Some(location) = action.map_now {
                    self.space.map_element(record.window.clone(), location, false);
                }
            }
        }

        drop(transaction);
    }

}

impl ManagedWindow {
    pub(crate) fn wl_surface(&self) -> WlSurface {
        self.window
            .toplevel()
            .expect("managed window missing toplevel")
            .wl_surface()
            .clone()
    }
    pub fn toplevel(&self) -> Option<&smithay::wayland::shell::xdg::ToplevelSurface> {
        self.window.toplevel()
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}

    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

impl SelectionHandler for SpidersWm {
    type SelectionUserData = ();
}

impl DataDeviceHandler for SpidersWm {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl OutputHandler for SpidersWm {}
