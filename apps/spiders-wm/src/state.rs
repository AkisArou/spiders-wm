use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use spiders_config::model::{Config, ConfigPaths};
use spiders_ipc::{IpcClientId, IpcServerState};

use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::desktop::{PopupManager, Space, Window};
use smithay::input::{Seat, SeatState};
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DisplayHandle};
use smithay::wayland::compositor::CompositorState;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;

use crate::frame_sync::{FrameSyncState, WindowFrameSyncState};
use crate::scene::adapter::SceneLayoutState;
use spiders_core::WindowId;
use spiders_core::wm::WmModel;
use spiders_scene::LayoutSnapshotNode;

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
    pub(crate) config_paths: Option<ConfigPaths>,
    pub(crate) config: Config,

    pub(crate) managed_windows: Vec<ManagedWindow>,
    pub(crate) frame_sync: FrameSyncState,
    pub(crate) titlebar_overlays: BTreeMap<WindowId, NativeTitlebarOverlay>,
    pub(crate) titlebar_layout: NativeTitlebarLayoutState,
    pub(crate) ipc_server: IpcServerState,
    pub(crate) ipc_clients: BTreeMap<IpcClientId, UnixStream>,
    pub(crate) ipc_socket_path: Option<PathBuf>,
    pub(crate) scene: SceneLayoutState,
    pub(crate) model: WmModel,
    pub(crate) next_window_id: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeTitlebarOverlay {
    pub(crate) rect: spiders_core::LayoutRect,
    pub(crate) pixels: Vec<u8>,
    pub(crate) hit_regions: Vec<NativeTitlebarHitRegion>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NativeTitlebarHitRegion {
    pub(crate) rect: spiders_core::LayoutRect,
    pub(crate) command: crate::runtime::WmCommand,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NativeTitlebarLayoutState {
    pub(crate) snapshot_root: Option<LayoutSnapshotNode>,
}

pub(crate) struct ManagedWindow {
    pub(crate) id: WindowId,
    pub(crate) window: Window,
    pub(crate) mapped: bool,
    pub(crate) frame_sync: WindowFrameSyncState,
}

impl ManagedWindow {
    pub fn toplevel(&self) -> Option<&smithay::wayland::shell::xdg::ToplevelSurface> {
        self.window.toplevel()
    }
}

impl Drop for SpidersWm {
    fn drop(&mut self) {
        if let Some(socket_path) = self.ipc_socket_path.as_ref() {
            let _ = std::fs::remove_file(socket_path);
        }
    }
}
