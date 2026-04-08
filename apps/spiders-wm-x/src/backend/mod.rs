mod atoms;
mod discovery;
mod event;
mod input;
mod output;

use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::net::UnixListener;
use std::process::Command;

use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::{Config, ConfigPaths};
use spiders_core::effect::{FocusTarget, WindowToggle, WmHostEffect, WorkspaceAssignment, WorkspaceTarget};
use spiders_core::event::WmEvent;
use spiders_core::focus::FocusTree;
use spiders_core::navigation::{WindowGeometryCandidate, managed_window_swap_positions, select_directional_focus_candidate};
use spiders_core::query::{QueryRequest, QueryResponse, state_snapshot_for_model};
use spiders_core::signal::WmSignal;
use spiders_core::snapshot::StateSnapshot;
use spiders_core::types::{ShellKind, WindowMode};
use spiders_core::wm::{WindowGeometry, WmModel};
use spiders_core::{SeatId, WindowId, WorkspaceId};
use spiders_ipc_core::IpcHandler;
use spiders_ipc_native::{
    IpcTransportError, NativeIpcServeError, NativeIpcState, accept_pending_ipc_clients,
    bind_native_ipc_listener, send_response,
};
use spiders_wm_runtime::{
    PreviewRenderAction, PreviewWindow, WmHost, WmRuntime, collect_snapshot_geometries,
    compute_layout_preview_from_source_layout, dispatch_wm_command,
};
use tracing::{info, warn};
use x11rb::xcb_ffi::XCBConnection;
use x11rb::connection::Connection;
use x11rb::protocol::randr;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageData, ClientMessageEvent,
    ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, EventMask, InputFocus, PropMode,
    StackMode, Window, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;

use crate::ipc::handle_debug_dump;
use spiders_ipc::DebugRequest;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, CURRENT_TIME};

use crate::config;

