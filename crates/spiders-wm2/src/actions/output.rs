use crate::model::{OutputId, OutputNode, TopologyState, WmState};

pub fn register_output(
    topology: &mut TopologyState,
    wm: &mut WmState,
    output_id: OutputId,
    name: String,
) {
    topology.outputs.insert(
        output_id.clone(),
        OutputNode {
            id: output_id.clone(),
            name,
            enabled: true,
            current_workspace: Some(wm.active_workspace.clone()),
            logical_size: (1280, 720),
        },
    );

    if wm.focused_output.is_none() {
        wm.focused_output = Some(output_id.clone());
    }

    if let Some(active_workspace) = wm.workspaces.get_mut(&wm.active_workspace) {
        if active_workspace.output.is_none() {
            active_workspace.output = Some(output_id);
        }
    }
}
