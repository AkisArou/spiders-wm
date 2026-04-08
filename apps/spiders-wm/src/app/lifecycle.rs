use std::process::Command;

use tracing::{error, info};

use crate::state::SpidersWm;

impl SpidersWm {
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
        let (config_paths, config) =
            crate::app::bootstrap::load_wm_config(self.config_paths.clone());
        self.config_paths = config_paths;
        self.scene.set_config_paths(self.config_paths.clone());
        self.config = config;
        let config = self.config.clone();
        let events = {
            let mut runtime = self.runtime();
            runtime.sync_layout_selection_defaults(&config);
            runtime.take_events()
        };
        if !events.is_empty() {
            self.broadcast_runtime_events(events);
            self.schedule_relayout();
        }
        self.emit_config_reloaded();
    }
}