use self::atoms::Atoms;
use self::discovery::{DiscoveredWindow, discover_window_for_event, discover_windows};
use self::event::{
    ManageEventHandler, ManageLoopDispatchState, install_manage_root_mask, observe_connection_events,
    register_ipc_client_source, run_manage_event_loop,
};
use self::input::{KeyboardBindings, binding_for_key_event, install_key_grabs, load_keyboard_bindings, uninstall_key_grabs};
use self::output::{DiscoveredOutput, discover_outputs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenDescriptor {
    pub(crate) index: usize,
    pub(crate) root_window: Window,
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl ScreenDescriptor {
    pub(crate) fn from_setup(index: usize, connection: &XCBConnection) -> Result<Self> {
        let screen = connection
            .setup()
            .roots
            .get(index)
            .context("failed to resolve the selected X screen")?;
        Ok(Self {
            index,
            root_window: screen.root,
            width: screen.width_in_pixels,
            height: screen.height_in_pixels,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BackendCapabilities {
    pub(crate) randr: bool,
}

pub(crate) struct BackendApp {
    connection: XCBConnection,
    atoms: Atoms,
    state: RuntimeState,
    ipc_listener: Option<UnixListener>,
}

impl BackendApp {
    pub(crate) fn connect() -> Result<Self> {
        let (config_paths, config) = config::load_config();
        let (connection, screen_index) = XCBConnection::connect(None).context("failed to connect to the X server")?;
        let display_name = std::env::var("DISPLAY").unwrap_or_else(|_| format!(":{screen_index}"));

        let screen = ScreenDescriptor::from_setup(screen_index, &connection)?;
        let atoms = Atoms::intern_all(&connection).context("failed to intern X atoms")?;
        let ewmh_window = create_ewmh_support_window(&connection, &screen)?;
        let discovered_outputs = discover_outputs(&connection, &screen)?;
        let capabilities = BackendCapabilities {
            randr: discovered_outputs.len() > 1
                || discovered_outputs.iter().any(|output| output.x != 0 || output.y != 0),
        };
        let discovered_windows = discover_windows(&connection, screen.root_window, &atoms)?;
        let keyboard_bindings = load_keyboard_bindings(&connection, &config)?;
        let state = RuntimeState::bootstrap(
            config_paths,
            display_name,
            config,
            screen,
            capabilities,
            ewmh_window,
            &discovered_outputs,
            &discovered_windows,
            keyboard_bindings,
        );

        Ok(Self { connection, atoms, state, ipc_listener: None })
    }

    pub(crate) fn log_bootstrap(&self) {
        let screen = self.state.screen();
        let config_paths = self.state.config_paths();
        let snapshot = self.state.snapshot();
        let capabilities = self.state.capabilities();

        info!(
            screen_num = screen.index,
            root_window_id = screen.root_window,
            width = screen.width,
            height = screen.height,
            randr = capabilities.randr,
            authored_config = config_paths.map(|paths| paths.authored_config.display().to_string()),
            prepared_config = config_paths.map(|paths| paths.prepared_config.display().to_string()),
            workspace_count = snapshot.workspaces.len(),
            output_count = snapshot.outputs.len(),
            window_count = snapshot.windows.len(),
            atom_count = self.atoms.known_atom_count(),
            "spiders-wm-x bootstrapped X11 backend"
        );
    }

    pub(crate) fn print_state_dump(&self) -> Result<()> {
        println!("{}", serde_json::to_string_pretty(&self.state.snapshot())?);
        Ok(())
    }

    pub(crate) fn observe(
        &mut self,
        event_limit: Option<usize>,
        idle_timeout_ms: Option<u64>,
    ) -> Result<()> {
        observe_connection_events(&self.connection, self.state.screen(), event_limit, idle_timeout_ms)
    }

    pub(crate) fn manage(&mut self) -> Result<()> {
        let screen = *self.state.screen();
        if let Some(socket_path) = self.state.ipc.init_socket_path("spiders-wm-x") {
            match bind_native_ipc_listener(&socket_path) {
                Ok(listener) => {
                    info!(path = %socket_path.display(), "spiders-wm-x bound IPC socket");
                    self.ipc_listener = Some(listener);
                }
                Err(error) => {
                    warn!(path = %socket_path.display(), %error, "failed to bind wm-x IPC socket");
                }
            }
        }
        install_manage_root_mask(&self.connection, &screen)?;
        install_key_grabs(&self.connection, screen.root_window, &self.state.keyboard_bindings.installed)?;
        self.scan_existing_windows()?;
        self.state.publish_ewmh_state(&self.connection, self.atoms)?;

        info!(
            root_window_id = screen.root_window,
            screen_num = screen.index,
            "spiders-wm-x acquired X11 window manager ownership"
        );

        let mut handler = BackendManageHandler {
            atoms: self.atoms,
            state: &mut self.state,
            ipc_listener: self.ipc_listener.as_ref(),
        };
        let result = run_manage_event_loop(&self.connection, &screen, self.ipc_listener.as_ref(), &mut handler);
        uninstall_key_grabs(&self.connection, screen.root_window, &self.state.keyboard_bindings.installed)?;
        result
    }

    fn scan_existing_windows(&mut self) -> Result<()> {
        let windows = discover_windows(&self.connection, self.state.screen.root_window, &self.atoms)?;
        for discovered in windows {
            install_managed_window_event_mask(&self.connection, discovered.window)?;
            self.state.ensure_runtime_window(&discovered);
            self.state.sync_window_identity(&discovered);
        }
        Ok(())
    }
}

struct RuntimeState {
    config_paths: Option<ConfigPaths>,
    display_name: String,
    config: Config,
    layout_service: Option<AuthoringLayoutService>,
    ewmh_window: Window,
    model: WmModel,
    screen: ScreenDescriptor,
    capabilities: BackendCapabilities,
    #[allow(dead_code)]
    outputs: Vec<DiscoveredOutput>,
    x_windows: BTreeMap<u32, WindowId>,
    stacking_order: Vec<u32>,
    workspace_hidden_windows: BTreeSet<u32>,
    keyboard_bindings: KeyboardBindings,
    quit_requested: bool,
    pending_events: Vec<WmEvent>,
    ipc: NativeIpcState,
}

impl RuntimeState {
    fn bootstrap(
        config_paths: Option<ConfigPaths>,
        display_name: String,
        config: Config,
        screen: ScreenDescriptor,
        capabilities: BackendCapabilities,
        ewmh_window: Window,
        discovered_outputs: &[DiscoveredOutput],
        discovered_windows: &[DiscoveredWindow],
        keyboard_bindings: KeyboardBindings,
    ) -> Self {
        let layout_service = config_paths.as_ref().and_then(crate::config::build_layout_service);
        let mut model = WmModel::default();
        let workspace_names = crate::config::configured_workspace_names(&config);
        let primary_output = discovered_outputs
            .iter()
            .find(|output| output.primary)
            .or_else(|| discovered_outputs.first())
            .expect("at least one X11 output must exist");

        {
            let mut runtime = WmRuntime::new(&mut model);
            let mut host = NoopHost;

            runtime.ensure_default_workspace(workspace_names[0].clone());
            for workspace_name in workspace_names.iter().skip(1) {
                runtime.ensure_workspace(workspace_name.clone());
            }

            let _ = runtime.handle_signal(
                &mut host,
                WmSignal::EnsureSeat { seat_id: SeatId::from("x11") },
            );
            for output in discovered_outputs {
                let _ = runtime.handle_signal(
                    &mut host,
                    WmSignal::OutputSynced {
                        output_id: output.output_id.clone(),
                        name: output.name.clone(),
                        logical_width: output.width,
                        logical_height: output.height,
                    },
                );
            }
            runtime.sync_layout_selection_defaults(&config);
            self::discovery::sync_discovered_windows(&mut runtime, discovered_windows);
            let _initial_events = runtime.take_events();
        }

        model.set_current_output(primary_output.output_id.clone());
        attach_workspaces_to_outputs(&mut model, discovered_outputs);
        for output in discovered_outputs {
            model.outputs.entry(output.output_id.clone()).and_modify(|model_output| {
                model_output.logical_x = output.x;
                model_output.logical_y = output.y;
            });
        }

        let x_windows = discovered_windows
            .iter()
            .map(|window| (window.window, window.window_id.clone()))
            .collect();
        let stacking_order = discovered_windows.iter().map(|window| window.window).collect();

        Self {
            config_paths,
            display_name,
            config,
            layout_service,
            ewmh_window,
            model,
            screen,
            capabilities,
            outputs: discovered_outputs.to_vec(),
            x_windows,
            stacking_order,
            workspace_hidden_windows: BTreeSet::new(),
            keyboard_bindings,
            quit_requested: false,
            pending_events: Vec::new(),
            ipc: NativeIpcState::default(),
        }
    }

    fn config_paths(&self) -> Option<&ConfigPaths> {
        self.config_paths.as_ref()
    }

    fn screen(&self) -> &ScreenDescriptor {
        &self.screen
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities
    }

    fn snapshot(&self) -> StateSnapshot {
        state_snapshot_for_model(&self.model)
    }

    fn query(&self, request: QueryRequest) -> QueryResponse {
        spiders_core::query::query_response_for_model(&self.model, request)
    }

    fn take_pending_events(&mut self) -> Vec<WmEvent> {
        std::mem::take(&mut self.pending_events)
    }

    fn reload_config(&mut self, connection: &XCBConnection) -> Result<bool> {
        let (config_paths, config) = config::load_config();
        let keyboard_bindings = load_keyboard_bindings(connection, &config)?;
        let workspace_names = crate::config::configured_workspace_names(&config);

        {
            let mut runtime = WmRuntime::new(&mut self.model);

            if let Some(default_workspace) = workspace_names.first() {
                runtime.ensure_default_workspace(default_workspace.clone());
                for workspace_name in workspace_names.iter().skip(1) {
                    runtime.ensure_workspace(workspace_name.clone());
                }
            }

            runtime.sync_layout_selection_defaults(&config);
            self.pending_events.extend(runtime.take_events());
        }

        self.config_paths = config_paths;
        self.layout_service = self.config_paths.as_ref().and_then(crate::config::build_layout_service);
        self.config = config;
        self.keyboard_bindings = keyboard_bindings;

        Ok(true)
    }

    fn ensure_runtime_window(&mut self, discovered: &DiscoveredWindow) {
        let mut runtime = WmRuntime::new(&mut self.model);
        self::discovery::sync_discovered_windows(&mut runtime, std::slice::from_ref(discovered));
        self.pending_events.extend(runtime.take_events());
        self.x_windows.insert(discovered.window, discovered.window_id.clone());
        self.workspace_hidden_windows.remove(&discovered.window);
        self.raise_in_stacking(discovered.window);
    }

    fn sync_window_identity(&mut self, discovered: &DiscoveredWindow) {
        let mut runtime = WmRuntime::new(&mut self.model);
        let mut host = NoopHost;
        let _ = runtime.handle_signal(
            &mut host,
            WmSignal::WindowIdentityChanged {
                window_id: discovered.window_id.clone(),
                title: discovered.title.clone(),
                app_id: discovered.app_id.clone(),
                class: discovered.class.clone(),
                instance: discovered.instance.clone(),
                role: discovered.role.clone(),
                window_type: discovered.window_type.clone(),
                urgent: discovered.urgent,
            },
        );
        self.pending_events.extend(runtime.take_events());
    }

    fn sync_window_mapped(&mut self, window_id: WindowId, mapped: bool) {
        let mut runtime = WmRuntime::new(&mut self.model);
        let _ = runtime.sync_window_mapped(window_id, mapped);
        self.pending_events.extend(runtime.take_events());
    }

    fn focus_window(&mut self, window_id: Option<WindowId>) {
        let seat_id = SeatId::from("x11");
        let mut runtime = WmRuntime::new(&mut self.model);
        let _ = runtime.request_focus_window_selection(seat_id, window_id);
        self.pending_events.extend(runtime.take_events());
    }

    fn focus_next_window(&mut self) -> Option<WindowId> {
        let seat_id = SeatId::from("x11");
        let window_order = self.window_order();
        let mut runtime = WmRuntime::new(&mut self.model);
        let focused = runtime.request_focus_next_window_selection(seat_id, window_order).focused_window_id;
        self.pending_events.extend(runtime.take_events());
        focused
    }

    fn focus_previous_window(&mut self) -> Option<WindowId> {
        let seat_id = SeatId::from("x11");
        let window_order = self.window_order();
        let mut runtime = WmRuntime::new(&mut self.model);
        let focused = runtime.request_focus_previous_window_selection(seat_id, window_order).focused_window_id;
        self.pending_events.extend(runtime.take_events());
        focused
    }

    fn focus_direction_window(&mut self, direction: spiders_core::command::FocusDirection) -> Option<WindowId> {
        let seat_id = SeatId::from("x11");
        let geometries = self
            .current_layout_geometries()
            .ok()?
            .into_iter()
            .filter_map(|(x_window, geometry)| self.window_id_for_x_window(x_window).map(|window_id| (window_id, geometry)))
            .collect::<Vec<_>>();
        info!(?direction, candidate_count = geometries.len(), current_focus = ?self.model.focused_window_id, "wm-x focus direction start");
        let mut runtime = WmRuntime::new(&mut self.model);
        let selection = runtime.request_focus_direction_window_selection(seat_id, direction, geometries);
        self.pending_events.extend(runtime.take_events());
        info!(?direction, focused_window_id = ?selection.focused_window_id, "wm-x focus direction finished");
        selection.focused_window_id
    }

    fn swap_focused_window_direction(&mut self, direction: spiders_core::command::FocusDirection) -> bool {
        let ordered_window_ids = self.tiled_window_order_on_current_workspace();
        let candidates = self.directional_swap_candidates(&ordered_window_ids);
        let Some(focused_window_id) = self.model.focused_window_id.clone() else {
            return false;
        };
        let Some(target_window_id) = select_directional_focus_candidate(
            &candidates,
            Some(focused_window_id.clone()),
            navigation_direction(direction),
            &self.model.last_focused_window_id_by_scope,
            self.model.focus_tree.as_ref(),
        ) else {
            return false;
        };
        let Some((focused_index, target_index)) =
            managed_window_swap_positions(&ordered_window_ids, focused_window_id.clone(), target_window_id.clone())
        else {
            return false;
        };
        let Some(focused_x_window) = self.x_window_for_window_id(&ordered_window_ids[focused_index]) else {
            return false;
        };
        let Some(target_x_window) = self.x_window_for_window_id(&ordered_window_ids[target_index]) else {
            return false;
        };

        self.swap_x_window_positions(focused_x_window, target_x_window);
        self.model.set_window_focused(Some(focused_window_id.clone()));
        info!(?direction, ?focused_window_id, ?target_window_id, "wm-x swapped focused window with directional neighbor");
        true
    }

    fn switch_to_workspace_index(&mut self, index: u32) -> bool {
        let Some(workspace_id) =
            self.model.workspaces.keys().cloned().collect::<Vec<_>>().get(index as usize).cloned()
        else {
            return false;
        };

        self.model.set_current_workspace(workspace_id);
        true
    }

    fn window_order(&self) -> Vec<WindowId> {
        self.stacking_order
            .iter()
            .filter_map(|x_window_id| self.x_windows.get(x_window_id).cloned())
            .collect()
    }

    fn tiled_window_order_on_current_workspace(&self) -> Vec<WindowId> {
        self.window_order()
            .into_iter()
            .filter(|window_id| {
                self.model.windows.get(window_id).is_some_and(|window| {
                    window.workspace_id == self.model.current_workspace_id
                        && window.mapped
                        && !window.floating
                        && !window.fullscreen
                        && !window.closing
                })
            })
            .collect()
    }

    fn select_named_workspace(&mut self, name: String) -> bool {
        let workspace_id = self
            .model
            .workspaces
            .iter()
            .find_map(|(workspace_id, workspace)| (workspace.name == name).then_some(workspace_id.clone()));

        match workspace_id {
            Some(workspace_id) => {
                let window_order = self.window_order();
                let mut runtime = WmRuntime::new(&mut self.model);
                let changed = runtime.request_select_workspace(workspace_id, window_order).is_some();
                self.pending_events.extend(runtime.take_events());
                changed
            }
            None => false,
        }
    }

    fn select_next_workspace(&mut self) -> bool {
        let window_order = self.window_order();
        let mut runtime = WmRuntime::new(&mut self.model);
        let changed = runtime.request_select_next_workspace(window_order).is_some();
        self.pending_events.extend(runtime.take_events());
        changed
    }

    fn select_previous_workspace(&mut self) -> bool {
        let window_order = self.window_order();
        let mut runtime = WmRuntime::new(&mut self.model);
        let changed = runtime.request_select_previous_workspace(window_order).is_some();
        self.pending_events.extend(runtime.take_events());
        changed
    }

    fn assign_focused_window_to_workspace(&mut self, workspace: u8, toggle: bool) -> bool {
        let workspace_id = spiders_core::WorkspaceId::from(workspace.to_string());
        let window_order = self.window_order();
        let mut runtime = WmRuntime::new(&mut self.model);
        let selection = if toggle {
            runtime.toggle_assign_focused_window_to_workspace(workspace_id, window_order)
        } else {
            runtime.assign_focused_window_to_workspace(workspace_id, window_order)
        };
        self.pending_events.extend(runtime.take_events());

        selection.focused_window_id.is_some() || self.model.focused_window_id.is_some()
    }

    fn toggle_focused_window_floating(&mut self) -> bool {
        let mut runtime = WmRuntime::new(&mut self.model);
        let changed = runtime.toggle_focused_window_floating().is_some();
        self.pending_events.extend(runtime.take_events());
        changed
    }

    fn toggle_focused_window_fullscreen(&mut self) -> bool {
        let mut runtime = WmRuntime::new(&mut self.model);
        let changed = runtime.toggle_focused_window_fullscreen().is_some();
        self.pending_events.extend(runtime.take_events());
        changed
    }

    fn request_close_focused_window(&mut self) -> Option<(Window, WindowId)> {
        let focused_window_id = self.model.focused_window_id.clone()?;
        let x_window = self
            .x_windows
            .iter()
            .find_map(|(x_window_id, window_id)| (window_id == &focused_window_id).then_some(*x_window_id))?;
        info!(window = x_window, ?focused_window_id, "wm-x close focused window selected");
        Some((x_window, focused_window_id))
    }

    fn focused_x_window(&self) -> Option<Window> {
        let focused_window_id = self.model.focused_window_id.as_ref()?;
        self.x_windows
            .iter()
            .find_map(|(x_window_id, window_id)| (window_id == focused_window_id).then_some(*x_window_id))
    }

    fn x_window_for_window_id(&self, target_window_id: &WindowId) -> Option<Window> {
        self.x_windows
            .iter()
            .find_map(|(x_window_id, window_id)| (window_id == target_window_id).then_some(*x_window_id))
    }

    fn activate_x_window(&mut self, window: Window) -> Option<WindowId> {
        let window_id = self.window_id_for_x_window(window)?;
        self.focus_window(Some(window_id.clone()));
        self.raise_in_stacking(window);
        Some(window_id)
    }

    fn move_window_to_workspace_index(&mut self, window: Window, index: u32) -> bool {
        let Some(window_id) = self.window_id_for_x_window(window) else {
            return false;
        };
        let Some(workspace_id) =
            self.model.workspaces.keys().cloned().collect::<Vec<_>>().get(index as usize).cloned()
        else {
            return false;
        };

        self.model.set_window_workspace(window_id, Some(workspace_id));
        true
    }

    fn set_window_fullscreen_for_x_window(&mut self, window: Window, fullscreen: bool) -> bool {
        let Some(window_id) = self.window_id_for_x_window(window) else {
            return false;
        };

        self.model.set_window_fullscreen(window_id, fullscreen);
        true
    }

    fn restack_x_window(&mut self, window: Window, detail: StackMode) -> bool {
        if !self.x_windows.contains_key(&window) {
            return false;
        }

        self.stacking_order.retain(|candidate| *candidate != window);
        match detail {
            StackMode::ABOVE => self.stacking_order.push(window),
            StackMode::BELOW => self.stacking_order.insert(0, window),
            _ => self.stacking_order.push(window),
        }
        true
    }

    fn set_window_floating_geometry(&mut self, window_id: WindowId, geometry: WindowGeometry) {
        let mut runtime = WmRuntime::new(&mut self.model);
        let _ = runtime.set_window_floating_geometry(window_id, geometry);
        self.pending_events.extend(runtime.take_events());
    }

    fn sync_actual_window_geometry(&mut self, window: Window, geometry: WindowGeometry) {
        let Some(window_id) = self.window_id_for_x_window(window) else {
            return;
        };

        self.set_window_floating_geometry(window_id, geometry);
    }

    fn current_layout_geometries(&mut self) -> Result<Vec<(Window, WindowGeometry)>> {
        let state = self.snapshot();
        let ordered_window_ids = self.window_order();
        let Some(layout_service) = self.layout_service.as_mut() else {
            return Ok(Vec::new());
        };
        let visible_workspaces = state
            .workspaces
            .iter()
            .filter(|workspace| workspace.visible)
            .cloned()
            .collect::<Vec<_>>();
        if visible_workspaces.is_empty() {
            return Ok(Vec::new());
        }

        let mut geometry_by_window = BTreeMap::new();
        let mut focus_tree_entries = Vec::new();

        for workspace in visible_workspaces {
            let Some(output) = workspace
                .output_id
                .as_ref()
                .and_then(|output_id| state.output_by_id(output_id))
                .or_else(|| state.current_output())
                .cloned()
            else {
                continue;
            };

            if let Some(fullscreen_window) = state.windows.iter().find(|window| {
                window.workspace_id.as_ref() == Some(&workspace.id)
                    && window.output_id.as_ref() == Some(&output.id)
                    && window.mapped
                    && window.mode.is_fullscreen()
            }) {
                geometry_by_window.insert(fullscreen_window.id.clone(), output_geometry(&output));
                continue;
            }

            let Some(prepared) = layout_service
                .evaluate_prepared_for_workspace(&self.config, &state, &workspace)
                .context("failed to evaluate prepared X11 layout")?
            else {
                continue;
            };

            let stylesheet_source = prepared.artifact.stylesheets.combined_source();
            let ordered_windows = ordered_window_ids
                .iter()
                .filter_map(|window_id| state.windows.iter().find(|window| &window.id == window_id))
                .filter(|window| {
                    window.workspace_id.as_ref() == Some(&workspace.id)
                        && window.output_id.as_ref() == Some(&output.id)
                        && window.mapped
                        && matches!(window.mode, WindowMode::Tiled)
                });
            let unordered_windows = state.windows.iter().filter(|window| {
                !ordered_window_ids.contains(&window.id)
                    && window.workspace_id.as_ref() == Some(&workspace.id)
                    && window.output_id.as_ref() == Some(&output.id)
                    && window.mapped
                    && matches!(window.mode, WindowMode::Tiled)
            });
            let windows = ordered_windows
                .chain(unordered_windows)
                .map(|window| PreviewWindow {
                    id: window.id.to_string(),
                    app_id: window.app_id.clone(),
                    title: window.title.clone(),
                    class: window.class.clone(),
                    instance: window.instance.clone(),
                    role: window.role.clone(),
                    shell: Some(match window.shell {
                        ShellKind::X11 => "x11".to_string(),
                        ShellKind::XdgToplevel => "xdg-toplevel".to_string(),
                        ShellKind::Unknown => "unknown".to_string(),
                    }),
                    window_type: window.window_type.clone(),
                    floating: window.mode.is_floating(),
                    fullscreen: window.mode.is_fullscreen(),
                    focused: window.focused,
                    workspace_name: workspace.name.clone(),
                })
                .collect::<Vec<_>>();

            if windows.is_empty() {
                continue;
            }

            let preview = compute_layout_preview_from_source_layout(
                &prepared.layout,
                &windows,
                Some(&self.config),
                Some(&workspace.name),
                &stylesheet_source,
                output.logical_width as f32,
                output.logical_height as f32,
            );
            let Some(snapshot_root) = preview.snapshot_root else {
                continue;
            };
            collect_focus_tree_entries(&snapshot_root, output.logical_x, output.logical_y, &mut focus_tree_entries);

            let mut workspace_geometry = BTreeMap::new();
            collect_snapshot_geometries(&snapshot_root, &mut workspace_geometry);
            for (window_id, mut geometry) in workspace_geometry {
                geometry.x += output.logical_x;
                geometry.y += output.logical_y;
                geometry_by_window.insert(window_id, geometry);
            }
        }

        self.model.set_focus_tree_value((!focus_tree_entries.is_empty()).then(|| FocusTree::from_window_geometries(&focus_tree_entries)));

        Ok(self
            .x_windows
            .iter()
            .filter_map(|(x_window_id, window_id)| {
                geometry_by_window.get(window_id).copied().map(|geometry| (*x_window_id, geometry))
            })
            .collect())
    }

    fn unmap_window(&mut self, window_id: WindowId) {
        let window_order = self.model.windows.keys().cloned().collect::<Vec<_>>();
        let mut runtime = WmRuntime::new(&mut self.model);
        let _ = runtime.unmap_window(window_id, window_order);
        self.pending_events.extend(runtime.take_events());
    }

    fn remove_window(&mut self, window: Window) {
        let Some(window_id) = self.x_windows.remove(&window) else {
            return;
        };
        self.stacking_order.retain(|candidate| *candidate != window);
        self.workspace_hidden_windows.remove(&window);

        let window_order = self.model.windows.keys().cloned().collect::<Vec<_>>();
        let mut runtime = WmRuntime::new(&mut self.model);
        let _ = runtime.remove_window(window_id, window_order);
        self.pending_events.extend(runtime.take_events());
    }

    fn window_id_for_x_window(&self, window: Window) -> Option<WindowId> {
        self.x_windows.get(&window).cloned()
    }

    fn raise_in_stacking(&mut self, x_window_id: u32) {
        self.stacking_order.retain(|candidate| *candidate != x_window_id);
        self.stacking_order.push(x_window_id);
    }

    fn swap_x_window_positions(&mut self, first: Window, second: Window) {
        let Some(first_index) = self.stacking_order.iter().position(|candidate| *candidate == first) else {
            return;
        };
        let Some(second_index) = self.stacking_order.iter().position(|candidate| *candidate == second) else {
            return;
        };

        self.stacking_order.swap(first_index, second_index);
    }

    fn directional_swap_candidates(&mut self, ordered_window_ids: &[WindowId]) -> Vec<WindowGeometryCandidate> {
        let mut geometry_by_window = self
            .current_layout_geometries()
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|(x_window, geometry)| self.window_id_for_x_window(x_window).map(|window_id| (window_id, geometry)))
            .collect::<BTreeMap<_, _>>();

        if geometry_by_window.is_empty()
            && let Some(focus_tree) = self.model.focus_tree.as_ref()
        {
            for (index, window_id) in focus_tree.ordered_window_ids().iter().enumerate() {
                geometry_by_window.entry(window_id.clone()).or_insert(WindowGeometry {
                    x: index as i32 * 100,
                    y: 0,
                    width: 100,
                    height: 100,
                });
            }
        }

        ordered_window_ids
            .iter()
            .filter_map(|window_id| {
                geometry_by_window.get(window_id).copied().map(|geometry| WindowGeometryCandidate {
                    window_id: window_id.clone(),
                    geometry,
                    scope_path: self
                        .model
                        .focus_scope_path(window_id)
                        .map(|scope_path| scope_path.to_vec())
                        .unwrap_or_else(|| vec![FocusTree::workspace_scope()]),
                })
            })
            .collect()
    }

    #[allow(dead_code)]
    fn refresh_outputs(&mut self, connection: &XCBConnection) -> Result<()> {
        let outputs = discover_outputs(connection, &self.screen)?;
        let previous_outputs = self.outputs.clone();
        let previous_current_output_id = self.model.current_output_id.clone();
        self.capabilities.randr =
            outputs.len() > 1 || outputs.iter().any(|output| output.x != 0 || output.y != 0);
        self.outputs = outputs.clone();

        let previous_output_names = previous_outputs
            .iter()
            .map(|output| (output.output_id.clone(), output.name.clone()))
            .collect::<BTreeMap<_, _>>();
        let previous_workspace_by_output_name = self
            .model
            .outputs
            .values()
            .filter_map(|output| {
                output
                    .focused_workspace_id
                    .clone()
                    .map(|workspace_id| (output.name.clone(), workspace_id))
            })
            .collect::<BTreeMap<_, _>>();
        let next_output_ids = outputs.iter().map(|output| output.output_id.clone()).collect::<BTreeSet<_>>();

        {
            let mut runtime = WmRuntime::new(&mut self.model);
            let mut host = NoopHost;

            for removed_output_id in previous_output_names.keys().filter(|output_id| !next_output_ids.contains(*output_id)) {
                let _ = runtime.handle_signal(
                    &mut host,
                    WmSignal::OutputRemoved { output_id: removed_output_id.clone() },
                );
            }

            for output in &outputs {
                let _ = runtime.handle_signal(
                    &mut host,
                    WmSignal::OutputSynced {
                        output_id: output.output_id.clone(),
                        name: output.name.clone(),
                        logical_width: output.width,
                        logical_height: output.height,
                    },
                );
            }

            let _ = runtime.take_events();
        }

        preserve_workspace_output_attachments(
            &mut self.model,
            &outputs,
            &previous_workspace_by_output_name,
        );
        for output in &outputs {
            self.model.outputs.entry(output.output_id.clone()).and_modify(|model_output| {
                model_output.logical_x = output.x;
                model_output.logical_y = output.y;
            });
        }

        if let Some(current_output_id) = previous_current_output_id.filter(|output_id| next_output_ids.contains(output_id)) {
            self.model.set_current_output(current_output_id);
        } else if let Some(primary_output) = outputs.iter().find(|output| output.primary).or_else(|| outputs.first()) {
            self.model.set_current_output(primary_output.output_id.clone());
        }

        Ok(())
    }

    fn publish_ewmh_state(&self, connection: &XCBConnection, atoms: Atoms) -> Result<()> {
        let snapshot = self.snapshot();
        let root = self.screen.root_window;

        change_root_property32(
            connection,
            root,
            atoms.net_supported,
            AtomEnum::ATOM.into(),
            &supported_atoms(atoms),
        )?;
        change_root_property32(
            connection,
            root,
            atoms.net_supporting_wm_check,
            AtomEnum::WINDOW.into(),
            &[self.ewmh_window],
        )?;
        change_root_property32(
            connection,
            self.ewmh_window,
            atoms.net_supporting_wm_check,
            AtomEnum::WINDOW.into(),
            &[self.ewmh_window],
        )?;

        if atoms.net_wm_name != u32::from(AtomEnum::NONE)
            && atoms.utf8_string != u32::from(AtomEnum::NONE)
        {
            connection
                .change_property8(
                    PropMode::REPLACE,
                    self.ewmh_window,
                    atoms.net_wm_name,
                    atoms.utf8_string,
                    b"spiders-wm-x",
                )?
                .check()?;
        }

        let active_window = snapshot
            .focused_window_id
            .as_ref()
            .and_then(|window_id| {
                self.x_windows.iter().find_map(|(x_window, id)| (id == window_id).then_some(*x_window))
            })
            .unwrap_or(0);
        change_root_property32(
            connection,
            root,
            atoms.net_active_window,
            AtomEnum::WINDOW.into(),
            &[active_window],
        )?;

        let client_list = snapshot
            .windows
            .iter()
            .filter_map(|window| {
                self.x_windows
                    .iter()
                    .find_map(|(x_window, id)| (id == &window.id).then_some(*x_window))
            })
            .collect::<Vec<_>>();
        let stacking_list = self
            .stacking_order
            .iter()
            .filter_map(|x_window| self.x_windows.contains_key(x_window).then_some(*x_window))
            .collect::<Vec<_>>();
        change_root_property32(
            connection,
            root,
            atoms.net_client_list,
            AtomEnum::WINDOW.into(),
            &client_list,
        )?;
        change_root_property32(
            connection,
            root,
            atoms.net_client_list_stacking,
            AtomEnum::WINDOW.into(),
            &stacking_list,
        )?;

        let current_desktop =
            desktop_index_for_workspace(&snapshot, snapshot.current_workspace_id.as_ref()).unwrap_or(0);
        change_root_property32(
            connection,
            root,
            atoms.net_current_desktop,
            AtomEnum::CARDINAL.into(),
            &[current_desktop],
        )?;
        change_root_property32(
            connection,
            root,
            atoms.net_number_of_desktops,
            AtomEnum::CARDINAL.into(),
            &[snapshot.workspaces.len() as u32],
        )?;

        if atoms.net_desktop_names != u32::from(AtomEnum::NONE)
            && atoms.utf8_string != u32::from(AtomEnum::NONE)
        {
            let desktop_names = snapshot
                .workspaces
                .iter()
                .map(|workspace| workspace.name.as_str())
                .collect::<Vec<_>>()
                .join("\0");
            connection
                .change_property8(
                    PropMode::REPLACE,
                    root,
                    atoms.net_desktop_names,
                    atoms.utf8_string,
                    desktop_names.as_bytes(),
                )?
                .check()?;
        }

        let workareas = workareas_for_snapshot(&snapshot);
        change_root_property32(
            connection,
            root,
            atoms.net_workarea,
            AtomEnum::CARDINAL.into(),
            &workareas,
        )?;

        for (x_window_id, window_id) in &self.x_windows {
            let snapshot_window = snapshot.windows.iter().find(|candidate| &candidate.id == window_id);

            if let Some(snapshot_window) = snapshot_window {
                if let Some(desktop_index) =
                    desktop_index_for_workspace(&snapshot, snapshot_window.workspace_id.as_ref())
                {
                    if let Err(error) = change_root_property32(
                        connection,
                        *x_window_id,
                        atoms.net_wm_desktop,
                        AtomEnum::CARDINAL.into(),
                        &[desktop_index],
                    ) {
                        if is_bad_window_reply_error(&error) {
                            warn!(window = *x_window_id, ?error, "skipping X11 EWMH desktop property update for invalid window");
                            continue;
                        }
                        return Err(error);
                    }
                } else {
                    if let Err(error) = delete_property(connection, *x_window_id, atoms.net_wm_desktop) {
                        if is_bad_window_reply_error(&error) {
                            warn!(window = *x_window_id, ?error, "skipping X11 EWMH desktop property delete for invalid window");
                            continue;
                        }
                        return Err(error);
                    }
                }

                let window_state_atoms = ewmh_window_state_atoms(atoms, snapshot_window);
                if window_state_atoms.is_empty() {
                    if let Err(error) = delete_property(connection, *x_window_id, atoms.net_wm_state) {
                        if is_bad_window_reply_error(&error) {
                            warn!(window = *x_window_id, ?error, "skipping X11 EWMH state delete for invalid window");
                            continue;
                        }
                        return Err(error);
                    }
                } else {
                    if let Err(error) = change_root_property32(
                        connection,
                        *x_window_id,
                        atoms.net_wm_state,
                        AtomEnum::ATOM.into(),
                        &window_state_atoms,
                    ) {
                        if is_bad_window_reply_error(&error) {
                            warn!(window = *x_window_id, ?error, "skipping X11 EWMH state update for invalid window");
                            continue;
                        }
                        return Err(error);
                    }
                }
            }
        }

        connection.flush().context("failed to flush EWMH state publication")?;
        Ok(())
    }
}

fn create_ewmh_support_window(connection: &XCBConnection, screen: &ScreenDescriptor) -> Result<Window> {
    let window = connection.generate_id()?;
    connection
        .create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            screen.root_window,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            COPY_FROM_PARENT,
            &CreateWindowAux::new().override_redirect(1),
        )?
        .check()
        .context("failed to create EWMH support window")?;
    connection.flush().context("failed to flush EWMH support window creation")?;
    Ok(window)
}

fn supported_atoms(atoms: Atoms) -> Vec<Atom> {
    [
        atoms.net_supported,
        atoms.net_supporting_wm_check,
        atoms.net_active_window,
        atoms.net_client_list,
        atoms.net_client_list_stacking,
        atoms.net_current_desktop,
        atoms.net_number_of_desktops,
        atoms.net_desktop_names,
        atoms.net_wm_desktop,
        atoms.net_wm_state,
        atoms.net_wm_state_fullscreen,
        atoms.net_wm_state_hidden,
        atoms.net_wm_state_focused,
        atoms.net_close_window,
        atoms.net_moveresize_window,
        atoms.net_restack_window,
        atoms.net_workarea,
    ]
    .into_iter()
    .filter(|atom| *atom != u32::from(AtomEnum::NONE))
    .collect()
}

fn change_root_property32(
    connection: &XCBConnection,
    window: Window,
    property: Atom,
    property_type: Atom,
    data: &[u32],
) -> Result<()> {
    if property == u32::from(AtomEnum::NONE) {
        return Ok(());
    }

    connection
        .change_property32(PropMode::REPLACE, window, property, property_type, data)?
        .check()
        .context("failed to publish X11 root property")?;
    Ok(())
}

fn is_bad_window_reply_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<x11rb::errors::ReplyError>()
        .is_some_and(|reply_error| {
            matches!(
                reply_error,
                x11rb::errors::ReplyError::X11Error(x11_error)
                    if x11_error.error_kind == x11rb::protocol::ErrorKind::Window
            )
        })
}

fn delete_property(connection: &XCBConnection, window: Window, property: Atom) -> Result<()> {
    if property == u32::from(AtomEnum::NONE) {
        return Ok(());
    }

    connection.delete_property(window, property)?.check().context("failed to delete X11 property")?;
    Ok(())
}

fn desktop_index_for_workspace(
    snapshot: &StateSnapshot,
    workspace_id: Option<&spiders_core::WorkspaceId>,
) -> Option<u32> {
    workspace_id.and_then(|workspace_id| {
        snapshot
            .workspaces
            .iter()
            .position(|workspace| &workspace.id == workspace_id)
            .map(|index| index as u32)
    })
}

fn stack_mode_from_ewmh(detail: u32) -> StackMode {
    match detail {
        1 => StackMode::BELOW,
        2 => StackMode::TOP_IF,
        3 => StackMode::BOTTOM_IF,
        4 => StackMode::OPPOSITE,
        _ => StackMode::ABOVE,
    }
}

fn should_window_be_visible(
    snapshot: &StateSnapshot,
    window: &spiders_core::snapshot::WindowSnapshot,
) -> bool {
    window.workspace_id.as_ref().is_some_and(|workspace_id| {
        snapshot
            .workspaces
            .iter()
            .any(|workspace| workspace.id == *workspace_id && workspace.visible)
    })
}

fn output_geometry(output: &spiders_core::snapshot::OutputSnapshot) -> WindowGeometry {
    WindowGeometry {
        x: output.logical_x,
        y: output.logical_y,
        width: output.logical_width as i32,
        height: output.logical_height as i32,
    }
}

fn collect_focus_tree_entries(
    node: &spiders_wm_runtime::PreviewSnapshotNode,
    x_offset: i32,
    y_offset: i32,
    out: &mut Vec<spiders_core::focus::FocusTreeWindowGeometry>,
) {
    if node.node_type == "window"
        && let (Some(window_id), Some(rect)) = (node.window_id.as_ref(), node.rect)
    {
        out.push(spiders_core::focus::FocusTreeWindowGeometry {
            window_id: window_id.clone(),
            geometry: WindowGeometry {
                x: rect.x.round() as i32 + x_offset,
                y: rect.y.round() as i32 + y_offset,
                width: rect.width.round() as i32,
                height: rect.height.round() as i32,
            },
        });
    }

    for child in &node.children {
        collect_focus_tree_entries(child, x_offset, y_offset, out);
    }
}

fn navigation_direction(direction: spiders_core::command::FocusDirection) -> spiders_core::navigation::NavigationDirection {
    match direction {
        spiders_core::command::FocusDirection::Left => spiders_core::navigation::NavigationDirection::Left,
        spiders_core::command::FocusDirection::Right => spiders_core::navigation::NavigationDirection::Right,
        spiders_core::command::FocusDirection::Up => spiders_core::navigation::NavigationDirection::Up,
        spiders_core::command::FocusDirection::Down => spiders_core::navigation::NavigationDirection::Down,
    }
}

fn workareas_for_snapshot(snapshot: &StateSnapshot) -> Vec<u32> {
    snapshot
        .workspaces
        .iter()
        .flat_map(|workspace| {
            let output = workspace
                .output_id
                .as_ref()
                .and_then(|output_id| snapshot.output_by_id(output_id))
                .or_else(|| snapshot.current_output());

            let x = output.map(|output| output.logical_x.max(0) as u32).unwrap_or(0);
            let y = output.map(|output| output.logical_y.max(0) as u32).unwrap_or(0);
            let width = output.map(|output| output.logical_width).unwrap_or(0);
            let height = output.map(|output| output.logical_height).unwrap_or(0);

            [x, y, width, height]
        })
        .collect()
}

fn ewmh_window_state_atoms(
    atoms: Atoms,
    window: &spiders_core::snapshot::WindowSnapshot,
) -> Vec<Atom> {
    let mut state = Vec::new();

    if window.mode.is_fullscreen() && atoms.net_wm_state_fullscreen != u32::from(AtomEnum::NONE) {
        state.push(atoms.net_wm_state_fullscreen);
    }
    if !window.mapped && atoms.net_wm_state_hidden != u32::from(AtomEnum::NONE) {
        state.push(atoms.net_wm_state_hidden);
    }
    if window.focused && atoms.net_wm_state_focused != u32::from(AtomEnum::NONE) {
        state.push(atoms.net_wm_state_focused);
    }

    state
}

fn attach_workspaces_to_outputs(model: &mut WmModel, outputs: &[DiscoveredOutput]) {
    if outputs.is_empty() {
        return;
    }

    let output_ids = outputs.iter().map(|output| output.output_id.clone()).collect::<Vec<_>>();
    let workspace_ids = model.workspaces.keys().cloned().collect::<Vec<_>>();

    for (index, workspace_id) in workspace_ids.into_iter().enumerate() {
        let output_id = output_ids[index % output_ids.len()].clone();
        model.attach_workspace_to_output(workspace_id.clone(), output_id.clone());
        model.outputs.entry(output_id.clone()).and_modify(|output| {
            if output.focused_workspace_id.is_none() {
                output.focused_workspace_id = Some(workspace_id.clone());
            }
        });
    }
}

fn preserve_workspace_output_attachments(
    model: &mut WmModel,
    outputs: &[DiscoveredOutput],
    previous_workspace_by_output_name: &BTreeMap<String, WorkspaceId>,
) {
    if outputs.is_empty() {
        return;
    }

    let next_output_ids = outputs.iter().map(|output| output.output_id.clone()).collect::<BTreeSet<_>>();

    for workspace in model.workspaces.values_mut() {
        if workspace.output_id.as_ref().is_some_and(|output_id| !next_output_ids.contains(output_id)) {
            workspace.output_id = None;
            workspace.visible = false;
            workspace.focused = false;
        }
    }

    let workspace_ids = model.workspaces.keys().cloned().collect::<Vec<_>>();
    let mut used_workspace_ids = BTreeSet::new();

    for output in outputs {
        if let Some(workspace_id) = previous_workspace_by_output_name.get(&output.name)
            && model.workspaces.contains_key(workspace_id)
        {
            model.attach_workspace_to_output(workspace_id.clone(), output.output_id.clone());
            model.outputs.entry(output.output_id.clone()).and_modify(|model_output| {
                model_output.focused_workspace_id = Some(workspace_id.clone());
            });
            used_workspace_ids.insert(workspace_id.clone());
        }
    }

    let mut available_workspace_ids = workspace_ids
        .into_iter()
        .filter(|workspace_id| !used_workspace_ids.contains(workspace_id))
        .collect::<Vec<_>>()
        .into_iter();

    for output in outputs {
        let already_attached = model
            .workspaces
            .values()
            .any(|workspace| workspace.output_id.as_ref() == Some(&output.output_id));
        if already_attached {
            continue;
        }

        let Some(workspace_id) = available_workspace_ids.next() else {
            continue;
        };
        model.attach_workspace_to_output(workspace_id.clone(), output.output_id.clone());
        model.outputs.entry(output.output_id.clone()).and_modify(|model_output| {
            if model_output.focused_workspace_id.is_none() {
                model_output.focused_workspace_id = Some(workspace_id.clone());
            }
        });
    }
}

fn install_managed_window_event_mask<C: Connection>(connection: &C, window: Window) -> Result<()> {
    connection
        .change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().event_mask(managed_window_event_mask()),
        )?
        .check()
        .context("failed to install managed X11 window event mask")?;
    connection.flush().context("failed to flush managed X11 window event mask")?;
    Ok(())
}

fn managed_window_event_mask() -> EventMask {
    EventMask::PROPERTY_CHANGE | EventMask::FOCUS_CHANGE | EventMask::STRUCTURE_NOTIFY
}

fn raise_and_focus_window<C: Connection>(connection: &C, window: Window) -> Result<()> {
    info!(window, "wm-x raise and focus start");
    connection
        .configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?
        .check()
        .context("failed to raise managed X11 window")?;
    if let Err(error) = connection
        .set_input_focus(InputFocus::POINTER_ROOT, window, CURRENT_TIME)?
        .check()
    {
        let is_match = matches!(
            &error,
            x11rb::errors::ReplyError::X11Error(x11_error)
                if x11_error.error_kind == x11rb::protocol::ErrorKind::Match
        );
        if is_match {
            warn!(window, ?error, "skipping X11 focus for window that is not viewable yet");
        } else {
            return Err(error).context("failed to focus managed X11 window");
        }
    }
    connection.flush().context("failed to flush managed X11 raise/focus")?;
    info!(window, "wm-x raise and focus finished");
    Ok(())
}

struct NoopHost;

impl WmHost for NoopHost {
    fn on_effect(&mut self, _effect: spiders_core::effect::WmHostEffect) -> PreviewRenderAction {
        PreviewRenderAction::None
    }
}

struct BackendManageHandler<'a> {
    atoms: Atoms,
    state: &'a mut RuntimeState,
    ipc_listener: Option<&'a UnixListener>,
}

impl ManageEventHandler for BackendManageHandler<'_> {
    fn should_exit(&self) -> bool {
        self.state.quit_requested
    }

    fn on_ipc_listener_ready(
        &mut self,
        handle: &calloop::LoopHandle<'_, ManageLoopDispatchState>,
    ) -> Result<()> {
        let Some(listener) = self.ipc_listener else {
            return Ok(());
        };

        for (client_id, stream) in accept_pending_ipc_clients(&mut self.state.ipc, listener) {
            register_ipc_client_source(handle, client_id, &stream)?;
        }

        Ok(())
    }

    fn on_ipc_client_ready(
        &mut self,
        connection: &XCBConnection,
        _handle: &calloop::LoopHandle<'_, ManageLoopDispatchState>,
        client_id: spiders_ipc::IpcClientId,
    ) -> Result<()> {
        let mut ipc = std::mem::take(&mut self.state.ipc);
        let result = {
            let mut handler = X11IpcHandler { backend: self, connection };
            spiders_ipc_native::serve_ipc_client_once(&mut ipc, client_id, &mut handler)
        };
        self.state.ipc = ipc;

        match result {
            Ok(_) => Ok(()),
            Err(NativeIpcServeError::Transport(IpcTransportError::Io(error)))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                self.state.ipc.remove_client(client_id);
                Ok(())
            }
            Err(NativeIpcServeError::Transport(IpcTransportError::Codec(
                spiders_ipc::IpcCodecError::EmptyFrame,
            ))) => {
                self.state.ipc.remove_client(client_id);
                Ok(())
            }
            Err(NativeIpcServeError::UnknownClient(_)) => Ok(()),
            Err(NativeIpcServeError::Transport(IpcTransportError::Codec(error))) => {
                warn!(client_id, %error, "discarding malformed wm-x IPC request");
                let response = self
                    .state
                    .ipc
                    .server
                    .error_response(client_id, None, error.to_string())
                    .map_err(anyhow::Error::from)?;
                if let Some(stream) = self.state.ipc.clients.get_mut(&client_id)
                    && let Err(error) = send_response(stream, &response)
                {
                    warn!(client_id, %error, "failed to send wm-x IPC error response");
                    self.state.ipc.remove_client(client_id);
                }
                Ok(())
            }
            Err(NativeIpcServeError::Transport(IpcTransportError::Io(error))) => {
                Err(error).context("failed reading wm-x IPC request")
            }
            Err(NativeIpcServeError::Handler(error)) => Err(anyhow::Error::new(error)),
        }
    }

    fn after_dispatch(&mut self, _connection: &XCBConnection) -> Result<()> {
        for event in self.state.take_pending_events() {
            self.state.ipc.broadcast_event(event);
        }

        Ok(())
    }

    fn on_map_request(&mut self, connection: &XCBConnection, window: Window) -> Result<()> {
        if let Some(discovered) = discover_window_for_event(connection, &self.atoms, window)? {
            install_managed_window_event_mask(connection, window)?;
            self.state.ensure_runtime_window(&discovered);
            self.state.sync_window_identity(&discovered);
            self.state.sync_window_mapped(discovered.window_id.clone(), true);
            self.state.focus_window(Some(discovered.window_id));

            connection.map_window(window)?.check().context("failed to map X11 window after map request")?;
            self.apply_shared_layout(connection)?;
            raise_and_focus_window(connection, window)?;
            self.state.publish_ewmh_state(connection, self.atoms)?;
            connection.flush().context("failed to flush X11 map request handling")?;
            return Ok(());
        }

        connection.map_window(window)?.check().context("failed to map X11 window after map request")?;
        connection.flush().context("failed to flush X11 map request handling")?;
        Ok(())
    }

    fn on_configure_request(
        &mut self,
        connection: &XCBConnection,
        event: &x11rb::protocol::xproto::ConfigureRequestEvent,
    ) -> Result<()> {
        let mut aux = ConfigureWindowAux::new();

        if event.value_mask.contains(x11rb::protocol::xproto::ConfigWindow::X) {
            aux = aux.x(i32::from(event.x));
        }
        if event.value_mask.contains(x11rb::protocol::xproto::ConfigWindow::Y) {
            aux = aux.y(i32::from(event.y));
        }
        if event.value_mask.contains(x11rb::protocol::xproto::ConfigWindow::WIDTH) {
            aux = aux.width(u32::from(event.width));
        }
        if event.value_mask.contains(x11rb::protocol::xproto::ConfigWindow::HEIGHT) {
            aux = aux.height(u32::from(event.height));
        }
        if event.value_mask.contains(x11rb::protocol::xproto::ConfigWindow::BORDER_WIDTH) {
            aux = aux.border_width(u32::from(event.border_width));
        }
        if event.value_mask.contains(x11rb::protocol::xproto::ConfigWindow::SIBLING) {
            aux = aux.sibling(event.sibling);
        }
        if event.value_mask.contains(x11rb::protocol::xproto::ConfigWindow::STACK_MODE) {
            aux = aux.stack_mode(event.stack_mode);
        }

        connection.configure_window(event.window, &aux)?.check().context("failed to apply X11 configure request")?;
        connection.flush().context("failed to flush X11 configure request")?;

        if let Some(window_id) = self.state.window_id_for_x_window(event.window) {
            let geometry = WindowGeometry {
                x: i32::from(event.x),
                y: i32::from(event.y),
                width: i32::from(event.width),
                height: i32::from(event.height),
            };
            self.state.set_window_floating_geometry(window_id, geometry);
        }

        Ok(())
    }

    fn on_unmap_notify(&mut self, connection: &XCBConnection, window: Window) -> Result<()> {
        if let Some(window_id) = self.state.window_id_for_x_window(window) {
            self.state.unmap_window(window_id);
            self.apply_shared_layout(connection)?;
            self.state.publish_ewmh_state(connection, self.atoms)?;
        }

        Ok(())
    }

    fn on_destroy_notify(&mut self, connection: &XCBConnection, window: Window) -> Result<()> {
        self.state.remove_window(window);
        self.apply_shared_layout(connection)?;
        self.state.publish_ewmh_state(connection, self.atoms)?;
        Ok(())
    }

    fn on_configure_notify(&mut self, event: &x11rb::protocol::xproto::ConfigureNotifyEvent) {
        let geometry = WindowGeometry {
            x: i32::from(event.x),
            y: i32::from(event.y),
            width: i32::from(event.width),
            height: i32::from(event.height),
        };
        self.state.sync_actual_window_geometry(event.window, geometry);
    }

    fn on_property_notify(
        &mut self,
        connection: &XCBConnection,
        window: Window,
        atom: u32,
    ) -> Result<()> {
        if atom != self.atoms.wm_name && atom != self.atoms.net_wm_name && atom != self.atoms.wm_class {
            return Ok(());
        }

        if let Some(discovered) = discover_window_for_event(connection, &self.atoms, window)? {
            install_managed_window_event_mask(connection, window)?;
            self.state.ensure_runtime_window(&discovered);
            self.state.sync_window_identity(&discovered);
            self.apply_shared_layout(connection)?;
            self.state.publish_ewmh_state(connection, self.atoms)?;
        }

        Ok(())
    }

    fn on_focus_in(
        &mut self,
        connection: &XCBConnection,
        event: &x11rb::protocol::xproto::FocusInEvent,
    ) -> Result<()> {
        if event.mode != x11rb::protocol::xproto::NotifyMode::NORMAL {
            info!(event = event.event, mode = ?event.mode, detail = ?event.detail, "wm-x ignoring transient focus-in for X11 grab transition");
            return Ok(());
        }

        let focused_window_id = self.state.window_id_for_x_window(event.event);
        if focused_window_id.is_none() {
            info!(event = event.event, mode = ?event.mode, detail = ?event.detail, "wm-x ignoring focus-in for unmanaged X11 window");
            return Ok(());
        }
        self.state.focus_window(focused_window_id);
        self.state.publish_ewmh_state(connection, self.atoms)?;
        Ok(())
    }

    fn on_key_press(
        &mut self,
        connection: &XCBConnection,
        event: &x11rb::protocol::xproto::KeyPressEvent,
    ) -> Result<()> {
        let Some(binding) = binding_for_key_event(&mut self.state.keyboard_bindings, event) else {
            return Ok(());
        };

        info!(command = ?binding, "wm-x executing key binding command");

        self.execute_command(connection, binding)
    }

    fn on_client_message(
        &mut self,
        connection: &XCBConnection,
        window: Window,
        type_atom: Atom,
        data: &ClientMessageData,
    ) -> Result<()> {
        let payload = data.as_data32();

        if type_atom == self.atoms.net_close_window {
            if let Some(window_id) = self.state.window_id_for_x_window(window) {
                self.request_close_window(connection, window, window_id)?;
                self.state.publish_ewmh_state(connection, self.atoms)?;
            }
        } else if type_atom == self.atoms.net_wm_desktop {
            let requested = payload[0];
            if self.state.move_window_to_workspace_index(window, requested) {
                self.apply_shared_layout(connection)?;
                self.state.publish_ewmh_state(connection, self.atoms)?;
            }
        } else if type_atom == self.atoms.net_wm_state {
            let action = payload[0];
            let first = payload[1];
            let second = payload[2];

            let mut changed = false;
            changed |= self.apply_net_wm_state_action(window, action, first);
            if second != u32::from(AtomEnum::NONE) {
                changed |= self.apply_net_wm_state_action(window, action, second);
            }

            if changed {
                self.apply_shared_layout(connection)?;
                self.state.publish_ewmh_state(connection, self.atoms)?;
            }
        } else if type_atom == self.atoms.net_active_window {
            if self.state.activate_x_window(window).is_some() {
                raise_and_focus_window(connection, window)?;
                self.state.publish_ewmh_state(connection, self.atoms)?;
            }
        } else if type_atom == self.atoms.net_current_desktop {
            let requested = payload[0];
            if self.state.switch_to_workspace_index(requested) {
                self.apply_shared_layout(connection)?;
                self.state.publish_ewmh_state(connection, self.atoms)?;
            }
        } else if type_atom == self.atoms.net_restack_window {
            let sibling = payload[1];
            let detail = stack_mode_from_ewmh(payload[2]);

            connection
                .configure_window(window, &ConfigureWindowAux::new().sibling(sibling).stack_mode(detail))?
                .check()
                .context("failed to apply _NET_RESTACK_WINDOW")?;
            connection.flush().context("failed to flush _NET_RESTACK_WINDOW")?;

            if self.state.restack_x_window(window, detail) {
                self.state.publish_ewmh_state(connection, self.atoms)?;
            }
        } else if type_atom == self.atoms.net_moveresize_window {
            let flags = payload[0];
            let mut aux = ConfigureWindowAux::new();

            if flags & (1 << 8) != 0 {
                aux = aux.x(payload[1] as i32);
            }
            if flags & (1 << 9) != 0 {
                aux = aux.y(payload[2] as i32);
            }
            if flags & (1 << 10) != 0 {
                aux = aux.width(payload[3].max(1));
            }
            if flags & (1 << 11) != 0 {
                aux = aux.height(payload[4].max(1));
            }

            connection
                .configure_window(window, &aux)?
                .check()
                .context("failed to apply _NET_MOVERESIZE_WINDOW")?;
            connection.flush().context("failed to flush _NET_MOVERESIZE_WINDOW")?;

            if let Some(window_id) = self.state.window_id_for_x_window(window) {
                let geometry = WindowGeometry {
                    x: payload[1] as i32,
                    y: payload[2] as i32,
                    width: payload[3] as i32,
                    height: payload[4] as i32,
                };
                self.state.set_window_floating_geometry(window_id, geometry);
            }
        }

        Ok(())
    }

    fn on_randr_notify(
        &mut self,
        connection: &XCBConnection,
        _event: &randr::NotifyEvent,
    ) -> Result<()> {
        self.state.refresh_outputs(connection)?;
        self.apply_shared_layout(connection)?;
        self.state.publish_ewmh_state(connection, self.atoms)?;
        Ok(())
    }

    fn on_randr_screen_change(
        &mut self,
        connection: &XCBConnection,
        _event: &randr::ScreenChangeNotifyEvent,
    ) -> Result<()> {
        self.state.refresh_outputs(connection)?;
        self.apply_shared_layout(connection)?;
        self.state.publish_ewmh_state(connection, self.atoms)?;
        Ok(())
    }
}

