use super::{OutputId, WindowId, WorkspaceId};

/// Stable window metadata owned by the compositor model.
///
/// This should evolve toward the durable state needed by config, rules, scene layout,
/// and focus/workspace policy. Smithay objects should remain outside this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowModel {
    pub id: WindowId,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub output_id: Option<OutputId>,
    pub workspace_id: Option<WorkspaceId>,
    pub mapped: bool,
    pub focused: bool,
    pub floating: bool,
    pub fullscreen: bool,
    pub closing: bool,
}

impl Default for WindowModel {
    fn default() -> Self {
        Self {
            id: WindowId(String::new()),
            app_id: None,
            title: None,
            output_id: None,
            workspace_id: None,
            mapped: false,
            focused: false,
            floating: false,
            fullscreen: false,
            closing: false,
        }
    }
}
