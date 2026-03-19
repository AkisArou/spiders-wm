use smithay::{
    desktop::Window,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::{IsAlive, SERIAL_COUNTER},
};
use tracing::trace;

use crate::{actions, placement, runtime::SpidersWm2};

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

        if was_focused {
            let next_surface = actions::next_focus_in_active_workspace(&self.app.wm)
                .and_then(|window_id| self.app.bindings.surface_for_window(&window_id));

            self.focus_window_surface(next_surface, SERIAL_COUNTER.next_serial());
        }

        self.refresh_active_workspace();
    }

    pub fn refresh_active_workspace(&mut self) {
        self.refresh_layout_artifacts();
        self.sync_desired_transaction();

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
            self.runtime
                .render_plan
                .stage_from_refresh_plan(plan, &self.app.wm);
            self.recompute_layout(plan);
        }
        let affected_windows = refresh_plan
            .map(|plan| plan.windows)
            .unwrap_or_else(|| self.app.bindings.known_windows().into_iter().collect());

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
                self.output_rect(),
                &window_id,
            );
            let location = presented_rect
                .map(|rect| rect.loc)
                .unwrap_or_else(|| (0, 0).into());

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

                self.apply_window_geometry(&window);

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

    pub fn cleanup_dead_windows(&mut self) {
        let dead_surfaces = self
            .app
            .bindings
            .known_windows()
            .into_iter()
            .filter_map(|window_id| self.app.bindings.surface_for_window(&window_id))
            .filter(|surface| !surface.alive())
            .collect::<Vec<_>>();

        for surface in dead_surfaces {
            self.unmap_window_surface(&surface);
        }
    }

    pub(crate) fn finalize_deferred_window_removals(&mut self) {
        for window_id in self.runtime.transactions.drain_deferred_removals() {
            self.app.bindings.unbind_window(&window_id);
            actions::remove_window(&mut self.app.topology, &mut self.app.wm, window_id);
        }
    }

    fn apply_window_geometry(&mut self, window: &Window) {
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

        let rect = placement::desired_window_rect(&self.app, self.output_rect(), &window_id);

        if let Some(rect) = rect {
            toplevel.with_pending_state(|state| {
                state.size = Some(rect.size);
            });

            if toplevel.is_initial_configure_sent() {
                if let Some(serial) = toplevel.send_pending_configure() {
                    self.runtime
                        .transactions
                        .register_configure(&window_id, serial);
                }
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

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::{
        config::built_in_default_config,
        model::{OutputId, OutputNode, WmState, WorkspaceId},
        placement,
        transactions::TransactionManager,
    };
    use spiders_shared::{
        ids::WindowId,
        wm::{
            OutputSnapshot, OutputTransform, ShellKind, StateSnapshot,
            WindowMode as SharedWindowMode, WindowSnapshot,
        },
    };

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