struct X11IpcHandler<'a, 'b> {
    backend: &'a mut BackendManageHandler<'b>,
    connection: &'a XCBConnection,
}

impl IpcHandler for X11IpcHandler<'_, '_> {
    type Error = std::io::Error;

    fn handle_query(&mut self, query: QueryRequest) -> Result<QueryResponse, Self::Error> {
        Ok(self.backend.state.query(query))
    }

    fn handle_command(
        &mut self,
        command: spiders_core::command::WmCommand,
    ) -> Result<(), Self::Error> {
        self.backend.execute_command(self.connection, command).map_err(std::io::Error::other)
    }

    fn handle_debug(&mut self, request: DebugRequest) -> Result<spiders_ipc::DebugResponse, Self::Error> {
        match request {
            DebugRequest::Dump { kind } => {
                let state_json = serde_json::to_string_pretty(&self.backend.state.snapshot())?;
                handle_debug_dump(kind, &state_json).map_err(std::io::Error::other)
            }
        }
    }
}

impl BackendManageHandler<'_> {
    fn execute_command(&mut self, connection: &XCBConnection, command: spiders_core::command::WmCommand) -> Result<()> {
        let root_window = self.state.screen.root_window;
        let post_actions = {
            let mut host = X11CommandHost {
                state: self.state,
                connection,
                relayout_needed: false,
                rebind_needed: false,
                previous_bindings: None,
                publish_ewmh_needed: false,
                focused_window: None,
                close_request: None,
            };

            dispatch_wm_command(&mut host, command);
            PostCommandActions {
                relayout_needed: host.relayout_needed,
                rebind_needed: host.rebind_needed,
                previous_bindings: host.previous_bindings,
                publish_ewmh_needed: host.publish_ewmh_needed,
                focused_window: host.focused_window,
                close_request: host.close_request,
            }
        };

        let mut publish_ewmh_needed = post_actions.publish_ewmh_needed;

        if let Some((window, window_id)) = post_actions.close_request {
            self.request_close_window(connection, window, window_id)?;
            publish_ewmh_needed = true;
        }
        if let Some(window) = post_actions.focused_window {
            raise_and_focus_window(connection, window)?;
            publish_ewmh_needed = true;
        }
        if post_actions.relayout_needed {
            self.apply_shared_layout(connection)?;
            publish_ewmh_needed = true;
        }
        if post_actions.rebind_needed {
            if let Some(previous_bindings) = post_actions.previous_bindings.as_ref() {
                uninstall_key_grabs(connection, root_window, previous_bindings)?;
            }
            install_key_grabs(connection, root_window, &self.state.keyboard_bindings.installed)?;
        }
        if publish_ewmh_needed {
            self.state.publish_ewmh_state(connection, self.atoms)?;
        }

        Ok(())
    }

    fn apply_workspace_visibility(&mut self, connection: &XCBConnection) -> Result<()> {
        let snapshot = self.state.snapshot();
        let windows = self
            .state
            .x_windows
            .iter()
            .map(|(x_window_id, window_id)| (*x_window_id, window_id.clone()))
            .collect::<Vec<_>>();
        let mut changed = false;

        for (x_window_id, window_id) in windows {
            let Some(snapshot_window) =
                snapshot.windows.iter().find(|snapshot_window| snapshot_window.id == window_id)
            else {
                continue;
            };

            let should_be_visible = should_window_be_visible(&snapshot, snapshot_window);

            if should_be_visible {
                if self.state.workspace_hidden_windows.remove(&x_window_id) {
                    connection
                        .map_window(x_window_id)?
                        .check()
                        .context("failed to remap X11 window for visible workspace")?;
                    self.state.sync_window_mapped(window_id, true);
                    changed = true;
                }
                continue;
            }

            if snapshot_window.mapped && !self.state.workspace_hidden_windows.contains(&x_window_id) {
                self.state.workspace_hidden_windows.insert(x_window_id);
                connection
                    .unmap_window(x_window_id)?
                    .check()
                    .context("failed to unmap X11 window for hidden workspace")?;
                self.state.sync_window_mapped(window_id, false);
                changed = true;
            }
        }

        if changed {
            connection.flush().context("failed to flush X11 workspace visibility changes")?;
        }

        Ok(())
    }

    fn apply_net_wm_state_action(&mut self, window: Window, action: u32, atom: Atom) -> bool {
        const REMOVE: u32 = 0;
        const ADD: u32 = 1;
        const TOGGLE: u32 = 2;

        if atom == self.atoms.net_wm_state_fullscreen {
            let current = self
                .state
                .window_id_for_x_window(window)
                .and_then(|id| self.state.model.windows.get(&id))
                .is_some_and(|window| window.fullscreen);
            let next = match action {
                REMOVE => false,
                ADD => true,
                TOGGLE => !current,
                _ => current,
            };
            return self.state.set_window_fullscreen_for_x_window(window, next);
        }

        false
    }

    fn apply_shared_layout(&mut self, connection: &XCBConnection) -> Result<()> {
        self.apply_workspace_visibility(connection)?;
        let geometries = self.state.current_layout_geometries()?;
        let snapshot = self.state.snapshot();

        for (window, geometry) in geometries.iter().copied() {
            let is_fullscreen = self
                .state
                .window_id_for_x_window(window)
                .and_then(|window_id| snapshot.windows.iter().find(|candidate| candidate.id == window_id))
                .is_some_and(|window| window.mode.is_fullscreen());
            let mut aux = ConfigureWindowAux::new()
                .x(geometry.x)
                .y(geometry.y)
                .width(geometry.width.max(1) as u32)
                .height(geometry.height.max(1) as u32);
            if is_fullscreen {
                aux = aux.stack_mode(StackMode::ABOVE);
            }

            if let Err(error) = connection
                .configure_window(window, &aux)?
                .check()
                .context("failed to apply shared layout geometry to X11 window")
            {
                if is_bad_window_reply_error(&error) {
                    warn!(window, ?error, "skipping shared X11 layout geometry update for invalid window");
                    continue;
                }
                return Err(error);
            }
        }

        if !geometries.is_empty() {
            connection.flush().context("failed to flush shared X11 layout application")?;
        }

        Ok(())
    }

    fn request_close_window(
        &mut self,
        connection: &XCBConnection,
        window: Window,
        window_id: WindowId,
    ) -> Result<()> {
        if supports_wm_delete_window(connection, self.atoms, window)? {
            let data = ClientMessageData::from([
                self.atoms.wm_delete_window,
                CURRENT_TIME,
                0,
                0,
                0,
            ]);
            let event = ClientMessageEvent {
                response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
                format: 32,
                sequence: 0,
                window,
                type_: self.atoms.wm_protocols,
                data,
            };
            connection
                .send_event(false, window, EventMask::NO_EVENT, event)?
                .check()
                .context("failed to send WM_DELETE_WINDOW to X11 client")?;
            connection.flush().context("failed to flush X11 close request")?;
        } else {
            connection
                .kill_client(window)?
                .check()
                .context("failed to kill X11 client without WM_DELETE_WINDOW support")?;
            connection.flush().context("failed to flush X11 client kill request")?;
        }

        self.state.model.set_window_closing(window_id, true);
        Ok(())
    }
}

