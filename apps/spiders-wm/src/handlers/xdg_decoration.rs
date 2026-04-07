use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use smithay::wayland::shell::xdg::ToplevelSurface;

use crate::state::SpidersWm;

impl XdgDecorationHandler for SpidersWm {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.apply_toplevel_decoration_mode(&toplevel);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: Mode) {
        self.apply_toplevel_decoration_mode(&toplevel);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.apply_toplevel_decoration_mode(&toplevel);
    }
}
