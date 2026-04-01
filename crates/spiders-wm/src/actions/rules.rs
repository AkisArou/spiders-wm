use spiders_config::model::{Config, WindowRule};
use spiders_shared::types::WindowMode;
use spiders_tree::WorkspaceId;

use crate::model::WindowState;

fn rule_matches_window(rule: &WindowRule, window: &WindowState) -> bool {
    let app_id_matches = rule
        .app_id
        .as_ref()
        .is_none_or(|app_id| window.app_id.as_ref() == Some(app_id));
    let title_matches = rule
        .title
        .as_ref()
        .is_none_or(|title| window.title.as_ref() == Some(title));

    app_id_matches && title_matches
}

pub fn configured_workspace_for_window(
    config: &Config,
    window: &WindowState,
) -> Option<WorkspaceId> {
    config
        .rules
        .iter()
        .find(|rule| rule_matches_window(rule, window) && !rule.workspaces.is_empty())
        .and_then(|rule| rule.workspaces.first())
        .map(|workspace| WorkspaceId::from(workspace.as_str()))
}

pub fn configured_mode_for_window(config: &Config, window: &WindowState) -> Option<WindowMode> {
    config
        .rules
        .iter()
        .find(|rule| rule_matches_window(rule, window))
        .and_then(|rule| {
            if rule.fullscreen == Some(true) {
                Some(WindowMode::Fullscreen)
            } else if rule.floating == Some(true) {
                Some(WindowMode::Floating { rect: None })
            } else {
                None
            }
        })
}