fn supports_wm_delete_window(
    connection: &XCBConnection,
    atoms: Atoms,
    window: Window,
) -> Result<bool> {
    if atoms.wm_protocols == u32::from(AtomEnum::NONE) || atoms.wm_delete_window == u32::from(AtomEnum::NONE) {
        return Ok(false);
    }

    let reply = match connection
        .get_property(false, window, atoms.wm_protocols, AtomEnum::ATOM, 0, 32)?
        .reply()
    {
        Ok(reply) => reply,
        Err(_) => return Ok(false),
    };

    Ok(reply.value32().is_some_and(|values| values.into_iter().any(|atom| atom == atoms.wm_delete_window)))
}

struct X11CommandHost<'a> {
    state: &'a mut RuntimeState,
    connection: &'a XCBConnection,
    relayout_needed: bool,
    rebind_needed: bool,
    previous_bindings: Option<Vec<input::InstalledBinding>>,
    publish_ewmh_needed: bool,
    focused_window: Option<Window>,
    close_request: Option<(Window, WindowId)>,
}

struct PostCommandActions {
    relayout_needed: bool,
    rebind_needed: bool,
    previous_bindings: Option<Vec<input::InstalledBinding>>,
    publish_ewmh_needed: bool,
    focused_window: Option<Window>,
    close_request: Option<(Window, WindowId)>,
}

