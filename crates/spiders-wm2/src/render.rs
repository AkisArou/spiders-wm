use std::collections::HashSet;

use crate::{
    model::{OutputId, WmState},
    transactions::RefreshPlan,
};

#[derive(Debug, Default)]
pub struct RenderPlan {
    full_scene: bool,
    dirty_outputs: HashSet<OutputId>,
    staged_full_scene: bool,
    staged_outputs: HashSet<OutputId>,
    staged_presentation_only: bool,
}

impl RenderPlan {
    #[cfg(test)]
    pub fn mark_from_refresh_plan(&mut self, refresh_plan: &RefreshPlan, wm: &WmState) {
        let (full_scene, outputs) = collect_dirty_outputs(refresh_plan, wm);

        if full_scene {
            self.mark_full_scene();
        }

        self.dirty_outputs.extend(outputs);
    }

    pub fn stage_from_refresh_plan(&mut self, refresh_plan: &RefreshPlan, wm: &WmState) {
        let (full_scene, outputs) = collect_dirty_outputs(refresh_plan, wm);
        self.staged_full_scene = full_scene;
        self.staged_presentation_only = refresh_plan.is_presentation_only()
            && refresh_plan.has_visual_updates()
            && (full_scene || !outputs.is_empty());
        self.staged_outputs = outputs;
    }

    pub fn commit_refresh_plan(&mut self, refresh_plan: &RefreshPlan, wm: &WmState) {
        let (full_scene, outputs) = collect_dirty_outputs(refresh_plan, wm);

        if full_scene {
            self.mark_full_scene();
        }

        self.dirty_outputs.extend(outputs);
        self.staged_full_scene = false;
        self.staged_outputs.clear();
        self.staged_presentation_only = false;
    }

    pub fn promote_staged(&mut self) {
        if !self.has_staged_updates() {
            self.staged_presentation_only = false;
            return;
        }

        if self.staged_full_scene {
            self.mark_full_scene();
        }

        self.dirty_outputs.extend(self.staged_outputs.drain());
        self.staged_full_scene = false;
        self.staged_presentation_only = false;
    }

    pub fn promote_staged_if_needed(&mut self) -> bool {
        if !self.has_staged_updates() {
            self.staged_presentation_only = false;
            return false;
        }

        self.promote_staged();
        true
    }

    pub fn has_staged_updates(&self) -> bool {
        self.staged_full_scene || !self.staged_outputs.is_empty()
    }

    pub fn staged_presentation_only(&self) -> bool {
        self.staged_presentation_only
    }

    pub fn can_skip_redraw_until_commit(&self) -> bool {
        self.staged_presentation_only && !self.is_dirty()
    }

    pub fn mark_output_dirty(&mut self, output_id: OutputId) {
        self.dirty_outputs.insert(output_id);
    }

    pub fn mark_full_scene(&mut self) {
        self.full_scene = true;
    }

    pub fn should_render_output(&self, output_id: &OutputId) -> bool {
        self.full_scene || self.dirty_outputs.contains(output_id)
    }

