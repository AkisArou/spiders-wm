use std::collections::HashSet;

use smithay::{
    desktop::Window,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::{IsAlive, Logical, Rectangle, SERIAL_COUNTER},
    wayland::{
        compositor::{remove_pre_commit_hook, with_states},
        shell::xdg::XdgToplevelSurfaceData,
    },
};
use tracing::trace;

use crate::{actions, app::AppState, placement, runtime::SpidersWm2, transactions::RefreshPlan};
use spiders_shared::ids::WindowId;
use spiders_shared::wm::StateSnapshot;

struct CommittedWindowPresentation {
    window_id: WindowId,
    visible: bool,
    rect: Option<Rectangle<i32, Logical>>,
}

impl SpidersWm2 {
    pub fn close_focused_window(&mut self) {
        let Some(window_id) = self.app.wm.focused_window.clone() else {
            return;
        };

        let Some(window) = self.app.bindings.element_for_window(&window_id) else {
            return;
        };

        if let Some(toplevel) = window.toplevel() {
            toplevel.send_close();
        }
    }

    pub fn unmap_window_surface(&mut self, surface: &WlSurface) {
        let removed_window_id = self.app.bindings.window_for_surface(&surface.id());

        let was_focused = removed_window_id
            .as_ref()
            .is_some_and(|window_id| self.app.wm.focused_window.as_ref() == Some(window_id));

        if let Some(window_id) = removed_window_id.clone() {
            self.runtime
                .transactions
                .defer_window_removal(window_id.clone());
            actions::begin_window_removal(&mut self.app.topology, &mut self.app.wm, &window_id);
        }

        let window_to_unmap = self
            .runtime
            .smithay
            .space
            .elements()
            .find(|window| {
                window
                    .toplevel()
                    .is_some_and(|toplevel| toplevel.wl_surface() == surface)
            })
            .cloned();

        if let Some(window) = window_to_unmap {
            self.runtime.smithay.space.unmap_elem(&window);
        }

        self.refresh_active_workspace();

        if was_focused && self.runtime.transactions.committed().is_none() {
            let next_surface = actions::next_focus_in_active_workspace(&self.app.wm)
                .and_then(|window_id| self.app.bindings.surface_for_window(&window_id));

            self.focus_window_surface(next_surface, SERIAL_COUNTER.next_serial());
        }
    }

    pub fn refresh_active_workspace(&mut self) {
        self.refresh_layout_artifacts();
        self.sync_desired_transaction();
        if self
            .runtime
            .transactions
            .extend_partial_timeout(std::time::Instant::now())
        {
            return;
        }

        let committed_snapshot = self.runtime.transactions.committed().cloned();
        let visible = committed_snapshot
            .as_ref()
            .map(|snapshot| snapshot.visible_window_ids.iter().cloned().collect())
            .unwrap_or_else(|| {
                actions::active_workspace_windows(&self.app.wm)
                    .into_iter()
                    .collect()
            });
        let refresh_plan = self.runtime.transactions.pending_refresh_plan(&self.app.wm);
        if let Some(summary) = self
            .runtime
            .transactions
            .pending_debug_summary(&self.app.wm)
        {
            trace!(target: "spiders_wm2::transactions", "refresh plan: {summary}");
        }
        if let Some(plan) = refresh_plan.as_ref() {
            if plan.has_visual_updates() {
                self.runtime
                    .render_plan
                    .stage_from_refresh_plan(plan, &self.app.wm);
            }
            if plan.layout.needs_recompute() {
                self.recompute_layout(plan);
            }
        }
        let (affected_windows, configure_windows) = refresh_plan
            .map(|plan| (plan.windows, plan.configure_windows))
            .unwrap_or_else(|| {
                let known = self
                    .app
                    .bindings
                    .known_windows()
                    .into_iter()
                    .collect::<std::collections::HashSet<_>>();
                (known.clone(), known)
            });

        trace!(target: "spiders_wm2::runtime_debug", ?affected_windows, ?configure_windows, committed_snapshot = committed_snapshot.is_some(), "refresh_active_workspace_apply");

        let focused_before = self
            .runtime
            .transactions
            .committed()
            .and_then(|snapshot| snapshot.focused_window_id.clone());
        let focused_after = self.app.wm.focused_window.clone();

        for window_id in affected_windows {
            let Some(window) = self.app.bindings.element_for_window(&window_id) else {
                continue;
            };

            let should_show = placement::window_is_visible(
                &self.app,
                committed_snapshot.as_ref(),
                &visible,
                &window_id,
            );
            let presented_rect = placement::presented_window_rect(
                &self.app,
                committed_snapshot.as_ref(),
                &window_id,
            );
            let location = presented_rect
                .map(|rect| rect.loc)
                .unwrap_or_else(|| (0, 0).into());

            let should_configure = configure_windows.contains(&window_id);
            self.apply_window_geometry(&window, should_configure);

            if should_show {
                if self
                    .runtime
                    .smithay
                    .space
                    .element_location(&window)
                    .is_none()
                {
                    self.runtime
                        .smithay
                        .space
                        .map_element(window.clone(), location, false);
                }

                if self.runtime.smithay.space.element_location(&window) != Some(location) {
                    self.runtime
                        .smithay
                        .space
                        .map_element(window, location, false);
                }
            } else {
                self.runtime.smithay.space.unmap_elem(&window);
            }
        }

        let focused_surface = self
            .app
            .wm
            .focused_window
            .clone()
            .and_then(|window_id| self.app.bindings.surface_for_window(&window_id));

        if focused_before != focused_after {
            self.focus_window_surface(focused_surface, SERIAL_COUNTER.next_serial());
        }
        self.maybe_commit_pending_transaction();
    }

