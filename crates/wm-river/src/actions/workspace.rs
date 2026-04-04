use spiders_core::WorkspaceId;

use crate::model::WmState;

pub fn activate_workspace(state: &mut WmState, workspace_id: &WorkspaceId) {
    state.current_workspace_id = Some(workspace_id.clone());
    if let Some(current_output) = state.current_output_id.clone() {
        state.assign_workspace_to_output(&current_output, workspace_id);
    }
    state.focused_window_id = None;
}
