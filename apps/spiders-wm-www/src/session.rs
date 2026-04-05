use std::collections::{BTreeMap, BTreeSet};

use spiders_config::model::Config;
use spiders_core::command::{LayoutCycleDirection, WmCommand};
use spiders_core::effect::{
    FocusTarget, WindowToggle, WmHostEffect, WorkspaceAssignment, WorkspaceTarget,
};
use spiders_core::resize::LayoutAdjustmentState;
use spiders_core::snapshot::WindowSnapshot;
use spiders_core::wm::WindowGeometry;
use spiders_core::{LayoutId, WindowId};
use spiders_scene::ComputedStyle;
pub use spiders_wm_runtime::{PreviewDiagnostic, PreviewSnapshotNode};
use spiders_wm_runtime::{
    PreviewLayoutComputation, PreviewSession as RuntimePreviewSession, PreviewWindow, WmHost,
    apply_preview_command as apply_runtime_preview_command, collect_snapshot_geometries,
    compute_layout_preview_from_source_layout as compute_runtime_layout_preview,
    dispatch_wm_command as dispatch_runtime_wm_command, display_command_label,
    empty_window_geometry, preview_window_snapshot,
    select_preview_workspace as select_runtime_preview_workspace,
    set_preview_focused_window as set_runtime_preview_focused_window,
};

const CANVAS_WIDTH: i32 = 3440;
const CANVAS_HEIGHT: i32 = 1440;
const MASTER_STACK_LAYOUT_ID: &str = "master-stack";
const FOCUS_REPRO_LAYOUT_ID: &str = "focus-repro";
const PREVIEW_LAYOUT_IDS: [&str; 2] = [MASTER_STACK_LAYOUT_ID, FOCUS_REPRO_LAYOUT_ID];

pub fn preview_layout_ids() -> impl Iterator<Item = LayoutId> {
    PREVIEW_LAYOUT_IDS.into_iter().map(LayoutId::from)
}

fn normalize_layout_id(layout_id: LayoutId) -> LayoutId {
    if PREVIEW_LAYOUT_IDS.contains(&layout_id.as_str()) {
        layout_id
    } else {
        LayoutId::from(MASTER_STACK_LAYOUT_ID)
    }
}

fn next_layout_id(current: &LayoutId) -> LayoutId {
    let index =
        PREVIEW_LAYOUT_IDS.iter().position(|layout_id| *layout_id == current.as_str()).unwrap_or(0);
    LayoutId::from(PREVIEW_LAYOUT_IDS[(index + 1) % PREVIEW_LAYOUT_IDS.len()])
}

fn previous_layout_id(current: &LayoutId) -> LayoutId {
    let index =
        PREVIEW_LAYOUT_IDS.iter().position(|layout_id| *layout_id == current.as_str()).unwrap_or(0);
    LayoutId::from(
        PREVIEW_LAYOUT_IDS[(index + PREVIEW_LAYOUT_IDS.len() - 1) % PREVIEW_LAYOUT_IDS.len()],
    )
}