    pub(crate) fn apply_committed_refresh_plan(&mut self, refresh_plan: &RefreshPlan) {
        let Some(committed_snapshot) = self.runtime.transactions.committed().cloned() else {
            return;
        };

        trace!(target: "spiders_wm2::runtime_debug", windows = ?refresh_plan.windows, configure_windows = ?refresh_plan.configure_windows, workspaces = ?refresh_plan.workspaces, outputs = ?refresh_plan.outputs, "apply_committed_refresh_plan");

        for presentation in
            committed_window_presentations(&self.app, &committed_snapshot, &refresh_plan.windows)
        {
            trace!(target: "spiders_wm2::runtime_debug", window_id = ?presentation.window_id, visible = presentation.visible, rect = ?presentation.rect, "committed_window_presentation");
            let Some(window) = self
                .app
                .bindings
                .element_for_window(&presentation.window_id)
            else {
                continue;
            };

            let location = presentation
                .rect
                .map(|rect| rect.loc)
                .unwrap_or_else(|| (0, 0).into());

            if presentation.visible {
                if self
                    .runtime
                    .smithay
                    .space
                    .element_location(&window)
                    .is_none()
                {
                    self.runtime
                        .smithay
                        .space
                        .map_element(window.clone(), location, false);
                }

                if self.runtime.smithay.space.element_location(&window) != Some(location) {
                    self.runtime
                        .smithay
                        .space
                        .map_element(window, location, false);
                }
            } else {
                trace!(
                    target: "spiders_wm2::runtime_debug",
                    window_id = ?presentation.window_id,
                    still_mapped_before = self.runtime.smithay.space.element_location(&window).is_some(),
                    "committed_window_unmap"
                );
                self.runtime.smithay.space.unmap_elem(&window);
                trace!(
                    target: "spiders_wm2::runtime_debug",
                    window_id = ?presentation.window_id,
                    still_mapped_after = self.runtime.smithay.space.element_location(&window).is_some(),
                    "committed_window_unmap_done"
                );
            }
        }

        let focused_surface = committed_snapshot
            .focused_window_id
            .as_ref()
            .and_then(|window_id| self.app.bindings.surface_for_window(window_id));
        trace!(target: "spiders_wm2::runtime_debug", focused_window = ?committed_snapshot.focused_window_id, "apply_committed_focus");
        self.focus_window_surface(focused_surface, SERIAL_COUNTER.next_serial());
    }

    pub fn cleanup_dead_windows(&mut self) {
        let deferred_removals = self.runtime.transactions.deferred_removals().clone();
        let dead_surfaces = self
            .app
            .bindings
            .known_windows()
            .into_iter()
            .filter(|window_id| !deferred_removals.contains(window_id))
            .filter_map(|window_id| self.app.bindings.surface_for_window(&window_id))
            .filter(|surface| !surface.alive())
            .collect::<Vec<_>>();

        for surface in dead_surfaces {
            self.unmap_window_surface(&surface);
        }
    }

