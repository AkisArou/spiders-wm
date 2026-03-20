use smithay::{
    desktop::WindowSurfaceType,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::{Logical, Point, Serial},
};

use crate::runtime::SpidersWm2;

impl SpidersWm2 {
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.runtime
            .smithay
            .space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(surface, point)| (surface, (point + location).to_f64()))
            })
    }

    pub fn focus_window_surface(&mut self, surface: Option<WlSurface>, serial: Serial) {
        if let Some(committed_surface) = self.committed_focus_surface() {
            if surface
                .as_ref()
                .is_none_or(|target| target.id() != committed_surface.id())
            {
                self.apply_focus_surface(Some(committed_surface), serial);
                return;
            }
        }

        self.apply_focus_surface(surface, serial);
    }

    fn committed_focus_surface(&self) -> Option<WlSurface> {
        self.runtime
            .transactions
            .pending()
            .and(self.runtime.transactions.committed())
            .and_then(|snapshot| snapshot.focused_window_id.as_ref())
            .and_then(|window_id| self.app.bindings.surface_for_window(window_id))
    }

    fn apply_focus_surface(&mut self, surface: Option<WlSurface>, serial: Serial) {
        let window_to_raise = surface
            .as_ref()
            .and_then(|target_surface| {
                self.runtime.smithay.space.elements().find(|window| {
                    window
                        .toplevel()
                        .is_some_and(|toplevel| toplevel.wl_surface().id() == target_surface.id())
                })
            })
            .cloned();

        if let Some(window) = window_to_raise {
            self.runtime.smithay.space.raise_element(&window, true);
        }

        self.runtime.smithay.space.elements().for_each(|mapped| {
            let is_focused = surface.as_ref().is_some_and(|target_surface| {
                mapped
                    .toplevel()
                    .is_some_and(|toplevel| toplevel.wl_surface().id() == target_surface.id())
            });

            let activation_changed = mapped.set_activated(is_focused);

            if activation_changed {
                if let Some(toplevel) = mapped.toplevel() {
                    toplevel.send_pending_configure();
                }
            }
        });

        if let Some(keyboard) = self.runtime.smithay.seat.get_keyboard() {
            keyboard.set_focus(self, surface, serial);
        }
    }
}
