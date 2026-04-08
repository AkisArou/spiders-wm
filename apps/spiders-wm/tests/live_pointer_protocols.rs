use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::delegate_compositor;
use smithay_client_toolkit::delegate_output;
use smithay_client_toolkit::delegate_registry;
use smithay_client_toolkit::delegate_shm;
use smithay_client_toolkit::delegate_xdg_shell;
use smithay_client_toolkit::delegate_xdg_window;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::registry_handlers;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_shm;
use wayland_client::protocol::wl_surface::{self, WlSurface};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_locked_pointer_v1::{
    self, ZwpLockedPointerV1,
};
use wayland_protocols::wp::pointer_constraints::zv1::client::zwp_pointer_constraints_v1::{
    Lifetime, ZwpPointerConstraintsV1,
};
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1;
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::{
    self, ZwpRelativePointerV1,
};

#[test]
#[ignore = "requires a live X11 session and intentionally runs the real wm binary"]
fn wm_live_pointer_protocols_bind_and_receive_events() {
    if std::env::var_os("SPIDERS_WM_RUN_LIVE_POINTER_PROTOCOLS").is_none() {
        eprintln!(
            "skipping live pointer protocol test; set SPIDERS_WM_RUN_LIVE_POINTER_PROTOCOLS=1 to enable"
        );
        return;
    }

    if std::env::var_os("DISPLAY").is_none() {
        eprintln!("skipping live pointer protocol test; no X11 DISPLAY is available for xdotool");
        return;
    }

    let workspace_root = workspace_root();
    let socket_name = unique_socket_name();
    let mut wm = spawn_wm(&workspace_root, &socket_name);
    wait_for_wayland_socket(&socket_name, Duration::from_secs(20));

    let probe = PointerProtocolProbe::spawn(socket_name.clone());
    wait_for(|| probe.configured.load(Ordering::SeqCst), Duration::from_secs(10), "xdg configure");
    assert!(probe.bound_pointer.load(Ordering::SeqCst), "seat pointer missing");
    assert!(
        probe.bound_relative_pointer_manager.load(Ordering::SeqCst),
        "relative pointer manager global missing"
    );
    assert!(
        probe.bound_pointer_constraints.load(Ordering::SeqCst),
        "pointer constraints global missing"
    );

    let window_id = wait_for_winit_window(Duration::from_secs(20));
    focus_window(&window_id);
    move_pointer(&window_id, 80, 80);
    move_pointer(&window_id, 140, 120);

    wait_for(
        || probe.locked_event_received.load(Ordering::SeqCst),
        Duration::from_secs(5),
        "locked pointer event",
    );
    wait_for(
        || probe.relative_motion_event_count.load(Ordering::SeqCst) > 0,
        Duration::from_secs(5),
        "relative motion event",
    );

    probe.stop();
    terminate_child(&mut wm);
}

struct PointerProtocolProbe {
    configured: Arc<AtomicBool>,
    bound_pointer: Arc<AtomicBool>,
    bound_relative_pointer_manager: Arc<AtomicBool>,
    bound_pointer_constraints: Arc<AtomicBool>,
    locked_event_received: Arc<AtomicBool>,
    relative_motion_event_count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl PointerProtocolProbe {
    fn spawn(socket_name: String) -> Self {
        let configured = Arc::new(AtomicBool::new(false));
        let bound_pointer = Arc::new(AtomicBool::new(false));
        let bound_relative_pointer_manager = Arc::new(AtomicBool::new(false));
        let bound_pointer_constraints = Arc::new(AtomicBool::new(false));
        let locked_event_received = Arc::new(AtomicBool::new(false));
        let relative_motion_event_count = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let thread = {
            let configured = Arc::clone(&configured);
            let bound_pointer = Arc::clone(&bound_pointer);
            let bound_relative_pointer_manager = Arc::clone(&bound_relative_pointer_manager);
            let bound_pointer_constraints = Arc::clone(&bound_pointer_constraints);
            let locked_event_received = Arc::clone(&locked_event_received);
            let relative_motion_event_count = Arc::clone(&relative_motion_event_count);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                run_pointer_protocol_client(
                    &socket_name,
                    configured,
                    bound_pointer,
                    bound_relative_pointer_manager,
                    bound_pointer_constraints,
                    locked_event_received,
                    relative_motion_event_count,
                    stop,
                );
            })
        };

