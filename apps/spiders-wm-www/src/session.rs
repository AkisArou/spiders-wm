use std::collections::{BTreeMap, BTreeSet};

use spiders_core::command::{FocusDirection, LayoutCycleDirection, WmCommand};
use spiders_core::resize::LayoutAdjustmentState;
use spiders_core::snapshot::WindowSnapshot;
use spiders_core::types::{ShellKind, WindowMode};
use spiders_core::wm::WindowGeometry;
use spiders_core::{LayoutId, OutputId, WindowId, WorkspaceId};
use spiders_wm_runtime::{
    FocusTarget, PreviewLayoutComputation, PreviewLayoutWindow,
    PreviewSessionState as RuntimePreviewSessionState, PreviewSessionWindow, WindowToggle,
    WmEnvironment, WorkspaceAssignment, WorkspaceTarget,
    apply_preview_command as apply_runtime_preview_command,
    compute_layout_preview as compute_runtime_layout_preview, display_command_label,
    execute_wm_command as execute_runtime_wm_command,
    select_preview_workspace as select_runtime_preview_workspace,
    set_preview_focused_window as set_runtime_preview_focused_window,
};
pub use spiders_wm_runtime::{PreviewDiagnostic, PreviewSnapshotNode};
use wasm_bindgen::JsValue;

