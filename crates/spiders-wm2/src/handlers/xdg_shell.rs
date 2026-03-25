use smithay::delegate_xdg_shell;
use smithay::desktop::{PopupKind, Window};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::{wl_seat, wl_surface::WlSurface};
use smithay::utils::Serial;
use smithay::wayland::compositor::{HookId, add_blocker, add_pre_commit_hook, with_states};
use smithay::wayland::shell::xdg::{PopupConfigureError, XdgShellState};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgToplevelSurfaceData,
};

use crate::state::SpidersWm;

impl XdgShellHandler for SpidersWm {
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

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        sync_toplevel_identity(self, surface.wl_surface());
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        sync_toplevel_identity(self, surface.wl_surface());
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.handle_window_close(surface.wl_surface());
    }
}

delegate_xdg_shell!(SpidersWm);

fn add_transaction_pre_commit_hook(surface: &WlSurface) -> HookId {
    add_pre_commit_hook::<SpidersWm, _>(surface, move |state, _display_handle, wl_surface| {
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

pub fn handle_commit(state: &mut SpidersWm, surface: &WlSurface) {
    sync_toplevel_identity(state, surface);

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

fn sync_toplevel_identity(state: &mut SpidersWm, surface: &WlSurface) {
    let Some(window_id) = state.window_id_for_surface(surface) else {
        return;
    };

    let (title, app_id) = with_states(surface, |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok())
            .map(|role| (role.title.clone(), role.app_id.clone()))
            .unwrap_or((None, None))
    });

    let _ = state.runtime().sync_window_identity(window_id, title, app_id);
}
