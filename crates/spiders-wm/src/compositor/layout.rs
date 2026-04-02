use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface};
use smithay::utils::{Logical, Point, Rectangle, Size};
use tracing::{debug, info};

use crate::frame_sync::SyncHandle;
use spiders_core::WindowId;
use spiders_core::wm::WindowGeometry;
use crate::state::SpidersWm;

impl SpidersWm {
    pub fn schedule_relayout(&mut self) {
        debug!(
            window_count = self.managed_window_count(),
            "wm schedule relayout"
        );
        let _ = self.start_relayout();
    }

    pub fn planned_layout_for_surface(
        &mut self,
        surface: &WlSurface,
    ) -> Option<(Point<i32, Logical>, Size<i32, Logical>)> {
        let output_geometry = self.current_output_geometry()?;
        let visible_window_ids = self.visible_managed_window_ids();
        let window_id = self.window_id_for_surface(surface)?;

        let fullscreen_window_id = self
            .model
            .fullscreen_window_on_current_workspace(visible_window_ids.iter().cloned());

        if let Some(fullscreen_window_id) = fullscreen_window_id.as_ref() {
            return (fullscreen_window_id == &window_id)
                .then_some((output_geometry.loc, output_geometry.size));
        }

        if self.window_is_floating(&window_id) {
            return self.floating_layout_for_window(&window_id, output_geometry);
        }

        let tiled_window_ids = self.tiled_visible_window_ids(&visible_window_ids);

        if let Some(target) = self.scene.compute_layout_target(
            &self.config,
            &self.model,
            &tiled_window_ids,
            &window_id,
        ) {
            return Some(target);
        }

        let visible_index = tiled_window_ids.iter().position(|id| id == &window_id)?;
        let fallback = plan_tiled_slot(output_geometry, tiled_window_ids.len(), visible_index)?;

        Some((fallback.location, fallback.size))
    }

    pub(crate) fn start_relayout(&mut self) -> Option<SyncHandle> {
        let output = self
            .current_output_cloned()
            .expect("output must exist before relayout");
        let output_geometry = self
            .output_geometry_for(&output)
            .expect("output geometry missing during relayout");

        let visible_window_ids = self.visible_managed_window_ids();
        let tiled_window_ids = self.tiled_visible_window_ids(&visible_window_ids);
        let fullscreen_window_id = self
            .model
            .fullscreen_window_on_current_workspace(visible_window_ids.iter().cloned());
        info!(
            visible_windows = visible_window_ids.len(),
            total_windows = self.managed_window_count(),
            fullscreen_window = ?fullscreen_window_id,
            "wm relayout start"
        );
        self.log_managed_window_state("before relayout");

        let windows_to_unmap = self
            .managed_windows()
            .iter()
            .filter(|record| {
                !self.window_is_on_current_workspace(&record.id)
                    || fullscreen_window_id
                        .as_ref()
                        .is_some_and(|window_id| *window_id != record.id)
            })
            .map(|record| record.window.clone())
            .collect::<Vec<_>>();
        for window in &windows_to_unmap {
            self.unmap_window_element(window);
        }

        if visible_window_ids.is_empty() {
            debug!("wm relayout skipped because there are no visible windows");
            return None;
        }

        if fullscreen_window_id.is_some() {
            for window_id in visible_window_ids
                .iter()
                .filter(|window_id| fullscreen_window_id.as_ref() != Some(window_id))
            {
                if let Some(toplevel) = self
                    .managed_window_for_id(window_id)
                    .and_then(|record| record.window.toplevel().cloned())
                {
                    if sync_toplevel_fullscreen_state(&toplevel, false, None) {
                        let _ = toplevel.send_configure();
                    }
                }
            }
        }

        let relayout_targets = if let Some(fullscreen_window_id) = fullscreen_window_id.as_ref() {
            visible_window_ids
                .iter()
                .filter(|window_id| *window_id == fullscreen_window_id)
                .cloned()
                .map(|window_id| crate::scene::adapter::LayoutTarget {
                    window_id,
                    location: output_geometry.loc,
                    size: output_geometry.size,
                    fullscreen: true,
                })
                .collect::<Vec<_>>()
        } else {
            let floating_window_ids = visible_window_ids
                .iter()
                .filter(|window_id| self.window_is_floating(window_id))
                .cloned()
                .collect::<Vec<_>>();
            let mut targets = if tiled_window_ids.is_empty() {
                Vec::new()
            } else {
                self.scene
                    .compute_layout_targets(&self.config, &self.model, &tiled_window_ids)?
            };
            targets.extend(
                floating_window_ids.iter().filter_map(|window_id| {
                    self.floating_layout_target(window_id, output_geometry)
                }),
            );
            targets
        };
        let mut relayout_transaction: Option<SyncHandle> = None;

        for target in relayout_targets {
            let Some(record) = self.managed_window_for_id(&target.window_id) else {
                continue;
            };

            let current_location = self.element_location(&record.window);
            let toplevel = record.window.toplevel().cloned();
            let window_id = record.id.clone();
            let mapped = record.mapped;

            if let Some(toplevel) = toplevel {
                let fullscreen_output = target
                    .fullscreen
                    .then(|| fullscreen_output_for_toplevel(&output, &toplevel))
                    .flatten();
                let mut needs_configure = false;
                toplevel.with_pending_state(|state| {
                    if state.size != Some(target.size) {
                        needs_configure = true;
                    }
                    if sync_pending_fullscreen_state(
                        state,
                        target.fullscreen,
                        fullscreen_output.clone(),
                    ) {
                        needs_configure = true;
                    }
                    state.size = Some(target.size);
                });

                debug!(
                    window = %window_id.0,
                    mapped,
                    pending_configures = self
                        .managed_window_for_id(&window_id)
                        .is_some_and(|record| record.frame_sync.has_pending_configures()),
                    current_location = ?current_location,
                    target_location = ?target.location,
                    target_size = ?target.size,
                    fullscreen = target.fullscreen,
                    needs_configure,
                    "wm relayout window plan"
                );

                if needs_configure {
                    let serial = toplevel.send_configure();
                    let transaction = relayout_transaction
                        .get_or_insert_with(|| crate::frame_sync::new_sync_handle(&self.event_loop))
                        .clone();
                    let record = self
                        .managed_window_mut_for_id(&target.window_id)
                        .expect("managed window disappeared during relayout");
                    record.frame_sync.track_pending_layout(
                        serial,
                        target.location,
                        target.size,
                        transaction,
                    );
                    debug!(window = %window_id.0, ?serial, "wm sent configure during relayout");
                } else {
                    let window = self.managed_window_for_id(&window_id).and_then(|record| {
                        (!record.frame_sync.has_pending_configures()).then(|| record.window.clone())
                    });

                    if let Some(window) = window {
                        self.map_window_element(window, target.location);
                        debug!(window = %window_id.0, location = ?target.location, "wm mapped window during relayout");
                    } else {
                        debug!(window = %window_id.0, "wm deferred remap until pending configure commits");
                    }
                }
            }
        }

        self.log_managed_window_state("after relayout");
        relayout_transaction
    }
}