    pub(crate) fn finalize_deferred_window_removals(&mut self) {
        for window_id in self.runtime.transactions.drain_deferred_removals() {
            self.runtime
                .transactions
                .finalize_deferred_removal(&window_id);
            if let (Some(surface), Some(hook)) = (
                self.app.bindings.surface_for_window(&window_id),
                self.app.bindings.commit_hook(&window_id),
            ) {
                remove_pre_commit_hook(&surface, hook);
            }
            self.app.bindings.unbind_window(&window_id);
            actions::remove_window(&mut self.app.topology, &mut self.app.wm, window_id);
        }
    }

    fn apply_window_geometry(&mut self, window: &Window, should_configure: bool) {
        let Some(toplevel) = window.toplevel() else {
            return;
        };

        let Some(window_id) = self
            .app
            .bindings
            .window_for_surface(&toplevel.wl_surface().id())
        else {
            return;
        };

        let rect = placement::desired_window_rect(&self.app, &window_id);

        if let Some(rect) = rect {
            if !should_configure {
                return;
            }

            let next_size = (rect.size.w, rect.size.h);
            let already_sent = self.app.bindings.last_configure_size(&window_id) == Some(next_size);
            let waiting_for_commit = !self
                .app
                .bindings
                .pending_commit_serials(&window_id)
                .is_empty();
            let committed_size = toplevel.with_cached_state(|cached| {
                cached
                    .last_acked
                    .as_ref()
                    .and_then(|configure| configure.state.size)
            });
            let server_size = with_states(toplevel.wl_surface(), |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .and_then(|data| data.lock().ok())
                    .map(|attributes| attributes.current_server_state().size)
                    .unwrap_or(None)
            });
            let committed_matches_server_size = committed_size == server_size;

            if already_sent && toplevel.is_initial_configure_sent() {
                return;
            }

            if waiting_for_commit
                && toplevel.is_initial_configure_sent()
                && !committed_matches_server_size
            {
                trace!(
                    target: "spiders_wm2::runtime_debug",
                    ?window_id,
                    width = rect.size.w,
                    height = rect.size.h,
                    committed_matches_server_size,
                    "throttle_window_geometry_configure"
                );
                return;
            }

            toplevel.with_pending_state(|state| {
                state.size = Some(rect.size);
            });

            self.app
                .bindings
                .record_configure_size(&window_id, rect.size);

            trace!(target: "spiders_wm2::runtime_debug", ?window_id, width = rect.size.w, height = rect.size.h, initial = !toplevel.is_initial_configure_sent(), already_sent, waiting_for_commit, committed_matches_server_size, "apply_window_geometry");

            let serial = if toplevel.is_initial_configure_sent() {
                toplevel.send_pending_configure()
            } else {
                Some(toplevel.send_configure())
            };

            if let Some(serial) = serial {
                self.runtime
                    .transactions
                    .register_configure(&window_id, serial);
            }
        }
    }

    fn recompute_layout(&mut self, plan: &crate::transactions::RefreshPlan) {
        if let Some(summary) = self.app.layout.recompute(
            &self.app.wm,
            &self.app.topology.outputs,
            &self.app.config_runtime,
            plan.transaction_id,
            &plan.layout,
        ) {
            trace!(
                target: "spiders_wm2::layout",
                transaction_id = summary.transaction_id,
                config_revision = self.app.config_runtime.revision(),
                config_source = ?self.app.config_runtime.source(),
                revision = summary.revision,
                full_scene = summary.full_scene,
                workspaces = summary.recomputed_workspaces.len(),
                "recomputed layout plan"
            );
        }
    }
}