        Self {
            configured,
            bound_pointer,
            bound_relative_pointer_manager,
            bound_pointer_constraints,
            locked_event_received,
            relative_motion_event_count,
            stop,
            thread: Some(thread),
        }
    }

    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_pointer_protocol_client(
    socket_name: &str,
    configured: Arc<AtomicBool>,
    bound_pointer: Arc<AtomicBool>,
    bound_relative_pointer_manager: Arc<AtomicBool>,
    bound_pointer_constraints: Arc<AtomicBool>,
    locked_event_received: Arc<AtomicBool>,
    relative_motion_event_count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
) {
    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", socket_name);
    }

    let connection = Connection::connect_to_env().expect("failed to connect to nested compositor");
    let (globals, mut event_queue) =
        registry_queue_init(&connection).expect("failed to initialize wayland registry queue");
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor missing");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg_wm_base missing");
    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::ServerDefault, &qh);
    let shm = Shm::bind(&globals, &qh).expect("wl_shm missing");
    let pool = SlotPool::new(256 * 256 * 4, &shm).expect("failed to create shm pool");

    let seat = globals.bind::<WlSeat, _, _>(&qh, 1..=7, ()).expect("wl_seat missing");
    let pointer = seat.get_pointer(&qh, ());
    bound_pointer.store(true, Ordering::SeqCst);

    let relative_pointer_manager = globals
        .bind::<ZwpRelativePointerManagerV1, _, _>(&qh, 1..=1, ())
        .expect("relative pointer manager missing");
    bound_relative_pointer_manager.store(true, Ordering::SeqCst);

    let pointer_constraints = globals
        .bind::<ZwpPointerConstraintsV1, _, _>(&qh, 1..=1, ())
        .expect("pointer constraints missing");
    bound_pointer_constraints.store(true, Ordering::SeqCst);

    let _relative_pointer = relative_pointer_manager.get_relative_pointer(&pointer, &qh, ());
    let _locked_pointer = pointer_constraints.lock_pointer(
        window.wl_surface(),
        &pointer,
        None,
        Lifetime::Persistent,
        &qh,
        (),
    );

    let mut client = PointerProtocolClient {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        configured,
        width: 256,
        height: 256,
        pool,
        buffer: None,
        window,
        locked_event_received,
        relative_motion_event_count,
    };

    client.window.set_title("spiders-live-pointer-client");
    client.window.commit();

    while !stop.load(Ordering::SeqCst) {
        let _ = event_queue.blocking_dispatch(&mut client);
    }
}

struct PointerProtocolClient {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    configured: Arc<AtomicBool>,
    width: u32,
    height: u32,
    pool: SlotPool,
    buffer: Option<Buffer>,
    window: Window,
    locked_event_received: Arc<AtomicBool>,
    relative_motion_event_count: Arc<AtomicUsize>,
}

impl PointerProtocolClient {
    fn draw(&mut self, qh: &QueueHandle<Self>) {
        let stride = self.width as i32 * 4;
        let buffer = self.buffer.get_or_insert_with(|| {
            self.pool
                .create_buffer(
                    self.width as i32,
                    self.height as i32,
                    stride,
                    wl_shm::Format::Argb8888,
                )
                .expect("failed to create shm buffer")
                .0
        });

        let canvas = match self.pool.canvas(buffer) {
            Some(canvas) => canvas,
            None => {
                let (fallback, canvas) = self
                    .pool
                    .create_buffer(
                        self.width as i32,
                        self.height as i32,
                        stride,
                        wl_shm::Format::Argb8888,
                    )
                    .expect("failed to allocate fallback shm buffer");
                *buffer = fallback;
                canvas
            }
        };

        for chunk in canvas.chunks_exact_mut(4) {
            chunk.copy_from_slice(&0xFF336699u32.to_le_bytes());
        }

        self.window.wl_surface().damage_buffer(0, 0, self.width as i32, self.height as i32);
        self.window.wl_surface().frame(qh, self.window.wl_surface().clone());
        buffer.attach_to(self.window.wl_surface()).expect("failed to attach shm buffer");
        self.window.wl_surface().commit();
    }
}

impl CompositorHandler for PointerProtocolClient {
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &WlOutput,
    ) {
    }
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }
}

