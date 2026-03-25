use smithay::delegate_xdg_shell;
use smithay::desktop::{PopupKind, Window};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::{wl_seat, wl_surface::WlSurface};
use smithay::utils::{Rectangle, Serial};
use smithay::wayland::compositor::{HookId, add_blocker, add_pre_commit_hook, with_states};
use smithay::wayland::shell::xdg::{PopupConfigureError, XdgShellState};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgToplevelSurfaceData,
};

use crate::state::SpidersWm2;

impl XdgShellHandler for SpidersWm2 {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let _hook = add_transaction_pre_commit_hook(surface.wl_surface());
        let window = Window::new_wayland_window(surface);
        self.add_window(window);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, _surface: ToplevelSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
    }

    fn resize_request(
        &mut self,
        _surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        _serial: Serial,
        _edges: xdg_toplevel::ResizeEdge,
    ) {
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.handle_window_close(surface.wl_surface());
    }
}

delegate_xdg_shell!(SpidersWm2);

fn add_transaction_pre_commit_hook(surface: &WlSurface) -> HookId {
    add_pre_commit_hook::<SpidersWm2, _>(surface, move |state, _display_handle, wl_surface| {
        let commit_serial = with_states(wl_surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().ok())
                .and_then(|role| role.last_acked.as_ref().map(|configure| configure.serial))
        });

        let Some(commit_serial) = commit_serial else {
            return;
        };

        let Some(record) = state.find_window_mut(wl_surface) else {
            return;
        };

        if let Some(transaction) = record.frame_sync.match_configure_commit(commit_serial) {
            if !transaction.is_completed() {
                transaction.register_deadline(&state.event_loop);

                if !transaction.is_last() {
                    if let Some(client) = wl_surface.client() {
                        transaction.add_notification(state.blocker_cleared_tx.clone(), client);
                        add_blocker(wl_surface, transaction.blocker());
                    }
                }
            }
        }
    })
}

pub fn handle_commit(state: &mut SpidersWm2, surface: &WlSurface) {
    let initial_configure_sent = with_states(surface, |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok())
            .map(|role| role.initial_configure_sent)
            .unwrap_or(false)
    });
    let planned_size = (!initial_configure_sent)
        .then(|| state.planned_layout_for_surface(surface).map(|(_, size)| size))
        .flatten();

    if let Some(record) = state.find_window_mut(surface) {
        if !initial_configure_sent {
            if let Some(toplevel) = record.toplevel() {
                if let Some(size) = planned_size {
                    toplevel.with_pending_state(|state| {
                        state.size = Some(size);
                    });
                }
                toplevel.send_configure();
            }
        }
    }

    state.popups.commit(surface);
    if let Some(popup) = state.popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                if !xdg.is_initial_configure_sent() {
                    let _: Result<_, PopupConfigureError> = xdg.send_configure();
                }
            }
            PopupKind::InputMethod(_) => {}
        }
    }
}

impl SpidersWm2 {
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = smithay::desktop::find_popup_root_surface(&PopupKind::Xdg(popup.clone()))
        else {
            return;
        };
        let Some(window) = self.space.elements().find(|window| {
            window
                .toplevel()
                .is_some_and(|toplevel| toplevel.wl_surface() == &root)
        }) else {
            return;
        };

        let output = self
            .space
            .outputs()
            .next()
            .expect("output missing for popup");
        let output_geo = self
            .space
            .output_geometry(output)
            .expect("output geometry missing");
        let window_geo = self
            .space
            .element_geometry(window)
            .unwrap_or(Rectangle::new((0, 0).into(), (0, 0).into()));

        let mut target = output_geo;
        target.loc -= smithay::desktop::get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
