use crate::model::wm::WmModel;
use crate::model::OutputId;

pub fn sync_output(
    model: &mut WmModel,
    output_id: impl Into<OutputId>,
    name: impl Into<String>,
    logical_width: u32,
    logical_height: u32,
) -> OutputId {
    let output_id = output_id.into();
    let name = name.into();
    let focused_workspace_id = model
        .outputs
        .get(&output_id)
        .and_then(|output| output.focused_workspace_id.clone())
        .or_else(|| model.current_workspace_id.clone());

    model.upsert_output(
        output_id.clone(),
        name,
        logical_width,
        logical_height,
        focused_workspace_id.clone(),
    );

    if let Some(workspace_id) = model.current_workspace_id.clone() {
        model.attach_workspace_to_output(workspace_id, output_id.clone());
    }

    if model.current_output_id.is_none() {
        model.set_current_output(output_id.clone());
    }

    output_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::workspace;
    use crate::model::WorkspaceId;

    #[test]
    fn syncing_new_output_uses_current_workspace_and_becomes_current_output() {
        let mut model = WmModel::default();
        workspace::ensure_default_workspace(&mut model, "1");

        let output_id = sync_output(&mut model, "winit", "winit", 1280, 720);

        assert_eq!(output_id, OutputId("winit".to_string()));
        assert_eq!(model.current_output_id, Some(OutputId("winit".to_string())));
        assert_eq!(
            model.outputs.get(&output_id).and_then(|output| output.focused_workspace_id.clone()),
            Some(WorkspaceId("1".to_string()))
        );
        assert_eq!(
            model.workspaces
                .get(&WorkspaceId("1".to_string()))
                .and_then(|workspace| workspace.output_id.clone()),
            Some(OutputId("winit".to_string()))
        );
    }

    #[test]
    fn syncing_existing_output_preserves_focused_workspace() {
        let mut model = WmModel::default();
        workspace::ensure_default_workspace(&mut model, "1");
        model.upsert_output(
            OutputId("winit".to_string()),
            "winit",
            1280,
            720,
            Some(WorkspaceId("2".to_string())),
        );

        let output_id = sync_output(&mut model, "winit", "winit", 1920, 1080);

        assert_eq!(output_id, OutputId("winit".to_string()));
        assert_eq!(
            model.outputs.get(&output_id).and_then(|output| output.focused_workspace_id.clone()),
            Some(WorkspaceId("2".to_string()))
        );
        assert_eq!(
            model.outputs.get(&output_id).map(|output| output.logical_width),
            Some(1920)
        );
    }
}