impl WmHost for X11CommandHost<'_> {
    fn on_effect(&mut self, effect: WmHostEffect) -> PreviewRenderAction {
        info!(?effect, "wm-x host effect received");
        match effect {
            WmHostEffect::SpawnCommand { command } => {
                if let Err(error) = x11_spawn_command(&self.state.display_name, &command) {
                    warn!(%command, ?error, "failed to spawn X11 wm command");
                }
            }
            WmHostEffect::RequestQuit => {
                self.state.quit_requested = true;
                info!("spiders-wm-x received quit request");
            }
            WmHostEffect::ActivateWorkspace { target } => {
                let changed = match target {
                    WorkspaceTarget::Named(name) => self.state.select_named_workspace(name),
                    WorkspaceTarget::Next => self.state.select_next_workspace(),
                    WorkspaceTarget::Previous => self.state.select_previous_workspace(),
                };
                self.relayout_needed |= changed;
                self.publish_ewmh_needed |= changed;
            }
            WmHostEffect::AssignFocusedWindowToWorkspace { assignment } => {
                let changed = match assignment {
                    WorkspaceAssignment::Move(workspace) => self.state.assign_focused_window_to_workspace(workspace, false),
                    WorkspaceAssignment::Toggle(workspace) => self.state.assign_focused_window_to_workspace(workspace, true),
                };
                self.relayout_needed |= changed;
                self.publish_ewmh_needed |= changed;
            }
            WmHostEffect::FocusWindow { target } => {
                let focused_window_id = match target {
                    FocusTarget::Next => self.state.focus_next_window(),
                    FocusTarget::Previous => self.state.focus_previous_window(),
                    FocusTarget::Window(window_id) => {
                        self.state.focus_window(Some(window_id.clone()));
                        Some(window_id)
                    }
                    FocusTarget::Direction(direction) => {
                        self.state.focus_direction_window(direction)
                    }
                };
                if focused_window_id.is_some() {
                    self.focused_window = self.state.focused_x_window();
                    self.publish_ewmh_needed = true;
                }
            }
            WmHostEffect::CloseFocusedWindow => {
                self.close_request = self.state.request_close_focused_window();
            }
            WmHostEffect::ReloadConfig => {
                let previous_bindings = self.state.keyboard_bindings.installed.clone();
                match self.state.reload_config(self.connection) {
                    Ok(changed) => {
                        self.relayout_needed |= changed;
                        self.rebind_needed |= changed;
                        self.previous_bindings = Some(previous_bindings);
                        self.publish_ewmh_needed |= changed;
                        if changed {
                            self.state.pending_events.push(WmEvent::ConfigReloaded);
                        }
                    }
                    Err(error) => {
                        warn!(?error, "spiders-wm-x failed to reload config");
                    }
                }
            }
            WmHostEffect::ToggleFocusedWindow { toggle } => {
                let changed = match toggle {
                    WindowToggle::Floating => self.state.toggle_focused_window_floating(),
                    WindowToggle::Fullscreen => self.state.toggle_focused_window_fullscreen(),
                };
                self.relayout_needed |= changed;
                self.publish_ewmh_needed |= changed;
            }
            WmHostEffect::SwapFocusedWindow { direction } => {
                let changed = self.state.swap_focused_window_direction(direction);
                self.relayout_needed |= changed;
                self.publish_ewmh_needed |= changed;
            }
            WmHostEffect::SetLayout { name } => {
                let mut runtime = WmRuntime::new(&mut self.state.model);
                let changed = runtime.set_current_workspace_layout(name).is_some();
                self.state.pending_events.extend(runtime.take_events());
                self.relayout_needed |= changed;
                self.publish_ewmh_needed |= changed;
            }
            WmHostEffect::CycleLayout { direction } => {
                let config = self.state.config.clone();
                let mut runtime = WmRuntime::new(&mut self.state.model);
                let changed = runtime.cycle_current_workspace_layout(&config, direction).is_some();
                self.state.pending_events.extend(runtime.take_events());
                self.relayout_needed |= changed;
                self.publish_ewmh_needed |= changed;
            }
        }
        PreviewRenderAction::None
    }
}

