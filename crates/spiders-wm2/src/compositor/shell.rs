use smithay::desktop::PopupKind;
use smithay::utils::Rectangle;
use smithay::wayland::shell::xdg::PopupSurface;
use tracing::info;

use crate::runtime::{RuntimeCommand, RuntimeResult};
use crate::state::SpidersWm;

impl SpidersWm {
    pub fn close_focused_window(&mut self) {
        let closing_window_id = match self
            .runtime()
            .execute(RuntimeCommand::RequestCloseFocusedWindowSelection)
        {
            RuntimeResult::CloseSelection(selection) => selection.closing_window_id,
            _ => None,
        };
        info!(closing_window = ?closing_window_id, "wm2 close focused window request");
        let Some(focused_surface) =
            closing_window_id.and_then(|window_id| self.surface_for_window_id(window_id))
        else {
            return;
        };

        self.capture_close_snapshot(&focused_surface);

        if let Some(record) = self.managed_window_for_surface(&focused_surface) {
            if let Some(toplevel) = record.window.toplevel() {
                toplevel.send_close();
            }
        }
    }

    pub fn toggle_focused_window_floating(&mut self) {
        let toggled_window_id = match self
            .runtime()
            .execute(RuntimeCommand::ToggleFocusedWindowFloating)
        {
            RuntimeResult::Window(toggled_window_id) => toggled_window_id,
            _ => None,
        };
        if toggled_window_id.is_none() {
            return;
        }

        self.schedule_relayout();
        if let Some(window_id) = toggled_window_id {
            let floating = self
                .model
                .windows
                .get(&window_id)
                .is_some_and(|window| window.floating);
            self.emit_window_floating_change(window_id, floating);
        }
    }

    pub fn toggle_focused_window_fullscreen(&mut self) {
        let toggled_window_id = match self
            .runtime()
            .execute(RuntimeCommand::ToggleFocusedWindowFullscreen)
        {
            RuntimeResult::Window(toggled_window_id) => toggled_window_id,
            _ => None,
        };
        if toggled_window_id.is_none() {
            return;
        }

        self.schedule_relayout();
        if let Some(window_id) = toggled_window_id {
            let fullscreen = self
                .model
                .windows
                .get(&window_id)
                .is_some_and(|window| window.fullscreen);
            self.emit_window_fullscreen_change(window_id, fullscreen);
        }
    }

    pub fn unconstrain_popup(&self, popup: &PopupSurface) {
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
