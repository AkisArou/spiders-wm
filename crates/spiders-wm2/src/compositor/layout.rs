use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface};
use smithay::reexports::wayland_server::Resource;
use smithay::utils::{Logical, Point, Size};
use tracing::{debug, info};

use crate::frame_sync::SyncHandle;
use crate::state::SpidersWm;

impl SpidersWm {
    pub fn schedule_relayout(&mut self) {
        debug!(
            window_count = self.managed_windows.len(),
            "wm2 schedule relayout"
        );
        let _ = self.start_relayout();
    }

    pub fn planned_layout_for_surface(
        &mut self,
        surface: &WlSurface,
    ) -> Option<(Point<i32, Logical>, Size<i32, Logical>)> {
        let output = self.space.outputs().next()?;
        let output_geometry = self.space.output_geometry(output)?;
        let visible_window_ids = self
            .visible_managed_window_positions()
            .into_iter()
            .map(|managed_index| self.managed_windows[managed_index].id.clone())
            .collect::<Vec<_>>();
        let window_id = self.window_id_for_surface(surface)?;

        let fullscreen_window_id = self
            .model
            .fullscreen_window_on_current_workspace(visible_window_ids.iter().cloned());

        if let Some(fullscreen_window_id) = fullscreen_window_id.as_ref() {
            return (fullscreen_window_id == &window_id)
                .then_some((output_geometry.loc, output_geometry.size));
        }

        if let Some(target) = self
            .scene
            .compute_layout_target(&self.config, &self.model, &visible_window_ids, &window_id)
        {
            return Some(target);
        }

        crate::scene::adapter::bootstrap_layout_target(output_geometry, &visible_window_ids, &window_id)
    }

    pub(crate) fn start_relayout(&mut self) -> Option<SyncHandle> {
        let output = self
            .space
            .outputs()
            .next()
            .cloned()
            .expect("output must exist before relayout");
        let output_geometry = self
            .space
            .output_geometry(&output)
            .expect("output geometry missing during relayout");

        let visible_positions = self.visible_managed_window_positions();
        let visible_window_ids = visible_positions
            .iter()
            .map(|managed_index| self.managed_windows[*managed_index].id.clone())
            .collect::<Vec<_>>();
        let fullscreen_window_id = self
            .model
            .fullscreen_window_on_current_workspace(visible_window_ids.iter().cloned());
        info!(
            visible_windows = visible_positions.len(),
            total_windows = self.managed_windows.len(),
            fullscreen_window = ?fullscreen_window_id,
            "wm2 relayout start"
        );
        self.log_managed_window_state("before relayout");

        for record in &self.managed_windows {
            if !self.model.window_is_on_current_workspace(record.id.clone())
                || fullscreen_window_id.as_ref().is_some_and(|window_id| *window_id != record.id)
            {
                self.space.unmap_elem(&record.window);
            }
        }

        if visible_positions.is_empty() {
            debug!("wm2 relayout skipped because there are no visible windows");
            return None;
        }

        if fullscreen_window_id.is_some() {
            for managed_index in self
                .visible_managed_window_positions()
                .into_iter()
                .filter(|managed_index| {
                    fullscreen_window_id.as_ref() != Some(&self.managed_windows[*managed_index].id)
                })
            {
                if let Some(toplevel) = self.managed_windows[managed_index].window.toplevel().cloned() {
                    if sync_toplevel_fullscreen_state(&toplevel, false, None) {
                        let _ = toplevel.send_configure();
                    }
                }
            }
        }

        let relayout_targets = if fullscreen_window_id.is_some() {
            crate::scene::adapter::bootstrap_layout_targets(
                output_geometry,
                &visible_window_ids,
                fullscreen_window_id.as_ref(),
            )
        } else {
            self.scene
                .compute_layout_targets(&self.config, &self.model, &visible_window_ids)
                .unwrap_or_else(|| {
                    crate::scene::adapter::bootstrap_layout_targets(
                        output_geometry,
                        &visible_window_ids,
                        None,
                    )
                })
        };
        let mut relayout_transaction: Option<SyncHandle> = None;

        for target in relayout_targets {
            let Some(managed_index) = self
                .managed_windows
                .iter()
                .position(|record| record.id == target.window_id)
            else {
                continue;
            };

            let current_location = self
                .space
                .element_location(&self.managed_windows[managed_index].window);
            let toplevel = self.managed_windows[managed_index].window.toplevel().cloned();

            if let Some(toplevel) = toplevel {
                let record = &mut self.managed_windows[managed_index];
                let fullscreen_output = target
                    .fullscreen
                    .then(|| fullscreen_output_for_toplevel(&output, &toplevel))
                    .flatten();
                let mut needs_configure = false;
                toplevel.with_pending_state(|state| {
                    if state.size != Some(target.size) {
                        needs_configure = true;
                    }
                    if sync_pending_fullscreen_state(state, target.fullscreen, fullscreen_output.clone()) {
                        needs_configure = true;
                    }
                    state.size = Some(target.size);
                });

                debug!(
                    window = %record.id.0,
                    mapped = record.mapped,
                    pending_configures = record.frame_sync.has_pending_configures(),
                    current_location = ?current_location,
                    target_location = ?target.location,
                    target_size = ?target.size,
                    fullscreen = target.fullscreen,
                    needs_configure,
                    "wm2 relayout window plan"
                );

                if needs_configure {
                    let serial = toplevel.send_configure();
                    let transaction = relayout_transaction
                        .get_or_insert_with(|| crate::frame_sync::new_sync_handle(&self.event_loop))
                        .clone();
                    record.frame_sync.track_pending_layout(
                        serial,
                        target.location,
                        target.size,
                        transaction,
                    );
                    debug!(window = %record.id.0, ?serial, "wm2 sent configure during relayout");
                } else if !record.frame_sync.has_pending_configures() {
                    self.space.map_element(record.window.clone(), target.location, false);
                    debug!(window = %record.id.0, location = ?target.location, "wm2 mapped window during relayout");
                } else {
                    debug!(window = %record.id.0, "wm2 deferred remap until pending configure commits");
                }
            }
        }

        self.log_managed_window_state("after relayout");
        relayout_transaction
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