fn x11_spawn_command(display_name: &str, command: &str) -> std::io::Result<std::process::Child> {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .env("DISPLAY", display_name)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("SWAYSOCK")
        .env("XDG_SESSION_TYPE", "x11")
        .spawn()
}

#[cfg(test)]
mod tests {
    use super::{
        X11CommandHost, RuntimeState, ScreenDescriptor, output_geometry,
        preserve_workspace_output_attachments, should_window_be_visible,
    };
    use spiders_core::effect::WmHostEffect;
    use spiders_config::model::Config;
    use spiders_core::focus::{FocusTree, FocusTreeWindowGeometry};
    use spiders_core::snapshot::{OutputSnapshot, StateSnapshot, WindowSnapshot, WorkspaceSnapshot};
    use spiders_core::types::{LayoutRef, OutputTransform, ShellKind, WindowMode};
    use spiders_core::wm::WindowGeometry;
    use spiders_core::{OutputId, WindowId, WorkspaceId};
    use spiders_wm_runtime::WmHost;

    use crate::backend::input::KeyboardBindings;
    use x11rb::xcb_ffi::XCBConnection;

    #[test]
    fn request_quit_sets_quit_flag() {
        let mut state = test_runtime_state();
        let (connection, _) = XCBConnection::connect(None).expect("x11 connection");
        let mut host = X11CommandHost {
            state: &mut state,
            connection: &connection,
            relayout_needed: false,
            rebind_needed: false,
            previous_bindings: None,
            publish_ewmh_needed: false,
            focused_window: None,
            close_request: None,
        };

        host.on_effect(WmHostEffect::RequestQuit);

        assert!(host.state.quit_requested);
    }

