use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use spiders_config::model::{Config, ConfigPaths};
use spiders_ipc_native::NativeIpcState;

use crate::backend::BackendState;
use crate::debug::DebugState;
use crate::frame_sync::{FrameSyncState, WindowFrameSyncState};
use crate::handlers::VirtualKeyboardManagerState;
use crate::scene::adapter::SceneLayoutState;
use smithay::desktop::{PopupManager, Space, Window};
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatState};
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DisplayHandle};
use smithay::utils::{Logical, Point};
use smithay::wayland::compositor::CompositorState;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::pointer_constraints::PointerConstraintsState;
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::xdg_activation::{XdgActivationState, XdgActivationTokenData};
use spiders_core::OutputId;
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
    pub _xdg_decoration_state: XdgDecorationState,
    pub shm_state: ShmState,
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    pub seat_state: SeatState<Self>,
    pub data_device_state: DataDeviceState,
    pub layer_shell_state: WlrLayerShellState,
    pub activation_state: XdgActivationState,
    pub _fractional_scale_manager_state: FractionalScaleManagerState,
    pub _pointer_constraints_state: PointerConstraintsState,
    pub _relative_pointer_manager_state: RelativePointerManagerState,
    pub _virtual_keyboard_manager_state: VirtualKeyboardManagerState,
    pub seat: Seat<Self>,
    pub cursor_image_status: CursorImageStatus,
    pub pointer_location: Point<f64, Logical>,
    pub backend: Option<BackendState>,

    pub focused_surface: Option<WlSurface>,
    pub(crate) layer_shell_focus_surface: Option<WlSurface>,
    pub(crate) pending_activation_requests: Vec<(WlSurface, XdgActivationTokenData)>,
    pub(crate) config_paths: Option<ConfigPaths>,
    pub(crate) config: Config,

    pub(crate) managed_windows: Vec<ManagedWindow>,
    pub(crate) frame_sync: FrameSyncState,
    pub(crate) scene_snapshot_root: Option<LayoutSnapshotNode>,
    pub(crate) scene_snapshot_roots_by_output: BTreeMap<OutputId, LayoutSnapshotNode>,
    pub(crate) ipc: NativeIpcState,
    pub(crate) ipc_socket_path: Option<PathBuf>,
    pub(crate) debug: DebugState,
    pub(crate) scene: SceneLayoutState,
    pub(crate) model: WmModel,
    pub(crate) next_window_id: u64,
    pub(crate) relayout_queued: bool,
    pub(crate) relayout_generation: u64,
    pub(crate) relayout_cause: RelayoutCause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RelayoutCause {
    #[default]
    General,
    FirstMapBurst,
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
