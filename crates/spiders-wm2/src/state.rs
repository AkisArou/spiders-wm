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
use smithay::utils::{Logical, Point, SERIAL_COUNTER, Serial, Size};
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

use crate::frame_sync::{FrameSyncState, Transaction, WindowFrameSyncState, Wm2RenderElements, plan_tiled_slot, plan_tiled_slots};

pub struct SpidersWm2 {
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

    managed_windows: Vec<ManagedWindow>,
    frame_sync: FrameSyncState,
}

pub(crate) struct ManagedWindow {
    pub(crate) window: Window,
    pub(crate) mapped: bool,
    pub(crate) frame_sync: WindowFrameSyncState,
}

impl SpidersWm2 {
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

            let override_terminal = std::env::var("SPIDERS_WM2_TERMINAL").ok();
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
                "Alt+Enter requested a terminal, but no supported terminal binary was found in PATH; set SPIDERS_WM2_TERMINAL to override"
            );
    }

    pub fn close_focused_window(&self) {
        let Some(focused_surface) = self.focused_surface.as_ref() else {
            return;
        };

        if let Some(record) = self
            .managed_windows
            .iter()
            .find(|record| record.wl_surface() == *focused_surface)
        {
            if let Some(toplevel) = record.window.toplevel() {
                toplevel.send_close();
            }
        }
    }

    pub fn set_focus(&mut self, surface: Option<WlSurface>, serial: Serial) {
        self.focused_surface = surface.clone();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, surface, serial);
        }

        for record in &self.managed_windows {
            let active = self
                .focused_surface
                .as_ref()
                .is_some_and(|focused| record.wl_surface() == *focused);
            record.window.set_activated(active);
            if let Some(toplevel) = record.window.toplevel() {
                let _ = toplevel.send_pending_configure();
            }
        }
    }

    pub fn add_window(&mut self, window: Window) {
        self.managed_windows.push(ManagedWindow {
            window,
            mapped: false,
            frame_sync: WindowFrameSyncState::default(),
        });
    }

    fn has_active_frame_sync(&self) -> bool {
        self.frame_sync
            .has_active_transitions(self.managed_windows.iter().map(|record| &record.frame_sync))
    }

    fn flush_queued_relayout(&mut self) {
        if self.has_active_frame_sync() || !self.frame_sync.take_queued_relayout() {
            return;
        }

        self.start_relayout(None);
    }

    /// Handles window close events with frame-perfect relayout coordination.
    ///
    /// # Close-Relayout Coordination
    ///
    /// When a window closes, we need to ensure:
    /// 1. The closing window's snapshot remains visible during close animation
    /// 2. Remaining windows relayout to new positions atomically
    /// 3. New layout doesn't appear until close animation finishes
    ///
    /// This is achieved through a transactional close-then-relayout flow:
    /// - **Close Transaction**: Lifetime from close until snapshot disappears
    /// - **Relayout Transaction**: Started when close completes
    /// - Both transactions block commits until complete
    /// - Overlays ensure visual continuity through the transition
    ///
    /// # Implementation
    ///
    /// - Capture window snapshot before unmapping (for close animation)
    /// - Create close transaction (monitors snapshot overlay)
    /// - Unmap window and update focus
    /// - Queue relayout to execute after close completes
    /// - Frame loop handles snapshot advance and overlay cleanup
    ///
    /// # Edge Cases
    ///
    /// - Multiple rapid closes: Each gets a transaction; relayout queues and deduplicates
    /// - Close during relayout: New close transaction created; relayout re-queued after
    /// - Last window closes: Relayout handles empty managed_windows gracefully
    pub fn handle_window_close(&mut self, surface: &WlSurface) {
        let Some(position) = self
            .managed_windows
            .iter()
            .position(|record| record.wl_surface() == *surface)
        else {
            return;
        };

        let record = self.managed_windows.remove(position);
        let transaction = Transaction::new();
        let monitor = transaction.monitor();

        if record.mapped {
            if let (Some(snapshot), Some(element_location)) = (
                record.frame_sync.snapshot_owned(),
                self.space.element_location(&record.window),
            ) {
                self.frame_sync.push_closing_window(snapshot.into_closing_window(
                    element_location,
                    record.window.geometry().loc,
                    monitor,
                ));
            }
            self.space.unmap_elem(&record.window);
        }

        if self
            .focused_surface
            .as_ref()
            .is_some_and(|focused| focused == surface)
        {
            let next_focus = self.managed_windows.last().map(ManagedWindow::wl_surface);
            self.set_focus(next_focus, SERIAL_COUNTER.next_serial());
        }

        self.schedule_relayout_with_transaction(Some(transaction));
    }

    pub fn find_window_mut(&mut self, surface: &WlSurface) -> Option<&mut ManagedWindow> {
        self.managed_windows
            .iter_mut()
            .find(|record| record.wl_surface() == *surface)
    }

    pub fn is_known_window_mapped(&self, surface: &WlSurface) -> bool {
        self.managed_windows
            .iter()
            .find(|record| record.wl_surface() == *surface)
            .is_some_and(|record| record.mapped)
    }

    pub fn handle_window_commit(&mut self, surface: &WlSurface) {
        let window_update = if let Some(record) = self.find_window_mut(surface) {
            let update = record.frame_sync.consume_commit_update(record.mapped);
            if !record.mapped && update.pending_location.is_some() {
                record.mapped = true;
                record.frame_sync.mark_snapshot_dirty();
            }

            Some((record.window.clone(), update.pending_location, update.first_map))
        } else {
            None
        };

        if let Some((window, pending_location, first_map)) = window_update {
            window.on_commit();

            if first_map {
                self.schedule_relayout();
                self.set_focus(Some(surface.clone()), SERIAL_COUNTER.next_serial());

                if let Some(record) = self.find_window_mut(surface) {
                    let pending_location = record.frame_sync.take_pending_location();
                    if pending_location.is_some() {
                        record.mapped = true;
                        record.frame_sync.mark_snapshot_dirty();
                    }

                    if let Some(location) = pending_location {
                        self.space.map_element(window, location, false);
                    }
                }

                return;
            }

            let location = pending_location.or_else(|| {
                self.find_window_mut(surface)
                    .and_then(|record| record.frame_sync.pending_location())
            });

            if let Some(location) = location {
                self.space.map_element(window, location, false);
            }
        }
    }

    pub fn notify_blocker_cleared(&mut self) {
        let display_handle = self.display_handle.clone();
        while let Ok(client) = self.blocker_cleared_rx.try_recv() {
            trace!("calling blocker_cleared");
            self.client_compositor_state(&client)
                .blocker_cleared(self, &display_handle);
        }
    }

    pub fn refresh_window_snapshots(&mut self, renderer: &mut GlesRenderer) {
        self.frame_sync.refresh_window_snapshots(
            renderer,
            self.managed_windows
                .iter_mut()
                .map(|record| (&record.window, record.mapped, &mut record.frame_sync)),
        );
    }

    pub fn advance_closing_windows(&mut self) {
        self.frame_sync.advance_closing_windows();
        self.flush_queued_relayout();
    }

    pub fn advance_resize_overlays(&mut self) {
        let remaps = self.frame_sync.finished_resize_overlay_mappings(
            self.managed_windows
                .iter_mut()
                .map(|record| (&record.window, &mut record.frame_sync)),
        );
        self.frame_sync.advance_resize_overlays(&mut self.space, remaps);
        self.flush_queued_relayout();
    }

    pub fn transition_render_elements(&self) -> Vec<Wm2RenderElements> {
        self.frame_sync
            .render_elements(self.managed_windows.iter().map(|record| &record.frame_sync))
    }

    pub fn schedule_relayout(&mut self) {
        self.schedule_relayout_with_transaction(None);
    }

    pub fn schedule_relayout_with_transaction(&mut self, transaction: Option<Transaction>) {
        if transaction.is_none() && self.has_active_frame_sync() {
            self.frame_sync.queue_relayout();
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
        let index = self
            .managed_windows
            .iter()
            .position(|record| record.wl_surface() == *surface)?;

        let slot = plan_tiled_slot(output_geometry, self.managed_windows.len(), index)?;
        Some((slot.location, slot.size))
    }

    fn start_relayout(&mut self, transaction: Option<Transaction>) {
        if self.managed_windows.is_empty() {
            return;
        }

        let output = self
            .space
            .outputs()
            .next()
            .expect("output must exist before relayout");
        let output_geometry = self
            .space
            .output_geometry(output)
            .expect("output geometry missing during relayout");

        let transaction = transaction.unwrap_or_else(Transaction::new);
        let slots = plan_tiled_slots(output_geometry, self.managed_windows.len());

        for (index, slot) in slots.into_iter().enumerate() {
            let current_location = self
                .space
                .element_location(&self.managed_windows[index].window);
            let toplevel = self.managed_windows[index].window.toplevel().cloned();

            if let Some(toplevel) = toplevel {
                let record = &mut self.managed_windows[index];
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
                    record.frame_sync.push_pending_configure_transaction(serial);
                }

                if let Some(location) = action.map_now {
                    self.space.map_element(record.window.clone(), location, false);
                }
            }
        }

        drop(transaction);
    }

    pub fn send_frames_for_windows(&self, output: &smithay::output::Output) {
        for record in &self.managed_windows {
            if !(record.mapped || record.frame_sync.pending_location().is_some()) {
                continue;
            }

            record.window.send_frame(
                output,
                self.start_time.elapsed(),
                Some(std::time::Duration::ZERO),
                |_, _| Some(output.clone()),
            );
        }
    }
}

impl ManagedWindow {
    fn wl_surface(&self) -> WlSurface {
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

impl SelectionHandler for SpidersWm2 {
    type SelectionUserData = ();
}

impl DataDeviceHandler for SpidersWm2 {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl OutputHandler for SpidersWm2 {}