    #[test]
    fn preserve_workspace_output_attachments_keeps_named_output_assignments() {
        let mut model = test_runtime_state().model;
        model.attach_workspace_to_output(WorkspaceId::from("1"), OutputId::from("out-1"));
        model.attach_workspace_to_output(WorkspaceId::from("2"), OutputId::from("out-2"));
        model.outputs.entry(OutputId::from("out-1")).and_modify(|output| {
            output.name = "HDMI-1".into();
            output.focused_workspace_id = Some(WorkspaceId::from("1"));
        });
        model.outputs.entry(OutputId::from("out-2")).and_modify(|output| {
            output.name = "DP-1".into();
            output.focused_workspace_id = Some(WorkspaceId::from("2"));
        });

        preserve_workspace_output_attachments(
            &mut model,
            &[
                super::output::DiscoveredOutput {
                    output_id: OutputId::from("out-2b"),
                    name: "DP-1".into(),
                    x: 1920,
                    y: 0,
                    width: 1920,
                    height: 1080,
                    primary: false,
                },
                super::output::DiscoveredOutput {
                    output_id: OutputId::from("out-1b"),
                    name: "HDMI-1".into(),
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                    primary: true,
                },
            ],
            &std::collections::BTreeMap::from([
                ("HDMI-1".to_string(), WorkspaceId::from("1")),
                ("DP-1".to_string(), WorkspaceId::from("2")),
            ]),
        );

        assert_eq!(
            model.workspaces.get(&WorkspaceId::from("1")).and_then(|workspace| workspace.output_id.clone()),
            Some(OutputId::from("out-1b"))
        );
        assert_eq!(
            model.workspaces.get(&WorkspaceId::from("2")).and_then(|workspace| workspace.output_id.clone()),
            Some(OutputId::from("out-2b"))
        );
    }

