use smithay::delegate_xdg_shell;
use smithay::desktop::{PopupKeyboardGrab, PopupKind, PopupPointerGrab, Window};
use smithay::input::pointer::Focus;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::{wl_seat, wl_surface::WlSurface};
use smithay::utils::Serial;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{PopupConfigureError, XdgShellState};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgToplevelSurfaceData,
};
use spiders_core::signal::WmSignal;
use spiders_css::AppearanceValue;
use tracing::{debug, info};

use crate::runtime::NoopHost;
use crate::state::SpidersWm;

impl XdgShellHandler for SpidersWm {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        info!(surface = ?surface.wl_surface().id(), "wm new toplevel");
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

    fn grab(&mut self, surface: PopupSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        let popup = PopupKind::Xdg(surface);
        let Ok(root) = smithay::desktop::find_popup_root_surface(&popup) else {
            return;
        };

        let root_layer_surface = self.layer_surface_for_surface(&root);
        let root_output = self.output_for_surface(&root);
        let focused_layer_output = self
            .layer_shell_focus_surface
            .as_ref()
            .and_then(|surface| self.output_for_surface(surface));
        if popup_grab_should_be_denied(
            root_layer_surface.is_some(),
            root_output.as_ref(),
            focused_layer_output.as_ref(),
        ) {
            let _ = smithay::desktop::PopupManager::dismiss_popup(&root, &popup);
            return;
        }

        let mut grab = match self.popups.grab_popup(root.clone(), popup, &self.seat, serial) {
            Ok(grab) => grab,
            Err(_) => return,
        };

        let keyboard = self.seat.get_keyboard().expect("keyboard missing");
        let pointer = self.seat.get_pointer().expect("pointer missing");

        let can_receive_keyboard_focus = popup_grab_can_take_keyboard_focus(
            root_layer_surface.is_some(),
            root_layer_surface.map(|layer_surface| layer_surface.can_receive_keyboard_focus()),
        );

        let keyboard_grab_mismatches = keyboard.is_grabbed()
            && popup_grab_has_mismatch(
                keyboard.has_grab(serial),
                grab.previous_serial().is_some_and(|s| keyboard.has_grab(s)),
            );
        let pointer_grab_mismatches = pointer.is_grabbed()
            && popup_grab_has_mismatch(
                pointer.has_grab(serial),
                grab.previous_serial().is_some_and(|s| pointer.has_grab(s)),
            );
        if (can_receive_keyboard_focus && keyboard_grab_mismatches) || pointer_grab_mismatches {
            let _ = grab.ungrab(smithay::desktop::PopupUngrabStrategy::All);
            return;
        }

        if can_receive_keyboard_focus {
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        sync_toplevel_identity(self, surface.wl_surface());
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        sync_toplevel_identity(self, surface.wl_surface());
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        info!(surface = ?surface.wl_surface().id(), "wm toplevel destroyed");
        self.finalize_window_destroy(surface.wl_surface());
    }
}

delegate_xdg_shell!(SpidersWm);

pub fn handle_commit(state: &mut SpidersWm, surface: &WlSurface) {
    sync_toplevel_identity(state, surface);
    state.refresh_fractional_scale_for_window_surface(surface);

    let initial_configure_sent = with_states(surface, |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok())
            .map(|role| role.initial_configure_sent)
            .unwrap_or(false)
    });
    let planned_layout =
        (!initial_configure_sent).then(|| state.planned_layout_for_surface(surface)).flatten();
    let planned_size = planned_layout.map(|(_, size)| size);
    let initial_transaction =
        (!initial_configure_sent).then(|| crate::frame_sync::new_sync_handle(&state.event_loop));

    if !initial_configure_sent {
        let toplevel = state.find_window_mut(surface).and_then(|record| record.toplevel().cloned());
        if let Some(toplevel) = toplevel {
            state.apply_toplevel_decoration_mode(&toplevel);

            if let Some(size) = planned_size {
                toplevel.with_pending_state(|state| {
                    state.size = Some(size);
                });
            }

            let window_id = state
                .window_id_for_surface(surface)
                .expect("window id missing for initial configure")
                .to_string();
            info!(window = %window_id, planned_size = ?planned_size, "wm sending initial configure");
            let serial = toplevel.send_configure();

            if let Some(record) = state.find_window_mut(surface)
                && let Some((location, size)) = planned_layout
            {
                record.frame_sync.track_pending_layout(
                    serial,
                    location,
                    size,
                    initial_transaction.expect("initial transaction missing"),
                );
            }

            let tracked_layout = planned_layout.map(|(location, size)| {
                format!("serial={serial:?} location={location:?} size={size:?}")
            });
            let configure_details = format!(
                "planned_size={planned_size:?} planned_layout={planned_layout:?} initial_configure_sent={initial_configure_sent}"
            );
            state.debug_protocol_event("send-initial-configure", Some(&window_id), || {
                configure_details
            });
            if let Some(details) = tracked_layout {
                state.debug_protocol_event("track-pending-layout", Some(&window_id), || details);
            }
        }
    }

    state.popups.commit(surface);
    if let Some(popup) = state.popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                if !xdg.is_initial_configure_sent() {
                    state.debug_protocol_event("send-popup-configure", None, || {
                        format!("surface={:?}", surface.id())
                    });
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

    let events = {
        let mut runtime = state.runtime();
        runtime.handle_signal(
            &mut NoopHost,
            WmSignal::WindowIdentityChanged {
                window_id,
                title,
                app_id,
                class: None,
                instance: None,
                role: None,
                window_type: None,
                urgent: false,
            },
        )
    };
    state.broadcast_runtime_events(events);
}