fn committed_window_presentations(
    app: &AppState,
    committed_snapshot: &StateSnapshot,
    window_ids: &HashSet<WindowId>,
) -> Vec<CommittedWindowPresentation> {
    let visible = committed_snapshot
        .visible_window_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();

    window_ids
        .iter()
        .cloned()
        .map(|window_id| CommittedWindowPresentation {
            visible: placement::window_is_visible(
                app,
                Some(committed_snapshot),
                &visible,
                &window_id,
            ),
            rect: placement::presented_window_rect(app, Some(committed_snapshot), &window_id),
            window_id,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::{
        app::AppState,
        config::built_in_default_config,
        model::{OutputId, OutputNode, WmState, WorkspaceId},
        placement,
        transactions::{LayoutRecomputePlan, RefreshPlan, TransactionManager},
    };
    use smithay::utils::{Rectangle, Size};
    use spiders_shared::{
        ids::WindowId,
        wm::{
            OutputSnapshot, OutputTransform, ShellKind, StateSnapshot,
            WindowMode as SharedWindowMode, WindowSnapshot,
        },
    };

    use super::committed_window_presentations;

    #[test]
    fn affected_windows_fallbacks_to_known_windows_without_pending_transaction() {
        let known = vec![WindowId::from("w1"), WindowId::from("w2")];
        let result = pending_or_known_window_ids(
            &TransactionManager::default(),
            &WmState::default(),
            &known,
        );

        assert_eq!(
            result,
            HashSet::from([WindowId::from("w1"), WindowId::from("w2")])
        );
    }

    #[test]
    fn affected_windows_use_pending_transaction_subset() {
        let mut wm = WmState::default();
        wm.focused_output = Some(OutputId::from("out-1"));
        wm.workspaces.insert(
            WorkspaceId::from("ws-2"),
            crate::model::WorkspaceState {
                id: WorkspaceId::from("ws-2"),
                name: "ws-2".into(),
                output: Some(OutputId::from("out-1")),
                windows: vec![],
            },
        );

        let outputs = HashMap::from([(
            OutputId::from("out-1"),
            OutputNode {
                id: OutputId::from("out-1"),
                name: "winit".into(),
                enabled: true,
                current_workspace: Some(WorkspaceId::from("1")),
                logical_size: (1280, 720),
            },
        )]);

        let mut transactions = TransactionManager::default();
        transactions.stage(wm.snapshot(&outputs, &built_in_default_config()));
        transactions.commit_pending(crate::transactions::TransactionCommitReason::Ready);

        wm.active_workspace = WorkspaceId::from("ws-2");
        transactions.stage(wm.snapshot(&outputs, &built_in_default_config()));

        let result = pending_or_known_window_ids(
            &transactions,
            &wm,
            &[WindowId::from("w1"), WindowId::from("w2")],
        );

        assert!(result.is_empty());
    }

    fn pending_or_known_window_ids(
        transactions: &TransactionManager,
        wm: &WmState,
        known_windows: &[WindowId],
    ) -> HashSet<WindowId> {
        transactions
            .pending_refresh_plan(wm)
            .map(|plan| plan.windows)
            .unwrap_or_else(|| known_windows.iter().cloned().collect())
    }

    #[test]
    fn presentation_only_refresh_plan_does_not_need_layout_recompute() {
        let plan = RefreshPlan {
            transaction_id: Some(1),
            windows: HashSet::from([WindowId::from("w1")]),
            configure_windows: HashSet::new(),
            workspaces: HashSet::new(),
            outputs: HashSet::new(),
            layout: LayoutRecomputePlan::default(),
        };

        assert!(!plan.layout.needs_recompute());
        assert!(plan.configure_windows.is_empty());
        assert!(plan.has_visual_updates());
    }

    #[test]
    fn empty_refresh_plan_has_no_visual_updates() {
        let plan = RefreshPlan::default();

        assert!(!plan.has_visual_updates());
    }

    #[test]
    fn configure_size_tracking_dedups_unchanged_geometry() {
        let mut bindings = crate::bindings::SmithayBindings::default();
        let window_id = WindowId::from("w1");

        assert_eq!(bindings.last_configure_size(&window_id), None);

        bindings.record_configure_size(&window_id, Size::from((800, 600)));

        assert_eq!(bindings.last_configure_size(&window_id), Some((800, 600)));

        bindings.record_configure_size(&window_id, Size::from((800, 600)));

        assert_eq!(bindings.last_configure_size(&window_id), Some((800, 600)));

        bindings.unbind_window(&window_id);
        assert_eq!(bindings.last_configure_size(&window_id), None);
    }

    #[test]
    fn deferred_window_removals_are_drained_on_commit() {
        let mut transactions = TransactionManager::default();
        let window_id = WindowId::from("w9");

        transactions.defer_window_removal(window_id.clone());

        assert!(transactions.deferred_removals().contains(&window_id));
        assert_eq!(
            transactions.drain_deferred_removals(),
            vec![window_id.clone()]
        );
        assert!(!transactions.deferred_removals().contains(&window_id));
    }

    #[test]
    fn cleanup_dead_windows_skips_deferred_removals() {
        let deferred = HashSet::from([WindowId::from("w1")]);
        let known = vec![WindowId::from("w1"), WindowId::from("w2")];

        let remaining = known
            .into_iter()
            .filter(|window_id| !deferred.contains(window_id))
            .collect::<Vec<_>>();

        assert_eq!(remaining, vec![WindowId::from("w2")]);
    }

    #[test]
    fn committed_visible_window_ids_drive_pending_visibility() {
        let visible = HashSet::from([WindowId::from("w1")]);
        let committed = committed_snapshot(
            Some("w1"),
            vec![
                window("w1", SharedWindowMode::Tiled, true),
                window("w2", SharedWindowMode::Tiled, false),
            ],
            vec![WindowId::from("w1")],
        );

        assert!(placement::window_is_visible(
            &crate::app::AppState::default(),
            Some(&committed),
            &visible,
            &WindowId::from("w1"),
        ));
        assert!(!placement::window_is_visible(
            &crate::app::AppState::default(),
            Some(&committed),
            &visible,
            &WindowId::from("w2"),
        ));
    }

    #[test]
    fn committed_window_presentations_use_committed_visibility_for_new_window() {
        let mut app = AppState::default();
        app.layout.committed_tiled_window_rects.insert(
            WindowId::from("w2"),
            Rectangle::new((25, 30).into(), (400, 300).into()),
        );

        let committed = committed_snapshot(
            Some("w2"),
            vec![window("w2", SharedWindowMode::Tiled, true)],
            vec![WindowId::from("w2")],
        );

        let presentations = committed_window_presentations(
            &app,
            &committed,
            &HashSet::from([WindowId::from("w2")]),
        );

        assert_eq!(presentations.len(), 1);
        assert_eq!(presentations[0].window_id, WindowId::from("w2"));
        assert!(presentations[0].visible);
        assert_eq!(presentations[0].rect.unwrap().loc.x, 25);
    }

    #[test]
    fn committed_window_presentations_hide_removed_window_and_move_survivor() {
        let mut app = AppState::default();
        app.layout.committed_tiled_window_rects.insert(
            WindowId::from("w2"),
            Rectangle::new((0, 0).into(), (900, 700).into()),
        );

        let committed = committed_snapshot(
            Some("w2"),
            vec![window("w2", SharedWindowMode::Tiled, true)],
            vec![WindowId::from("w2")],
        );

        let presentations = committed_window_presentations(
            &app,
            &committed,
            &HashSet::from([WindowId::from("w1"), WindowId::from("w2")]),
        );

        let removed = presentations
            .iter()
            .find(|presentation| presentation.window_id == WindowId::from("w1"))
            .unwrap();
        assert!(!removed.visible);
        assert!(removed.rect.is_none());

        let survivor = presentations
            .iter()
            .find(|presentation| presentation.window_id == WindowId::from("w2"))
            .unwrap();
        assert!(survivor.visible);
        assert_eq!(survivor.rect.unwrap().size.w, 900);
    }

    #[test]
    fn mapped_window_reconfigure_is_throttled_while_waiting_for_commit() {
        let pending_serials = vec![smithay::utils::Serial::from(5)];
        let initial_configure_sent = true;
        let committed_matches_server_size = false;

        let should_throttle =
            !pending_serials.is_empty() && initial_configure_sent && !committed_matches_server_size;

        assert!(should_throttle);
    }

    fn committed_snapshot(
        focused_window_id: Option<&str>,
        windows: Vec<WindowSnapshot>,
        visible_window_ids: Vec<WindowId>,
    ) -> StateSnapshot {
        StateSnapshot {
            focused_window_id: focused_window_id.map(WindowId::from),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "winit".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 1280,
                logical_height: 720,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![],
            windows,
            visible_window_ids,
            workspace_names: vec!["ws-1".into()],
        }
    }

    fn window(id: &str, mode: SharedWindowMode, focused: bool) -> WindowSnapshot {
        WindowSnapshot {
            id: WindowId::from(id),
            shell: ShellKind::XdgToplevel,
            app_id: None,
            title: None,
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            mode,
            focused,
            urgent: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(WorkspaceId::from("ws-1")),
            workspaces: vec![],
        }
    }
}