    #[test]
    fn preserve_workspace_output_attachments_assigns_new_output_to_unattached_workspace() {
        let mut model = test_runtime_state().model;
        model.attach_workspace_to_output(WorkspaceId::from("1"), OutputId::from("out-1"));
        model.outputs.entry(OutputId::from("out-1")).and_modify(|output| {
            output.name = "HDMI-1".into();
            output.focused_workspace_id = Some(WorkspaceId::from("1"));
        });

        preserve_workspace_output_attachments(
            &mut model,
            &[
                super::output::DiscoveredOutput {
                    output_id: OutputId::from("out-1b"),
                    name: "HDMI-1".into(),
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                    primary: true,
                },
                super::output::DiscoveredOutput {
                    output_id: OutputId::from("out-2b"),
                    name: "DP-1".into(),
                    x: 1920,
                    y: 0,
                    width: 1920,
                    height: 1080,
                    primary: false,
                },
            ],
            &std::collections::BTreeMap::from([("HDMI-1".to_string(), WorkspaceId::from("1"))]),
        );

        assert_eq!(
            model.workspaces.get(&WorkspaceId::from("1")).and_then(|workspace| workspace.output_id.clone()),
            Some(OutputId::from("out-1b"))
        );
        assert_eq!(
            model.workspaces.get(&WorkspaceId::from("2")).and_then(|workspace| workspace.output_id.clone()),
            Some(OutputId::from("out-2b"))
        );
    }

    #[test]
    fn select_next_workspace_advances_current_workspace() {
        let mut state = test_runtime_state();

        assert!(state.select_next_workspace());
        assert_eq!(state.model.current_workspace_id, Some(WorkspaceId::from("2")));

        assert!(state.select_next_workspace());
        assert_eq!(state.model.current_workspace_id, Some(WorkspaceId::from("3")));

        assert!(state.select_next_workspace());
        assert_eq!(state.model.current_workspace_id, Some(WorkspaceId::from("1")));
    }

    #[test]
    fn select_previous_workspace_rewinds_current_workspace() {
        let mut state = test_runtime_state();

        assert!(state.select_previous_workspace());
        assert_eq!(state.model.current_workspace_id, Some(WorkspaceId::from("3")));

        assert!(state.select_previous_workspace());
        assert_eq!(state.model.current_workspace_id, Some(WorkspaceId::from("2")));
    }

    #[test]
    fn swap_focused_window_direction_swaps_neighbor_positions() {
        let mut state = test_runtime_state();
        state.model.insert_window(WindowId::from("w1"), Some(WorkspaceId::from("1")), Some(OutputId::from("out-1")));
        state.model.insert_window(WindowId::from("w2"), Some(WorkspaceId::from("1")), Some(OutputId::from("out-1")));
        state.model.insert_window(WindowId::from("w3"), Some(WorkspaceId::from("1")), Some(OutputId::from("out-1")));
        state.model.set_window_mapped(WindowId::from("w1"), true);
        state.model.set_window_mapped(WindowId::from("w2"), true);
        state.model.set_window_mapped(WindowId::from("w3"), true);
        state.model.set_window_focused(Some(WindowId::from("w1")));
        state.x_windows.insert(11, WindowId::from("w1"));
        state.x_windows.insert(22, WindowId::from("w2"));
        state.x_windows.insert(33, WindowId::from("w3"));
        state.stacking_order = vec![11, 22, 33];
        state.model.set_focus_tree_value(Some(FocusTree::from_window_geometries(&[
            FocusTreeWindowGeometry {
                window_id: WindowId::from("w1"),
                geometry: WindowGeometry { x: 0, y: 0, width: 400, height: 400 },
            },
            FocusTreeWindowGeometry {
                window_id: WindowId::from("w2"),
                geometry: WindowGeometry { x: 500, y: 0, width: 400, height: 400 },
            },
            FocusTreeWindowGeometry {
                window_id: WindowId::from("w3"),
                geometry: WindowGeometry { x: 0, y: 500, width: 400, height: 400 },
            },
        ])));

        assert!(state.swap_focused_window_direction(spiders_core::command::FocusDirection::Right));
        assert_eq!(state.stacking_order, vec![22, 11, 33]);
        assert_eq!(state.model.focused_window_id, Some(WindowId::from("w1")));
    }

    #[test]
    fn swap_focused_window_direction_returns_false_without_directional_neighbor() {
        let mut state = test_runtime_state();
        state.model.insert_window(WindowId::from("w1"), Some(WorkspaceId::from("1")), Some(OutputId::from("out-1")));
        state.model.set_window_mapped(WindowId::from("w1"), true);
        state.model.set_window_focused(Some(WindowId::from("w1")));
        state.x_windows.insert(11, WindowId::from("w1"));
        state.stacking_order = vec![11];
        state.model.set_focus_tree_value(Some(FocusTree::from_window_geometries(&[
            FocusTreeWindowGeometry {
                window_id: WindowId::from("w1"),
                geometry: WindowGeometry { x: 0, y: 0, width: 500, height: 900 },
            },
        ])));

        assert!(!state.swap_focused_window_direction(spiders_core::command::FocusDirection::Right));
        assert_eq!(state.stacking_order, vec![11]);
    }

    #[test]
    fn window_visibility_tracks_workspace_visibility() {
        let window = WindowSnapshot {
            id: WindowId::from("w1"),
            shell: ShellKind::X11,
            app_id: None,
            title: None,
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            mode: WindowMode::Tiled,
            focused: false,
            urgent: false,
            closing: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(WorkspaceId::from("ws-1")),
            workspaces: vec!["1".into()],
        };
        let visible_workspace = WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_workspaces: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef { name: "master-stack".into() }),
        };
        let hidden_workspace = WorkspaceSnapshot { visible: false, ..visible_workspace.clone() };
        let base_snapshot = StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 1920,
                logical_height: 1080,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![visible_workspace],
            windows: vec![window.clone()],
            visible_window_ids: vec![window.id.clone()],
            workspace_names: vec!["1".into()],
        };

        assert!(should_window_be_visible(&base_snapshot, &window));
        assert!(
            !should_window_be_visible(
                &StateSnapshot { workspaces: vec![hidden_workspace], ..base_snapshot.clone() },
                &window
            )
        );
    }

    #[test]
    fn output_geometry_uses_full_output_rect() {
        let output = OutputSnapshot {
            id: OutputId::from("out-1"),
            name: "HDMI-A-1".into(),
            logical_x: 100,
            logical_y: 50,
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
        };

        let geometry = output_geometry(&output);

        assert_eq!(geometry.x, 100);
        assert_eq!(geometry.y, 50);
        assert_eq!(geometry.width, 2560);
        assert_eq!(geometry.height, 1440);
    }

    fn test_runtime_state() -> RuntimeState {
        let config = Config {
            workspaces: vec!["1".into(), "2".into(), "3".into()],
            ..Config::default()
        };

        RuntimeState::bootstrap(
            None,
            ":1".into(),
            config,
            ScreenDescriptor { index: 0, root_window: 1, width: 1440, height: 900 },
            Default::default(),
            2,
            &[super::output::DiscoveredOutput {
                output_id: OutputId::from("out-1"),
                name: "screen-0".into(),
                x: 0,
                y: 0,
                width: 1440,
                height: 900,
                primary: true,
            }],
            &[],
            KeyboardBindings::empty_for_tests(),
        )
    }
}