fn normalize_workspace_names(workspace_names: Vec<String>) -> Vec<String> {
    if workspace_names.is_empty() {
        vec!["1".to_string(), "2".to_string(), "3".to_string()]
    } else {
        workspace_names
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewSessionState {
    pub snapshot_root: Option<PreviewSnapshotNode>,
    pub diagnostics: Vec<PreviewDiagnostic>,
    pub event_log: Vec<String>,
    pub last_action: String,
    runtime_state: RuntimePreviewSession,
    window_geometries: BTreeMap<WindowId, WindowGeometry>,
    unclaimed_window_ids: BTreeSet<WindowId>,
    stylesheets_by_layout: BTreeMap<LayoutId, String>,
}

impl PreviewSessionState {
    pub fn new(
        active_layout: LayoutId,
        workspace_names: Vec<String>,
        stylesheets_by_layout: BTreeMap<LayoutId, String>,
    ) -> Self {
        let workspace_names = normalize_workspace_names(workspace_names);
        let active_workspace_name =
            workspace_names.first().cloned().unwrap_or_else(|| "1".to_string());
        let mut state = Self {
            snapshot_root: None,
            diagnostics: Vec::new(),
            event_log: vec!["Alt+Return spawns foot".to_string()],
            last_action: "Alt+Return spawns foot".to_string(),
            runtime_state: RuntimePreviewSession {
                active_layout: normalize_layout_id(active_layout),
                active_workspace_name,
                workspace_names,
                windows: initial_windows(),
                remembered_focus_by_scope: BTreeMap::new(),
                layout_adjustments: LayoutAdjustmentState::default(),
            },
            window_geometries: BTreeMap::new(),
            unclaimed_window_ids: BTreeSet::new(),
            stylesheets_by_layout,
        };

        state.apply_runtime_state(set_runtime_preview_focused_window(
            state.runtime_state.clone(),
            Some(WindowId::from("win-1")),
            None,
        ));
        state
    }

    pub fn sync_inputs(
        &mut self,
        workspace_names: Vec<String>,
        stylesheets_by_layout: BTreeMap<LayoutId, String>,
    ) {
        let workspace_names = normalize_workspace_names(workspace_names);
        if self.runtime_state.workspace_names == workspace_names
            && self.stylesheets_by_layout == stylesheets_by_layout
        {
            return;
        }

        self.stylesheets_by_layout = stylesheets_by_layout;
        self.runtime_state.workspace_names = workspace_names;
        if !self
            .runtime_state
            .workspace_names
            .iter()
            .any(|name| name == &self.runtime_state.active_workspace_name)
        {
            self.runtime_state.active_workspace_name = self
                .runtime_state
                .workspace_names
                .first()
                .cloned()
                .unwrap_or_else(|| "1".to_string());
        }
    }

    pub fn switch_layout(&mut self, active_layout: LayoutId) {
        let active_layout = normalize_layout_id(active_layout);
        if self.runtime_state.active_layout == active_layout {
            return;
        }

        self.runtime_state.active_layout = active_layout;
        self.last_action = format!("click layout -> {}", self.runtime_state.active_layout.as_str());
        self.push_log(self.last_action.clone());
    }

    pub fn active_layout(&self) -> &LayoutId {
        &self.runtime_state.active_layout
    }

    pub fn canvas_width(&self) -> i32 {
        CANVAS_WIDTH
    }

    pub fn canvas_height(&self) -> i32 {
        CANVAS_HEIGHT
    }

    pub fn runtime_state(&self) -> &RuntimePreviewSession {
        &self.runtime_state
    }

    pub fn active_workspace_name(&self) -> &str {
        &self.runtime_state.active_workspace_name
    }

    pub fn workspace_names(&self) -> &[String] {
        &self.runtime_state.workspace_names
    }

    pub fn focused_window_id(&self) -> Option<WindowId> {
        self.runtime_state
            .windows
            .iter()
            .find(|window| window.focused)
            .map(|window| WindowId::from(window.id.as_str()))
    }

    pub fn visible_windows(&self) -> Vec<WindowSnapshot> {
        self.runtime_state
            .windows
            .iter()
            .filter(|window| window.workspace_name == self.runtime_state.active_workspace_name)
            .map(runtime_window_snapshot)
            .collect()
    }

    pub fn claimed_visible_windows(&self) -> Vec<WindowSnapshot> {
        self.visible_windows()
            .into_iter()
            .filter(|window| {
                let geometry = self.window_geometry(&window.id);
                geometry.width > 0 && geometry.height > 0
            })
            .collect()
    }

    pub fn unclaimed_visible_windows(&self) -> Vec<WindowSnapshot> {
        self.visible_windows()
            .into_iter()
            .filter(|window| self.unclaimed_window_ids.contains(&window.id))
            .collect()
    }

    pub fn visible_window_count(&self) -> usize {
        self.visible_windows().len()
    }

    pub fn window_geometry(&self, window_id: &WindowId) -> WindowGeometry {
        self.window_geometries.get(window_id).copied().unwrap_or_else(empty_window_geometry)
    }

    pub fn window_name(&self, window_id: &WindowId) -> String {
        self.runtime_state_window(window_id)
            .map(runtime_window_snapshot)
            .map(|window| {
                let title = window.title.as_deref().unwrap_or(window.id.as_str());
                window
                    .app_id
                    .as_deref()
                    .map(|app_id| format!("{app_id} ({title})"))
                    .unwrap_or_else(|| title.to_string())
            })
            .unwrap_or_else(|| window_id.as_str().to_string())
    }

    pub fn snapshot_node_for_window(&self, window_id: &WindowId) -> Option<&PreviewSnapshotNode> {
        fn find<'a>(
            node: &'a PreviewSnapshotNode,
            window_id: &WindowId,
        ) -> Option<&'a PreviewSnapshotNode> {
            if node.window_id.as_ref() == Some(window_id) {
                return Some(node);
            }

            node.children.iter().find_map(|child| find(child, window_id))
        }

        self.snapshot_root.as_ref().and_then(|root| find(root, window_id))
    }

    pub fn window_titlebar_styles(
        &self,
        window_id: &WindowId,
    ) -> (Option<ComputedStyle>, Option<ComputedStyle>) {
        self.snapshot_node_for_window(window_id)
            .map(|node| (node.layout_style.clone(), node.titlebar_style.clone()))
            .unwrap_or((None, None))
    }

    pub fn apply_command(&mut self, command: WmCommand) {
        let label = display_command_label(&command);
        match command {
            WmCommand::CycleLayout { direction } => {
                self.runtime_state.active_layout = match direction
                    .unwrap_or(LayoutCycleDirection::Next)
                {
                    LayoutCycleDirection::Next => next_layout_id(&self.runtime_state.active_layout),
                    LayoutCycleDirection::Previous => {
                        previous_layout_id(&self.runtime_state.active_layout)
                    }
                };
                self.last_action =
                    format!("{} -> {}", label, self.runtime_state.active_layout.as_str());
                self.push_log(self.last_action.clone());
            }
            WmCommand::SetLayout { name } => {
                if PREVIEW_LAYOUT_IDS.contains(&name.as_str()) {
                    let layout = LayoutId::from(name.as_str());
                    self.runtime_state.active_layout = layout;
                    self.last_action =
                        format!("set layout -> {}", self.runtime_state.active_layout.as_str());
                    self.push_log(self.last_action.clone());
                }
            }
            command => {
                self.last_action = label;
                dispatch_runtime_wm_command(self, command);
                if self.last_action == "quit" {
                    self.push_log("quit ignored in preview".to_string());
                } else {
                    self.push_log(format!(
                        "{} -> {} on workspace {}",
                        self.last_action,
                        self.focused_window_id()
                            .as_ref()
                            .map(|window_id| self.window_name(window_id))
                            .unwrap_or_else(|| "none".to_string()),
                        self.runtime_state.active_workspace_name,
                    ));
                }
            }
        }
    }

    pub fn select_workspace(&mut self, workspace_name: String) {
        if !self.runtime_state.workspace_names.contains(&workspace_name) {
            return;
        }

        let next_state = select_runtime_preview_workspace(
            self.runtime_state.clone(),
            &workspace_name,
            self.snapshot_root.as_ref(),
        );
        if next_state == self.runtime_state {
            return;
        }

        self.apply_runtime_state(next_state);
        self.last_action =
            format!("view workspace -> {}", self.runtime_state.active_workspace_name);
        let target = self
            .focused_window_id()
            .as_ref()
            .map(|window_id| self.window_name(window_id))
            .unwrap_or_else(|| "none".to_string());
        self.push_log(format!(
            "workspace {} -> focus {target}",
            self.runtime_state.active_workspace_name
        ));
    }

    pub fn set_focus(&mut self, window_id: WindowId) {
        if self.focused_window_id().as_ref() == Some(&window_id) {
            return;
        }

        let from = self
            .focused_window_id()
            .as_ref()
            .map(|focused_id| self.window_name(focused_id))
            .unwrap_or_else(|| "none".to_string());
        let to = self.window_name(&window_id);
        let next_state = set_runtime_preview_focused_window(
            self.runtime_state.clone(),
            Some(window_id),
            self.snapshot_root.as_ref(),
        );
        self.apply_runtime_state(next_state);
        self.last_action = format!("click focus -> {to}");
        self.push_log(format!("Selected {to} from {from}"));
    }

    pub fn apply_layout_source(
        &mut self,
        layout: spiders_core::SourceLayoutNode,
        config: Option<&Config>,
    ) {
        let layout_windows = self
            .runtime_state
            .windows
            .iter()
            .filter(|window| {
                window.workspace_name == self.runtime_state.active_workspace_name
                    && !window.floating
            })
            .cloned()
            .collect::<Vec<_>>();

        let result = compute_runtime_layout_preview(
            &layout,
            &layout_windows,
            config,
            Some(&self.runtime_state.active_workspace_name),
            self.stylesheets_by_layout
                .get(&self.runtime_state.active_layout)
                .map(String::as_str)
                .unwrap_or(""),
            CANVAS_WIDTH as f32,
            CANVAS_HEIGHT as f32,
        );

        self.apply_preview_computation(result);
    }

    pub fn apply_preview_failure(&mut self, source: &'static str, message: String) {
        self.snapshot_root = None;
        self.diagnostics = vec![PreviewDiagnostic {
            source: source.to_string(),
            level: "error".to_string(),
            message,
        }];
        self.unclaimed_window_ids = self
            .runtime_state
            .windows
            .iter()
            .filter(|window| {
                window.workspace_name == self.runtime_state.active_workspace_name
                    && !window.floating
            })
            .map(|window| WindowId::from(window.id.as_str()))
            .collect();
        self.sync_window_geometries_from_snapshot();
    }

    fn apply_preview_computation(&mut self, computation: PreviewLayoutComputation) {
        let unclaimed_ids = computation
            .unclaimed_window_ids
            .into_iter()
            .map(|window_id| WindowId::from(window_id.as_str()))
            .collect::<BTreeSet<_>>();

        self.snapshot_root = computation.snapshot_root;
        self.diagnostics = computation.diagnostics;
        self.unclaimed_window_ids = unclaimed_ids;
        self.sync_window_geometries_from_snapshot();
    }

    fn sync_window_geometries_from_snapshot(&mut self) {
        let mut geometries = BTreeMap::new();
        if let Some(snapshot_root) = self.snapshot_root.as_ref() {
            collect_snapshot_geometries(snapshot_root, &mut geometries);
        }

        for window in &self.runtime_state.windows {
            let window_id = WindowId::from(window.id.as_str());
            if window.workspace_name != self.runtime_state.active_workspace_name {
                continue;
            }

            self.window_geometries.insert(
                window_id.clone(),
                geometries.get(&window_id).copied().unwrap_or_else(empty_window_geometry),
            );
        }
    }

    fn apply_runtime_state(&mut self, next_state: RuntimePreviewSession) {
        let current_window_ids = next_state
            .windows
            .iter()
            .map(|window| WindowId::from(window.id.as_str()))
            .collect::<BTreeSet<_>>();

        self.runtime_state = next_state;
        self.window_geometries.retain(|window_id, _| current_window_ids.contains(window_id));
        self.unclaimed_window_ids = self
            .unclaimed_window_ids
            .iter()
            .filter(|window_id| current_window_ids.contains(*window_id))
            .cloned()
            .collect();
    }

    fn apply_host_runtime_command(&mut self, command: WmCommand) {
        let next_state = apply_runtime_preview_command(
            self.runtime_state.clone(),
            command,
            self.snapshot_root.as_ref(),
        );
        self.apply_runtime_state(next_state);
    }

    fn push_log(&mut self, entry: String) {
        self.event_log.insert(0, entry);
        self.event_log.truncate(10);
    }

    fn runtime_state_window(&self, window_id: &WindowId) -> Option<&PreviewWindow> {
        self.runtime_state.windows.iter().find(|window| window.id.as_str() == window_id.as_str())
    }
}