impl SpidersWm {
    fn tiled_visible_window_ids(&self, visible_window_ids: &[WindowId]) -> Vec<WindowId> {
        visible_window_ids
            .iter()
            .filter(|window_id| !self.window_is_floating(window_id))
            .cloned()
            .collect()
    }

    fn floating_layout_for_window(
        &mut self,
        window_id: &WindowId,
        output_geometry: Rectangle<i32, Logical>,
    ) -> Option<(Point<i32, Logical>, Size<i32, Logical>)> {
        let geometry = self.ensure_floating_window_geometry(window_id, output_geometry)?;
        Some((
            Point::from((geometry.x, geometry.y)),
            Size::from((geometry.width, geometry.height)),
        ))
    }

    fn floating_layout_target(
        &mut self,
        window_id: &WindowId,
        output_geometry: Rectangle<i32, Logical>,
    ) -> Option<crate::scene::adapter::LayoutTarget> {
        let (location, size) = self.floating_layout_for_window(window_id, output_geometry)?;
        Some(crate::scene::adapter::LayoutTarget {
            window_id: window_id.clone(),
            location,
            size,
            fullscreen: false,
        })
    }

    pub(crate) fn ensure_floating_window_geometry(
        &mut self,
        window_id: &WindowId,
        output_geometry: Rectangle<i32, Logical>,
    ) -> Option<WindowGeometry> {
        if let Some(geometry) = self.window_floating_geometry(window_id) {
            return Some(geometry);
        }

        let geometry = self
            .managed_window_for_id(window_id)
            .and_then(|record| {
                self.element_location(&record.window)
                    .map(|location| WindowGeometry {
                        x: location.x,
                        y: location.y,
                        width: record.window.geometry().size.w.max(1),
                        height: record.window.geometry().size.h.max(1),
                    })
            })
            .unwrap_or_else(|| default_floating_geometry(output_geometry));

        self.set_window_floating_geometry(window_id.clone(), geometry);
        Some(geometry)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RelayoutSlot {
    location: Point<i32, Logical>,
    size: Size<i32, Logical>,
}

fn plan_tiled_slot(
    output_geometry: Rectangle<i32, Logical>,
    count: usize,
    index: usize,
) -> Option<RelayoutSlot> {
    if count == 0 || index >= count {
        return None;
    }

    let output_width = output_geometry.size.w.max(1);
    let output_height = output_geometry.size.h.max(1);

    if count == 1 {
        return Some(RelayoutSlot {
            location: output_geometry.loc,
            size: Size::from((output_width, output_height)),
        });
    }

    let master_width = ((output_width * 3) / 5).max(1);
    let stack_width = (output_width - master_width).max(1);

    if index == 0 {
        return Some(RelayoutSlot {
            location: output_geometry.loc,
            size: Size::from((master_width, output_height)),
        });
    }

    let stack_count = (count - 1) as i32;
    let stack_index = (index - 1) as i32;
    let base_height = (output_height / stack_count).max(1);
    let remainder = output_height.rem_euclid(stack_count);
    let height = (base_height + i32::from(stack_index < remainder)).max(1);
    let y = output_geometry.loc.y + stack_index * base_height + remainder.min(stack_index);

    Some(RelayoutSlot {
        location: Point::from((output_geometry.loc.x + master_width, y)),
        size: Size::from((stack_width, height)),
    })
}

fn default_floating_geometry(output_geometry: Rectangle<i32, Logical>) -> WindowGeometry {
    let width = ((output_geometry.size.w * 4) / 5)
        .max(320)
        .min(output_geometry.size.w.max(1));
    let height = ((output_geometry.size.h * 4) / 5)
        .max(240)
        .min(output_geometry.size.h.max(1));
    let x = output_geometry.loc.x + (output_geometry.size.w - width) / 2;
    let y = output_geometry.loc.y + (output_geometry.size.h - height) / 2;

    WindowGeometry {
        x,
        y,
        width,
        height,
    }
}

fn sync_toplevel_fullscreen_state(
    toplevel: &smithay::wayland::shell::xdg::ToplevelSurface,
    fullscreen: bool,
    fullscreen_output: Option<WlOutput>,
) -> bool {
    let mut changed = false;
    toplevel.with_pending_state(|state| {
        changed = sync_pending_fullscreen_state(state, fullscreen, fullscreen_output.clone());
    });
    changed
}

fn sync_pending_fullscreen_state(
    state: &mut smithay::wayland::shell::xdg::ToplevelState,
    fullscreen: bool,
    fullscreen_output: Option<WlOutput>,
) -> bool {
    let output_changed = state.fullscreen_output != fullscreen_output;
    state.fullscreen_output = fullscreen_output;

    if fullscreen {
        state.states.set(xdg_toplevel::State::Fullscreen) || output_changed
    } else {
        state.states.unset(xdg_toplevel::State::Fullscreen) || output_changed
    }
}

fn fullscreen_output_for_toplevel(
    output: &Output,
    toplevel: &smithay::wayland::shell::xdg::ToplevelSurface,
) -> Option<WlOutput> {
    let client = toplevel.wl_surface().client()?;
    output.client_outputs(&client).next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_tiled_slot_uses_master_column_for_first_window() {
        let output = Rectangle::new((10, 20).into(), (100, 50).into());

        let slot = plan_tiled_slot(output, 2, 0).expect("master slot should exist");

        assert_eq!(slot.location, Point::from((10, 20)));
        assert_eq!(slot.size, Size::from((60, 50)));
    }

    #[test]
    fn plan_tiled_slot_splits_stack_windows_by_height() {
        let output = Rectangle::new((10, 20).into(), (100, 101).into());

        let slot = plan_tiled_slot(output, 4, 2).expect("stack slot should exist");

        assert_eq!(slot.location, Point::from((70, 54)));
        assert_eq!(slot.size, Size::from((40, 34)));
    }

    #[test]
    fn default_floating_geometry_centers_window_on_output() {
        let output = Rectangle::new((10, 20).into(), (1000, 800).into());

        let geometry = default_floating_geometry(output);

        assert_eq!(geometry.width, 800);
        assert_eq!(geometry.height, 640);
        assert_eq!(geometry.x, 110);
        assert_eq!(geometry.y, 100);
    }

    #[test]
    fn sync_pending_fullscreen_state_sets_fullscreen_flag() {
        let mut state = smithay::wayland::shell::xdg::ToplevelState::default();

        let changed = sync_pending_fullscreen_state(&mut state, true, None);

        assert!(changed);
        assert!(state.states.contains(xdg_toplevel::State::Fullscreen));
        assert_eq!(state.fullscreen_output, None);
    }

    #[test]
    fn sync_pending_fullscreen_state_clears_fullscreen_flag() {
        let mut state = smithay::wayland::shell::xdg::ToplevelState::default();
        state.states.set(xdg_toplevel::State::Fullscreen);

        let changed = sync_pending_fullscreen_state(&mut state, false, None);

        assert!(changed);
        assert!(!state.states.contains(xdg_toplevel::State::Fullscreen));
        assert_eq!(state.fullscreen_output, None);
    }

    #[test]
    fn sync_pending_fullscreen_state_is_stable_when_unchanged() {
        let mut state = smithay::wayland::shell::xdg::ToplevelState::default();

        let changed = sync_pending_fullscreen_state(&mut state, false, None);

        assert!(!changed);
        assert!(!state.states.contains(xdg_toplevel::State::Fullscreen));
        assert_eq!(state.fullscreen_output, None);
    }
}
