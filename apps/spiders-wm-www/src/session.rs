use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use spiders_core::focus::{FocusTree, FocusTreeWindowGeometry};
use spiders_core::navigation::NavigationDirection;
use spiders_core::wm::WindowGeometry;
use spiders_core::{LayoutRect, WindowId};
use spiders_web_bindings::{
    apply_preview_command, apply_preview_snapshot_overrides, compute_layout_preview,
};
use wasm_bindgen::JsValue;

const CANVAS_WIDTH: i32 = 3440;
const CANVAS_HEIGHT: i32 = 1440;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreviewLayoutId {
    MasterStack,
    FocusRepro,
}

impl PreviewLayoutId {
    pub const ALL: [Self; 2] = [Self::MasterStack, Self::FocusRepro];

    pub fn title(self) -> &'static str {
        match self {
            Self::MasterStack => "master-stack",
            Self::FocusRepro => "focus-repro",
        }
    }

    pub fn display_title(self) -> &'static str {
        self.title()
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::MasterStack => {
                "Matches the playground master-stack preview: a single master slot with a stacked remainder column."
            }
            Self::FocusRepro => {
                "Matches the playground focus-repro preview: main pane, top pane, then a split bottom row as windows increase."
            }
        }
    }

    pub fn prompt(self) -> &'static str {
        match self {
            Self::MasterStack => {
                "This preview follows the runtime playground flow: shared bindings session state, authored CSS, and a live layout selection."
            }
            Self::FocusRepro => {
                "This preview mirrors the playground focus repro workspace. Use the keyboard bindings or click panes to drive the same preview session state."
            }
        }
    }

    fn next(self) -> Self {
        match self {
            Self::MasterStack => Self::FocusRepro,
            Self::FocusRepro => Self::MasterStack,
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "master-stack" => Some(Self::MasterStack),
            "focus-repro" => Some(Self::FocusRepro),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewEnvironment {
    pub workspace_names: Vec<String>,
    pub stylesheets: BTreeMap<PreviewLayoutId, String>,
}

impl PreviewEnvironment {
    pub fn stylesheet(&self, layout: PreviewLayoutId) -> &str {
        self.stylesheets
            .get(&layout)
            .map(String::as_str)
            .unwrap_or("")
    }

    fn normalized_workspace_names(&self) -> Vec<String> {
        if self.workspace_names.is_empty() {
            vec!["1".to_string(), "2".to_string(), "3".to_string()]
        } else {
            self.workspace_names.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewSessionCommand {
    pub name: String,
    #[serde(default)]
    pub arg: Option<PreviewSessionCommandArg>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PreviewSessionCommandArg {
    String(String),
    Number(i32),
}

impl PreviewSessionCommand {
    pub fn focus_dir(direction: &str) -> Self {
        Self {
            name: "focus_dir".to_string(),
            arg: Some(PreviewSessionCommandArg::String(direction.to_string())),
        }
    }

    pub fn display_label(&self) -> String {
        match self.arg.as_ref() {
            Some(PreviewSessionCommandArg::String(value)) => {
                format!("{}({value})", self.name)
            }
            Some(PreviewSessionCommandArg::Number(value)) => {
                format!("{}({value})", self.name)
            }
            None => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PreviewSnapshotClasses {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSnapshotNode {
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "class", alias = "className")]
    pub class_name: Option<PreviewSnapshotClasses>,
    #[serde(default)]
    pub rect: Option<LayoutRect>,
    #[serde(default, rename = "window_id", alias = "windowId")]
    pub window_id: Option<WindowId>,
    #[serde(default)]
    pub axis: Option<String>,
    #[serde(default)]
    pub reverse: bool,
    #[serde(default)]
    pub children: Vec<PreviewSnapshotNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewDiagnostic {
    pub source: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewSessionWindow {
    pub id: WindowId,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub class: Option<String>,
    pub instance: Option<String>,
    pub role: Option<String>,
    pub shell: Option<String>,
    pub window_type: Option<String>,
    pub floating: bool,
    pub fullscreen: bool,
    pub focused: bool,
    pub workspace_name: String,
    pub badge: String,
    pub subtitle: String,
    pub accent: String,
    pub geometry: WindowGeometry,
}

impl PreviewSessionWindow {
    fn playground(
        id: &str,
        app_id: &str,
        title: &str,
        class: &str,
        instance: &str,
        workspace_name: &str,
    ) -> Self {
        let visuals = default_window_visuals(id, Some(app_id), Some(title), id.len());

        Self {
            id: WindowId::from(id),
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
            badge: visuals.badge,
            subtitle: visuals.subtitle,
            accent: visuals.accent,
            geometry: WindowGeometry {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
        }
    }

    fn from_bindings(window: BindingsSessionWindow, index: usize) -> Self {
        let visuals = default_window_visuals(
            window.id.as_str(),
            window.app_id.as_deref(),
            window.title.as_deref(),
            index,
        );

        Self {
            id: WindowId::from(window.id.as_str()),
            app_id: window.app_id,
            title: window.title,
            class: window.class,
            instance: window.instance,
            role: window.role,
            shell: window.shell,
            window_type: window.window_type,
            floating: window.floating,
            fullscreen: window.fullscreen,
            focused: window.focused,
            workspace_name: window.workspace_name,
            badge: visuals.badge,
            subtitle: visuals.subtitle,
            accent: visuals.accent,
            geometry: WindowGeometry {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
        }
    }

    pub fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or_else(|| self.id.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewSessionState {
    pub active_layout: PreviewLayoutId,
    pub active_workspace_name: String,
    pub workspace_names: Vec<String>,
    pub windows: Vec<PreviewSessionWindow>,
    pub remembered_focus_by_scope: BTreeMap<String, WindowId>,
    pub master_ratio_by_workspace: BTreeMap<String, f32>,
    pub stack_weights_by_workspace: BTreeMap<String, BTreeMap<String, f32>>,
    pub snapshot_root: Option<PreviewSnapshotNode>,
    pub diagnostics: Vec<PreviewDiagnostic>,
    pub unclaimed_windows: Vec<PreviewSessionWindow>,
    pub event_log: Vec<String>,
    pub last_action: String,
    environment: PreviewEnvironment,
}

impl PreviewSessionState {
    pub fn new(active_layout: PreviewLayoutId, environment: PreviewEnvironment) -> Self {
        let workspace_names = environment.normalized_workspace_names();
        let active_workspace_name = workspace_names
            .first()
            .cloned()
            .unwrap_or_else(|| "1".to_string());
        let mut state = Self {
            active_layout,
            active_workspace_name,
            workspace_names,
            windows: initial_windows(),
            remembered_focus_by_scope: BTreeMap::new(),
            master_ratio_by_workspace: BTreeMap::new(),
            stack_weights_by_workspace: BTreeMap::new(),
            snapshot_root: None,
            diagnostics: Vec::new(),
            unclaimed_windows: Vec::new(),
            event_log: vec!["Alt+Return spawns foot".to_string()],
            last_action: "Alt+Return spawns foot".to_string(),
            environment,
        };

        state.set_focus_internal(Some(WindowId::from("win-1")));
        state
    }

    pub fn sync_environment(&mut self, environment: PreviewEnvironment) {
        if self.environment == environment {
            return;
        }

        self.environment = environment;
        self.workspace_names = self.environment.normalized_workspace_names();
        if !self
            .workspace_names
            .iter()
            .any(|name| name == &self.active_workspace_name)
        {
            self.active_workspace_name = self
                .workspace_names
                .first()
                .cloned()
                .unwrap_or_else(|| "1".to_string());
        }

    }

    pub fn switch_layout(&mut self, active_layout: PreviewLayoutId) {
        if self.active_layout == active_layout {
            return;
        }

        self.active_layout = active_layout;
        self.last_action = format!("click layout -> {}", active_layout.display_title());
        self.push_log(self.last_action.clone());
    }

    pub fn reset(&mut self) {
        let environment = self.environment.clone();
        *self = Self::new(self.active_layout, environment);
    }

    pub fn canvas_width(&self) -> i32 {
        CANVAS_WIDTH
    }

    pub fn canvas_height(&self) -> i32 {
        CANVAS_HEIGHT
    }

    pub fn prompt(&self) -> &'static str {
        self.active_layout.prompt()
    }

    pub fn display_title(&self) -> &'static str {
        self.active_layout.display_title()
    }

    pub fn summary(&self) -> &'static str {
        self.active_layout.summary()
    }

    pub fn selected_layout_name(&self) -> String {
        self.active_layout.title().to_string()
    }

    pub fn layout_name_for_workspace(&self, _workspace_name: &str) -> String {
        self.selected_layout_name()
    }

    pub fn focused_window_id(&self) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|window| window.focused)
            .map(|window| window.id.clone())
    }

    pub fn visible_windows(&self) -> Vec<PreviewSessionWindow> {
        self.windows
            .iter()
            .filter(|window| window.workspace_name == self.active_workspace_name)
            .cloned()
            .collect()
    }

    pub fn claimed_visible_windows(&self) -> Vec<PreviewSessionWindow> {
        self.visible_windows()
            .into_iter()
            .filter(|window| window.geometry.width > 0 && window.geometry.height > 0)
            .collect()
    }

    pub fn claimed_visible_window_count(&self) -> usize {
        self.claimed_visible_windows().len()
    }

    pub fn unclaimed_visible_windows(&self) -> Vec<PreviewSessionWindow> {
        self.unclaimed_windows.clone()
    }

    pub fn visible_window_count(&self) -> usize {
        self.visible_windows().len()
    }

    pub fn window_name(&self, window_id: &WindowId) -> String {
        self.windows
            .iter()
            .find(|window| &window.id == window_id)
            .map(|window| format!("{} ({})", window.badge, window.display_title()))
            .unwrap_or_else(|| window_id.as_str().to_string())
    }

    pub fn current_scope_path(&self) -> Vec<String> {
        let Some(focused_window_id) = self.focused_window_id() else {
            return Vec::new();
        };

        self.focus_tree()
            .scope_path(&focused_window_id)
            .map(|scope_path| scope_path.iter().map(ToString::to_string).collect())
            .unwrap_or_default()
    }

    pub fn remembered_rows(&self) -> Vec<(String, String)> {
        self.remembered_focus_by_scope
            .iter()
            .map(|(scope, window_id)| (scope.to_string(), self.window_name(window_id)))
            .collect()
    }

    pub fn apply_command(&mut self, command: PreviewSessionCommand) {
        if command.name == "cycle_layout" {
            self.active_layout = self.active_layout.next();
            self.last_action = format!("{} -> {}", command.display_label(), self.active_layout.title());
            self.push_log(self.last_action.clone());
            return;
        }

        if command.name == "set_layout" {
            if let Some(PreviewSessionCommandArg::String(layout_name)) = command.arg.as_ref() {
                if let Some(layout) = PreviewLayoutId::from_name(layout_name) {
                    self.active_layout = layout;
                    self.last_action = format!("set layout -> {}", self.active_layout.title());
                    self.push_log(self.last_action.clone());
                }
            }
            return;
        }

        let state_value = match serde_wasm_bindgen::to_value(&BindingsSessionState::from(&*self)) {
            Ok(value) => value,
            Err(error) => {
                self.last_action = format!("{} -> error", command.display_label());
                self.push_log(format!("{} failed: {error}", command.display_label()));
                return;
            }
        };
        let command_value = match serde_wasm_bindgen::to_value(&command) {
            Ok(value) => value,
            Err(error) => {
                self.last_action = format!("{} -> error", command.display_label());
                self.push_log(format!("{} failed: {error}", command.display_label()));
                return;
            }
        };
        let snapshot_value = self
            .snapshot_root
            .as_ref()
            .and_then(|snapshot| serde_wasm_bindgen::to_value(snapshot).ok())
            .unwrap_or(wasm_bindgen::JsValue::NULL);

        match apply_preview_command(state_value, command_value, snapshot_value) {
            Ok(next_value) => match serde_wasm_bindgen::from_value::<BindingsSessionState>(next_value) {
                Ok(next_state) => {
                    self.apply_bindings_state(next_state);
                    self.last_action = command.display_label();
                    self.push_log(format!(
                        "{} -> {} on workspace {}",
                        self.last_action,
                        self.focused_window_id()
                            .as_ref()
                            .map(|window_id| self.window_name(window_id))
                            .unwrap_or_else(|| "none".to_string()),
                        self.active_workspace_name,
                    ));
                }
                Err(error) => {
                    self.last_action = format!("{} -> error", command.display_label());
                    self.push_log(format!("{} failed: {error}", command.display_label()));
                }
            },
            Err(error) => {
                self.last_action = format!("{} -> error", command.display_label());
                self.push_log(format!("{} failed: {error:?}", command.display_label()));
            }
        }
    }

    pub fn select_workspace(&mut self, workspace_name: String) {
        if !self.workspace_names.contains(&workspace_name) || self.active_workspace_name == workspace_name {
            return;
        }

        self.active_workspace_name = workspace_name.clone();
        let next_focus = self
            .windows
            .iter()
            .rev()
            .find(|window| window.workspace_name == workspace_name)
            .map(|window| window.id.clone());

        self.set_focus_internal(next_focus.clone());
        self.last_action = format!("view workspace -> {}", self.active_workspace_name);
        let target = next_focus
            .as_ref()
            .map(|window_id| self.window_name(window_id))
            .unwrap_or_else(|| "none".to_string());
        self.push_log(format!("workspace {} -> focus {target}", self.active_workspace_name));
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
        self.set_focus_internal(Some(window_id));
        self.last_action = format!("click focus -> {to}");
        self.push_log(format!("Selected {to} from {from}"));
    }

    pub fn navigate(&mut self, direction: NavigationDirection) {
        self.apply_command(PreviewSessionCommand::focus_dir(direction_name(direction)));
    }

    fn focus_tree(&self) -> FocusTree {
        let entries = self
            .claimed_visible_windows()
            .into_iter()
            .map(|window| FocusTreeWindowGeometry {
                window_id: window.id,
                geometry: window.geometry,
            })
            .collect::<Vec<_>>();
        FocusTree::from_window_geometries(&entries)
    }

    fn set_focus_internal(&mut self, focused_window_id: Option<WindowId>) {
        if let Some(window_id) = focused_window_id.as_ref() {
            if let Some(window) = self.windows.iter().find(|window| &window.id == window_id) {
                self.active_workspace_name = window.workspace_name.clone();
            }
        }

        for window in &mut self.windows {
            window.focused = focused_window_id.as_ref() == Some(&window.id);
        }

        if focused_window_id.is_some() {
            self.remember_current_focus();
        }
    }

    fn remember_current_focus(&mut self) {
        let Some(focused_window_id) = self.focused_window_id() else {
            return;
        };

        let focus_tree = self.focus_tree();
        if let Some(scope_path) = focus_tree.scope_path(&focused_window_id) {
            for scope in scope_path {
                self.remembered_focus_by_scope
                    .insert(scope.to_string(), focused_window_id.clone());
            }
        }
    }

    pub fn apply_layout_renderable(&mut self, layout_renderable: JsValue) {
        let layout_windows = self
            .windows
            .iter()
            .filter(|window| window.workspace_name == self.active_workspace_name && !window.floating)
            .cloned()
            .collect::<Vec<_>>();
        let render_windows = layout_windows
            .iter()
            .map(RenderLayoutWindow::from)
            .collect::<Vec<_>>();
        let Ok(windows_value) = serde_wasm_bindgen::to_value(&render_windows) else {
            self.apply_preview_failure("layout", "failed to serialize layout windows".to_string());
            return;
        };

        match compute_layout_preview(
            layout_renderable,
            windows_value,
            self.environment.stylesheet(self.active_layout),
            CANVAS_WIDTH as f32,
            CANVAS_HEIGHT as f32,
        ) {
            Ok(result_value) => {
                match serde_wasm_bindgen::from_value::<BindingsPreviewComputation>(result_value) {
                    Ok(result) => self.apply_preview_computation(layout_windows, result),
                    Err(error) => self.apply_preview_failure("layout", error.to_string()),
                }
            }
            Err(error) => {
                self.apply_preview_failure("layout", js_error_to_string(error));
            }
        }
    }

    pub fn apply_preview_failure(&mut self, source: &'static str, message: String) {
        self.snapshot_root = None;
        self.diagnostics = vec![PreviewDiagnostic {
            source: source.to_string(),
            level: "error".to_string(),
            message,
        }];
        self.unclaimed_windows = self
            .windows
            .iter()
            .filter(|window| window.workspace_name == self.active_workspace_name && !window.floating)
            .cloned()
            .collect();
        self.sync_window_geometries_from_snapshot();
    }

    fn apply_preview_computation(
        &mut self,
        layout_windows: Vec<PreviewSessionWindow>,
        computation: BindingsPreviewComputation,
    ) {
        let unclaimed_ids = computation
            .unclaimed_windows
            .into_iter()
            .map(|window| window.id)
            .collect::<BTreeSet<_>>();

        self.snapshot_root = computation
            .snapshot_root
            .and_then(normalize_preview_snapshot_node);
        self.diagnostics = computation.diagnostics;
        self.unclaimed_windows = layout_windows
            .into_iter()
            .filter(|window| unclaimed_ids.contains(window.id.as_str()))
            .collect();
        self.apply_snapshot_overrides();
        self.sync_window_geometries_from_snapshot();
    }

    fn apply_snapshot_overrides(&mut self) {
        let Some(snapshot_root) = self.snapshot_root.clone() else {
            return;
        };

        let state_value = match serde_wasm_bindgen::to_value(&BindingsSessionState::from(&*self)) {
            Ok(value) => value,
            Err(_) => return,
        };
        let snapshot_value = match serde_wasm_bindgen::to_value(&snapshot_root) {
            Ok(value) => value,
            Err(_) => return,
        };

        if let Ok(next_snapshot_value) = apply_preview_snapshot_overrides(state_value, snapshot_value) {
            if let Ok(next_snapshot) = serde_wasm_bindgen::from_value::<RawBindingsPreviewSnapshotNode>(next_snapshot_value) {
                self.snapshot_root = normalize_preview_snapshot_node(next_snapshot);
            }
        }
    }

    fn sync_window_geometries_from_snapshot(&mut self) {
        let mut geometries = BTreeMap::new();
        if let Some(snapshot_root) = self.snapshot_root.as_ref() {
            collect_snapshot_geometries(snapshot_root, &mut geometries);
        }

        for window in &mut self.windows {
            if window.workspace_name != self.active_workspace_name {
                continue;
            }

            window.geometry = geometries.get(&window.id).copied().unwrap_or(WindowGeometry {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
        }
    }

    fn apply_bindings_state(&mut self, next_state: BindingsSessionState) {
        let mut existing = self
            .windows
            .iter()
            .cloned()
            .map(|window| (window.id.clone(), window))
            .collect::<BTreeMap<_, _>>();

        self.active_workspace_name = next_state.active_workspace_name;
        self.workspace_names = next_state.workspace_names;
        self.remembered_focus_by_scope = next_state.remembered_focus_by_scope;
        self.master_ratio_by_workspace = next_state.master_ratio_by_workspace;
        self.stack_weights_by_workspace = next_state.stack_weights_by_workspace;
        self.windows = next_state
            .windows
            .into_iter()
            .enumerate()
            .map(|(index, window)| {
                let window_id = WindowId::from(window.id.as_str());
                if let Some(mut existing_window) = existing.remove(&window_id) {
                    existing_window.app_id = window.app_id;
                    existing_window.title = window.title;
                    existing_window.class = window.class;
                    existing_window.instance = window.instance;
                    existing_window.role = window.role;
                    existing_window.shell = window.shell;
                    existing_window.window_type = window.window_type;
                    existing_window.floating = window.floating;
                    existing_window.fullscreen = window.fullscreen;
                    existing_window.focused = window.focused;
                    existing_window.workspace_name = window.workspace_name;
                    existing_window
                } else {
                    PreviewSessionWindow::from_bindings(window, index)
                }
            })
            .collect();
    }

    fn push_log(&mut self, entry: String) {
        self.event_log.insert(0, entry);
        self.event_log.truncate(10);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindingsSessionState {
    active_workspace_name: String,
    workspace_names: Vec<String>,
    windows: Vec<BindingsSessionWindow>,
    #[serde(default)]
    remembered_focus_by_scope: BTreeMap<String, WindowId>,
    #[serde(default)]
    master_ratio_by_workspace: BTreeMap<String, f32>,
    #[serde(default)]
    stack_weights_by_workspace: BTreeMap<String, BTreeMap<String, f32>>,
}

impl From<&PreviewSessionState> for BindingsSessionState {
    fn from(value: &PreviewSessionState) -> Self {
        Self {
            active_workspace_name: value.active_workspace_name.clone(),
            workspace_names: value.workspace_names.clone(),
            windows: value.windows.iter().map(BindingsSessionWindow::from).collect(),
            remembered_focus_by_scope: value.remembered_focus_by_scope.clone(),
            master_ratio_by_workspace: value.master_ratio_by_workspace.clone(),
            stack_weights_by_workspace: value.stack_weights_by_workspace.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindingsSessionWindow {
    id: String,
    #[serde(default, rename = "app_id", alias = "appId")]
    app_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    class: Option<String>,
    #[serde(default)]
    instance: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default, rename = "window_type", alias = "windowType")]
    window_type: Option<String>,
    #[serde(default)]
    floating: bool,
    #[serde(default)]
    fullscreen: bool,
    #[serde(default)]
    focused: bool,
    workspace_name: String,
}

impl From<&PreviewSessionWindow> for BindingsSessionWindow {
    fn from(value: &PreviewSessionWindow) -> Self {
        Self {
            id: value.id.as_str().to_string(),
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
            workspace_name: value.workspace_name.clone(),
        }
    }
}

struct WindowVisuals {
    badge: String,
    subtitle: String,
    accent: String,
}

#[derive(Debug, Clone, Serialize)]
struct RenderLayoutWindow {
    id: String,
    #[serde(rename = "app_id")]
    app_id: Option<String>,
    title: Option<String>,
    class: Option<String>,
    instance: Option<String>,
    role: Option<String>,
    shell: Option<String>,
    #[serde(rename = "window_type")]
    window_type: Option<String>,
    floating: bool,
    fullscreen: bool,
    focused: bool,
}

impl From<&PreviewSessionWindow> for RenderLayoutWindow {
    fn from(value: &PreviewSessionWindow) -> Self {
        Self {
            id: value.id.as_str().to_string(),
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
}

#[derive(Debug, Clone, Deserialize)]
struct BindingsPreviewComputation {
    snapshot_root: Option<RawBindingsPreviewSnapshotNode>,
    #[serde(default)]
    diagnostics: Vec<PreviewDiagnostic>,
    #[serde(default)]
    unclaimed_windows: Vec<BindingsPreviewWindow>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawBindingsPreviewSnapshotNode {
    Tagged(TaggedBindingsPreviewSnapshotNode),
    Wrapped(WrappedBindingsPreviewSnapshotNode),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaggedBindingsPreviewSnapshotNode {
    #[serde(rename = "type")]
    node_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "class", alias = "className")]
    class_name: Option<PreviewSnapshotClasses>,
    #[serde(default)]
    rect: Option<LayoutRect>,
    #[serde(default, rename = "window_id", alias = "windowId")]
    window_id: Option<WindowId>,
    #[serde(default)]
    axis: Option<String>,
    #[serde(default)]
    reverse: bool,
    #[serde(default)]
    children: Vec<RawBindingsPreviewSnapshotNode>,
}

#[derive(Debug, Clone, Deserialize)]
struct WrappedBindingsPreviewSnapshotNode {
    #[serde(default)]
    workspace: Option<WrappedBindingsPreviewSnapshotFields>,
    #[serde(default)]
    group: Option<WrappedBindingsPreviewSnapshotFields>,
    #[serde(default)]
    window: Option<WrappedBindingsPreviewSnapshotFields>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrappedBindingsPreviewSnapshotFields {
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "class", alias = "className")]
    class_name: Option<PreviewSnapshotClasses>,
    #[serde(default)]
    rect: Option<LayoutRect>,
    #[serde(default, rename = "window_id", alias = "windowId")]
    window_id: Option<WindowId>,
    #[serde(default)]
    axis: Option<String>,
    #[serde(default)]
    reverse: bool,
    #[serde(default)]
    children: Vec<RawBindingsPreviewSnapshotNode>,
}

#[derive(Debug, Clone, Deserialize)]
struct BindingsPreviewWindow {
    id: String,
}

fn initial_windows() -> Vec<PreviewSessionWindow> {
    vec![
        PreviewSessionWindow::playground("win-1", "foot", "Terminal 1", "foot", "foot", "1"),
        PreviewSessionWindow::playground("win-2", "zen", "Spec Draft", "zen-browser", "zen", "1"),
        PreviewSessionWindow::playground("win-3", "slack", "Engineering", "Slack", "slack", "1"),
        PreviewSessionWindow::playground("win-4", "foot", "Terminal 2", "foot", "foot", "2"),
        PreviewSessionWindow::playground("win-5", "zen", "Reference", "zen-browser", "zen", "2"),
        PreviewSessionWindow::playground("win-6", "foot", "Terminal 3", "foot", "foot", "3"),
    ]
}

fn collect_snapshot_geometries(node: &PreviewSnapshotNode, out: &mut BTreeMap<WindowId, WindowGeometry>) {
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

fn normalize_preview_snapshot_node(
    node: RawBindingsPreviewSnapshotNode,
) -> Option<PreviewSnapshotNode> {
    match node {
        RawBindingsPreviewSnapshotNode::Tagged(node) => Some(PreviewSnapshotNode {
            node_type: node.node_type,
            id: node.id,
            class_name: node.class_name,
            rect: node.rect,
            window_id: node.window_id,
            axis: node.axis,
            reverse: node.reverse,
            children: node
                .children
                .into_iter()
                .filter_map(normalize_preview_snapshot_node)
                .collect(),
        }),
        RawBindingsPreviewSnapshotNode::Wrapped(node) => {
            if let Some(node) = node.workspace {
                return Some(normalize_wrapped_preview_snapshot_node(
                    "workspace",
                    node,
                ));
            }

            if let Some(node) = node.group {
                return Some(normalize_wrapped_preview_snapshot_node("group", node));
            }

            if let Some(node) = node.window {
                return Some(normalize_wrapped_preview_snapshot_node("window", node));
            }

            None
        }
    }
}

fn normalize_wrapped_preview_snapshot_node(
    node_type: &str,
    node: WrappedBindingsPreviewSnapshotFields,
) -> PreviewSnapshotNode {
    PreviewSnapshotNode {
        node_type: node_type.to_string(),
        id: node.id,
        class_name: node.class_name,
        rect: node.rect,
        window_id: node.window_id,
        axis: node.axis,
        reverse: node.reverse,
        children: node
            .children
            .into_iter()
            .filter_map(normalize_preview_snapshot_node)
            .collect(),
    }
}

fn default_window_visuals(window_id: &str, app_id: Option<&str>, title: Option<&str>, index: usize) -> WindowVisuals {
    const PALETTE: [&str; 8] = [
        "#7dd3fc",
        "#f97316",
        "#34d399",
        "#facc15",
        "#818cf8",
        "#06b6d4",
        "#e879f9",
        "#fb7185",
    ];

    let seed = title.or(app_id).unwrap_or(window_id);
    let hash = seed.bytes().fold(index, |acc, byte| acc + byte as usize);
    let badge = seed
        .chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());

    WindowVisuals {
        badge,
        subtitle: app_id.unwrap_or("preview window").to_string(),
        accent: PALETTE[hash % PALETTE.len()].to_string(),
    }
}

fn direction_name(direction: NavigationDirection) -> &'static str {
    match direction {
        NavigationDirection::Left => "left",
        NavigationDirection::Right => "right",
        NavigationDirection::Up => "up",
        NavigationDirection::Down => "down",
    }
}

fn js_error_to_string(error: JsValue) -> String {
    error
        .as_string()
        .unwrap_or_else(|| format!("{error:?}"))
}