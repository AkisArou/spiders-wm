use std::collections::HashSet;

use crate::{
    model::{OutputId, WmState},
    transactions::RefreshPlan,
};

#[derive(Debug, Default)]
pub struct RenderPlan {
    full_scene: bool,
    dirty_outputs: HashSet<OutputId>,
}

impl RenderPlan {
    pub fn mark_from_refresh_plan(&mut self, refresh_plan: &RefreshPlan, wm: &WmState) {
        if refresh_plan.layout.full_scene {
            self.mark_full_scene();
        }

        self.dirty_outputs
            .extend(refresh_plan.outputs.iter().cloned());

        for workspace_id in &refresh_plan.workspaces {
            if let Some(output_id) = wm
                .workspaces
                .get(workspace_id)
                .and_then(|workspace| workspace.output.clone())
            {
                self.dirty_outputs.insert(output_id);
            }
        }

        for window_id in &refresh_plan.windows {
            if let Some(output_id) = wm
                .windows
                .get(window_id)
                .and_then(|window| window.output.clone())
            {
                self.dirty_outputs.insert(output_id);
            }
        }
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
}