const CANVAS_WIDTH: i32 = 3440;
const CANVAS_HEIGHT: i32 = 1440;
const PREVIEW_OUTPUT_ID: &str = "preview-output";
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
    pub active_layout: LayoutId,
    pub snapshot_root: Option<PreviewSnapshotNode>,
    pub diagnostics: Vec<PreviewDiagnostic>,
    pub event_log: Vec<String>,
    pub last_action: String,
    runtime_state: RuntimePreviewSessionState,
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
            active_layout: normalize_layout_id(active_layout),
            snapshot_root: None,
            diagnostics: Vec::new(),
            event_log: vec!["Alt+Return spawns foot".to_string()],
            last_action: "Alt+Return spawns foot".to_string(),
            runtime_state: RuntimePreviewSessionState {
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
        if self.active_layout == active_layout {
            return;
        }

        self.active_layout = active_layout;
        self.last_action = format!("click layout -> {}", self.active_layout.as_str());
        self.push_log(self.last_action.clone());
    }

    pub fn canvas_width(&self) -> i32 {
        CANVAS_WIDTH
    }

    pub fn canvas_height(&self) -> i32 {
        CANVAS_HEIGHT
    }

    pub fn runtime_state(&self) -> &RuntimePreviewSessionState {
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

    pub fn apply_command(&mut self, command: WmCommand) {
        let label = display_command_label(&command);
        match command {
            WmCommand::CycleLayout { direction } => {
                self.active_layout = match direction.unwrap_or(LayoutCycleDirection::Next) {
                    LayoutCycleDirection::Next => next_layout_id(&self.active_layout),
                    LayoutCycleDirection::Previous => previous_layout_id(&self.active_layout),
                };
                self.last_action = format!("{} -> {}", label, self.active_layout.as_str());
                self.push_log(self.last_action.clone());
            }
            WmCommand::SetLayout { name } => {
                if PREVIEW_LAYOUT_IDS.contains(&name.as_str()) {
                    let layout = LayoutId::from(name.as_str());
                    self.active_layout = layout;
                    self.last_action = format!("set layout -> {}", self.active_layout.as_str());
                    self.push_log(self.last_action.clone());
                }
            }
            command => {
                self.last_action = label;
                execute_runtime_wm_command(self, command);
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

    pub fn apply_layout_renderable(&mut self, layout_renderable: JsValue) {
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
        let render_windows = layout_windows.iter().map(preview_layout_window).collect::<Vec<_>>();

        let result = compute_runtime_layout_preview(
            layout_renderable,
            &render_windows,
            self.stylesheets_by_layout.get(&self.active_layout).map(String::as_str).unwrap_or(""),
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

    fn apply_runtime_state(&mut self, next_state: RuntimePreviewSessionState) {
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

    fn runtime_state_window(&self, window_id: &WindowId) -> Option<&PreviewSessionWindow> {
        self.runtime_state.windows.iter().find(|window| window.id.as_str() == window_id.as_str())
    }
}

impl WmEnvironment for PreviewSessionState {
    fn spawn_command(&mut self, command: &str) {
        self.apply_host_runtime_command(WmCommand::Spawn { command: command.to_string() });
    }

    fn request_quit(&mut self) {
        self.apply_host_runtime_command(WmCommand::Quit);
    }

    fn activate_workspace(&mut self, target: WorkspaceTarget) {
        match target {
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
        }
    }

    fn assign_focused_window_to_workspace(&mut self, assignment: WorkspaceAssignment) {
        match assignment {
            WorkspaceAssignment::Move(workspace) => {
                self.apply_host_runtime_command(WmCommand::AssignFocusedWindowToWorkspace {
                    workspace,
                });
            }
            WorkspaceAssignment::Toggle(workspace) => {
                self.apply_host_runtime_command(WmCommand::ToggleAssignFocusedWindowToWorkspace {
                    workspace,
                });
            }
        }
    }

    fn spawn_terminal(&mut self) {
        self.apply_host_runtime_command(WmCommand::SpawnTerminal);
    }

    fn focus_window(&mut self, target: FocusTarget) {
        match target {
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
        }
    }

    fn close_focused_window(&mut self) {
        self.apply_host_runtime_command(WmCommand::CloseFocusedWindow);
    }

    fn reload_config(&mut self) {
        self.apply_host_runtime_command(WmCommand::ReloadConfig);
    }

    fn toggle_focused_window(&mut self, toggle: WindowToggle) {
        match toggle {
            WindowToggle::Floating => self.apply_host_runtime_command(WmCommand::ToggleFloating),
            WindowToggle::Fullscreen => {
                self.apply_host_runtime_command(WmCommand::ToggleFullscreen)
            }
        }
    }

    fn swap_focused_window(&mut self, direction: FocusDirection) {
        self.apply_host_runtime_command(WmCommand::SwapDirection { direction });
    }
}

fn preview_layout_window(value: &PreviewSessionWindow) -> PreviewLayoutWindow {
    PreviewLayoutWindow {
        id: value.id.clone(),
        app_id: value.app_id.clone(),
        title: value.title.clone(),
        class: value.class.clone(),
        instance: value.instance.clone(),
        role: value.role.clone(),
        shell: value.shell.clone(),
        window_type: value.window_type.clone(),
        floating: value.floating,
        fullscreen: value.fullscreen,
        focused: value.focused,
    }
}

fn initial_windows() -> Vec<PreviewSessionWindow> {
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
) -> PreviewSessionWindow {
    PreviewSessionWindow {
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

fn collect_snapshot_geometries(
    node: &PreviewSnapshotNode,
    out: &mut BTreeMap<WindowId, WindowGeometry>,
) {
    if node.node_type == "window" {
        if let (Some(window_id), Some(rect)) = (node.window_id.as_ref(), node.rect) {
            out.insert(
                window_id.clone(),
                WindowGeometry {
                    x: rect.x.round() as i32,
                    y: rect.y.round() as i32,
                    width: rect.width.round() as i32,
                    height: rect.height.round() as i32,
                },
            );
        }
    }

    for child in &node.children {
        collect_snapshot_geometries(child, out);
    }
}

fn empty_window_geometry() -> WindowGeometry {
    WindowGeometry { x: 0, y: 0, width: 0, height: 0 }
}

fn runtime_window_snapshot(window: &PreviewSessionWindow) -> WindowSnapshot {
    let mode = if window.fullscreen {
        WindowMode::Fullscreen
    } else if window.floating {
        WindowMode::Floating { rect: None }
    } else {
        WindowMode::Tiled
    };

    WindowSnapshot {
        id: WindowId::from(window.id.as_str()),
        shell: match window.shell.as_deref() {
            Some("x11") => ShellKind::X11,
            Some("xdg-toplevel") | Some("xdg_toplevel") => ShellKind::XdgToplevel,
            _ => ShellKind::Unknown,
        },
        app_id: window.app_id.clone(),
        title: window.title.clone(),
        class: window.class.clone(),
        instance: window.instance.clone(),
        role: window.role.clone(),
        window_type: window.window_type.clone(),
        mapped: true,
        mode,
        focused: window.focused,
        urgent: false,
        closing: false,
        output_id: Some(OutputId::from(PREVIEW_OUTPUT_ID)),
        workspace_id: Some(WorkspaceId::from(window.workspace_name.as_str())),
        workspaces: vec![window.workspace_name.clone()],
    }
}