    pub fn clear_output(&mut self, output_id: &OutputId) {
        self.dirty_outputs.remove(output_id);
        if self.dirty_outputs.is_empty() {
            self.full_scene = false;
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.full_scene || !self.dirty_outputs.is_empty()
    }
}

fn collect_dirty_outputs(refresh_plan: &RefreshPlan, wm: &WmState) -> (bool, HashSet<OutputId>) {
    let mut outputs = HashSet::new();

    if refresh_plan.layout.full_scene {
        return (true, HashSet::new());
    }

    outputs.extend(refresh_plan.outputs.iter().cloned());

    for workspace_id in &refresh_plan.workspaces {
        if let Some(output_id) = wm
            .workspaces
            .get(workspace_id)
            .and_then(|workspace| workspace.output.clone())
        {
            outputs.insert(output_id);
        }
    }

    for window_id in &refresh_plan.windows {
        if let Some(output_id) = wm
            .windows
            .get(window_id)
            .and_then(|window| window.output.clone())
        {
            outputs.insert(output_id);
        }
    }

    (false, outputs)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::RenderPlan;
    use crate::{
        model::{ManagedWindowState, OutputId, WindowId, WmState, WorkspaceId, WorkspaceState},
        transactions::{LayoutRecomputePlan, RefreshPlan},
    };

    #[test]
    fn render_plan_collects_output_from_dirty_window_scope() {
        let mut wm = WmState::default();
        wm.windows.insert(
            WindowId::from("w1"),
            ManagedWindowState::tiled(
                WindowId::from("w1"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );

        let mut render_plan = RenderPlan::default();
        render_plan.mark_from_refresh_plan(
            &RefreshPlan {
                transaction_id: Some(1),
                windows: HashSet::from([WindowId::from("w1")]),
                configure_windows: HashSet::from([WindowId::from("w1")]),
                workspaces: HashSet::new(),
                outputs: HashSet::new(),
                layout: LayoutRecomputePlan::default(),
            },
            &wm,
        );

        assert!(render_plan.should_render_output(&OutputId::from("out-1")));
    }

    #[test]
    fn render_plan_collects_output_from_workspace_scope() {
        let mut wm = WmState::default();
        wm.workspaces.insert(
            WorkspaceId::from("ws-1"),
            WorkspaceState {
                id: WorkspaceId::from("ws-1"),
                name: "ws-1".into(),
                output: Some(OutputId::from("out-2")),
                windows: vec![],
            },
        );

        let mut render_plan = RenderPlan::default();
        render_plan.mark_from_refresh_plan(
            &RefreshPlan {
                transaction_id: Some(2),
                windows: HashSet::new(),
                configure_windows: HashSet::new(),
                workspaces: HashSet::from([WorkspaceId::from("ws-1")]),
                outputs: HashSet::new(),
                layout: LayoutRecomputePlan::default(),
            },
            &wm,
        );

        assert!(render_plan.should_render_output(&OutputId::from("out-2")));
    }

    #[test]
    fn clearing_last_output_resets_dirty_state() {
        let mut render_plan = RenderPlan::default();
        render_plan.mark_output_dirty(OutputId::from("out-1"));

        render_plan.clear_output(&OutputId::from("out-1"));

        assert!(!render_plan.is_dirty());
    }

    #[test]
    fn staged_updates_do_not_render_until_promoted() {
        let mut wm = WmState::default();
        wm.windows.insert(
            WindowId::from("w1"),
            ManagedWindowState::tiled(
                WindowId::from("w1"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );

        let mut render_plan = RenderPlan::default();
        render_plan.stage_from_refresh_plan(
            &RefreshPlan {
                transaction_id: Some(3),
                windows: HashSet::from([WindowId::from("w1")]),
                configure_windows: HashSet::from([WindowId::from("w1")]),
                workspaces: HashSet::new(),
                outputs: HashSet::new(),
                layout: LayoutRecomputePlan::default(),
            },
            &wm,
        );

        assert!(render_plan.has_staged_updates());
        assert!(!render_plan.should_render_output(&OutputId::from("out-1")));

        render_plan.promote_staged();

        assert!(render_plan.should_render_output(&OutputId::from("out-1")));
    }

    #[test]
    fn presentation_only_staged_updates_are_tracked() {
        let mut wm = WmState::default();
        wm.windows.insert(
            WindowId::from("w1"),
            ManagedWindowState::tiled(
                WindowId::from("w1"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );

        let mut render_plan = RenderPlan::default();
        render_plan.stage_from_refresh_plan(
            &RefreshPlan {
                transaction_id: Some(4),
                windows: HashSet::from([WindowId::from("w1")]),
                configure_windows: HashSet::new(),
                workspaces: HashSet::new(),
                outputs: HashSet::new(),
                layout: LayoutRecomputePlan::default(),
            },
            &wm,
        );

        assert!(render_plan.staged_presentation_only());
        assert!(render_plan.has_staged_updates());
    }

    #[test]
    fn layout_or_configure_staged_updates_are_not_presentation_only() {
        let mut wm = WmState::default();
        wm.windows.insert(
            WindowId::from("w1"),
            ManagedWindowState::tiled(
                WindowId::from("w1"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );

        let mut render_plan = RenderPlan::default();
        render_plan.stage_from_refresh_plan(
            &RefreshPlan {
                transaction_id: Some(5),
                windows: HashSet::from([WindowId::from("w1")]),
                configure_windows: HashSet::from([WindowId::from("w1")]),
                workspaces: HashSet::new(),
                outputs: HashSet::new(),
                layout: LayoutRecomputePlan::default(),
            },
            &wm,
        );

        assert!(!render_plan.staged_presentation_only());
    }

    #[test]
    fn presentation_only_without_staged_outputs_does_not_skip_as_pending_redraw() {
        let mut render_plan = RenderPlan::default();
        render_plan.stage_from_refresh_plan(
            &RefreshPlan {
                transaction_id: Some(6),
                windows: HashSet::new(),
                configure_windows: HashSet::new(),
                workspaces: HashSet::new(),
                outputs: HashSet::new(),
                layout: LayoutRecomputePlan::default(),
            },
            &WmState::default(),
        );

        assert!(!render_plan.staged_presentation_only());
        assert!(!render_plan.can_skip_redraw_until_commit());
    }

    #[test]
    fn empty_refresh_plan_does_not_stage_presentation_only_work() {
        let mut render_plan = RenderPlan::default();
        render_plan.stage_from_refresh_plan(&RefreshPlan::default(), &WmState::default());

        assert!(!render_plan.staged_presentation_only());
        assert!(!render_plan.has_staged_updates());
    }

    #[test]
    fn presentation_only_staged_updates_can_skip_redraw_until_commit() {
        let mut wm = WmState::default();
        wm.windows.insert(
            WindowId::from("w1"),
            ManagedWindowState::tiled(
                WindowId::from("w1"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );

        let mut render_plan = RenderPlan::default();
        render_plan.stage_from_refresh_plan(
            &RefreshPlan {
                transaction_id: Some(7),
                windows: HashSet::from([WindowId::from("w1")]),
                configure_windows: HashSet::new(),
                workspaces: HashSet::new(),
                outputs: HashSet::new(),
                layout: LayoutRecomputePlan::default(),
            },
            &wm,
        );

        assert!(render_plan.staged_presentation_only());
        assert!(render_plan.can_skip_redraw_until_commit());

        render_plan.mark_output_dirty(OutputId::from("out-1"));

        assert!(!render_plan.can_skip_redraw_until_commit());
    }

    #[test]
    fn promote_staged_if_needed_is_noop_without_staged_updates() {
        let mut render_plan = RenderPlan::default();

        assert!(!render_plan.promote_staged_if_needed());
        assert!(!render_plan.is_dirty());
    }

    #[test]
    fn promote_staged_if_needed_promotes_real_staged_outputs() {
        let mut wm = WmState::default();
        wm.windows.insert(
            WindowId::from("w1"),
            ManagedWindowState::tiled(
                WindowId::from("w1"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );

        let mut render_plan = RenderPlan::default();
        render_plan.stage_from_refresh_plan(
            &RefreshPlan {
                transaction_id: Some(8),
                windows: HashSet::from([WindowId::from("w1")]),
                configure_windows: HashSet::new(),
                workspaces: HashSet::new(),
                outputs: HashSet::new(),
                layout: LayoutRecomputePlan::default(),
            },
            &wm,
        );

        assert!(render_plan.promote_staged_if_needed());
        assert!(render_plan.is_dirty());
        assert!(render_plan.should_render_output(&OutputId::from("out-1")));
    }

    #[test]
    fn commit_refresh_plan_marks_outputs_even_if_staged_state_was_lost() {
        let mut wm = WmState::default();
        wm.windows.insert(
            WindowId::from("w1"),
            ManagedWindowState::tiled(
                WindowId::from("w1"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );

        let refresh_plan = RefreshPlan {
            transaction_id: Some(9),
            windows: HashSet::from([WindowId::from("w1")]),
            configure_windows: HashSet::from([WindowId::from("w1")]),
            workspaces: HashSet::new(),
            outputs: HashSet::new(),
            layout: LayoutRecomputePlan::default(),
        };

        let mut render_plan = RenderPlan::default();
        render_plan.commit_refresh_plan(&refresh_plan, &wm);

        assert!(render_plan.is_dirty());
        assert!(render_plan.should_render_output(&OutputId::from("out-1")));
        assert!(!render_plan.has_staged_updates());
    }
}
