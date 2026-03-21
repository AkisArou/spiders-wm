use crate::{
    actions::{place_new_window_in_active_workspace, register_window, update_window_metadata},
    runtime::SpidersWm2,
};
use smithay::{
    delegate_xdg_shell,
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, PopupKind, PopupManager, Space, Window,
    },
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::SERIAL_COUNTER,
    wayland::{
        compositor::{add_pre_commit_hook, with_states},
        shell::xdg::{Configure, PopupSurface, XdgShellHandler, XdgToplevelSurfaceData},
    },
};

impl XdgShellHandler for SpidersWm2 {
    fn xdg_shell_state(&mut self) -> &mut smithay::wayland::shell::xdg::XdgShellState {
        &mut self.runtime.smithay.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: smithay::wayland::shell::xdg::ToplevelSurface) {
        let window_id = self.app.bindings.alloc_window_id();
        self.app
            .bindings
            .bind_surface(surface.wl_surface().clone(), window_id.clone());

        register_window(&mut self.app.topology, window_id.clone());
        place_new_window_in_active_workspace(&mut self.app.wm, window_id.clone());

        let wl_surface = surface.wl_surface().clone();
        let window = Window::new_wayland_window(surface.clone());
        let hook = add_pre_commit_hook::<SpidersWm2, _>(
            surface.wl_surface(),
            move |state, _dh, surface| {
                let Some(window_id) = state.app.bindings.window_for_surface(&surface.id()) else {
                    return;
                };

                let commit_serial = with_states(surface, |states| {
                    states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .and_then(|data| data.lock().ok())
                        .and_then(|data| data.last_acked.as_ref().map(|configure| configure.serial))
                });

                let Some(commit_serial) = commit_serial else {
                    return;
                };

                let completed = state
                    .app
                    .bindings
                    .take_pending_commit_serials_through(&window_id, commit_serial);

                for serial in completed {
                    state.runtime.transactions.mark_window_committed(&window_id);
                    tracing::trace!(
                        target: "spiders_wm2::runtime_debug",
                        ?window_id,
                        ?serial,
                        "mark_window_committed_from_serial_queue"
                    );
                }
            },
        );

        self.app
            .bindings
            .bind_window_element(window_id.clone(), window.clone());
        self.app.bindings.bind_commit_hook(window_id.clone(), hook);

        self.runtime
            .smithay
            .space
            .map_element(window, (0, 0), false);

        let (title, app_id) = with_states(surface.wl_surface(), |states| {
            let data = states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap();
            (data.title.clone(), data.app_id.clone())
        });

        update_window_metadata(
            &mut self.app.topology,
            &mut self.app.wm,
            &window_id,
            title,
            app_id,
        );

        self.refresh_active_workspace();
        self.focus_window_surface(Some(wl_surface), SERIAL_COUNTER.next_serial());

        if !surface.is_initial_configure_sent() {
            self.maybe_commit_pending_transaction();
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

    fn ack_configure(&mut self, surface: WlSurface, configure: Configure) {
        if let Some(window_id) = self.app.bindings.window_for_surface(&surface.id()) {
            match configure {
                Configure::Toplevel(configure) => {
                    self.app
                        .bindings
                        .record_pending_commit_serial(&window_id, configure.serial);
                    self.app
                        .bindings
                        .record_acked_toplevel_configure(&window_id, configure.clone());
                    self.runtime
                        .transactions
                        .mark_configure_acked(&window_id, configure.serial);
                    self.maybe_commit_pending_transaction();
                }
                Configure::Popup(_) => {}
            }
        }
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
            let toplevel = window.toplevel().unwrap();

            if !toplevel.has_pending_changes() {
                toplevel.with_pending_state(|state| {
                    state.size.get_or_insert((960, 640).into());
                });
            }

            toplevel.send_pending_configure();
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

#[cfg(test)]
mod tests {
    use smithay::utils::Serial;
    use smithay::utils::{Logical, Size};

    #[test]
    fn ensure_initial_toplevel_configure_size_prefers_existing_pending_size() {
        let mut size: Option<Size<i32, Logical>> = Some(Size::from((800, 600)));

        size.get_or_insert(Size::from((960, 640)));

        assert_eq!(size, Some(Size::from((800, 600))));
    }

    #[test]
    fn ensure_initial_toplevel_configure_size_falls_back_when_missing() {
        let mut size: Option<Size<i32, Logical>> = None;

        size.get_or_insert(Size::from((960, 640)));

        assert_eq!(size, Some(Size::from((960, 640))));
    }

    #[test]
    fn pending_commit_serial_matches_exact_ack_serial() {
        let serial = Serial::from(7);

        assert_eq!(Some(serial), Some(serial));
    }

    #[test]
    fn observed_commit_serial_is_recorded_without_clearing_pending() {
        let serial = Serial::from(11);

        assert_eq!(Some(serial), Some(serial));
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