fn popup_grab_can_take_keyboard_focus(
    root_is_layer_surface: bool,
    layer_can_receive_keyboard_focus: Option<bool>,
) -> bool {
    if root_is_layer_surface {
        layer_can_receive_keyboard_focus.unwrap_or(false)
    } else {
        true
    }
}

fn popup_grab_has_mismatch(
    current_grab_matches_serial: bool,
    previous_grab_matches_serial: bool,
) -> bool {
    !(current_grab_matches_serial || previous_grab_matches_serial)
}

fn popup_grab_should_be_denied(
    root_is_layer_surface: bool,
    root_output: Option<&smithay::output::Output>,
    focused_layer_output: Option<&smithay::output::Output>,
) -> bool {
    !root_is_layer_surface && root_output.is_some() && root_output == focused_layer_output
}

impl SpidersWm {
    pub(crate) fn apply_toplevel_decoration_mode(&mut self, toplevel: &ToplevelSurface) {
        let Some(window_id) = self.window_id_for_surface(toplevel.wl_surface()) else {
            return;
        };

        let visible_window_ids = self.layout_window_ids_for_window(&window_id);
        let workspace_id = self.window_workspace_id(&window_id);
        let workspace_layout = workspace_id
            .as_ref()
            .and_then(|id| self.model.workspaces.get(id))
            .and_then(|workspace| workspace.effective_layout.as_ref())
            .map(|layout| layout.name.clone());
        let appearance = self
            .scene
            .compute_window_appearance_plan(&self.config, &self.model, &visible_window_ids, &window_id)
            .map(|plan| plan.appearance)
            .unwrap_or(AppearanceValue::Auto);

        let decoration_mode = match appearance {
            AppearanceValue::Auto => None,
            AppearanceValue::None => Some(Mode::ServerSide),
        };

        toplevel.with_pending_state(|state| {
            state.decoration_mode = decoration_mode;
        });

        debug!(
            window = %window_id.0,
            ?workspace_id,
            workspace_layout = ?workspace_layout,
            visible_window_ids = ?visible_window_ids,
            appearance = ?appearance,
            decoration_mode = ?decoration_mode,
            "wm applied toplevel decoration mode"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_grab_keyboard_focus_allowed_for_regular_windows() {
        assert!(popup_grab_can_take_keyboard_focus(false, None));
    }

    #[test]
    fn popup_grab_keyboard_focus_depends_on_layer_interactivity() {
        assert!(popup_grab_can_take_keyboard_focus(true, Some(true)));
        assert!(!popup_grab_can_take_keyboard_focus(true, Some(false)));
        assert!(!popup_grab_can_take_keyboard_focus(true, None));
    }

    #[test]
    fn popup_grab_mismatch_requires_current_or_previous_serial_match() {
        assert!(!popup_grab_has_mismatch(true, false));
        assert!(!popup_grab_has_mismatch(false, true));
        assert!(popup_grab_has_mismatch(false, false));
    }

    #[test]
    fn popup_grab_denied_for_regular_window_when_layer_has_focus() {
        let output = output("a");

        assert!(popup_grab_should_be_denied(false, Some(&output), Some(&output)));
    }

    #[test]
    fn popup_grab_allowed_for_layer_roots_even_when_layer_has_focus() {
        let output = output("a");

        assert!(!popup_grab_should_be_denied(true, Some(&output), Some(&output)));
    }

    #[test]
    fn popup_grab_allowed_for_regular_window_without_layer_focus() {
        let output = output("a");

        assert!(!popup_grab_should_be_denied(false, Some(&output), None));
    }

    #[test]
    fn popup_grab_allowed_for_regular_window_on_different_output() {
        assert!(!popup_grab_should_be_denied(
            false,
            Some(&output("a")),
            Some(&output("b")),
        ));
    }

    fn output(name: &str) -> smithay::output::Output {
        smithay::output::Output::new(
            name.to_string(),
            smithay::output::PhysicalProperties {
                size: (0, 0).into(),
                subpixel: smithay::output::Subpixel::Unknown,
                make: "test".to_string(),
                model: "test".to_string(),
                serial_number: "test".to_string(),
            },
        )
    }
}
