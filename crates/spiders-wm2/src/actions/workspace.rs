use crate::model::wm::WmModel;
use crate::model::{WindowId, WorkspaceId};

pub fn ensure_workspace(model: &mut WmModel, name: impl Into<String>) -> WorkspaceId {
    let name = name.into();
    let workspace_id = WorkspaceId(name.clone());
    model.upsert_workspace(workspace_id.clone(), name);
    workspace_id
}

pub fn ensure_default_workspace(model: &mut WmModel, name: impl Into<String>) -> WorkspaceId {
    let workspace_id = ensure_workspace(model, name);
    if model.current_workspace_id.is_none() {
        let _ = select_workspace(model, workspace_id.clone());
    }
    workspace_id
}

pub fn select_workspace(model: &mut WmModel, workspace_id: WorkspaceId) -> Option<WorkspaceId> {
    if !model.workspaces.contains_key(&workspace_id) {
        return None;
    }

    model.set_current_workspace(workspace_id.clone());
    Some(workspace_id)
}

pub fn select_next_workspace(model: &mut WmModel) -> Option<WorkspaceId> {
    let next_workspace_id = match model.current_workspace_id.as_ref() {
        Some(current_id) => model
            .workspaces
            .keys()
            .find(|workspace_id| *workspace_id > current_id)
            .cloned()
            .or_else(|| model.workspaces.keys().next().cloned()),
        None => model.workspaces.keys().next().cloned(),
    }?;

    select_workspace(model, next_workspace_id)
}

pub fn place_new_window(model: &mut WmModel, window_id: WindowId) -> WindowId {
    let workspace_id = model.current_workspace_id.clone();
    let output_id = model.current_output_id.clone();
    model.insert_window(window_id, workspace_id, output_id);
    window_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OutputId, WorkspaceId};

    #[test]
    fn ensuring_default_workspace_creates_and_selects_it() {
        let mut model = WmModel::default();

        let workspace_id = ensure_default_workspace(&mut model, "1");

        assert_eq!(workspace_id, WorkspaceId("1".to_string()));
        assert_eq!(model.current_workspace_id, Some(WorkspaceId("1".to_string())));
        assert_eq!(
            model.workspaces.get(&WorkspaceId("1".to_string())).map(|workspace| workspace.focused),
            Some(true)
        );
    }

    #[test]
    fn selecting_workspace_updates_focus_and_visibility() {
        let mut model = WmModel::default();
        ensure_workspace(&mut model, "1");
        ensure_workspace(&mut model, "2");
        ensure_default_workspace(&mut model, "1");

        let selected = select_workspace(&mut model, WorkspaceId("2".to_string()));

        assert_eq!(selected, Some(WorkspaceId("2".to_string())));
        assert_eq!(model.current_workspace_id, Some(WorkspaceId("2".to_string())));
        assert_eq!(
            model.workspaces.get(&WorkspaceId("1".to_string())).map(|workspace| workspace.focused),
            Some(false)
        );
        assert_eq!(
            model.workspaces.get(&WorkspaceId("2".to_string())).map(|workspace| workspace.visible),
            Some(true)
        );
    }

    #[test]
    fn selecting_next_workspace_advances_and_wraps() {
        let mut model = WmModel::default();
        ensure_workspace(&mut model, "1");
        ensure_workspace(&mut model, "2");
        ensure_workspace(&mut model, "3");
        ensure_default_workspace(&mut model, "2");

        let next = select_next_workspace(&mut model);
        assert_eq!(next, Some(WorkspaceId("3".to_string())));

        let wrapped = select_next_workspace(&mut model);
        assert_eq!(wrapped, Some(WorkspaceId("1".to_string())));
    }

    #[test]
    fn places_new_window_on_current_workspace_and_output() {
        let mut model = WmModel::default();
        ensure_default_workspace(&mut model, "1");
        model.current_output_id = Some(OutputId("winit".to_string()));

        let window_id = place_new_window(&mut model, WindowId(5));

        assert_eq!(window_id, WindowId(5));
        let window = model.windows.get(&WindowId(5)).expect("window missing");
        assert_eq!(window.workspace_id, Some(WorkspaceId("1".to_string())));
        assert_eq!(window.output_id, Some(OutputId("winit".to_string())));
    }

    #[test]
    fn places_new_window_even_without_current_workspace_or_output() {
        let mut model = WmModel::default();

        let window_id = place_new_window(&mut model, WindowId(6));

        assert_eq!(window_id, WindowId(6));
        let window = model.windows.get(&WindowId(6)).expect("window missing");
        assert_eq!(window.workspace_id, None);
        assert_eq!(window.output_id, None);
    }
}