impl WmHost for PreviewSessionState {
    fn on_effect(&mut self, effect: WmHostEffect) {
        match effect {
            WmHostEffect::SpawnCommand { command } => {
                self.apply_host_runtime_command(WmCommand::Spawn { command });
            }
            WmHostEffect::RequestQuit => self.apply_host_runtime_command(WmCommand::Quit),
            WmHostEffect::ActivateWorkspace { target } => match target {
                WorkspaceTarget::Named(name) => {
                    let next_state = select_runtime_preview_workspace(
                        self.runtime_state.clone(),
                        &name,
                        self.snapshot_root.as_ref(),
                    );
                    self.apply_runtime_state(next_state);
                }
                WorkspaceTarget::Next => {
                    self.apply_host_runtime_command(WmCommand::SelectNextWorkspace);
                }
                WorkspaceTarget::Previous => {
                    self.apply_host_runtime_command(WmCommand::SelectPreviousWorkspace);
                }
            },
            WmHostEffect::AssignFocusedWindowToWorkspace { assignment } => match assignment {
                WorkspaceAssignment::Move(workspace) => {
                    self.apply_host_runtime_command(WmCommand::AssignFocusedWindowToWorkspace {
                        workspace,
                    });
                }
                WorkspaceAssignment::Toggle(workspace) => {
                    self.apply_host_runtime_command(
                        WmCommand::ToggleAssignFocusedWindowToWorkspace { workspace },
                    );
                }
            },
            WmHostEffect::SpawnTerminal => {
                self.apply_host_runtime_command(WmCommand::SpawnTerminal)
            }
            WmHostEffect::FocusWindow { target } => match target {
                FocusTarget::Next => self.apply_host_runtime_command(WmCommand::FocusNextWindow),
                FocusTarget::Previous => {
                    self.apply_host_runtime_command(WmCommand::FocusPreviousWindow)
                }
                FocusTarget::Direction(direction) => {
                    self.apply_host_runtime_command(WmCommand::FocusDirection { direction })
                }
                FocusTarget::Window(window_id) => {
                    let next_state = set_runtime_preview_focused_window(
                        self.runtime_state.clone(),
                        Some(window_id),
                        self.snapshot_root.as_ref(),
                    );
                    self.apply_runtime_state(next_state);
                }
            },
            WmHostEffect::CloseFocusedWindow => {
                self.apply_host_runtime_command(WmCommand::CloseFocusedWindow);
            }
            WmHostEffect::ReloadConfig => self.apply_host_runtime_command(WmCommand::ReloadConfig),
            WmHostEffect::ToggleFocusedWindow { toggle } => match toggle {
                WindowToggle::Floating => {
                    self.apply_host_runtime_command(WmCommand::ToggleFloating)
                }
                WindowToggle::Fullscreen => {
                    self.apply_host_runtime_command(WmCommand::ToggleFullscreen)
                }
            },
            WmHostEffect::SwapFocusedWindow { direction } => {
                self.apply_host_runtime_command(WmCommand::SwapDirection { direction });
            }
            WmHostEffect::SetLayout { name } => {
                if PREVIEW_LAYOUT_IDS.contains(&name.as_str()) {
                    let layout = LayoutId::from(name.as_str());
                    self.runtime_state.active_layout = layout;
                }
            }
            WmHostEffect::CycleLayout { direction } => {
                self.runtime_state.active_layout = match direction
                    .unwrap_or(LayoutCycleDirection::Next)
                {
                    LayoutCycleDirection::Next => next_layout_id(&self.runtime_state.active_layout),
                    LayoutCycleDirection::Previous => {
                        previous_layout_id(&self.runtime_state.active_layout)
                    }
                };
            }
        }
    }
}