impl WindowHandler for PointerProtocolClient {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {}

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        self.width = configure.new_size.0.map(|v| v.get()).unwrap_or(256);
        self.height = configure.new_size.1.map(|v| v.get()).unwrap_or(256);
        self.configured.store(true, Ordering::SeqCst);
        self.draw(qh);
    }
}

impl OutputHandler for PointerProtocolClient {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
}

impl ShmHandler for PointerProtocolClient {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for PointerProtocolClient {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}

impl Dispatch<WlSeat, ()> for PointerProtocolClient {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wayland_client::protocol::wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlPointer, ()> for PointerProtocolClient {
    fn event(
        _: &mut Self,
        _: &WlPointer,
        _: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpRelativePointerManagerV1, ()> for PointerProtocolClient {
    fn event(
        _: &mut Self,
        _: &ZwpRelativePointerManagerV1,
        _: wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpRelativePointerV1, ()> for PointerProtocolClient {
    fn event(
        state: &mut Self,
        _: &ZwpRelativePointerV1,
        event: zwp_relative_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_relative_pointer_v1::Event::RelativeMotion { .. } = event {
            state.relative_motion_event_count.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl Dispatch<ZwpPointerConstraintsV1, ()> for PointerProtocolClient {
    fn event(
        _: &mut Self,
        _: &ZwpPointerConstraintsV1,
        _: wayland_protocols::wp::pointer_constraints::zv1::client::zwp_pointer_constraints_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpLockedPointerV1, ()> for PointerProtocolClient {
    fn event(
        state: &mut Self,
        _: &ZwpLockedPointerV1,
        event: zwp_locked_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_locked_pointer_v1::Event::Locked = event {
            state.locked_event_received.store(true, Ordering::SeqCst);
        }
    }
}

delegate_compositor!(PointerProtocolClient);
delegate_output!(PointerProtocolClient);
delegate_shm!(PointerProtocolClient);
delegate_xdg_shell!(PointerProtocolClient);
delegate_xdg_window!(PointerProtocolClient);
delegate_registry!(PointerProtocolClient);

fn wait_for<F>(mut condition: F, timeout: Duration, label: &str)
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }

    panic!("timed out waiting for {label}");
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf()
}

fn spawn_wm(workspace_root: &Path, socket_name: &str) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_spiders-wm"));
    let inherit_logs = std::env::var_os("SPIDERS_WM_LIVE_LOG").is_some();
    command
        .current_dir(workspace_root)
        .env("NO_COLOR", "1")
        .env("SPIDERS_WM_WAYLAND_SOCKET", socket_name)
        .stdout(if inherit_logs { Stdio::inherit() } else { Stdio::null() })
        .stderr(if inherit_logs { Stdio::inherit() } else { Stdio::null() });
    command.env_remove("WAYLAND_DISPLAY");
    command.env_remove("WAYLAND_SOCKET");
    command.spawn().expect("failed to spawn wm live pointer protocol process")
}

fn unique_socket_name() -> String {
    format!("spiders-live-pointer-{}", std::process::id())
}

fn wait_for_wayland_socket(socket_name: &str, timeout: Duration) {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR missing");
    let socket_path = Path::new(&runtime_dir).join(socket_name);
    wait_for(|| socket_path.exists(), timeout, "nested wayland socket");
}

fn wait_for_winit_window(timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let output = Command::new("xdotool")
            .args(["search", "--name", "spiders-wm-winit"])
            .output()
            .expect("failed to search for nested winit window");
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(window_id) = stdout.lines().next() {
                return window_id.to_string();
            }
        }
        thread::sleep(Duration::from_millis(100));
    }

    panic!("timed out waiting for nested winit window");
}

fn focus_window(window_id: &str) {
    let status = Command::new("xdotool")
        .args(["windowactivate", window_id])
        .status()
        .expect("failed to focus nested winit window");
    assert!(status.success(), "xdotool windowactivate failed");
}

fn move_pointer(window_id: &str, x: i32, y: i32) {
    let status = Command::new("xdotool")
        .args(["mousemove", "--window", window_id, &x.to_string(), &y.to_string()])
        .status()
        .expect("failed to move pointer in nested winit window");
    assert!(status.success(), "xdotool mousemove failed");
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
