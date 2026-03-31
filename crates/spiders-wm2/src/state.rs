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
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DisplayHandle};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::DataDeviceHandler;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;

use crate::frame_sync::{FrameSyncState, WindowFrameSyncState};
use crate::model::{WindowId, wm::WmModel};
use crate::scene::adapter::SceneLayoutState;

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
    pub(crate) ipc_server: IpcServerState,
    pub(crate) ipc_clients: BTreeMap<IpcClientId, UnixStream>,
    pub(crate) ipc_socket_path: Option<PathBuf>,
    pub(crate) scene: SceneLayoutState,
    pub(crate) model: WmModel,
    pub(crate) next_window_id: u64,
}

pub(crate) struct ManagedWindow {
    pub(crate) id: WindowId,
    pub(crate) window: Window,
    pub(crate) mapped: bool,
    pub(crate) frame_sync: WindowFrameSyncState,
}

impl SpidersWm {}

impl ManagedWindow {
    pub fn toplevel(&self) -> Option<&smithay::wayland::shell::xdg::ToplevelSurface> {
        self.window.toplevel()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use spiders_shared::command::WmCommand;

    fn unique_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("spiders-wm2-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("failed to create temp root");
        root
    }

    fn write_authored_config(path: &Path, command_expression: &str) {
        fs::write(
            path,
            format!(
                r#"
import * as commands from "spiders-wm/commands";

export default {{
  workspaces: ["1", "2"],
  bindings: {{
    mod: "super",
    entries: [
      {{ bind: ["mod", "Return"], command: {command_expression} }},
    ],
  }},
}};
"#,
            ),
        )
        .expect("failed to write authored config");
    }

    #[test]
    fn load_wm_config_with_paths_decodes_authored_toggle_workspace_binding() {
        let root = unique_root("config-load");
        let project_root = root.join("project");
        let cache_root = root.join("cache");
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&cache_root).unwrap();

        let authored_config = project_root.join("config.ts");
        let prepared_config = cache_root.join("config.js");
        write_authored_config(&authored_config, "commands.toggle_workspace(2)");

        let (paths, config) = crate::app::bootstrap::load_wm_config(Some(ConfigPaths::new(
            &authored_config,
            &prepared_config,
        )));

        assert_eq!(
            paths,
            Some(ConfigPaths::new(&authored_config, &prepared_config))
        );
        assert!(prepared_config.exists());
        assert_eq!(config.workspaces, vec!["1".to_string(), "2".to_string()]);
        assert_eq!(config.bindings.len(), 1);
        assert_eq!(config.bindings[0].trigger, "super+Return");
        assert_eq!(
            config.bindings[0].command,
            WmCommand::ToggleAssignFocusedWindowToWorkspace { workspace: 2 }
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_wm_config_with_paths_refreshes_prepared_config_after_authored_changes() {
        let root = unique_root("config-reload");
        let project_root = root.join("project");
        let cache_root = root.join("cache");
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&cache_root).unwrap();

        let authored_config = project_root.join("config.ts");
        let prepared_config = cache_root.join("config.js");
        let paths = ConfigPaths::new(&authored_config, &prepared_config);

        write_authored_config(&authored_config, "commands.toggle_fullscreen()");
        let (_, initial_config) = crate::app::bootstrap::load_wm_config(Some(paths.clone()));
        assert_eq!(initial_config.bindings.len(), 1);
        assert_eq!(
            initial_config.bindings[0].command,
            WmCommand::ToggleFullscreen
        );

        std::thread::sleep(Duration::from_millis(20));
        write_authored_config(&authored_config, "commands.reload_config()");

        let (_, reloaded_config) = crate::app::bootstrap::load_wm_config(Some(paths));
        assert_eq!(reloaded_config.bindings.len(), 1);
        assert_eq!(reloaded_config.bindings[0].trigger, "super+Return");
        assert_eq!(reloaded_config.bindings[0].command, WmCommand::ReloadConfig);

        let _ = fs::remove_dir_all(root);
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

impl Drop for SpidersWm {
    fn drop(&mut self) {
        if let Some(socket_path) = self.ipc_socket_path.as_ref() {
            let _ = std::fs::remove_file(socket_path);
        }
    }
}
