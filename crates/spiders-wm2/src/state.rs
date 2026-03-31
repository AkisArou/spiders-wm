use std::ffi::OsString;
use std::sync::mpsc::{Receiver, Sender};
use std::path::PathBuf;
use std::process::Command;
use std::os::unix::net::UnixStream;
use std::collections::BTreeMap;

use spiders_config::model::{Config, ConfigPaths};
use spiders_ipc::{IpcClientId, IpcServerState};

use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::desktop::{PopupManager, Space, Window, WindowSurfaceType};
use smithay::input::{Seat, SeatState};
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DisplayHandle};
use smithay::utils::{Logical, Point};
use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::DataDeviceHandler;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use tracing::{debug, error, info};

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

impl SpidersWm {
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
        self.managed_window_for_surface(surface).map(|record| record.id.clone())
    }

    pub fn managed_window_for_surface(&self, surface: &WlSurface) -> Option<&ManagedWindow> {
        self.managed_windows
            .iter()
            .find(|record| record.window.toplevel().is_some_and(|toplevel| toplevel.wl_surface() == surface))
    }

    pub fn managed_window_mut_for_surface(&mut self, surface: &WlSurface) -> Option<&mut ManagedWindow> {
        self.managed_windows
            .iter_mut()
            .find(|record| record.window.toplevel().is_some_and(|toplevel| toplevel.wl_surface() == surface))
    }

    pub fn managed_window_position_for_surface(&self, surface: &WlSurface) -> Option<usize> {
        self.managed_windows
            .iter()
            .position(|record| record.window.toplevel().is_some_and(|toplevel| toplevel.wl_surface() == surface))
    }

    pub fn surface_for_window_id(&self, window_id: WindowId) -> Option<WlSurface> {
        self.managed_windows
            .iter()
            .find(|record| record.id == window_id)
            .and_then(|record| record.window.toplevel().map(|toplevel| toplevel.wl_surface().clone()))
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
                self.model.window_is_layout_eligible(&record.id).then_some(index)
            })
            .collect()
    }

    pub(crate) fn managed_window_debug_summary(&self) -> Vec<String> {
        self.managed_windows
            .iter()
            .map(|record| {
                format!(
                    "{}:mapped={}:closing={}:snapshot={}:pending_configures={}",
                    record.id.0,
                    record.mapped,
                    self.model
                        .windows
                        .get(&record.id)
                        .is_some_and(|window| window.closing),
                    record.frame_sync.has_close_snapshot(),
                    record.frame_sync.has_pending_configures(),
                )
            })
            .collect()
    }

    pub(crate) fn log_managed_window_state(&self, reason: &str) {
        debug!(
            reason,
            windows = ?self.managed_window_debug_summary(),
            closing_overlays = self.frame_sync.overlay_count(),
            focused = ?self.focused_surface.as_ref().and_then(|surface| self.window_id_for_surface(surface)),
            "wm2 managed window state"
        );
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

    pub fn spawn_command(&self, command_line: &str) {
        let mut command = Command::new("sh");
        command.arg("-lc").arg(command_line);
        command.env("WAYLAND_DISPLAY", &self.socket_name);

        match command.spawn() {
            Ok(_) => info!(command = command_line, "spawned wm command"),
            Err(err) => error!(command = command_line, %err, "failed to spawn wm command"),
        }
    }

    pub fn reload_config(&mut self) {
        let (config_paths, config) = crate::app::bootstrap::load_wm_config(self.config_paths.clone());
        self.config_paths = config_paths;
        self.scene.set_config_paths(self.config_paths.clone());
        self.config = config;
        self.emit_config_reloaded();
    }

    pub fn notify_blocker_cleared(&mut self) {
        let display_handle = self.display_handle.clone();
        while let Ok(client) = self.blocker_cleared_rx.try_recv() {
            self.client_compositor_state(&client)
                .blocker_cleared(self, &display_handle);
        }
    }

    pub fn prune_completed_closing_overlays(&mut self) {
        self.frame_sync.prune_completed_closing_overlays();
    }
}

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

        assert_eq!(paths, Some(ConfigPaths::new(&authored_config, &prepared_config)));
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
        assert_eq!(initial_config.bindings[0].command, WmCommand::ToggleFullscreen);

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
