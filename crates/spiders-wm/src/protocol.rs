#![allow(non_upper_case_globals, unused)]

pub mod river_window_management_v1 {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;

        wayland_scanner::generate_interfaces!("protocol/river-window-management-v1.xml");
    }

    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocol/river-window-management-v1.xml");
}

pub mod river_layer_shell {
    use wayland_client;
    use wayland_client::protocol::*;

    pub use super::river_window_management_v1::{river_output_v1, river_seat_v1};

    pub mod __interfaces {
        use super::super::river_window_management_v1::__interfaces::*;
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;

        wayland_scanner::generate_interfaces!("protocol/river-layer-shell-v1.xml");
    }

    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocol/river-layer-shell-v1.xml");
}

pub mod river_xkb_bindings {
    use wayland_client;
    use wayland_client::protocol::*;

    pub use super::river_window_management_v1::river_seat_v1;

    pub mod __interfaces {
        use super::super::river_window_management_v1::__interfaces::*;
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;

        wayland_scanner::generate_interfaces!("protocol/river-xkb-bindings-v1.xml");
    }

    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocol/river-xkb-bindings-v1.xml");
}

pub mod river_input_management {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;

        wayland_scanner::generate_interfaces!("protocol/river-input-management-v1.xml");
    }

    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocol/river-input-management-v1.xml");
}

pub mod river_xkb_config {
    use wayland_client;
    use wayland_client::protocol::*;

    pub use super::river_input_management::river_input_device_v1;

    pub mod __interfaces {
        use super::super::river_input_management::__interfaces::*;
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;

        wayland_scanner::generate_interfaces!("protocol/river-xkb-config-v1.xml");
    }

    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocol/river-xkb-config-v1.xml");
}

pub mod river_libinput_config {
    use wayland_client;
    use wayland_client::protocol::*;

    pub use super::river_input_management::river_input_device_v1;

    pub mod __interfaces {
        use super::super::river_input_management::__interfaces::*;
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;

        wayland_scanner::generate_interfaces!("protocol/river-libinput-config-v1.xml");
    }

    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocol/river-libinput-config-v1.xml");
}

pub const RIVER_WINDOW_MANAGEMENT_GLOBAL: &str = "river_window_manager_v1";
pub const RIVER_LAYER_SHELL_GLOBAL: &str = "river_layer_shell_v1";
pub const RIVER_XKB_BINDINGS_GLOBAL: &str = "river_xkb_bindings_v1";
pub const RIVER_INPUT_MANAGEMENT_GLOBAL: &str = "river_input_manager_v1";
pub const RIVER_XKB_CONFIG_GLOBAL: &str = "river_xkb_config_v1";
pub const RIVER_LIBINPUT_CONFIG_GLOBAL: &str = "river_libinput_config_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiverProtocolGlobal {
    WindowManager,
    LayerShell,
    XkbBindings,
    InputManagement,
    XkbConfig,
    LibinputConfig,
}

impl RiverProtocolGlobal {
    pub fn interface_name(self) -> &'static str {
        match self {
            Self::WindowManager => RIVER_WINDOW_MANAGEMENT_GLOBAL,
            Self::LayerShell => RIVER_LAYER_SHELL_GLOBAL,
            Self::XkbBindings => RIVER_XKB_BINDINGS_GLOBAL,
            Self::InputManagement => RIVER_INPUT_MANAGEMENT_GLOBAL,
            Self::XkbConfig => RIVER_XKB_CONFIG_GLOBAL,
            Self::LibinputConfig => RIVER_LIBINPUT_CONFIG_GLOBAL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RiverProtocolSupport {
    pub window_management: bool,
    pub layer_shell: bool,
    pub xkb_bindings: bool,
    pub input_management: bool,
    pub xkb_config: bool,
    pub libinput_config: bool,
}

impl RiverProtocolSupport {
    pub fn supports_minimum_viable_wm(&self) -> bool {
        self.window_management
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_viable_wm_requires_window_management() {
        assert!(!RiverProtocolSupport::default().supports_minimum_viable_wm());
        assert!(RiverProtocolSupport {
            window_management: true,
            layer_shell: false,
            xkb_bindings: false,
            input_management: false,
            xkb_config: false,
            libinput_config: false,
        }
        .supports_minimum_viable_wm());
    }
}
