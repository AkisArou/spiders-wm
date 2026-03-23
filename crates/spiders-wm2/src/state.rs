use std::ffi::OsString;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use smithay::backend::renderer::gles::GlesRenderer;
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
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::DataDeviceHandler;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::{error, info, trace, warn};

use crate::closing::{ClosingWindow, ResizingWindow, WindowSnapshot, Wm2RenderElements};
use crate::transaction::Transaction;

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
    pub seat_state: SeatState<Self>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<Self>,

    pub focused_surface: Option<WlSurface>,

    managed_windows: Vec<ManagedWindow>,
    closing_windows: Vec<ClosingWindow>,
}

pub(crate) struct ManagedWindow {
    pub(crate) window: Window,
    pub(crate) mapped: bool,
    pub(crate) pending_location: Option<Point<i32, Logical>>,
    pub(crate) matched_configure_commit: bool,
    pub(crate) snapshot: Option<WindowSnapshot>,
    pub(crate) resize_overlay: Option<ResizingWindow>,
    pub(crate) snapshot_dirty: bool,
    pub(crate) transaction_for_next_configure: Option<Transaction>,
    pub(crate) pending_transactions: Vec<(Serial, Transaction)>,
}

impl SpidersWm2 {
    pub fn new(event_loop: &mut EventLoop<'static, Self>, display: Display<Self>) -> Self {
        let start_time = std::time::Instant::now();
        let display_handle = display.handle();
        let compositor_state = CompositorState::new::<Self>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
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
            seat_state,
            data_device_state,
            seat,
            focused_surface: None,
            managed_windows: Vec::new(),
            closing_windows: Vec::new(),
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
            pending_location: None,
            matched_configure_commit: false,
            snapshot: None,
            resize_overlay: None,
            snapshot_dirty: true,
            transaction_for_next_configure: None,
            pending_transactions: Vec::new(),
        });
    }

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
                record.snapshot,
                self.space.element_location(&record.window),
            ) {
                self.closing_windows.push(snapshot.into_closing_window(
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
            let first_map = !record.mapped && record.pending_location.is_none();
            let matched_configure_commit = record.matched_configure_commit;
            record.matched_configure_commit = false;
            let pending_location = if first_map || matched_configure_commit {
                record.pending_location.take()
            } else {
                record.pending_location
            };

            if matched_configure_commit {
                record.resize_overlay = None;
            }
            if !record.mapped && pending_location.is_some() {
                record.mapped = true;
                record.snapshot_dirty = true;
            }

            Some((record.window.clone(), pending_location, first_map))
        } else {
            None
        };

        if let Some((window, pending_location, first_map)) = window_update {
            window.on_commit();

            if first_map {
                self.schedule_relayout();
                self.set_focus(Some(surface.clone()), SERIAL_COUNTER.next_serial());

                if let Some(record) = self.find_window_mut(surface) {
                    let pending_location = record.pending_location.take();
                    if pending_location.is_some() {
                        record.mapped = true;
                        record.snapshot_dirty = true;
                    }

                    if let Some(location) = pending_location {
                        self.space.map_element(window, location, false);
                    }
                }

                return;
            }

            let location = pending_location.or_else(|| {
                self.find_window_mut(surface)
                    .and_then(|record| record.pending_location)
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
        for index in 0..self.managed_windows.len() {
            let needs_snapshot = {
                let record = &self.managed_windows[index];
                record.mapped && record.snapshot_dirty
            };
            if !needs_snapshot {
                continue;
            }

            let window = self.managed_windows[index].window.clone();
            match WindowSnapshot::capture(renderer, &window) {
                Ok(Some(snapshot)) => {
                    let record = &mut self.managed_windows[index];
                    record.snapshot = Some(snapshot);
                    record.snapshot_dirty = false;
                }
                Ok(None) => {}
                Err(err) => {
                    warn!(%err, "failed to refresh window snapshot");
                }
            }
        }
    }

    pub fn advance_closing_windows(&mut self) {
        let now = std::time::Instant::now();
        for window in &mut self.closing_windows {
            window.advance(now);
        }
        self.closing_windows.retain(|window| !window.is_finished(now));
    }

    pub fn advance_resize_overlays(&mut self) {
        for record in &mut self.managed_windows {
            let finished = record
                .resize_overlay
                .as_ref()
                .is_some_and(ResizingWindow::is_finished);
            if !finished {
                continue;
            }

            record.resize_overlay = None;
            if let Some(location) = record.pending_location {
                self.space.map_element(record.window.clone(), location, false);
            }
        }
    }

    pub fn transition_render_elements(&self) -> Vec<Wm2RenderElements> {
        let now = std::time::Instant::now();
        let mut elements: Vec<Wm2RenderElements> = self
            .managed_windows
            .iter()
            .filter_map(|record| record.resize_overlay.as_ref())
            .map(ResizingWindow::render_element)
            .collect();
        elements.extend(
            self.closing_windows
                .iter()
                .map(|window| window.render_element(now)),
        );
        elements
    }

    pub fn schedule_relayout(&mut self) {
        self.schedule_relayout_with_transaction(None);
    }

    pub fn schedule_relayout_with_transaction(&mut self, transaction: Option<Transaction>) {
        self.start_relayout(transaction);
    }

    pub fn planned_layout_for_surface(
        &self,
        surface: &WlSurface,
    ) -> Option<(Point<i32, Logical>, Size<i32, Logical>)> {
        let output = self.space.outputs().next()?;
        let output_geometry = self.space.output_geometry(output)?;
        let count = self.managed_windows.len() as i32;
        if count <= 0 {
            return None;
        }

        let index = self
            .managed_windows
            .iter()
            .position(|record| record.wl_surface() == *surface)? as i32;

        let base_width = (output_geometry.size.w / count).max(1);
        let remainder = output_geometry.size.w.rem_euclid(count);

        let width = (base_width + i32::from(index < remainder)).max(1);
        let x = output_geometry.loc.x + index * base_width + remainder.min(index);
        let location = Point::from((x, output_geometry.loc.y));
        let size = Size::from((width, output_geometry.size.h.max(1)));
        Some((location, size))
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

        let count = self.managed_windows.len() as i32;
        let base_width = (output_geometry.size.w / count).max(1);
        let remainder = output_geometry.size.w.rem_euclid(count);

        let transaction = transaction.unwrap_or_else(Transaction::new);
        let mut x = output_geometry.loc.x;

        for index in 0..self.managed_windows.len() {
            let extra = if (index as i32) < remainder { 1 } else { 0 };
            let width = (base_width + extra).max(1);
            let size = Size::from((width, output_geometry.size.h.max(1)));
            let location = Point::from((x, output_geometry.loc.y));
            let current_location = self
                .space
                .element_location(&self.managed_windows[index].window);
            let toplevel = self.managed_windows[index].window.toplevel().cloned();

            if let Some(toplevel) = toplevel {
                let record = &mut self.managed_windows[index];
                let mut needs_configure = !record.mapped;
                toplevel.with_pending_state(|state| {
                    if state.size != Some(size) {
                        needs_configure = true;
                    }
                    state.size = Some(size);
                });

                if needs_configure {
                    if record.mapped {
                        if let (Some(snapshot), Some(_current_location)) =
                            (record.snapshot.as_ref(), current_location)
                        {
                            record.resize_overlay = Some(snapshot.into_resizing_window(
                                location,
                                record.window.geometry().loc,
                                record.window.geometry().size,
                                size,
                                transaction.monitor(),
                            ));
                            self.space.unmap_elem(&record.window);
                        }
                    }
                    record.pending_location = Some(location);
                    record.transaction_for_next_configure = Some(transaction.clone());
                    let serial = toplevel.send_configure();
                    if let Some(transaction) = record.transaction_for_next_configure.take() {
                        record.pending_transactions.push((serial, transaction));
                    }
                } else {
                    record.resize_overlay = None;
                    record.pending_location = Some(location);
                }
            }

            let record = &mut self.managed_windows[index];
            if record.mapped && record.resize_overlay.is_none() {
                self.space.map_element(record.window.clone(), location, false);
            } else {
                record.pending_location = Some(location);
            }

            x += width;
        }

        drop(transaction);
    }

    pub fn send_frames_for_windows(&self, output: &smithay::output::Output) {
        for record in &self.managed_windows {
            if !(record.mapped || record.pending_location.is_some()) {
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

    pub fn take_pending_transaction(&mut self, commit_serial: Serial) -> Option<Transaction> {
        let mut transaction = None;
        while let Some((serial, _)) = self.pending_transactions.first() {
            if commit_serial.is_no_older_than(serial) {
                let (_, pending) = self.pending_transactions.remove(0);
                transaction = Some(pending);
            } else {
                break;
            }
        }
        transaction
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
