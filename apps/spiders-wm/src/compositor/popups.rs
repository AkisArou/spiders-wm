use smithay::desktop::PopupKind;
use smithay::utils::Rectangle;
use smithay::wayland::shell::xdg::PopupSurface;

use crate::state::SpidersWm;

impl SpidersWm {
    pub fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = smithay::desktop::find_popup_root_surface(&PopupKind::Xdg(popup.clone()))
        else {
            return;
        };
        let Some(window) = self.window_for_root_surface(&root) else {
            return;
        };

        let output_geo = self.current_output_geometry().expect("output geometry missing");
        let window_geo =
            self.element_geometry(&window).unwrap_or(Rectangle::new((0, 0).into(), (0, 0).into()));

        let mut target = output_geo;
        target.loc -= smithay::desktop::get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