fn initial_windows() -> Vec<PreviewWindow> {
    vec![
        playground_window("win-1", "foot", "Terminal 1", "foot", "foot", "1"),
        playground_window("win-2", "zen", "Spec Draft", "zen-browser", "zen", "1"),
        playground_window("win-3", "slack", "Engineering", "Slack", "slack", "1"),
        playground_window("win-4", "foot", "Terminal 2", "foot", "foot", "2"),
        playground_window("win-5", "zen", "Reference", "zen-browser", "zen", "2"),
        playground_window("win-6", "foot", "Terminal 3", "foot", "foot", "3"),
    ]
}

fn playground_window(
    id: &str,
    app_id: &str,
    title: &str,
    class: &str,
    instance: &str,
    workspace_name: &str,
) -> PreviewWindow {
    PreviewWindow {
        id: id.to_string(),
        app_id: Some(app_id.to_string()),
        title: Some(title.to_string()),
        class: Some(class.to_string()),
        instance: Some(instance.to_string()),
        role: None,
        shell: Some("xdg_toplevel".to_string()),
        window_type: None,
        floating: false,
        fullscreen: false,
        focused: false,
        workspace_name: workspace_name.to_string(),
    }
}

fn runtime_window_snapshot(window: &PreviewWindow) -> WindowSnapshot {
    preview_window_snapshot(window, Some(window.workspace_name.as_str()))
}
