use smithay::{
    delegate_xdg_shell,
    desktop::{
        PopupKind, PopupManager, Space, Window, find_popup_root_surface, get_popup_toplevel_coords,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::SERIAL_COUNTER,
    wayland::{
        compositor::with_states,
        shell::xdg::{PopupSurface, XdgShellHandler, XdgToplevelSurfaceData},
    },
};

use crate::{
    runtime::SpidersWm2,
    wm::{place_new_window_in_active_workspace, register_window},
};

impl XdgShellHandler for SpidersWm2 {
    fn xdg_shell_state(&mut self) -> &mut smithay::wayland::shell::xdg::XdgShellState {
        &mut self.runtime.smithay.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: smithay::wayland::shell::xdg::ToplevelSurface) {
        let window_id = self.app.bindings.alloc_window_id();
        self.app
            .bindings
            .bind_surface(surface.wl_surface().clone(), window_id);

        register_window(&mut self.app.topology, window_id);
        place_new_window_in_active_workspace(&mut self.app.wm, window_id);

        let wl_surface = surface.wl_surface().clone();
        let window = Window::new_wayland_window(surface.clone());

        self.app
            .bindings
            .bind_window_element(window_id, window.clone());

        self.runtime
            .smithay
            .space
            .map_element(window, (0, 0), false);

        self.refresh_active_workspace();
        self.focus_window_surface(Some(wl_surface), SERIAL_COUNTER.next_serial());

        if !surface.is_initial_configure_sent() {
            surface.send_configure();
        }
    }

    fn new_popup(
        &mut self,
        surface: smithay::wayland::shell::xdg::PopupSurface,
        _positioner: smithay::wayland::shell::xdg::PositionerState,
    ) {
        self.unconstrain_popup(&surface);
        let _ = self
            .runtime
            .smithay
            .popups
            .track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: smithay::wayland::shell::xdg::PopupSurface,
        positioner: smithay::wayland::shell::xdg::PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });

        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn grab(
        &mut self,
        _surface: smithay::wayland::shell::xdg::PopupSurface,
        _seat: smithay::reexports::wayland_server::protocol::wl_seat::WlSeat,
        _serial: smithay::utils::Serial,
    ) {
    }
}

delegate_xdg_shell!(SpidersWm2);

pub fn handle_commit(popups: &mut PopupManager, space: &Space<Window>, surface: &WlSurface) {
    if let Some(window) = space
        .elements()
        .find(|window| window.toplevel().unwrap().wl_surface() == surface)
        .cloned()
    {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });

        if !initial_configure_sent {
            window.toplevel().unwrap().send_configure();
        }
    }

    popups.commit(surface);

    if let Some(popup) = popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                if !xdg.is_initial_configure_sent() {
                    xdg.send_configure()
                        .expect("initial popup configure failed");
                }
            }
            PopupKind::InputMethod(_) => {}
        }
    }
}

impl SpidersWm2 {
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };

        let Some(window) = self
            .runtime
            .smithay
            .space
            .elements()
            .find(|window| window.toplevel().unwrap().wl_surface() == &root)
        else {
            return;
        };

        let Some(output) = self.runtime.smithay.space.outputs().next() else {
            return;
        };

        let Some(output_geo) = self.runtime.smithay.space.output_geometry(output) else {
            return;
        };

        let Some(window_geo) = self.runtime.smithay.space.element_geometry(window) else {
            return;
        };

        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

// use smithay::{
//     delegate_xdg_shell,
//     desktop::{
//         find_popup_root_surface, get_popup_toplevel_coords, PopupKind, PopupManager, Space, Window,
//     },
//     reexports::wayland_server::protocol::{wl_seat, wl_surface::WlSurface},
//     utils::{Serial, SERIAL_COUNTER},
//     wayland::{
//         compositor::with_states,
//         shell::xdg::{
//             PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
//             XdgToplevelSurfaceData,
//         },
//     },
// };
//
// use crate::state::SpidersWm2;
//
// impl XdgShellHandler for SpidersWm2 {
//     fn xdg_shell_state(&mut self) -> &mut XdgShellState {
//         &mut self.xdg_shell_state
//     }
//
//     fn new_toplevel(&mut self, surface: ToplevelSurface) {
//         let window = Window::new_wayland_window(surface.clone());
//         self.space.map_element(window.clone(), (0, 0), false);
//         self.focus_window(Some(window), SERIAL_COUNTER.next_serial());
//
//         if !surface.is_initial_configure_sent() {
//             surface.send_configure();
//         }
//     }
//
//     fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
//         self.unconstrain_popup(&surface);
//         let _ = self.popups.track_popup(PopupKind::Xdg(surface));
//     }
//
//     fn reposition_request(
//         &mut self,
//         surface: PopupSurface,
//         positioner: PositionerState,
//         token: u32,
//     ) {
//         surface.with_pending_state(|state| {
//             state.geometry = positioner.get_geometry();
//             state.positioner = positioner;
//         });
//         self.unconstrain_popup(&surface);
//         surface.send_repositioned(token);
//     }
//
//     fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
// }
//
// delegate_xdg_shell!(SpidersWm2);
//
// pub fn handle_commit(popups: &mut PopupManager, space: &Space<Window>, surface: &WlSurface) {
//     if let Some(window) = space
//         .elements()
//         .find(|window| window.toplevel().unwrap().wl_surface() == surface)
//         .cloned()
//     {
//         let initial_configure_sent = with_states(surface, |states| {
//             states
//                 .data_map
//                 .get::<XdgToplevelSurfaceData>()
//                 .unwrap()
//                 .lock()
//                 .unwrap()
//                 .initial_configure_sent
//         });
//
//         if !initial_configure_sent {
//             window.toplevel().unwrap().send_configure();
//         }
//     }
//
//     popups.commit(surface);
//     if let Some(popup) = popups.find_popup(surface) {
//         match popup {
//             PopupKind::Xdg(ref xdg) => {
//                 if !xdg.is_initial_configure_sent() {
//                     xdg.send_configure()
//                         .expect("initial popup configure failed");
//                 }
//             }
//             PopupKind::InputMethod(_) => {}
//         }
//     }
// }
//
// impl SpidersWm2 {
//     fn unconstrain_popup(&self, popup: &PopupSurface) {
//         let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
//             return;
//         };
//         let Some(window) = self
//             .space
//             .elements()
//             .find(|window| window.toplevel().unwrap().wl_surface() == &root)
//         else {
//             return;
//         };
//
//         let Some(output) = self.space.outputs().next() else {
//             return;
//         };
//         let Some(output_geo) = self.space.output_geometry(output) else {
//             return;
//         };
//         let Some(window_geo) = self.space.element_geometry(window) else {
//             return;
//         };
//
//         let mut target = output_geo;
//         target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
//         target.loc -= window_geo.loc;
//
//         popup.with_pending_state(|state| {
//             state.geometry = state.positioner.get_unconstrained_geometry(target);
//         });
//     }
// }
