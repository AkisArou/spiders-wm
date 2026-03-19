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
        self.staged_outputs = outputs;
    }

    pub fn promote_staged(&mut self) {
        if self.staged_full_scene {
            self.mark_full_scene();
        }

        self.dirty_outputs.extend(self.staged_outputs.drain());
        self.staged_full_scene = false;
    }

    pub fn has_staged_updates(&self) -> bool {
        self.staged_full_scene || !self.staged_outputs.is_empty()
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
}
