mod atoms;
mod discovery;
mod event;
mod output;

use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};

use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::{Config, ConfigPaths};
use spiders_core::query::state_snapshot_for_model;
use spiders_core::signal::WmSignal;
use spiders_core::snapshot::StateSnapshot;
use spiders_core::types::{ShellKind, WindowMode};
use spiders_core::wm::{WindowGeometry, WmModel};
use spiders_core::{SeatId, WindowId};
use spiders_wm_runtime::{
    PreviewRenderAction, PreviewWindow, WmHost, WmRuntime, collect_snapshot_geometries,
    compute_layout_preview_from_source_layout,
};
use tracing::info;
use xcb::{Connection, Xid, XidNew, randr};

use crate::config;

use self::atoms::Atoms;
use self::discovery::{DiscoveredWindow, discover_windows};
use self::event::{
    ManageEventHandler, install_manage_root_mask, observe_connection_events, run_manage_event_loop,
};
use self::output::{DiscoveredOutput, discover_outputs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenDescriptor {
    pub(crate) index: i32,
    pub(crate) root_window_id: u32,
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl ScreenDescriptor {
    pub(crate) fn from_x_screen(index: i32, screen: &xcb::x::Screen) -> Self {
        Self {
            index,
            root_window_id: screen.root().resource_id(),
            width: screen.width_in_pixels(),
            height: screen.height_in_pixels(),
        }
    }

    pub(crate) fn root_window(&self) -> xcb::x::Window {
        xcb::x::Window::new(self.root_window_id)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BackendCapabilities {
    pub(crate) randr: bool,
}

pub(crate) struct BackendApp {
    connection: Connection,
    atoms: Atoms,
    state: RuntimeState,
}

impl BackendApp {
    pub(crate) fn connect() -> Result<Self> {
        let (config_paths, config) = config::load_config();
        let (connection, screen_index) =
            Connection::connect(None).context("failed to connect to the X server")?;

        let screen = connection
            .get_setup()
            .roots()
            .nth(screen_index as usize)
            .context("failed to resolve the selected X screen")?;
        let atoms = Atoms::intern_all(&connection).context("failed to intern X atoms")?;
        let screen = ScreenDescriptor::from_x_screen(screen_index, &screen);
        let ewmh_window = create_ewmh_support_window(&connection, &screen)?;
        let discovered_outputs = discover_outputs(&connection, &screen)?;
        let capabilities = BackendCapabilities {
            randr: discovered_outputs.len() > 1
                || discovered_outputs.iter().any(|output| output.x != 0 || output.y != 0),
        };
        let discovered_windows = discover_windows(&connection, &screen, &atoms)?;
        let state = RuntimeState::bootstrap(
            config_paths,
            config,
            screen,
            capabilities,
            ewmh_window,
            &discovered_outputs,
            &discovered_windows,
        );

        Ok(Self { connection, atoms, state })
    }

    pub(crate) fn log_bootstrap(&self) {
        let screen = self.state.screen();
        let config_paths = self.state.config_paths();
        let snapshot = self.state.snapshot();
        let capabilities = self.state.capabilities();

        info!(
            screen_num = screen.index,
            root_window_id = screen.root_window_id,
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
        observe_connection_events(
            &self.connection,
            self.state.screen(),
            event_limit,
            idle_timeout_ms,
        )
    }

    pub(crate) fn manage(&mut self) -> Result<()> {
        let screen = *self.state.screen();
        install_manage_root_mask(&self.connection, &screen)?;
        self.state.publish_ewmh_state(&self.connection, self.atoms)?;

        info!(
            root_window_id = screen.root_window_id,
            screen_num = screen.index,
            "spiders-wm-x acquired X11 window manager ownership"
        );

        let mut handler = BackendManageHandler { atoms: self.atoms, state: &mut self.state };

        run_manage_event_loop(&self.connection, &screen, &mut handler)
    }
}

struct RuntimeState {
    config_paths: Option<ConfigPaths>,
    #[allow(dead_code)]
    config: Config,
    layout_service: Option<AuthoringLayoutService>,
    ewmh_window: xcb::x::Window,
    model: WmModel,
    screen: ScreenDescriptor,
    capabilities: BackendCapabilities,
    outputs: Vec<DiscoveredOutput>,
    x_windows: BTreeMap<u32, WindowId>,
    stacking_order: Vec<u32>,
    workspace_hidden_windows: BTreeSet<u32>,
}

impl RuntimeState {
    fn bootstrap(
        config_paths: Option<ConfigPaths>,
        config: Config,
        screen: ScreenDescriptor,
        capabilities: BackendCapabilities,
        ewmh_window: xcb::x::Window,
        discovered_outputs: &[DiscoveredOutput],
        discovered_windows: &[DiscoveredWindow],
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
            let _ = runtime.take_events();
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
            .map(|window| (window.window.resource_id(), window.window_id.clone()))
            .collect();
        let stacking_order =
            discovered_windows.iter().map(|window| window.window.resource_id()).collect();

        Self {
            config_paths,
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

    fn ensure_runtime_window(&mut self, discovered: &DiscoveredWindow) {
        let mut runtime = WmRuntime::new(&mut self.model);
        self::discovery::sync_discovered_windows(&mut runtime, std::slice::from_ref(discovered));
        self.x_windows.insert(discovered.window.resource_id(), discovered.window_id.clone());
        self.workspace_hidden_windows.remove(&discovered.window.resource_id());
        self.raise_in_stacking(discovered.window.resource_id());
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
            },
        );
    }

    fn sync_window_mapped(&mut self, window_id: WindowId, mapped: bool) {
        let mut runtime = WmRuntime::new(&mut self.model);
        let _ = runtime.sync_window_mapped(window_id, mapped);
    }

    fn focus_window(&mut self, window_id: Option<WindowId>) {
        let seat_id = SeatId::from("x11");
        let mut runtime = WmRuntime::new(&mut self.model);
        let _ = runtime.request_focus_window_selection(seat_id, window_id);
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

    fn activate_x_window(&mut self, window: xcb::x::Window) -> Option<WindowId> {
        let window_id = self.window_id_for_x_window(window)?;
        self.focus_window(Some(window_id.clone()));
        self.raise_in_stacking(window.resource_id());
        Some(window_id)
    }

    fn move_window_to_workspace_index(&mut self, window: xcb::x::Window, index: u32) -> bool {
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

    fn set_window_fullscreen_for_x_window(
        &mut self,
        window: xcb::x::Window,
        fullscreen: bool,
    ) -> bool {
        let Some(window_id) = self.window_id_for_x_window(window) else {
            return false;
        };

        self.model.set_window_fullscreen(window_id, fullscreen);
        true
    }

    fn restack_x_window(&mut self, window: xcb::x::Window, detail: xcb::x::StackMode) -> bool {
        let x_window_id = window.resource_id();
        if !self.x_windows.contains_key(&x_window_id) {
            return false;
        }

        self.stacking_order.retain(|candidate| *candidate != x_window_id);
        match detail {
            xcb::x::StackMode::Above => self.stacking_order.push(x_window_id),
            xcb::x::StackMode::Below => self.stacking_order.insert(0, x_window_id),
            _ => self.stacking_order.push(x_window_id),
        }
        true
    }

    fn set_window_floating_geometry(&mut self, window_id: WindowId, geometry: WindowGeometry) {
        let mut runtime = WmRuntime::new(&mut self.model);
        let _ = runtime.set_window_floating_geometry(window_id, geometry);
    }

    fn sync_actual_window_geometry(&mut self, window: xcb::x::Window, geometry: WindowGeometry) {
        let Some(window_id) = self.window_id_for_x_window(window) else {
            return;
        };

        self.set_window_floating_geometry(window_id, geometry);
    }

    fn current_layout_geometries(&mut self) -> Result<Vec<(xcb::x::Window, WindowGeometry)>> {
        let state = self.snapshot();
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
            let windows = state
                .windows
                .iter()
                .filter(|window| {
                    window.workspace_id.as_ref() == Some(&workspace.id)
                        && window.output_id.as_ref() == Some(&output.id)
                        && window.mapped
                        && matches!(window.mode, WindowMode::Tiled)
                })
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

            let mut workspace_geometry = BTreeMap::new();
            collect_snapshot_geometries(&snapshot_root, &mut workspace_geometry);
            for (window_id, mut geometry) in workspace_geometry {
                geometry.x += output.logical_x;
                geometry.y += output.logical_y;
                geometry_by_window.insert(window_id, geometry);
            }
        }

        Ok(self
            .x_windows
            .iter()
            .filter_map(|(x_window_id, window_id)| {
                geometry_by_window
                    .get(window_id)
                    .copied()
                    .map(|geometry| (xcb::x::Window::new(*x_window_id), geometry))
            })
            .collect())
    }

    fn unmap_window(&mut self, window_id: WindowId) {
        let window_order = self.model.windows.keys().cloned().collect::<Vec<_>>();
        let mut runtime = WmRuntime::new(&mut self.model);
        let _ = runtime.unmap_window(window_id, window_order);
    }

    fn remove_window(&mut self, x_window_id: u32) {
        let Some(window_id) = self.x_windows.remove(&x_window_id) else {
            return;
        };
        self.stacking_order.retain(|candidate| *candidate != x_window_id);
        self.workspace_hidden_windows.remove(&x_window_id);

        let window_order = self.model.windows.keys().cloned().collect::<Vec<_>>();
        let mut runtime = WmRuntime::new(&mut self.model);
        let _ = runtime.remove_window(window_id, window_order);
    }

    fn window_id_for_x_window(&self, window: xcb::x::Window) -> Option<WindowId> {
        self.x_windows.get(&window.resource_id()).cloned()
    }

    fn raise_in_stacking(&mut self, x_window_id: u32) {
        self.stacking_order.retain(|candidate| *candidate != x_window_id);
        self.stacking_order.push(x_window_id);
    }

    fn refresh_outputs(&mut self, connection: &Connection) -> Result<()> {
        let outputs = discover_outputs(connection, &self.screen)?;
        self.capabilities.randr =
            outputs.len() > 1 || outputs.iter().any(|output| output.x != 0 || output.y != 0);
        self.outputs = outputs.clone();

        {
            let mut runtime = WmRuntime::new(&mut self.model);
            let mut host = NoopHost;

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

        attach_workspaces_to_outputs(&mut self.model, &outputs);
        for output in &outputs {
            self.model.outputs.entry(output.output_id.clone()).and_modify(|model_output| {
                model_output.logical_x = output.x;
                model_output.logical_y = output.y;
            });
        }

        if let Some(primary_output) =
            outputs.iter().find(|output| output.primary).or_else(|| outputs.first())
        {
            self.model.set_current_output(primary_output.output_id.clone());
        }

        Ok(())
    }

    fn publish_ewmh_state(&self, connection: &Connection, atoms: Atoms) -> Result<()> {
        let snapshot = self.snapshot();
        let root = self.screen.root_window();

        change_root_property(
            connection,
            root,
            atoms.net_supported,
            xcb::x::ATOM_ATOM,
            &supported_atoms(atoms),
        )?;
        change_root_property(
            connection,
            root,
            atoms.net_supporting_wm_check,
            xcb::x::ATOM_WINDOW,
            &[self.ewmh_window],
        )?;
        change_root_property(
            connection,
            self.ewmh_window,
            atoms.net_supporting_wm_check,
            xcb::x::ATOM_WINDOW,
            &[self.ewmh_window],
        )?;

        if atoms.net_wm_name != xcb::x::ATOM_NONE && atoms.utf8_string != xcb::x::ATOM_NONE {
            connection.send_and_check_request(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: self.ewmh_window,
                property: atoms.net_wm_name,
                r#type: atoms.utf8_string,
                data: b"spiders-wm-x",
            })?;
        }

        let active_window = snapshot
            .focused_window_id
            .as_ref()
            .and_then(|window_id| {
                self.x_windows.iter().find_map(|(x_window, id)| {
                    (id == window_id).then_some(xcb::x::Window::new(*x_window))
                })
            })
            .unwrap_or(xcb::x::Window::none());
        change_root_property(
            connection,
            root,
            atoms.net_active_window,
            xcb::x::ATOM_WINDOW,
            &[active_window],
        )?;

        let client_list = snapshot
            .windows
            .iter()
            .filter_map(|window| {
                self.x_windows.iter().find_map(|(x_window, id)| {
                    (id == &window.id).then_some(xcb::x::Window::new(*x_window))
                })
            })
            .collect::<Vec<_>>();
        let stacking_list = self
            .stacking_order
            .iter()
            .filter_map(|x_window| {
                self.x_windows.contains_key(x_window).then_some(xcb::x::Window::new(*x_window))
            })
            .collect::<Vec<_>>();
        change_root_property(
            connection,
            root,
            atoms.net_client_list,
            xcb::x::ATOM_WINDOW,
            &client_list,
        )?;
        change_root_property(
            connection,
            root,
            atoms.net_client_list_stacking,
            xcb::x::ATOM_WINDOW,
            &stacking_list,
        )?;

        let current_desktop =
            desktop_index_for_workspace(&snapshot, snapshot.current_workspace_id.as_ref())
                .unwrap_or(0);
        change_root_property(
            connection,
            root,
            atoms.net_current_desktop,
            xcb::x::ATOM_CARDINAL,
            &[current_desktop],
        )?;
        change_root_property(
            connection,
            root,
            atoms.net_number_of_desktops,
            xcb::x::ATOM_CARDINAL,
            &[snapshot.workspaces.len() as u32],
        )?;

        if atoms.net_desktop_names != xcb::x::ATOM_NONE && atoms.utf8_string != xcb::x::ATOM_NONE {
            let desktop_names = snapshot
                .workspaces
                .iter()
                .map(|workspace| workspace.name.as_str())
                .collect::<Vec<_>>()
                .join("\0");
            connection.send_and_check_request(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: root,
                property: atoms.net_desktop_names,
                r#type: atoms.utf8_string,
                data: desktop_names.as_bytes(),
            })?;
        }

        let workareas = workareas_for_snapshot(&snapshot);
        change_root_property(
            connection,
            root,
            atoms.net_workarea,
            xcb::x::ATOM_CARDINAL,
            &workareas,
        )?;

        for (x_window_id, window_id) in &self.x_windows {
            let window = xcb::x::Window::new(*x_window_id);
            let snapshot_window =
                snapshot.windows.iter().find(|candidate| &candidate.id == window_id);

            if let Some(snapshot_window) = snapshot_window {
                if let Some(desktop_index) =
                    desktop_index_for_workspace(&snapshot, snapshot_window.workspace_id.as_ref())
                {
                    change_root_property(
                        connection,
                        window,
                        atoms.net_wm_desktop,
                        xcb::x::ATOM_CARDINAL,
                        &[desktop_index],
                    )?;
                } else {
                    delete_property(connection, window, atoms.net_wm_desktop)?;
                }

                let window_state_atoms = ewmh_window_state_atoms(atoms, snapshot_window);
                if window_state_atoms.is_empty() {
                    delete_property(connection, window, atoms.net_wm_state)?;
                } else {
                    change_root_property(
                        connection,
                        window,
                        atoms.net_wm_state,
                        xcb::x::ATOM_ATOM,
                        &window_state_atoms,
                    )?;
                }
            }
        }

        connection.flush().context("failed to flush EWMH state publication")?;
        Ok(())
    }
}

fn create_ewmh_support_window(
    connection: &Connection,
    screen: &ScreenDescriptor,
) -> Result<xcb::x::Window> {
    let window = connection.generate_id();
    connection
        .send_and_check_request(&xcb::x::CreateWindow {
            depth: xcb::x::COPY_FROM_PARENT as u8,
            wid: window,
            parent: screen.root_window(),
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            border_width: 0,
            class: xcb::x::WindowClass::InputOutput,
            visual: xcb::x::COPY_FROM_PARENT,
            value_list: &[xcb::x::Cw::OverrideRedirect(true)],
        })
        .context("failed to create EWMH support window")?;
    connection.flush().context("failed to flush EWMH support window creation")?;
    Ok(window)
}

fn supported_atoms(atoms: Atoms) -> Vec<xcb::x::Atom> {
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
    .filter(|atom| *atom != xcb::x::ATOM_NONE)
    .collect()
}

fn change_root_property<P: xcb::x::PropEl>(
    connection: &Connection,
    window: xcb::x::Window,
    property: xcb::x::Atom,
    property_type: xcb::x::Atom,
    data: &[P],
) -> Result<()> {
    if property == xcb::x::ATOM_NONE {
        return Ok(());
    }

    connection
        .send_and_check_request(&xcb::x::ChangeProperty {
            mode: xcb::x::PropMode::Replace,
            window,
            property,
            r#type: property_type,
            data,
        })
        .context("failed to publish X11 root property")?;
    Ok(())
}

fn delete_property(
    connection: &Connection,
    window: xcb::x::Window,
    property: xcb::x::Atom,
) -> Result<()> {
    if property == xcb::x::ATOM_NONE {
        return Ok(());
    }

    connection
        .send_and_check_request(&xcb::x::DeleteProperty { window, property })
        .context("failed to delete X11 property")?;
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

fn stack_mode_from_ewmh(detail: u32) -> xcb::x::StackMode {
    match detail {
        1 => xcb::x::StackMode::Below,
        2 => xcb::x::StackMode::TopIf,
        3 => xcb::x::StackMode::BottomIf,
        4 => xcb::x::StackMode::Opposite,
        _ => xcb::x::StackMode::Above,
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
) -> Vec<xcb::x::Atom> {
    let mut state = Vec::new();

    if window.mode.is_fullscreen() && atoms.net_wm_state_fullscreen != xcb::x::ATOM_NONE {
        state.push(atoms.net_wm_state_fullscreen);
    }
    if !window.mapped && atoms.net_wm_state_hidden != xcb::x::ATOM_NONE {
        state.push(atoms.net_wm_state_hidden);
    }
    if window.focused && atoms.net_wm_state_focused != xcb::x::ATOM_NONE {
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

struct BackendManageHandler<'a> {
    atoms: Atoms,
    state: &'a mut RuntimeState,
}

impl ManageEventHandler for BackendManageHandler<'_> {
    fn on_map_request(&mut self, connection: &Connection, window: xcb::x::Window) -> Result<()> {
        if let Some(discovered) =
            discover_windows_for_single(connection, self.state.screen(), &self.atoms, window)?
        {
            install_managed_window_event_mask(connection, window)?;
            self.state.ensure_runtime_window(&discovered);
            self.state.sync_window_identity(&discovered);
            self.state.sync_window_mapped(discovered.window_id.clone(), true);
            self.state.focus_window(Some(discovered.window_id));
            raise_and_focus_window(connection, window)?;
            self.apply_shared_layout(connection)?;
            self.state.publish_ewmh_state(connection, self.atoms)?;
        }

        connection
            .send_and_check_request(&xcb::x::MapWindow { window })
            .context("failed to map X11 window after map request")?;
        connection.flush().context("failed to flush X11 map request handling")?;
        Ok(())
    }

    fn on_configure_request(
        &mut self,
        connection: &Connection,
        event: &xcb::x::ConfigureRequestEvent,
    ) -> Result<()> {
        let mut value_list = Vec::new();
        let value_mask = event.value_mask();

        if value_mask.contains(xcb::x::ConfigWindowMask::X) {
            value_list.push(xcb::x::ConfigWindow::X(i32::from(event.x())));
        }
        if value_mask.contains(xcb::x::ConfigWindowMask::Y) {
            value_list.push(xcb::x::ConfigWindow::Y(i32::from(event.y())));
        }
        if value_mask.contains(xcb::x::ConfigWindowMask::WIDTH) {
            value_list.push(xcb::x::ConfigWindow::Width(u32::from(event.width())));
        }
        if value_mask.contains(xcb::x::ConfigWindowMask::HEIGHT) {
            value_list.push(xcb::x::ConfigWindow::Height(u32::from(event.height())));
        }
        if value_mask.contains(xcb::x::ConfigWindowMask::BORDER_WIDTH) {
            value_list.push(xcb::x::ConfigWindow::BorderWidth(u32::from(event.border_width())));
        }
        if value_mask.contains(xcb::x::ConfigWindowMask::SIBLING) {
            value_list.push(xcb::x::ConfigWindow::Sibling(event.sibling()));
        }
        if value_mask.contains(xcb::x::ConfigWindowMask::STACK_MODE) {
            value_list.push(xcb::x::ConfigWindow::StackMode(event.stack_mode()));
        }

        if !value_list.is_empty() {
            connection
                .send_and_check_request(&xcb::x::ConfigureWindow {
                    window: event.window(),
                    value_list: &value_list,
                })
                .context("failed to apply X11 configure request")?;
            connection.flush().context("failed to flush X11 configure request")?;
        }

        let is_managed = self.state.window_id_for_x_window(event.window());

        if let Some(window_id) = is_managed {
            let geometry = WindowGeometry {
                x: i32::from(event.x()),
                y: i32::from(event.y()),
                width: i32::from(event.width()),
                height: i32::from(event.height()),
            };
            self.state.set_window_floating_geometry(window_id, geometry);
        }

        Ok(())
    }

    fn on_unmap_notify(&mut self, connection: &Connection, window: xcb::x::Window) -> Result<()> {
        if let Some(window_id) = self.state.window_id_for_x_window(window) {
            self.state.unmap_window(window_id);
            self.apply_shared_layout(connection)?;
            self.state.publish_ewmh_state(connection, self.atoms)?;
        }

        Ok(())
    }

    fn on_destroy_notify(&mut self, connection: &Connection, window: xcb::x::Window) -> Result<()> {
        self.state.remove_window(window.resource_id());
        self.apply_shared_layout(connection)?;
        self.state.publish_ewmh_state(connection, self.atoms)?;
        Ok(())
    }

    fn on_configure_notify(&mut self, event: &xcb::x::ConfigureNotifyEvent) {
        let geometry = WindowGeometry {
            x: i32::from(event.x()),
            y: i32::from(event.y()),
            width: i32::from(event.width()),
            height: i32::from(event.height()),
        };
        self.state.sync_actual_window_geometry(event.window(), geometry);
    }

    fn on_property_notify(
        &mut self,
        connection: &Connection,
        window: xcb::x::Window,
        atom: xcb::x::Atom,
    ) -> Result<()> {
        if atom != self.atoms.wm_name
            && atom != self.atoms.net_wm_name
            && atom != self.atoms.wm_class
        {
            return Ok(());
        }

        if let Some(discovered) =
            discover_windows_for_single(connection, self.state.screen(), &self.atoms, window)?
        {
            install_managed_window_event_mask(connection, window)?;
            self.state.ensure_runtime_window(&discovered);
            self.state.sync_window_identity(&discovered);
            self.apply_shared_layout(connection)?;
            self.state.publish_ewmh_state(connection, self.atoms)?;
        }

        Ok(())
    }

    fn on_focus_in(&mut self, connection: &Connection, window: xcb::x::Window) -> Result<()> {
        self.state.focus_window(self.state.window_id_for_x_window(window));
        self.state.publish_ewmh_state(connection, self.atoms)?;
        Ok(())
    }

    fn on_client_message(
        &mut self,
        connection: &Connection,
        window: xcb::x::Window,
        type_atom: xcb::x::Atom,
        data: &xcb::x::ClientMessageData,
    ) -> Result<()> {
        let payload = match data {
            xcb::x::ClientMessageData::Data32(values) => *values,
            _ => return Ok(()),
        };

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
            let first = xcb::x::Atom::new(payload[1]);
            let second = xcb::x::Atom::new(payload[2]);

            let mut changed = false;
            changed |= self.apply_net_wm_state_action(window, action, first);
            if second != xcb::x::ATOM_NONE {
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
            let sibling = xcb::x::Window::new(payload[1]);
            let detail = stack_mode_from_ewmh(payload[2]);

            connection
                .send_and_check_request(&xcb::x::ConfigureWindow {
                    window,
                    value_list: &[
                        xcb::x::ConfigWindow::Sibling(sibling),
                        xcb::x::ConfigWindow::StackMode(detail),
                    ],
                })
                .context("failed to apply _NET_RESTACK_WINDOW")?;
            connection.flush().context("failed to flush _NET_RESTACK_WINDOW")?;

            if self.state.restack_x_window(window, detail) {
                self.state.publish_ewmh_state(connection, self.atoms)?;
            }
        } else if type_atom == self.atoms.net_moveresize_window {
            let flags = payload[0];
            let mut value_list = Vec::new();

            if flags & (1 << 8) != 0 {
                value_list.push(xcb::x::ConfigWindow::X(payload[1] as i32));
            }
            if flags & (1 << 9) != 0 {
                value_list.push(xcb::x::ConfigWindow::Y(payload[2] as i32));
            }
            if flags & (1 << 10) != 0 {
                value_list.push(xcb::x::ConfigWindow::Width(payload[3].max(1)));
            }
            if flags & (1 << 11) != 0 {
                value_list.push(xcb::x::ConfigWindow::Height(payload[4].max(1)));
            }

            if !value_list.is_empty() {
                connection
                    .send_and_check_request(&xcb::x::ConfigureWindow {
                        window,
                        value_list: &value_list,
                    })
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
        }

        Ok(())
    }

    fn on_randr_notify(
        &mut self,
        connection: &Connection,
        event: &randr::NotifyEvent,
    ) -> Result<()> {
        match event.sub_code() {
            randr::Notify::CrtcChange
            | randr::Notify::OutputChange
            | randr::Notify::OutputProperty
            | randr::Notify::ResourceChange => {
                self.state.refresh_outputs(connection)?;
                self.apply_shared_layout(connection)?;
                self.state.publish_ewmh_state(connection, self.atoms)?;
            }
            randr::Notify::ProviderChange
            | randr::Notify::ProviderProperty
            | randr::Notify::Lease => {}
        }

        Ok(())
    }
}

impl BackendManageHandler<'_> {
    fn apply_workspace_visibility(&mut self, connection: &Connection) -> Result<()> {
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

            let window = xcb::x::Window::new(x_window_id);
            let should_be_visible = should_window_be_visible(&snapshot, snapshot_window);

            if should_be_visible {
                if self.state.workspace_hidden_windows.remove(&x_window_id) {
                    connection
                        .send_and_check_request(&xcb::x::MapWindow { window })
                        .context("failed to remap X11 window for visible workspace")?;
                    self.state.sync_window_mapped(window_id, true);
                    changed = true;
                }
                continue;
            }

            if snapshot_window.mapped && !self.state.workspace_hidden_windows.contains(&x_window_id) {
                self.state.workspace_hidden_windows.insert(x_window_id);
                connection
                    .send_and_check_request(&xcb::x::UnmapWindow { window })
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

    fn apply_net_wm_state_action(
        &mut self,
        window: xcb::x::Window,
        action: u32,
        atom: xcb::x::Atom,
    ) -> bool {
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

    fn apply_shared_layout(&mut self, connection: &Connection) -> Result<()> {
        self.apply_workspace_visibility(connection)?;
        let geometries = self.state.current_layout_geometries()?;
        let snapshot = self.state.snapshot();

        for (window, geometry) in geometries.iter().copied() {
            let is_fullscreen = self
                .state
                .window_id_for_x_window(window)
                .and_then(|window_id| snapshot.windows.iter().find(|candidate| candidate.id == window_id))
                .is_some_and(|window| window.mode.is_fullscreen());
            let mut value_list = vec![
                xcb::x::ConfigWindow::X(geometry.x),
                xcb::x::ConfigWindow::Y(geometry.y),
                xcb::x::ConfigWindow::Width(geometry.width.max(1) as u32),
                xcb::x::ConfigWindow::Height(geometry.height.max(1) as u32),
            ];
            if is_fullscreen {
                value_list.push(xcb::x::ConfigWindow::StackMode(xcb::x::StackMode::Above));
            }

            connection
                .send_and_check_request(&xcb::x::ConfigureWindow {
                    window,
                    value_list: &value_list,
                })
                .context("failed to apply shared layout geometry to X11 window")?;
        }

        if !geometries.is_empty() {
            connection.flush().context("failed to flush shared X11 layout application")?;
        }

        Ok(())
    }

    fn request_close_window(
        &mut self,
        connection: &Connection,
        window: xcb::x::Window,
        window_id: WindowId,
    ) -> Result<()> {
        if supports_wm_delete_window(connection, self.atoms, window)? {
            let data = xcb::x::ClientMessageData::Data32([
                self.atoms.wm_delete_window.resource_id(),
                xcb::x::CURRENT_TIME,
                0,
                0,
                0,
            ]);
            connection
                .send_and_check_request(&xcb::x::SendEvent {
                    propagate: false,
                    destination: xcb::x::SendEventDest::Window(window),
                    event_mask: xcb::x::EventMask::NO_EVENT,
                    event: &xcb::x::ClientMessageEvent::new(window, self.atoms.wm_protocols, data),
                })
                .context("failed to send WM_DELETE_WINDOW to X11 client")?;
            connection.flush().context("failed to flush X11 close request")?;
        } else {
            connection
                .send_and_check_request(&xcb::x::KillClient { resource: window.resource_id() })
                .context("failed to kill X11 client without WM_DELETE_WINDOW support")?;
            connection.flush().context("failed to flush X11 client kill request")?;
        }

        self.state.model.set_window_closing(window_id, true);
        Ok(())
    }
}

fn discover_windows_for_single(
    connection: &Connection,
    _screen: &ScreenDescriptor,
    atoms: &Atoms,
    window: xcb::x::Window,
) -> Result<Option<DiscoveredWindow>> {
    self::discovery::discover_window_for_event(connection, atoms, window)
}

fn supports_wm_delete_window(
    connection: &Connection,
    atoms: Atoms,
    window: xcb::x::Window,
) -> Result<bool> {
    if atoms.wm_protocols == xcb::x::ATOM_NONE || atoms.wm_delete_window == xcb::x::ATOM_NONE {
        return Ok(false);
    }

    let reply = match connection.wait_for_reply(connection.send_request(&xcb::x::GetProperty {
        delete: false,
        window,
        property: atoms.wm_protocols,
        r#type: xcb::x::ATOM_ATOM,
        long_offset: 0,
        long_length: 32,
    })) {
        Ok(reply) => reply,
        Err(_) => return Ok(false),
    };

    Ok(reply.value::<xcb::x::Atom>().iter().any(|atom| *atom == atoms.wm_delete_window))
}

fn install_managed_window_event_mask(
    connection: &Connection,
    window: xcb::x::Window,
) -> Result<()> {
    connection
        .send_and_check_request(&xcb::x::ChangeWindowAttributes {
            window,
            value_list: &[xcb::x::Cw::EventMask(managed_window_event_mask())],
        })
        .context("failed to install managed X11 window event mask")?;
    connection.flush().context("failed to flush managed X11 window event mask")?;
    Ok(())
}

fn managed_window_event_mask() -> xcb::x::EventMask {
    xcb::x::EventMask::PROPERTY_CHANGE
        | xcb::x::EventMask::FOCUS_CHANGE
        | xcb::x::EventMask::STRUCTURE_NOTIFY
}

fn raise_and_focus_window(connection: &Connection, window: xcb::x::Window) -> Result<()> {
    connection
        .send_and_check_request(&xcb::x::ConfigureWindow {
            window,
            value_list: &[xcb::x::ConfigWindow::StackMode(xcb::x::StackMode::Above)],
        })
        .context("failed to raise managed X11 window")?;
    connection
        .send_and_check_request(&xcb::x::SetInputFocus {
            revert_to: xcb::x::InputFocus::PointerRoot,
            focus: window,
            time: xcb::x::CURRENT_TIME,
        })
        .context("failed to focus managed X11 window")?;
    connection.flush().context("failed to flush managed X11 raise/focus")?;
    Ok(())
}

struct NoopHost;

impl WmHost for NoopHost {
    fn on_effect(&mut self, _effect: spiders_core::effect::WmHostEffect) -> PreviewRenderAction {
        PreviewRenderAction::None
    }
}

#[cfg(test)]
mod tests {
    use super::{output_geometry, should_window_be_visible};
    use spiders_core::snapshot::{OutputSnapshot, StateSnapshot, WindowSnapshot, WorkspaceSnapshot};
    use spiders_core::types::{LayoutRef, OutputTransform, ShellKind, WindowMode};
    use spiders_core::{OutputId, WindowId, WorkspaceId};

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
}
