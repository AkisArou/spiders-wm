use std::process::Command;

use tracing::{error, info};

use crate::state::SpidersWm;

impl SpidersWm {
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
