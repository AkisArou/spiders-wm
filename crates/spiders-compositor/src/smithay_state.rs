#[cfg(feature = "smithay-winit")]
mod imp {
    use std::collections::{HashMap, HashSet};

    use crate::backend::{
        BackendDiscoveryEvent, BackendOutputSnapshot, BackendSeatSnapshot, BackendSource,
        BackendSurfaceSnapshot, BackendTopologySnapshot,
    };
    use smithay::backend::renderer::utils::on_commit_buffer_handler;
    use smithay::delegate_compositor;
    use smithay::delegate_data_control;
    use smithay::delegate_data_device;
    use smithay::delegate_ext_data_control;
    use smithay::delegate_layer_shell;
    use smithay::delegate_output;
    use smithay::delegate_presentation;
    use smithay::delegate_primary_selection;
    use smithay::delegate_seat;
    use smithay::delegate_shm;
    use smithay::delegate_xdg_decoration;
    use smithay::delegate_xdg_shell;
    use smithay::input::keyboard::XkbConfig;
    use smithay::input::pointer::CursorImageStatus;
    use smithay::input::{Seat, SeatHandler, SeatState};
    use smithay::output::Output;
    use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
    use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
    use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
    use smithay::reexports::wayland_server::protocol::wl_buffer;
    use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
    use smithay::reexports::wayland_server::protocol::wl_seat;
    use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
    use smithay::reexports::wayland_server::Resource;
    use smithay::reexports::wayland_server::{BindError, Client, Display, DisplayHandle};
    use smithay::utils::{Serial, SERIAL_COUNTER};
    use smithay::wayland::buffer::BufferHandler;
    use smithay::wayland::compositor::{
        get_parent, get_role, is_sync_subsurface, with_states, BufferAssignment,
    };
    use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
    use smithay::wayland::output::{OutputHandler, OutputManagerState};
    use smithay::wayland::presentation::PresentationState;
    use smithay::wayland::selection::data_device::{
        DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler,
    };
    use smithay::wayland::selection::ext_data_control::{
        DataControlHandler as ExtDataControlHandler, DataControlState as ExtDataControlState,
    };
    use smithay::wayland::selection::primary_selection::{
        set_primary_focus, PrimarySelectionHandler, PrimarySelectionState,
    };
    use smithay::wayland::selection::wlr_data_control::{
        DataControlHandler as WlrDataControlHandler, DataControlState as WlrDataControlState,
    };
    use smithay::wayland::selection::{SelectionHandler, SelectionSource, SelectionTarget};
    use smithay::wayland::shell::wlr_layer::{
        ExclusiveZone as WlrExclusiveZone, KeyboardInteractivity as WlrKeyboardInteractivity,
        Layer as WlrLayer, LayerSurface, LayerSurfaceConfigure, LayerSurfaceData,
        WlrLayerShellHandler, WlrLayerShellState,
    };
    use smithay::wayland::shell::xdg::{
        decoration::{XdgDecorationHandler, XdgDecorationState},
        PopupSurface, PositionerState, ToplevelSurface, XdgPopupSurfaceData, XdgShellHandler,
        XdgShellState, XdgToplevelSurfaceData, XDG_POPUP_ROLE, XDG_TOPLEVEL_ROLE,
    };
    use smithay::wayland::shm::{ShmHandler, ShmState};
    use smithay::wayland::socket::ListeningSocketSource;
    use spiders_effects::TitlebarEffects;
    use spiders_runtime::{
        LayerExclusiveZone, LayerKeyboardInteractivity, LayerSurfaceMetadata, LayerSurfaceTier,
    };
    use spiders_shared::api::WmAction;
    use spiders_shared::ids::{OutputId, WindowId};
    use spiders_shared::layout::LayoutRect;
    use spiders_shared::wm::{OutputTransform, StateSnapshot};

    use crate::smithay_workspace::{WorkspaceHandler, WorkspaceManagerState};
    use crate::titlebar::TitlebarRenderItem;

    #[derive(Debug, thiserror::Error)]
    pub enum SmithayStateError {
        #[error(transparent)]
        Keyboard(#[from] smithay::input::keyboard::Error),
        #[error(transparent)]
        SocketBind(#[from] BindError),
    }

    #[derive(Debug, Default)]
    pub struct SmithayClientState {
        pub compositor_state: CompositorClientState,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithaySurfaceRoleCounts {
        pub toplevel: usize,
        pub popup: usize,
        pub unmanaged: usize,
        pub layer: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownToplevelSurface {
        pub surface_id: String,
        pub window_id: WindowId,
        pub decoration_policy: SmithayWindowDecorationPolicySnapshot,
        pub titlebar: Option<SmithayTitlebarRenderSnapshot>,
        pub configure: SmithayXdgToplevelConfigureSnapshot,
        pub metadata: SmithayXdgToplevelMetadataSnapshot,
        pub requests: SmithayXdgToplevelRequestSnapshot,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayWindowDecorationPolicySnapshot {
        pub decorations_visible: bool,
        pub titlebar_visible: bool,
        pub titlebar_style: TitlebarEffects,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayTitlebarRenderSnapshot {
        pub title: String,
        pub app_id: Option<String>,
        pub style: TitlebarEffects,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct SmithayWindowRenderSnapshot {
        pub window_id: WindowId,
        pub window_rect: LayoutRect,
        pub content_offset_y: f32,
    }

    #[derive(Debug, Clone)]
    pub struct SmithayRenderableToplevelSurface {
        pub window_id: WindowId,
        pub surface: ToplevelSurface,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TitlebarHitKind {
        Move,
        ResizeTop,
        ResizeBottom,
        ResizeLeft,
        ResizeRight,
        ResizeTopLeft,
        ResizeTopRight,
        ResizeBottomLeft,
        ResizeBottomRight,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayTitlebarHitTarget {
        pub window_id: WindowId,
        pub kind: TitlebarHitKind,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct ActiveTitlebarInteraction {
        window_id: WindowId,
        kind: TitlebarHitKind,
        pointer_origin: (f64, f64),
        initial_window_rect: LayoutRect,
        titlebar_height: f32,
    }

    impl Default for SmithayWindowDecorationPolicySnapshot {
        fn default() -> Self {
            Self {
                decorations_visible: true,
                titlebar_visible: true,
                titlebar_style: TitlebarEffects::default(),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayXdgToplevelConfigureSnapshot {
        pub last_acked_serial: Option<u32>,
        pub activated: bool,
        pub fullscreen: bool,
        pub maximized: bool,
        pub pending_configure_count: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayXdgToplevelMetadataSnapshot {
        pub title: Option<String>,
        pub app_id: Option<String>,
        pub parent_surface_id: Option<String>,
        pub min_size: Option<(i32, i32)>,
        pub max_size: Option<(i32, i32)>,
        pub window_geometry: Option<(i32, i32, i32, i32)>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayXdgToplevelRequestSnapshot {
        pub last_move_serial: Option<u32>,
        pub last_resize_serial: Option<u32>,
        pub last_resize_edge: Option<String>,
        pub last_window_menu_serial: Option<u32>,
        pub last_window_menu_location: Option<(i32, i32)>,
        pub minimize_requested: bool,
        pub last_request_kind: Option<String>,
        pub request_count: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownPopupSurface {
        pub surface_id: String,
        pub parent: SmithayPopupParentSnapshot,
        pub configure: SmithayXdgPopupConfigureSnapshot,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayXdgPopupConfigureSnapshot {
        pub last_acked_serial: Option<u32>,
        pub pending_configure_count: usize,
        pub last_reposition_token: Option<u32>,
        pub reactive: bool,
        pub geometry: (i32, i32, i32, i32),
        pub last_grab_serial: Option<u32>,
        pub grab_requested: bool,
        pub last_request_kind: Option<String>,
        pub request_count: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SmithayPopupParentSnapshot {
        Resolved {
            surface_id: String,
            window_id: Option<WindowId>,
        },
        Unresolved,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownUnmanagedSurface {
        pub surface_id: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownLayerSurface {
        pub surface_id: String,
        pub output_id: Option<OutputId>,
        pub metadata: LayerSurfaceMetadata,
        pub configure: SmithayLayerSurfaceConfigureSnapshot,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayLayerSurfaceConfigureSnapshot {
        pub last_acked_serial: Option<u32>,
        pub pending_configure_count: usize,
        pub last_configured_size: Option<(i32, i32)>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithaySelectionOfferSnapshot {
        pub mime_types: Vec<String>,
        pub source_kind: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayClipboardSelectionSnapshot {
        pub seat_name: String,
        pub target: String,
        pub selection: Option<SmithaySelectionOfferSnapshot>,
        pub focused_client_id: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayPrimarySelectionSnapshot {
        pub seat_name: String,
        pub target: String,
        pub selection: Option<SmithaySelectionOfferSnapshot>,
        pub focused_client_id: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithaySelectionProtocolSupportSnapshot {
        pub data_device: bool,
        pub primary_selection: bool,
        pub wlr_data_control: bool,
        pub ext_data_control: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithaySeatSnapshot {
        pub name: String,
        pub has_keyboard: bool,
        pub has_pointer: bool,
        pub has_touch: bool,
        pub focused_surface_id: Option<String>,
        pub focused_surface_role: Option<String>,
        pub focused_window_id: Option<WindowId>,
        pub focused_output_id: Option<OutputId>,
        pub cursor_image: String,
        pub cursor_surface_id: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownOutput {
        pub id: OutputId,
        pub name: String,
        pub logical_width: Option<u32>,
        pub logical_height: Option<u32>,
        pub transform: OutputTransform,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayOutputSnapshot {
        pub known_output_ids: Vec<OutputId>,
        pub known_outputs: Vec<SmithayKnownOutput>,
        pub active_output_id: Option<OutputId>,
        pub layer_surface_output_count: usize,
        pub active_output_attached_surface_count: usize,
        pub mapped_surface_count: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayKnownSurfacesSnapshot {
        pub all: Vec<SmithayKnownSurface>,
        pub toplevels: Vec<SmithayKnownToplevelSurface>,
        pub popups: Vec<SmithayKnownPopupSurface>,
        pub unmanaged: Vec<SmithayKnownUnmanagedSurface>,
        pub layers: Vec<SmithayKnownLayerSurface>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SmithayKnownSurface {
        Toplevel(SmithayKnownToplevelSurface),
        Popup(SmithayKnownPopupSurface),
        Layer(SmithayKnownLayerSurface),
        Unmanaged(SmithayKnownUnmanagedSurface),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayStateSnapshot {
        pub seat_name: String,
        pub seat: SmithaySeatSnapshot,
        pub outputs: SmithayOutputSnapshot,
        pub tracked_surface_count: usize,
        pub tracked_toplevel_count: usize,
        pub pending_discovery_event_count: usize,
        pub role_counts: SmithaySurfaceRoleCounts,
        pub known_surfaces: SmithayKnownSurfacesSnapshot,
        pub selection_protocols: SmithaySelectionProtocolSupportSnapshot,
        pub clipboard_selection: SmithayClipboardSelectionSnapshot,
        pub primary_selection: SmithayPrimarySelectionSnapshot,
    }

    impl ClientData for SmithayClientState {
        fn initialized(&self, _client_id: ClientId) {}

        fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
    }

    #[derive(Debug)]
    pub struct SpidersSmithayState {
        pub display_handle: DisplayHandle,
        pub compositor_state: CompositorState,
        pub shm_state: ShmState,
        pub xdg_decoration_state: XdgDecorationState,
        pub xdg_shell_state: XdgShellState,
        pub layer_shell_state: WlrLayerShellState,
        pub output_manager_state: OutputManagerState,
        pub presentation_state: PresentationState,
        pub data_device_state: DataDeviceState,
        pub primary_selection_state: PrimarySelectionState,
        pub wlr_data_control_state: WlrDataControlState,
        pub ext_data_control_state: ExtDataControlState,
        pub workspace_manager_state: WorkspaceManagerState,
        pub seat_state: SeatState<Self>,
        pub seat: Seat<Self>,
        pub seat_name: String,
        next_window_serial: u64,
        known_seat_names: Vec<String>,
        active_seat_name: Option<String>,
        smithay_outputs: HashMap<OutputId, Output>,
        toplevel_window_ids: HashMap<String, WindowId>,
        known_output_ids: Vec<OutputId>,
        known_output_metadata: HashMap<OutputId, SmithayKnownOutput>,
        active_output_id: Option<OutputId>,
        layer_output_ids: HashMap<String, OutputId>,
        layer_metadata: HashMap<String, LayerSurfaceMetadata>,
        layer_configures: HashMap<String, SmithayLayerSurfaceConfigureSnapshot>,
        xdg_toplevel_configures: HashMap<String, SmithayXdgToplevelConfigureSnapshot>,
        xdg_toplevel_metadata: HashMap<String, SmithayXdgToplevelMetadataSnapshot>,
        xdg_toplevel_requests: HashMap<String, SmithayXdgToplevelRequestSnapshot>,
        xdg_toplevel_decoration_policies: HashMap<String, SmithayWindowDecorationPolicySnapshot>,
        xdg_popup_configures: HashMap<String, SmithayXdgPopupConfigureSnapshot>,
        clipboard_selection: Option<SmithaySelectionOfferSnapshot>,
        clipboard_focus_client_id: Option<String>,
        primary_selection: Option<SmithaySelectionOfferSnapshot>,
        primary_focus_client_id: Option<String>,
        titlebar_render_plan: Vec<TitlebarRenderItem>,
        window_render_plan: Vec<SmithayWindowRenderSnapshot>,
        floating_window_ids: HashSet<WindowId>,
        floating_window_overrides: HashMap<WindowId, LayoutRect>,
        active_titlebar_interaction: Option<ActiveTitlebarInteraction>,
        needs_redraw: bool,
        pointer_location: Option<(f64, f64)>,
        focused_surface_id: Option<String>,
        cursor_image: String,
        cursor_surface_id: Option<String>,
        tracked_surfaces: HashMap<String, SmithayTrackedSurfaceKind>,
        mapped_surface_ids: HashSet<String>,
        popup_parent_links: HashMap<String, SmithayPopupParentLink>,
        pending_discovery_events: Vec<BackendDiscoveryEvent>,
        pending_workspace_actions: Vec<WmAction>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SmithayTrackedSurfaceKind {
        Toplevel,
        Popup,
        Layer,
        Unmanaged,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SmithaySurfaceLifecycleAction {
        Unmap,
        Remove,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SmithayPopupParentLink {
        parent: SmithayPopupParentSnapshot,
    }

    impl SpidersSmithayState {
        pub fn new(
            display: &Display<Self>,
            seat_name: impl Into<String>,
        ) -> Result<Self, SmithayStateError> {
            let display_handle = display.handle();
            let compositor_state = CompositorState::new::<Self>(&display_handle);
            let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
            let xdg_decoration_state = XdgDecorationState::new::<Self>(&display_handle);
            let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
            let layer_shell_state = WlrLayerShellState::new::<Self>(&display_handle);
            let output_manager_state =
                OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
            let presentation_state = PresentationState::new::<Self>(&display_handle, 1);
            let data_device_state = DataDeviceState::new::<Self>(&display_handle);
            let primary_selection_state = PrimarySelectionState::new::<Self>(&display_handle);
            let wlr_data_control_state = WlrDataControlState::new::<Self, _>(
                &display_handle,
                Some(&primary_selection_state),
                |_| true,
            );
            let ext_data_control_state = ExtDataControlState::new::<Self, _>(
                &display_handle,
                Some(&primary_selection_state),
                |_| true,
            );
            let workspace_manager_state = WorkspaceManagerState::new::<Self>(&display_handle);
            let mut seat_state = SeatState::new();
            let seat_name = seat_name.into();
            let mut seat = seat_state.new_wl_seat(&display_handle, seat_name.clone());
            seat.add_keyboard(XkbConfig::default(), 200, 25)?;
            seat.add_pointer();

            Ok(Self {
                display_handle,
                compositor_state,
                shm_state,
                xdg_decoration_state,
                xdg_shell_state,
                layer_shell_state,
                output_manager_state,
                presentation_state,
                data_device_state,
                primary_selection_state,
                wlr_data_control_state,
                ext_data_control_state,
                workspace_manager_state,
                seat_state,
                seat,
                seat_name: seat_name.clone(),
                next_window_serial: 1,
                known_seat_names: vec![seat_name.clone()],
                active_seat_name: Some(seat_name),
                smithay_outputs: HashMap::new(),
                toplevel_window_ids: HashMap::new(),
                known_output_ids: Vec::new(),
                known_output_metadata: HashMap::new(),
                active_output_id: None,
                layer_output_ids: HashMap::new(),
                layer_metadata: HashMap::new(),
                layer_configures: HashMap::new(),
                xdg_toplevel_configures: HashMap::new(),
                xdg_toplevel_metadata: HashMap::new(),
                xdg_toplevel_requests: HashMap::new(),
                xdg_toplevel_decoration_policies: HashMap::new(),
                xdg_popup_configures: HashMap::new(),
                clipboard_selection: None,
                clipboard_focus_client_id: None,
                primary_selection: None,
                primary_focus_client_id: None,
                titlebar_render_plan: Vec::new(),
                window_render_plan: Vec::new(),
                floating_window_ids: HashSet::new(),
                floating_window_overrides: HashMap::new(),
                active_titlebar_interaction: None,
                needs_redraw: true,
                pointer_location: None,
                focused_surface_id: None,
                cursor_image: "default".into(),
                cursor_surface_id: None,
                tracked_surfaces: HashMap::new(),
                mapped_surface_ids: HashSet::new(),
                popup_parent_links: HashMap::new(),
                pending_discovery_events: Vec::new(),
                pending_workspace_actions: Vec::new(),
            })
        }

        pub fn bind_auto_socket_source(&self) -> Result<ListeningSocketSource, SmithayStateError> {
            ListeningSocketSource::new_auto().map_err(Into::into)
        }

        pub fn take_discovery_events(&mut self) -> Vec<BackendDiscoveryEvent> {
            std::mem::take(&mut self.pending_discovery_events)
        }

        pub fn take_workspace_actions(&mut self) -> Vec<WmAction> {
            std::mem::take(&mut self.pending_workspace_actions)
        }

        fn current_seat_name(&self) -> &str {
            self.active_seat_name
                .as_deref()
                .unwrap_or(self.seat_name.as_str())
        }

        pub fn queue_workspace_action(&mut self, action: WmAction) {
            self.pending_workspace_actions.push(action);
        }

        pub fn register_seat_name(&mut self, seat_name: impl Into<String>, active: bool) {
            let seat_name = seat_name.into();
            if !self
                .known_seat_names
                .iter()
                .any(|known| known == &seat_name)
            {
                self.known_seat_names.push(seat_name.clone());
            }

            if active || self.active_seat_name.is_none() {
                self.active_seat_name = Some(seat_name.clone());
            }

            self.pending_discovery_events
                .push(BackendDiscoveryEvent::SeatDiscovered { seat_name, active });
        }

        pub fn activate_seat_name(&mut self, seat_name: &str) {
            if self.active_seat_name.as_deref() == Some(seat_name) {
                return;
            }

            if self.known_seat_names.iter().any(|known| known == seat_name) {
                self.active_seat_name = Some(seat_name.to_owned());
                self.pending_discovery_events
                    .push(BackendDiscoveryEvent::SeatDiscovered {
                        seat_name: seat_name.to_owned(),
                        active: true,
                    });
            }
        }

        pub fn remove_seat_name(&mut self, seat_name: &str) {
            let known_before = self.known_seat_names.len();
            self.known_seat_names.retain(|known| known != seat_name);
            if self.known_seat_names.len() == known_before {
                return;
            }

            if self.active_seat_name.as_deref() == Some(seat_name) {
                self.active_seat_name = self.known_seat_names.first().cloned();
            }

            self.pending_discovery_events
                .push(BackendDiscoveryEvent::SeatLost {
                    seat_name: seat_name.to_owned(),
                });
        }

        pub fn register_output_id(&mut self, output_id: OutputId, active: bool) {
            if !self
                .known_output_ids
                .iter()
                .any(|known| known == &output_id)
            {
                self.known_output_ids.push(output_id.clone());
            }

            self.known_output_metadata
                .entry(output_id.clone())
                .or_insert_with(|| SmithayKnownOutput {
                    id: output_id.clone(),
                    name: output_id.to_string(),
                    logical_width: None,
                    logical_height: None,
                    transform: OutputTransform::Normal,
                });

            if active || self.active_output_id.is_none() {
                self.active_output_id = Some(output_id);
            }

            self.refresh_workspace_output_groups();
        }

        pub fn register_output_snapshot(
            &mut self,
            output_id: OutputId,
            output_name: impl Into<String>,
            size: Option<(u32, u32)>,
            active: bool,
        ) {
            self.register_output_id(output_id.clone(), active);
            self.known_output_metadata.insert(
                output_id.clone(),
                SmithayKnownOutput {
                    id: output_id,
                    name: output_name.into(),
                    logical_width: size.map(|(width, _)| width),
                    logical_height: size.map(|(_, height)| height),
                    transform: OutputTransform::Normal,
                },
            );
        }

        pub fn register_smithay_output(
            &mut self,
            output_id: OutputId,
            output: Output,
            size: Option<(u32, u32)>,
            active: bool,
        ) {
            let output_name = output.name();
            self.smithay_outputs.insert(output_id.clone(), output);
            self.register_output_snapshot(output_id, output_name, size, active);
            self.refresh_workspace_output_groups();
        }

        pub fn update_output_size(&mut self, output_id: &OutputId, size: (u32, u32)) {
            if let Some(output) = self.smithay_outputs.get(output_id) {
                let mode = smithay::output::Mode {
                    size: (size.0 as i32, size.1 as i32).into(),
                    refresh: 60_000,
                };
                output.change_current_state(Some(mode), None, None, None);
                output.set_preferred(mode);
            }

            if let Some(metadata) = self.known_output_metadata.get_mut(output_id) {
                metadata.logical_width = Some(size.0);
                metadata.logical_height = Some(size.1);
            }

            self.needs_redraw = true;
        }

        pub fn update_active_output_size(&mut self, size: (u32, u32)) {
            let output_id = self
                .active_output_id
                .clone()
                .or_else(|| self.known_output_ids.first().cloned());

            if let Some(output_id) = output_id {
                self.update_output_size(&output_id, size);
            }
        }

        pub fn refresh_workspace_state(&mut self, snapshot: &StateSnapshot) {
            self.workspace_manager_state
                .refresh_from_snapshot::<Self>(&self.display_handle, snapshot);
            self.floating_window_ids = snapshot
                .windows
                .iter()
                .filter(|window| window.floating)
                .map(|window| window.id.clone())
                .collect();
            self.floating_window_overrides
                .retain(|window_id, _| self.floating_window_ids.contains(window_id));
        }

        pub fn refresh_workspace_output_groups(&mut self) {
            self.workspace_manager_state
                .refresh_output_groups::<Self>(&self.display_handle, &self.known_output_ids);
        }

        pub fn refresh_window_decoration_policies(
            &mut self,
            policies: &[(WindowId, SmithayWindowDecorationPolicySnapshot)],
        ) {
            let previous = self.xdg_toplevel_decoration_policies.clone();
            let mut next = HashMap::new();
            let toplevels = self
                .toplevel_window_ids
                .iter()
                .map(|(surface_id, window_id)| (surface_id.clone(), window_id.clone()))
                .collect::<Vec<_>>();

            for (surface_id, window_id) in toplevels {
                let policy = policies
                    .iter()
                    .find(|(candidate, _)| candidate == &window_id)
                    .map(|(_, policy)| policy.clone())
                    .unwrap_or_default();

                if previous.get(&surface_id) != Some(&policy) {
                    self.apply_window_decoration_policy(&surface_id, &policy);
                }

                next.insert(surface_id, policy);
            }

            self.xdg_toplevel_decoration_policies = next;
            self.needs_redraw = true;
        }

        pub fn refresh_titlebar_render_plan(&mut self, plan: &[TitlebarRenderItem]) {
            let plan = plan
                .iter()
                .cloned()
                .map(|mut item| {
                    if let Some(rect) = self.floating_window_overrides.get(&item.window_id).copied()
                    {
                        item.window_rect = rect;
                        item.titlebar_rect = LayoutRect {
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: item.titlebar_rect.height.min(rect.height.max(0.0)),
                        };
                    }
                    item
                })
                .collect::<Vec<_>>();

            if self.titlebar_render_plan != plan {
                self.titlebar_render_plan = plan;
                self.needs_redraw = true;
            }
        }

        pub fn current_titlebar_render_plan(&self) -> &[TitlebarRenderItem] {
            &self.titlebar_render_plan
        }

        pub fn refresh_window_render_plan(&mut self, plan: &[SmithayWindowRenderSnapshot]) {
            let plan = plan
                .iter()
                .cloned()
                .map(|mut item| {
                    if let Some(rect) = self.floating_window_overrides.get(&item.window_id).copied()
                    {
                        item.window_rect = rect;
                    }
                    item
                })
                .collect::<Vec<_>>();

            if self.window_render_plan != plan {
                self.window_render_plan = plan;
                self.needs_redraw = true;
            }
        }

        pub fn current_window_render_plan(&self) -> &[SmithayWindowRenderSnapshot] {
            &self.window_render_plan
        }

        pub fn is_floating_window(&self, window_id: &WindowId) -> bool {
            self.floating_window_ids.contains(window_id)
        }

        pub fn take_redraw_request(&mut self) -> bool {
            std::mem::take(&mut self.needs_redraw)
        }

        pub fn update_pointer_location(&mut self, x: f64, y: f64) {
            self.pointer_location = Some((x, y));
        }

        pub fn update_titlebar_cursor_feedback(&mut self) {
            self.cursor_image = self
                .titlebar_hit_target_at_pointer()
                .map(|hit| titlebar_cursor_name(hit.kind))
                .unwrap_or("default")
                .into();
            self.cursor_surface_id = None;
        }

        pub fn active_smithay_output(&self) -> Option<Output> {
            self.active_output_id
                .as_ref()
                .and_then(|output_id| self.smithay_outputs.get(output_id))
                .cloned()
                .or_else(|| {
                    self.known_output_ids
                        .first()
                        .and_then(|output_id| self.smithay_outputs.get(output_id))
                        .cloned()
                })
        }

        pub fn active_output_bounds(&self) -> Option<LayoutRect> {
            self.active_output_id
                .as_ref()
                .or_else(|| self.known_output_ids.first())
                .and_then(|output_id| self.known_output_metadata.get(output_id))
                .and_then(|output| {
                    Some(LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: output.logical_width? as f32,
                        height: output.logical_height? as f32,
                    })
                })
        }

        pub fn renderable_toplevel_surfaces(&self) -> Vec<SmithayRenderableToplevelSurface> {
            self.xdg_shell_state
                .toplevel_surfaces()
                .into_iter()
                .filter_map(|surface| {
                    let surface_id = smithay_surface_id(surface.wl_surface());
                    self.toplevel_window_ids
                        .get(&surface_id)
                        .cloned()
                        .map(|window_id| SmithayRenderableToplevelSurface {
                            window_id,
                            surface: surface.clone(),
                        })
                })
                .collect()
        }

        pub fn titlebar_hit_target_at_pointer(&self) -> Option<SmithayTitlebarHitTarget> {
            let (x, y) = self.pointer_location?;
            self.titlebar_render_plan.iter().find_map(|item| {
                if !layout_rect_contains(item.titlebar_rect, x, y) {
                    return None;
                }

                let edge_width = f64::from((item.titlebar_rect.width * 0.15).clamp(12.0, 32.0));
                let vertical_edge = f64::from((item.titlebar_rect.height * 0.4).clamp(8.0, 18.0));
                let relative_x = x - f64::from(item.titlebar_rect.x);
                let relative_y = y - f64::from(item.titlebar_rect.y);
                let left = relative_x <= edge_width;
                let right = relative_x >= f64::from(item.titlebar_rect.width) - edge_width;
                let top = relative_y <= vertical_edge;
                let bottom = relative_y >= f64::from(item.titlebar_rect.height) - vertical_edge;
                let kind = if top && left {
                    TitlebarHitKind::ResizeTopLeft
                } else if top && right {
                    TitlebarHitKind::ResizeTopRight
                } else if bottom && left {
                    TitlebarHitKind::ResizeBottomLeft
                } else if bottom && right {
                    TitlebarHitKind::ResizeBottomRight
                } else if top {
                    TitlebarHitKind::ResizeTop
                } else if bottom {
                    TitlebarHitKind::ResizeBottom
                } else if left {
                    TitlebarHitKind::ResizeLeft
                } else if right {
                    TitlebarHitKind::ResizeRight
                } else {
                    TitlebarHitKind::Move
                };

                Some(SmithayTitlebarHitTarget {
                    window_id: item.window_id.clone(),
                    kind,
                })
            })
        }

        pub fn begin_titlebar_interaction(&mut self, hit: &SmithayTitlebarHitTarget) -> bool {
            if !self.is_floating_window(&hit.window_id) {
                return false;
            }
            let Some(item) = self
                .titlebar_render_plan
                .iter()
                .find(|item| item.window_id == hit.window_id)
            else {
                return false;
            };
            let Some(pointer_origin) = self.pointer_location else {
                return false;
            };

            self.active_titlebar_interaction = Some(ActiveTitlebarInteraction {
                window_id: hit.window_id.clone(),
                kind: hit.kind,
                pointer_origin,
                initial_window_rect: item.window_rect,
                titlebar_height: item.titlebar_rect.height,
            });
            true
        }

        pub fn update_titlebar_interaction(&mut self) {
            let Some(interaction) = self.active_titlebar_interaction.clone() else {
                return;
            };
            let Some(pointer) = self.pointer_location else {
                return;
            };

            let dx = (pointer.0 - interaction.pointer_origin.0) as f32;
            let dy = (pointer.1 - interaction.pointer_origin.1) as f32;
            let mut rect = interaction.initial_window_rect;
            match interaction.kind {
                TitlebarHitKind::Move => {
                    rect.x += dx;
                    rect.y += dy;
                }
                TitlebarHitKind::ResizeTop => {
                    rect.y += dy;
                    rect.height -= dy;
                }
                TitlebarHitKind::ResizeBottom => {
                    rect.height += dy;
                }
                TitlebarHitKind::ResizeLeft => {
                    rect.x += dx;
                    rect.width -= dx;
                }
                TitlebarHitKind::ResizeRight => {
                    rect.width += dx;
                }
                TitlebarHitKind::ResizeTopLeft => {
                    rect.x += dx;
                    rect.width -= dx;
                    rect.y += dy;
                    rect.height -= dy;
                }
                TitlebarHitKind::ResizeTopRight => {
                    rect.width += dx;
                    rect.y += dy;
                    rect.height -= dy;
                }
                TitlebarHitKind::ResizeBottomLeft => {
                    rect.x += dx;
                    rect.width -= dx;
                    rect.height += dy;
                }
                TitlebarHitKind::ResizeBottomRight => {
                    rect.width += dx;
                    rect.height += dy;
                }
            }

            rect.width = rect.width.max(160.0);
            rect.height = rect.height.max(interaction.titlebar_height + 64.0);
            if let Some(bounds) = self.active_output_bounds() {
                rect = clamp_floating_rect(rect, bounds, interaction.titlebar_height + 64.0);
            }
            self.floating_window_overrides
                .insert(interaction.window_id.clone(), rect);
            for item in &mut self.titlebar_render_plan {
                if item.window_id == interaction.window_id {
                    item.window_rect = rect;
                    item.titlebar_rect = LayoutRect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: item.titlebar_rect.height.min(rect.height.max(0.0)),
                    };
                }
            }
            for item in &mut self.window_render_plan {
                if item.window_id == interaction.window_id {
                    item.window_rect = rect;
                }
            }
            self.needs_redraw = true;
        }

        pub fn end_titlebar_interaction(&mut self) -> Option<(WindowId, LayoutRect)> {
            let interaction = self.active_titlebar_interaction.take()?;
            let rect = self
                .floating_window_overrides
                .get(&interaction.window_id)
                .copied()
                .unwrap_or(interaction.initial_window_rect);
            Some((interaction.window_id, rect))
        }

        pub fn has_active_titlebar_interaction(&self) -> bool {
            self.active_titlebar_interaction.is_some()
        }

        pub fn focus_window_from_titlebar(&mut self, window_id: &WindowId) {
            let Some(surface) = self
                .xdg_shell_state
                .toplevel_surfaces()
                .iter()
                .find(|surface| {
                    let surface_id = smithay_surface_id(surface.wl_surface());
                    self.toplevel_window_ids.get(&surface_id) == Some(window_id)
                })
                .cloned()
            else {
                return;
            };

            let serial = SERIAL_COUNTER.next_serial();
            if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.set_focus(self, Some(surface.wl_surface().clone()), serial);
            }
            self.queue_workspace_action(WmAction::FocusWindow {
                window_id: window_id.clone(),
            });
        }

        pub fn note_titlebar_pointer_request(
            &mut self,
            window_id: &WindowId,
            kind: TitlebarHitKind,
        ) {
            let Some(surface_id) = self
                .toplevel_window_ids
                .iter()
                .find(|(_, candidate)| *candidate == window_id)
                .map(|(surface_id, _)| surface_id.clone())
            else {
                return;
            };

            let request_kind = match kind {
                TitlebarHitKind::Move => "titlebar-move",
                TitlebarHitKind::ResizeTop => "titlebar-resize-top",
                TitlebarHitKind::ResizeBottom => "titlebar-resize-bottom",
                TitlebarHitKind::ResizeLeft => "titlebar-resize-left",
                TitlebarHitKind::ResizeRight => "titlebar-resize-right",
                TitlebarHitKind::ResizeTopLeft => "titlebar-resize-top-left",
                TitlebarHitKind::ResizeTopRight => "titlebar-resize-top-right",
                TitlebarHitKind::ResizeBottomLeft => "titlebar-resize-bottom-left",
                TitlebarHitKind::ResizeBottomRight => "titlebar-resize-bottom-right",
            };
            let snapshot = self.note_xdg_toplevel_request(surface_id.clone(), request_kind);
            self.xdg_toplevel_requests.insert(surface_id, snapshot);
        }

        fn apply_window_decoration_policy(
            &mut self,
            surface_id: &str,
            policy: &SmithayWindowDecorationPolicySnapshot,
        ) {
            let Some(surface) = self
                .xdg_shell_state
                .toplevel_surfaces()
                .iter()
                .find(|surface| smithay_surface_id(surface.wl_surface()) == surface_id)
                .cloned()
            else {
                return;
            };

            let mode = if policy.decorations_visible {
                zxdg_toplevel_decoration_v1::Mode::ServerSide
            } else {
                zxdg_toplevel_decoration_v1::Mode::ClientSide
            };

            surface.with_pending_state(|state| {
                state.decoration_mode = Some(mode);
            });

            if surface.is_initial_configure_sent() {
                surface.send_pending_configure();
                let configure = self
                    .xdg_toplevel_configures
                    .get(surface_id)
                    .cloned()
                    .unwrap_or_else(default_xdg_toplevel_configure_snapshot);
                self.note_xdg_toplevel_configure_sent(
                    surface_id.to_owned(),
                    configure.activated,
                    configure.fullscreen,
                    configure.maximized,
                );
            }
        }

        pub fn backend_surface_snapshots(&self) -> Vec<BackendSurfaceSnapshot> {
            let mut snapshots = Vec::new();

            for toplevel in &self.known_surfaces_snapshot().toplevels {
                snapshots.push(BackendSurfaceSnapshot::Window {
                    surface_id: toplevel.surface_id.clone(),
                    window_id: toplevel.window_id.clone(),
                    output_id: self.focused_output_id(&toplevel.surface_id),
                });
            }

            for popup in &self.known_surfaces_snapshot().popups {
                let parent_surface_id = match &popup.parent {
                    SmithayPopupParentSnapshot::Resolved { surface_id, .. } => surface_id.clone(),
                    SmithayPopupParentSnapshot::Unresolved => {
                        format!("unresolved-parent-{}", popup.surface_id)
                    }
                };
                snapshots.push(BackendSurfaceSnapshot::Popup {
                    surface_id: popup.surface_id.clone(),
                    output_id: self.layer_output_ids.get(&popup.surface_id).cloned(),
                    parent_surface_id,
                });
            }

            for layer in &self.known_surfaces_snapshot().layers {
                snapshots.push(BackendSurfaceSnapshot::Layer {
                    surface_id: layer.surface_id.clone(),
                    output_id: layer.output_id.clone().unwrap_or_else(|| {
                        self.active_output_id
                            .clone()
                            .or_else(|| self.known_output_ids.first().cloned())
                            .unwrap_or_else(|| OutputId::from("unknown-output"))
                    }),
                    metadata: layer.metadata.clone(),
                });
            }

            for unmanaged in &self.known_surfaces_snapshot().unmanaged {
                snapshots.push(BackendSurfaceSnapshot::Unmanaged {
                    surface_id: unmanaged.surface_id.clone(),
                });
            }

            snapshots
        }

        pub fn backend_topology_snapshot(&self, generation: u64) -> BackendTopologySnapshot {
            let seats = vec![BackendSeatSnapshot {
                seat_name: self.current_seat_name().to_owned(),
                active: true,
            }];
            let seats = if self.known_seat_names.is_empty() {
                seats
            } else {
                self.known_seat_names
                    .iter()
                    .cloned()
                    .map(|seat_name| BackendSeatSnapshot {
                        active: self.active_seat_name.as_deref() == Some(seat_name.as_str()),
                        seat_name,
                    })
                    .collect()
            };

            let outputs = self
                .known_output_ids
                .iter()
                .map(|output_id| {
                    let metadata = self.known_output_metadata.get(output_id).cloned();
                    BackendOutputSnapshot {
                        snapshot: spiders_shared::wm::OutputSnapshot {
                            id: output_id.clone(),
                            name: metadata
                                .as_ref()
                                .map(|output| output.name.clone())
                                .unwrap_or_else(|| output_id.to_string()),
                            logical_width: metadata
                                .as_ref()
                                .and_then(|output| output.logical_width)
                                .unwrap_or(0),
                            logical_height: metadata
                                .as_ref()
                                .and_then(|output| output.logical_height)
                                .unwrap_or(0),
                            scale: 1,
                            transform: metadata
                                .as_ref()
                                .map(|output| output.transform)
                                .unwrap_or(OutputTransform::Normal),
                            enabled: true,
                            current_workspace_id: None,
                        },
                        active: self.active_output_id.as_ref() == Some(output_id),
                    }
                })
                .collect();

            BackendTopologySnapshot {
                source: BackendSource::Smithay,
                generation,
                seats,
                outputs,
                surfaces: self.backend_surface_snapshots(),
            }
        }

        pub fn activate_output_id(&mut self, output_id: OutputId) {
            if self.active_output_id.as_ref() == Some(&output_id) {
                return;
            }

            self.active_output_id = Some(output_id.clone());
            self.refresh_workspace_output_groups();
            self.pending_discovery_events
                .push(BackendDiscoveryEvent::OutputActivated { output_id });
        }

        pub fn remove_output_id(&mut self, output_id: &OutputId) {
            let known_before = self.known_output_ids.len();
            self.known_output_ids.retain(|known| known != output_id);

            let mut removed = self.known_output_ids.len() != known_before;
            if self.active_output_id.as_ref() == Some(output_id) {
                self.active_output_id = self.known_output_ids.first().cloned();
                removed = true;
            }

            self.known_output_metadata.remove(output_id);

            self.layer_output_ids
                .retain(|_, attached| attached != output_id);

            if removed {
                self.smithay_outputs.remove(output_id);
                self.refresh_workspace_output_groups();
                self.pending_discovery_events
                    .push(BackendDiscoveryEvent::OutputLost {
                        output_id: output_id.clone(),
                    });
            }
        }

        #[cfg(test)]
        pub(crate) fn track_test_surface_snapshot(&mut self, snapshot: BackendSurfaceSnapshot) {
            match &snapshot {
                BackendSurfaceSnapshot::Window {
                    surface_id,
                    window_id,
                    ..
                } => {
                    self.toplevel_window_ids
                        .insert(surface_id.clone(), window_id.clone());
                    self.xdg_toplevel_configures
                        .entry(surface_id.clone())
                        .or_insert_with(default_xdg_toplevel_configure_snapshot);
                    self.xdg_toplevel_metadata
                        .entry(surface_id.clone())
                        .or_insert_with(default_xdg_toplevel_metadata_snapshot);
                    self.xdg_toplevel_requests
                        .entry(surface_id.clone())
                        .or_insert_with(default_xdg_toplevel_request_snapshot);
                    self.xdg_toplevel_decoration_policies
                        .entry(surface_id.clone())
                        .or_default();
                }
                BackendSurfaceSnapshot::Popup {
                    surface_id,
                    output_id,
                    parent_surface_id,
                } => {
                    let parent = if parent_surface_id == &format!("unresolved-parent-{surface_id}")
                    {
                        SmithayPopupParentSnapshot::Unresolved
                    } else {
                        SmithayPopupParentSnapshot::Resolved {
                            surface_id: parent_surface_id.clone(),
                            window_id: self.toplevel_window_ids.get(parent_surface_id).cloned(),
                        }
                    };
                    self.popup_parent_links
                        .insert(surface_id.clone(), SmithayPopupParentLink { parent });
                    if let Some(output_id) = output_id
                        .clone()
                        .or_else(|| self.layer_output_ids.get(parent_surface_id).cloned())
                    {
                        self.layer_output_ids.insert(surface_id.clone(), output_id);
                    }
                    self.xdg_popup_configures
                        .entry(surface_id.clone())
                        .or_insert_with(default_xdg_popup_configure_snapshot);
                }
                BackendSurfaceSnapshot::Layer {
                    surface_id,
                    output_id,
                    metadata,
                } => {
                    self.layer_output_ids
                        .insert(surface_id.clone(), output_id.clone());
                    self.layer_metadata
                        .insert(surface_id.clone(), metadata.clone());
                }
                BackendSurfaceSnapshot::Unmanaged { .. } => {}
            }

            self.track_surface_snapshot(snapshot);
        }

        #[cfg(test)]
        pub(crate) fn track_test_surface_loss(&mut self, surface_id: &str) {
            if matches!(
                self.tracked_surfaces.get(surface_id),
                Some(SmithayTrackedSurfaceKind::Toplevel)
            ) {
                self.toplevel_window_ids.remove(surface_id);
                self.xdg_toplevel_decoration_policies.remove(surface_id);
            }

            self.track_surface_loss_by_id(surface_id.to_owned());
        }

        #[cfg(test)]
        pub(crate) fn track_test_surface_unmap(&mut self, surface_id: &str) {
            self.track_surface_unmap_by_id(surface_id.to_owned());
        }

        #[cfg(test)]
        pub(crate) fn layer_output_id(&self, surface_id: &str) -> Option<&OutputId> {
            self.layer_output_ids.get(surface_id)
        }

        #[cfg(test)]
        pub(crate) fn track_test_popup_parent(
            &mut self,
            surface_id: &str,
            parent_surface_id: &str,
        ) {
            self.popup_parent_links.insert(
                surface_id.to_owned(),
                SmithayPopupParentLink {
                    parent: SmithayPopupParentSnapshot::Resolved {
                        surface_id: parent_surface_id.to_owned(),
                        window_id: self.toplevel_window_ids.get(parent_surface_id).cloned(),
                    },
                },
            );
        }

        #[cfg(test)]
        pub(crate) fn track_layer_popup_surface_for_test(
            &mut self,
            parent_surface_id: &str,
            popup_surface_id: &str,
        ) {
            self.popup_parent_links.insert(
                popup_surface_id.to_owned(),
                SmithayPopupParentLink {
                    parent: SmithayPopupParentSnapshot::Resolved {
                        surface_id: parent_surface_id.to_owned(),
                        window_id: None,
                    },
                },
            );
            self.xdg_popup_configures
                .entry(popup_surface_id.to_owned())
                .or_insert_with(default_xdg_popup_configure_snapshot);

            let output_id = self.layer_output_ids.get(parent_surface_id).cloned();
            if let Some(output_id) = output_id.clone() {
                self.layer_output_ids
                    .insert(popup_surface_id.to_owned(), output_id);
            }

            self.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: popup_surface_id.to_owned(),
                output_id,
                parent_surface_id: parent_surface_id.to_owned(),
            });
        }

        #[cfg(test)]
        pub(crate) fn set_test_popup_configure_snapshot(
            &mut self,
            surface_id: &str,
            snapshot: SmithayXdgPopupConfigureSnapshot,
        ) {
            self.xdg_popup_configures
                .insert(surface_id.to_owned(), snapshot);
        }

        #[cfg(test)]
        pub(crate) fn record_test_xdg_popup_configure_sent(
            &mut self,
            surface_id: &str,
            reposition_token: Option<u32>,
            reactive: bool,
            geometry: (i32, i32, i32, i32),
        ) {
            self.note_xdg_popup_configure_sent(
                surface_id.to_owned(),
                reposition_token,
                reactive,
                geometry,
            );
        }

        #[cfg(test)]
        pub(crate) fn record_test_initial_xdg_popup_configure_sent(
            &mut self,
            surface_id: &str,
            reactive: bool,
            geometry: (i32, i32, i32, i32),
        ) {
            self.record_xdg_popup_configure_sent(surface_id.to_owned(), None, reactive, geometry);
        }

        #[cfg(test)]
        pub(crate) fn record_test_xdg_popup_configure_acked(
            &mut self,
            surface_id: &str,
            serial: u32,
            reposition_token: Option<u32>,
            reactive: bool,
            geometry: (i32, i32, i32, i32),
        ) {
            self.acknowledge_xdg_popup_configure(
                surface_id.to_owned(),
                serial,
                reposition_token,
                reactive,
                geometry,
            );
        }

        #[cfg(test)]
        pub(crate) fn record_test_xdg_popup_request(
            &mut self,
            surface_id: &str,
            request_kind: &str,
            update: impl FnOnce(&mut SmithayXdgPopupConfigureSnapshot),
        ) {
            let mut snapshot = self.note_xdg_popup_request(surface_id.to_owned(), request_kind);
            update(&mut snapshot);
            self.xdg_popup_configures
                .insert(surface_id.to_owned(), snapshot);
        }

        #[cfg(test)]
        pub(crate) fn set_test_layer_configure_snapshot(
            &mut self,
            surface_id: &str,
            snapshot: SmithayLayerSurfaceConfigureSnapshot,
        ) {
            self.layer_configures
                .insert(surface_id.to_owned(), snapshot);
        }

        #[cfg(test)]
        pub(crate) fn record_test_layer_configure_sent(
            &mut self,
            surface_id: &str,
            configured_size: Option<(i32, i32)>,
        ) {
            self.note_layer_configure_sent(surface_id.to_owned(), configured_size);
        }

        #[cfg(test)]
        pub(crate) fn record_test_layer_configure_acked(
            &mut self,
            surface_id: &str,
            serial: u32,
            configured_size: Option<(i32, i32)>,
        ) {
            self.acknowledge_layer_configure(surface_id.to_owned(), serial, configured_size);
        }

        #[cfg(test)]
        pub(crate) fn set_test_toplevel_metadata_snapshot(
            &mut self,
            surface_id: &str,
            snapshot: SmithayXdgToplevelMetadataSnapshot,
        ) {
            self.xdg_toplevel_metadata
                .insert(surface_id.to_owned(), snapshot);
        }

        #[cfg(test)]
        pub(crate) fn set_test_toplevel_configure_snapshot(
            &mut self,
            surface_id: &str,
            snapshot: SmithayXdgToplevelConfigureSnapshot,
        ) {
            self.xdg_toplevel_configures
                .insert(surface_id.to_owned(), snapshot);
        }

        #[cfg(test)]
        pub(crate) fn set_test_toplevel_request_snapshot(
            &mut self,
            surface_id: &str,
            snapshot: SmithayXdgToplevelRequestSnapshot,
        ) {
            self.xdg_toplevel_requests
                .insert(surface_id.to_owned(), snapshot);
        }

        #[cfg(test)]
        pub(crate) fn record_test_xdg_toplevel_configure_sent(
            &mut self,
            surface_id: &str,
            activated: bool,
            fullscreen: bool,
            maximized: bool,
        ) {
            self.note_xdg_toplevel_configure_sent(
                surface_id.to_owned(),
                activated,
                fullscreen,
                maximized,
            );
        }

        #[cfg(test)]
        pub(crate) fn record_test_xdg_toplevel_configure_acked(
            &mut self,
            surface_id: &str,
            serial: u32,
            activated: bool,
            fullscreen: bool,
            maximized: bool,
        ) {
            self.acknowledge_xdg_toplevel_configure(
                surface_id.to_owned(),
                serial,
                activated,
                fullscreen,
                maximized,
            );
        }

        #[cfg(test)]
        pub(crate) fn set_test_clipboard_selection(
            &mut self,
            selection: Option<SmithaySelectionOfferSnapshot>,
        ) {
            self.clipboard_selection = selection;
        }

        #[cfg(test)]
        pub(crate) fn set_test_clipboard_focus_client_id(&mut self, client_id: Option<&str>) {
            self.clipboard_focus_client_id = client_id.map(str::to_owned);
        }

        #[cfg(test)]
        pub(crate) fn set_test_primary_selection(
            &mut self,
            selection: Option<SmithaySelectionOfferSnapshot>,
        ) {
            self.primary_selection = selection;
        }

        #[cfg(test)]
        pub(crate) fn set_test_primary_focus_client_id(&mut self, client_id: Option<&str>) {
            self.primary_focus_client_id = client_id.map(str::to_owned);
        }

        #[cfg(test)]
        pub(crate) fn set_test_focused_surface_id(&mut self, surface_id: Option<&str>) {
            self.focused_surface_id = surface_id.map(str::to_owned);
        }

        #[cfg(test)]
        pub(crate) fn record_test_seat_focus_event(&mut self, surface_id: Option<&str>) {
            self.focused_surface_id = surface_id.map(str::to_owned);
            let focused_window_id = self
                .focused_surface_id
                .as_ref()
                .and_then(|surface_id| self.focused_window_id(surface_id));
            let focused_output_id = self
                .focused_surface_id
                .as_ref()
                .and_then(|surface_id| self.focused_output_id(surface_id));
            self.pending_discovery_events
                .push(BackendDiscoveryEvent::SeatFocusChanged {
                    seat_name: self.current_seat_name().to_owned(),
                    window_id: focused_window_id,
                    output_id: focused_output_id,
                });
        }

        #[cfg(test)]
        pub(crate) fn set_test_cursor_image(
            &mut self,
            cursor_image: impl Into<String>,
            cursor_surface_id: Option<&str>,
        ) {
            self.cursor_image = cursor_image.into();
            self.cursor_surface_id = cursor_surface_id.map(str::to_owned);
        }

        pub fn snapshot(&self) -> SmithayStateSnapshot {
            let role_counts = self.role_counts();
            let known_surfaces = self.known_surfaces_snapshot();
            let focused_surface_role = self
                .focused_surface_id
                .as_ref()
                .and_then(|surface_id| self.focused_surface_role(surface_id));
            let focused_window_id = self
                .focused_surface_id
                .as_ref()
                .and_then(|surface_id| self.focused_window_id(surface_id));
            let focused_output_id = self
                .focused_surface_id
                .as_ref()
                .and_then(|surface_id| self.focused_output_id(surface_id));
            SmithayStateSnapshot {
                seat_name: self.current_seat_name().to_owned(),
                seat: SmithaySeatSnapshot {
                    name: self.current_seat_name().to_owned(),
                    has_keyboard: self.seat.get_keyboard().is_some(),
                    has_pointer: self.seat.get_pointer().is_some(),
                    has_touch: self.seat.get_touch().is_some(),
                    focused_surface_id: self.focused_surface_id.clone(),
                    focused_surface_role,
                    focused_window_id,
                    focused_output_id,
                    cursor_image: self.cursor_image.clone(),
                    cursor_surface_id: self.cursor_surface_id.clone(),
                },
                outputs: SmithayOutputSnapshot {
                    known_output_ids: self.known_output_ids.clone(),
                    known_outputs: self
                        .known_output_ids
                        .iter()
                        .filter_map(|output_id| self.known_output_metadata.get(output_id).cloned())
                        .collect(),
                    active_output_id: self.active_output_id.clone(),
                    layer_surface_output_count: self.layer_output_ids.len(),
                    active_output_attached_surface_count: self
                        .active_output_id
                        .as_ref()
                        .map(|active_output_id| {
                            self.layer_output_ids
                                .values()
                                .filter(|output_id| *output_id == active_output_id)
                                .count()
                        })
                        .unwrap_or(0),
                    mapped_surface_count: self.mapped_surface_ids.len(),
                },
                tracked_surface_count: self.tracked_surfaces.len(),
                tracked_toplevel_count: self.toplevel_window_ids.len(),
                pending_discovery_event_count: self.pending_discovery_events.len(),
                role_counts,
                known_surfaces,
                selection_protocols: SmithaySelectionProtocolSupportSnapshot {
                    data_device: true,
                    primary_selection: true,
                    wlr_data_control: true,
                    ext_data_control: true,
                },
                clipboard_selection: SmithayClipboardSelectionSnapshot {
                    seat_name: self.current_seat_name().to_owned(),
                    target: "clipboard".into(),
                    selection: self.clipboard_selection.clone(),
                    focused_client_id: self.clipboard_focus_client_id.clone(),
                },
                primary_selection: SmithayPrimarySelectionSnapshot {
                    seat_name: self.current_seat_name().to_owned(),
                    target: "primary".into(),
                    selection: self.primary_selection.clone(),
                    focused_client_id: self.primary_focus_client_id.clone(),
                },
            }
        }

        fn track_surface_snapshot(&mut self, snapshot: BackendSurfaceSnapshot) {
            let (surface_id, kind) = match &snapshot {
                BackendSurfaceSnapshot::Window { surface_id, .. } => {
                    (surface_id.clone(), SmithayTrackedSurfaceKind::Toplevel)
                }
                BackendSurfaceSnapshot::Popup { surface_id, .. } => {
                    (surface_id.clone(), SmithayTrackedSurfaceKind::Popup)
                }
                BackendSurfaceSnapshot::Layer { surface_id, .. } => {
                    (surface_id.clone(), SmithayTrackedSurfaceKind::Layer)
                }
                BackendSurfaceSnapshot::Unmanaged { surface_id } => {
                    (surface_id.clone(), SmithayTrackedSurfaceKind::Unmanaged)
                }
            };

            if let BackendSurfaceSnapshot::Layer {
                surface_id,
                output_id,
                metadata,
            } = &snapshot
            {
                self.layer_output_ids
                    .insert(surface_id.clone(), output_id.clone());
                self.layer_metadata
                    .insert(surface_id.clone(), metadata.clone());
            }

            if let BackendSurfaceSnapshot::Popup {
                surface_id,
                output_id,
                parent_surface_id,
            } = &snapshot
            {
                if let Some(output_id) = output_id
                    .clone()
                    .or_else(|| self.layer_output_ids.get(parent_surface_id).cloned())
                {
                    self.layer_output_ids.insert(surface_id.clone(), output_id);
                }
            }

            if self.tracked_surfaces.contains_key(&surface_id) {
                if self.mapped_surface_ids.insert(surface_id) {
                    self.pending_discovery_events
                        .push(snapshot_into_discovery_event(snapshot));
                }
                return;
            }

            self.mapped_surface_ids.insert(surface_id.clone());
            self.tracked_surfaces.insert(surface_id, kind);

            self.pending_discovery_events
                .push(snapshot_into_discovery_event(snapshot));
        }

        fn track_surface_unmap_by_id(&mut self, surface_id: String) {
            self.track_surface_lifecycle_by_id(surface_id, SmithaySurfaceLifecycleAction::Unmap);
        }

        fn track_surface_loss_by_id(&mut self, surface_id: String) {
            self.track_surface_lifecycle_by_id(surface_id, SmithaySurfaceLifecycleAction::Remove);
        }

        fn track_surface_lifecycle_by_id(
            &mut self,
            surface_id: String,
            action: SmithaySurfaceLifecycleAction,
        ) {
            if self.tracked_surfaces.contains_key(&surface_id) {
                let focus_cleared = self.focused_surface_id.as_deref() == Some(surface_id.as_str());
                if focus_cleared {
                    self.focused_surface_id = None;
                }
                if self.cursor_surface_id.as_deref() == Some(surface_id.as_str()) {
                    self.cursor_surface_id = None;
                    self.cursor_image = "default".into();
                }

                match action {
                    SmithaySurfaceLifecycleAction::Unmap => {
                        if self.mapped_surface_ids.remove(&surface_id) {
                            self.pending_discovery_events
                                .push(BackendDiscoveryEvent::SurfaceUnmapped { surface_id });
                        }
                    }
                    SmithaySurfaceLifecycleAction::Remove => {
                        self.tracked_surfaces.remove(&surface_id);
                        self.mapped_surface_ids.remove(&surface_id);
                        self.popup_parent_links.remove(&surface_id);
                        self.layer_output_ids.remove(&surface_id);
                        self.layer_metadata.remove(&surface_id);
                        self.layer_configures.remove(&surface_id);
                        self.xdg_toplevel_configures.remove(&surface_id);
                        self.xdg_toplevel_metadata.remove(&surface_id);
                        self.xdg_toplevel_requests.remove(&surface_id);
                        self.xdg_popup_configures.remove(&surface_id);
                        self.pending_discovery_events
                            .push(BackendDiscoveryEvent::SurfaceLost { surface_id });
                    }
                }

                if focus_cleared {
                    self.pending_discovery_events
                        .push(BackendDiscoveryEvent::SeatFocusChanged {
                            seat_name: self.current_seat_name().to_owned(),
                            window_id: None,
                            output_id: None,
                        });
                }
            }
        }

        fn track_layer_surface(
            &mut self,
            surface: &LayerSurface,
            output: Option<WlOutput>,
            layer: WlrLayer,
            namespace: String,
        ) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            let output_id = self.resolve_layer_output_id(output.as_ref());
            let metadata = layer_surface_metadata(surface, namespace, layer);
            if let Some(output_id) = output_id.as_ref() {
                self.layer_output_ids
                    .insert(surface_id.clone(), output_id.clone());
            }
            self.layer_metadata
                .insert(surface_id.clone(), metadata.clone());
            self.layer_configures.insert(
                surface_id.clone(),
                layer_surface_configure_snapshot_for(surface),
            );

            self.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id,
                output_id: output_id.unwrap_or_else(|| OutputId::from("unassigned-layer-output")),
                metadata,
            });
        }

        fn note_layer_configure_sent(
            &mut self,
            surface_id: String,
            configured_size: Option<(i32, i32)>,
        ) {
            let mut snapshot = self
                .layer_configures
                .get(&surface_id)
                .cloned()
                .unwrap_or_else(default_layer_surface_configure_snapshot);
            snapshot.pending_configure_count += 1;
            snapshot.last_configured_size = configured_size.or(snapshot.last_configured_size);
            self.layer_configures.insert(surface_id, snapshot);
        }

        fn acknowledge_layer_configure(
            &mut self,
            surface_id: String,
            serial: u32,
            configured_size: Option<(i32, i32)>,
        ) {
            let mut snapshot = self
                .layer_configures
                .get(&surface_id)
                .cloned()
                .unwrap_or_else(default_layer_surface_configure_snapshot);
            snapshot.last_acked_serial = Some(serial);
            snapshot.pending_configure_count = snapshot.pending_configure_count.saturating_sub(1);
            snapshot.last_configured_size = configured_size.or(snapshot.last_configured_size);
            self.layer_configures.insert(surface_id, snapshot);
        }

        #[cfg(test)]
        fn note_xdg_popup_configure_sent(
            &mut self,
            surface_id: String,
            reposition_token: Option<u32>,
            reactive: bool,
            geometry: (i32, i32, i32, i32),
        ) {
            let snapshot = self.note_xdg_popup_request(surface_id.clone(), "reposition");
            self.xdg_popup_configures
                .insert(surface_id.clone(), snapshot);
            self.record_xdg_popup_configure_sent(surface_id, reposition_token, reactive, geometry);
        }

        fn record_xdg_popup_configure_sent(
            &mut self,
            surface_id: String,
            reposition_token: Option<u32>,
            reactive: bool,
            geometry: (i32, i32, i32, i32),
        ) {
            let mut snapshot = self
                .xdg_popup_configures
                .get(&surface_id)
                .cloned()
                .unwrap_or_else(default_xdg_popup_configure_snapshot);
            snapshot.pending_configure_count += 1;
            snapshot.last_reposition_token = reposition_token.or(snapshot.last_reposition_token);
            snapshot.reactive = reactive;
            snapshot.geometry = geometry;
            self.xdg_popup_configures.insert(surface_id, snapshot);
        }

        fn acknowledge_xdg_popup_configure(
            &mut self,
            surface_id: String,
            serial: u32,
            reposition_token: Option<u32>,
            reactive: bool,
            geometry: (i32, i32, i32, i32),
        ) {
            let mut snapshot = self
                .xdg_popup_configures
                .get(&surface_id)
                .cloned()
                .unwrap_or_else(default_xdg_popup_configure_snapshot);
            snapshot.last_acked_serial = Some(serial);
            snapshot.pending_configure_count = snapshot.pending_configure_count.saturating_sub(1);
            snapshot.last_reposition_token = reposition_token.or(snapshot.last_reposition_token);
            snapshot.reactive = reactive;
            snapshot.geometry = geometry;
            self.xdg_popup_configures.insert(surface_id, snapshot);
        }

        fn note_xdg_popup_request(
            &mut self,
            surface_id: String,
            request_kind: &str,
        ) -> SmithayXdgPopupConfigureSnapshot {
            let mut snapshot = self
                .xdg_popup_configures
                .get(&surface_id)
                .cloned()
                .unwrap_or_else(default_xdg_popup_configure_snapshot);
            snapshot.last_request_kind = Some(request_kind.to_owned());
            snapshot.request_count += 1;
            snapshot
        }

        fn note_xdg_toplevel_configure_sent(
            &mut self,
            surface_id: String,
            activated: bool,
            fullscreen: bool,
            maximized: bool,
        ) {
            let mut snapshot = self
                .xdg_toplevel_configures
                .get(&surface_id)
                .cloned()
                .unwrap_or_else(default_xdg_toplevel_configure_snapshot);
            snapshot.pending_configure_count += 1;
            snapshot.activated = activated;
            snapshot.fullscreen = fullscreen;
            snapshot.maximized = maximized;
            self.xdg_toplevel_configures.insert(surface_id, snapshot);
        }

        fn acknowledge_xdg_toplevel_configure(
            &mut self,
            surface_id: String,
            serial: u32,
            activated: bool,
            fullscreen: bool,
            maximized: bool,
        ) {
            let mut snapshot = self
                .xdg_toplevel_configures
                .get(&surface_id)
                .cloned()
                .unwrap_or_else(default_xdg_toplevel_configure_snapshot);
            snapshot.last_acked_serial = Some(serial);
            snapshot.pending_configure_count = snapshot.pending_configure_count.saturating_sub(1);
            snapshot.activated = activated;
            snapshot.fullscreen = fullscreen;
            snapshot.maximized = maximized;
            self.xdg_toplevel_configures.insert(surface_id, snapshot);
        }

        fn note_xdg_toplevel_request(
            &mut self,
            surface_id: String,
            request_kind: &str,
        ) -> SmithayXdgToplevelRequestSnapshot {
            let mut snapshot = self
                .xdg_toplevel_requests
                .get(&surface_id)
                .cloned()
                .unwrap_or_else(default_xdg_toplevel_request_snapshot);
            snapshot.last_request_kind = Some(request_kind.to_owned());
            snapshot.request_count += 1;
            snapshot
        }

        fn track_toplevel_surface(&mut self, surface: &WlSurface) {
            let surface_id = smithay_surface_id(surface);
            let window_id = self.window_id_for_surface(&surface_id);
            self.xdg_toplevel_configures.insert(
                surface_id.clone(),
                xdg_toplevel_configure_snapshot_for(surface),
            );
            self.xdg_toplevel_metadata.insert(
                surface_id.clone(),
                xdg_toplevel_metadata_snapshot_for(surface),
            );
            self.xdg_toplevel_decoration_policies
                .entry(surface_id.clone())
                .or_default();
            self.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                window_id,
                surface_id,
                output_id: None,
            });
        }

        fn track_popup_surface(&mut self, surface: &PopupSurface) {
            let wl_surface = surface.wl_surface();
            let surface_id = smithay_surface_id(wl_surface);
            let parent = surface
                .get_parent_surface()
                .map(|parent| {
                    let parent_surface_id = smithay_surface_id(&parent);
                    SmithayPopupParentSnapshot::Resolved {
                        window_id: self.toplevel_window_ids.get(&parent_surface_id).cloned(),
                        surface_id: parent_surface_id,
                    }
                })
                .unwrap_or(SmithayPopupParentSnapshot::Unresolved);

            self.popup_parent_links.insert(
                surface_id.clone(),
                SmithayPopupParentLink {
                    parent: parent.clone(),
                },
            );
            self.xdg_popup_configures.insert(
                surface_id.clone(),
                xdg_popup_configure_snapshot_for(surface),
            );

            let parent_surface_id = popup_parent_surface_id(&parent, &surface_id);
            let output_id = match &parent {
                SmithayPopupParentSnapshot::Resolved { surface_id, .. } => {
                    self.layer_output_ids.get(surface_id).cloned()
                }
                SmithayPopupParentSnapshot::Unresolved => None,
            };

            self.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id,
                output_id,
                parent_surface_id,
            });
        }

        fn track_layer_popup_surface(&mut self, parent: &LayerSurface, popup: &PopupSurface) {
            let parent_surface_id = smithay_surface_id(parent.wl_surface());
            let popup_surface_id = smithay_surface_id(popup.wl_surface());

            self.popup_parent_links.insert(
                popup_surface_id.clone(),
                SmithayPopupParentLink {
                    parent: SmithayPopupParentSnapshot::Resolved {
                        surface_id: parent_surface_id.clone(),
                        window_id: None,
                    },
                },
            );
            self.xdg_popup_configures
                .entry(popup_surface_id.clone())
                .or_insert_with(default_xdg_popup_configure_snapshot);
            if let Some(output_id) = self.layer_output_ids.get(&parent_surface_id).cloned() {
                self.layer_output_ids
                    .insert(popup_surface_id.clone(), output_id.clone());
                self.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                    surface_id: popup_surface_id,
                    output_id: Some(output_id),
                    parent_surface_id,
                });
            } else {
                self.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                    surface_id: popup_surface_id,
                    output_id: None,
                    parent_surface_id,
                });
            }
        }

        fn track_toplevel_surface_loss(&mut self, surface: &ToplevelSurface) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            self.toplevel_window_ids.remove(&surface_id);
            self.xdg_toplevel_decoration_policies.remove(&surface_id);
            self.track_surface_loss_by_id(surface_id);
        }

        fn track_popup_surface_loss(&mut self, surface: &PopupSurface) {
            self.track_surface_loss_by_id(smithay_surface_id(surface.wl_surface()));
        }

        fn track_committed_surface(&mut self, surface: &WlSurface) {
            if is_sync_subsurface(surface) {
                return;
            }

            self.needs_redraw = true;

            let root = root_surface(surface);
            let role = get_role(&root);
            let surface_id = smithay_surface_id(&root);

            match role {
                Some(XDG_TOPLEVEL_ROLE) => {
                    if surface_has_buffer(&root) {
                        self.track_toplevel_surface(&root);
                    } else if self.tracked_surfaces.contains_key(&surface_id) {
                        self.track_surface_unmap_by_id(surface_id);
                    }
                }
                Some(XDG_POPUP_ROLE) => {
                    if surface_has_buffer(&root) {
                        self.track_popup_surface_by_root(&root);
                    } else if self.tracked_surfaces.contains_key(&surface_id) {
                        self.track_surface_unmap_by_id(surface_id);
                    }
                }
                _ if is_layer_surface(&root) => {
                    if surface_has_buffer(&root) {
                        if let Some(output_id) = self.layer_output_ids.get(&surface_id).cloned() {
                            let metadata = self
                                .layer_metadata
                                .get(&surface_id)
                                .cloned()
                                .unwrap_or_else(default_layer_surface_metadata);
                            self.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                                surface_id,
                                output_id,
                                metadata,
                            });
                        }
                    } else if self.tracked_surfaces.contains_key(&surface_id) {
                        self.track_surface_unmap_by_id(surface_id);
                    }
                }
                _ => {
                    if surface_has_buffer(&root) {
                        self.track_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                            surface_id,
                        });
                    }
                }
            }
        }

        fn track_popup_surface_by_root(&mut self, surface: &WlSurface) {
            let surface_id = smithay_surface_id(surface);
            let parent = with_states(surface, |states| {
                states
                    .data_map
                    .get::<XdgPopupSurfaceData>()
                    .and_then(|data| {
                        data.lock().ok().and_then(|data| {
                            data.parent.clone().map(|parent| {
                                let parent_surface_id = smithay_surface_id(&parent);
                                SmithayPopupParentSnapshot::Resolved {
                                    window_id: self
                                        .toplevel_window_ids
                                        .get(&parent_surface_id)
                                        .cloned(),
                                    surface_id: parent_surface_id,
                                }
                            })
                        })
                    })
            })
            .unwrap_or(SmithayPopupParentSnapshot::Unresolved);

            self.popup_parent_links.insert(
                surface_id.clone(),
                SmithayPopupParentLink {
                    parent: parent.clone(),
                },
            );
            self.xdg_popup_configures
                .entry(surface_id.clone())
                .or_insert_with(default_xdg_popup_configure_snapshot);

            let parent_surface_id = popup_parent_surface_id(&parent, &surface_id);
            let output_id = match &parent {
                SmithayPopupParentSnapshot::Resolved { surface_id, .. } => {
                    self.layer_output_ids.get(surface_id).cloned()
                }
                SmithayPopupParentSnapshot::Unresolved => None,
            };

            self.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id,
                output_id,
                parent_surface_id,
            });
        }

        fn resolve_layer_output_id(&self, output: Option<&WlOutput>) -> Option<OutputId> {
            output
                .and_then(Output::from_resource)
                .map(|output| OutputId::from(output.name()))
                .or_else(|| self.active_output_id.clone())
                .or_else(|| self.known_output_ids.first().cloned())
        }

        fn window_id_for_surface(&mut self, surface_id: &str) -> WindowId {
            if let Some(window_id) = self.toplevel_window_ids.get(surface_id) {
                return window_id.clone();
            }

            let window_id = WindowId::from(format!("smithay-window-{}", self.next_window_serial));
            self.next_window_serial += 1;
            self.toplevel_window_ids
                .insert(surface_id.to_owned(), window_id.clone());
            window_id
        }

        fn role_counts(&self) -> SmithaySurfaceRoleCounts {
            let mut counts = SmithaySurfaceRoleCounts {
                toplevel: 0,
                popup: 0,
                unmanaged: 0,
                layer: 0,
            };

            for kind in self.tracked_surfaces.values() {
                match kind {
                    SmithayTrackedSurfaceKind::Toplevel => counts.toplevel += 1,
                    SmithayTrackedSurfaceKind::Popup => counts.popup += 1,
                    SmithayTrackedSurfaceKind::Unmanaged => counts.unmanaged += 1,
                    SmithayTrackedSurfaceKind::Layer => counts.layer += 1,
                }
            }

            counts
        }

        fn focused_surface_role(&self, surface_id: &str) -> Option<String> {
            self.tracked_surfaces
                .get(surface_id)
                .map(|kind| match kind {
                    SmithayTrackedSurfaceKind::Toplevel => "toplevel".into(),
                    SmithayTrackedSurfaceKind::Popup => "popup".into(),
                    SmithayTrackedSurfaceKind::Unmanaged => "unmanaged".into(),
                    SmithayTrackedSurfaceKind::Layer => "layer".into(),
                })
        }

        fn focused_window_id(&self, surface_id: &str) -> Option<WindowId> {
            self.toplevel_window_ids
                .get(surface_id)
                .cloned()
                .or_else(|| {
                    self.popup_parent_links.get(surface_id).and_then(|parent| {
                        match &parent.parent {
                            SmithayPopupParentSnapshot::Resolved { window_id, .. } => {
                                window_id.clone()
                            }
                            SmithayPopupParentSnapshot::Unresolved => None,
                        }
                    })
                })
        }

        fn focused_output_id(&self, surface_id: &str) -> Option<OutputId> {
            self.layer_output_ids.get(surface_id).cloned().or_else(|| {
                self.popup_parent_links
                    .get(surface_id)
                    .and_then(|parent| match &parent.parent {
                        SmithayPopupParentSnapshot::Resolved { surface_id, .. } => {
                            self.layer_output_ids.get(surface_id).cloned()
                        }
                        SmithayPopupParentSnapshot::Unresolved => None,
                    })
            })
        }

        fn known_surfaces_snapshot(&self) -> SmithayKnownSurfacesSnapshot {
            let mut toplevels = self
                .toplevel_window_ids
                .iter()
                .map(|(surface_id, window_id)| {
                    let decoration_policy = self
                        .xdg_toplevel_decoration_policies
                        .get(surface_id)
                        .cloned()
                        .unwrap_or_default();
                    let metadata = self
                        .xdg_toplevel_metadata
                        .get(surface_id)
                        .cloned()
                        .unwrap_or_else(default_xdg_toplevel_metadata_snapshot);

                    SmithayKnownToplevelSurface {
                        surface_id: surface_id.clone(),
                        window_id: window_id.clone(),
                        titlebar: titlebar_render_snapshot(&decoration_policy, &metadata),
                        decoration_policy,
                        configure: self
                            .xdg_toplevel_configures
                            .get(surface_id)
                            .cloned()
                            .unwrap_or_else(default_xdg_toplevel_configure_snapshot),
                        metadata,
                        requests: self
                            .xdg_toplevel_requests
                            .get(surface_id)
                            .cloned()
                            .unwrap_or_else(default_xdg_toplevel_request_snapshot),
                    }
                })
                .collect::<Vec<_>>();
            toplevels.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));

            let mut popups = self
                .tracked_surfaces
                .iter()
                .filter_map(|(surface_id, kind)| {
                    (*kind == SmithayTrackedSurfaceKind::Popup).then(|| {
                        let parent = self.popup_parent_links.get(surface_id);
                        SmithayKnownPopupSurface {
                            surface_id: surface_id.clone(),
                            parent: parent
                                .map(|parent| parent.parent.clone())
                                .unwrap_or(SmithayPopupParentSnapshot::Unresolved),
                            configure: self
                                .xdg_popup_configures
                                .get(surface_id)
                                .cloned()
                                .unwrap_or_else(default_xdg_popup_configure_snapshot),
                        }
                    })
                })
                .collect::<Vec<_>>();
            popups.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));

            let mut unmanaged = self
                .tracked_surfaces
                .iter()
                .filter_map(|(surface_id, kind)| {
                    (*kind == SmithayTrackedSurfaceKind::Unmanaged).then(|| {
                        SmithayKnownUnmanagedSurface {
                            surface_id: surface_id.clone(),
                        }
                    })
                })
                .collect::<Vec<_>>();
            unmanaged.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));

            let mut layers = self
                .tracked_surfaces
                .iter()
                .filter_map(|(surface_id, kind)| {
                    (*kind == SmithayTrackedSurfaceKind::Layer).then(|| SmithayKnownLayerSurface {
                        surface_id: surface_id.clone(),
                        output_id: self.layer_output_ids.get(surface_id).cloned(),
                        metadata: self
                            .layer_metadata
                            .get(surface_id)
                            .cloned()
                            .unwrap_or_else(default_layer_surface_metadata),
                        configure: self
                            .layer_configures
                            .get(surface_id)
                            .cloned()
                            .unwrap_or_else(default_layer_surface_configure_snapshot),
                    })
                })
                .collect::<Vec<_>>();
            layers.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));

            let mut all =
                Vec::with_capacity(toplevels.len() + popups.len() + unmanaged.len() + layers.len());
            all.extend(toplevels.iter().cloned().map(SmithayKnownSurface::Toplevel));
            all.extend(popups.iter().cloned().map(SmithayKnownSurface::Popup));
            all.extend(layers.iter().cloned().map(SmithayKnownSurface::Layer));
            all.extend(
                unmanaged
                    .iter()
                    .cloned()
                    .map(SmithayKnownSurface::Unmanaged),
            );
            all.sort_by(|left, right| {
                known_surface_sort_key(left).cmp(&known_surface_sort_key(right))
            });

            SmithayKnownSurfacesSnapshot {
                all,
                toplevels,
                popups,
                unmanaged,
                layers,
            }
        }
    }

    fn known_surface_sort_key(surface: &SmithayKnownSurface) -> (&'static str, &str) {
        match surface {
            SmithayKnownSurface::Toplevel(surface) => ("toplevel", &surface.surface_id),
            SmithayKnownSurface::Popup(surface) => ("popup", &surface.surface_id),
            SmithayKnownSurface::Layer(surface) => ("layer", &surface.surface_id),
            SmithayKnownSurface::Unmanaged(surface) => ("unmanaged", &surface.surface_id),
        }
    }

    fn smithay_surface_id(surface: &WlSurface) -> String {
        format!("wl-surface-{}", surface.id().protocol_id())
    }

    fn root_surface(surface: &WlSurface) -> WlSurface {
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        root
    }

    fn surface_has_buffer(surface: &WlSurface) -> bool {
        with_states(surface, |states| {
            let mut attributes = states
                .cached_state
                .get::<smithay::wayland::compositor::SurfaceAttributes>();
            let pending = matches!(
                attributes.pending().buffer,
                Some(BufferAssignment::NewBuffer(_))
            );
            let current = matches!(
                attributes.current().buffer,
                Some(BufferAssignment::NewBuffer(_))
            );
            pending || current
        })
    }

    fn popup_parent_surface_id(parent: &SmithayPopupParentSnapshot, surface_id: &str) -> String {
        match parent {
            SmithayPopupParentSnapshot::Resolved { surface_id, .. } => surface_id.clone(),
            SmithayPopupParentSnapshot::Unresolved => format!("unresolved-parent-{surface_id}"),
        }
    }

    fn is_layer_surface(surface: &WlSurface) -> bool {
        with_states(surface, |states| {
            states.data_map.get::<LayerSurfaceData>().is_some()
        })
    }

    fn layer_surface_tier(layer: WlrLayer) -> LayerSurfaceTier {
        match layer {
            WlrLayer::Background => LayerSurfaceTier::Background,
            WlrLayer::Bottom => LayerSurfaceTier::Bottom,
            WlrLayer::Top => LayerSurfaceTier::Top,
            WlrLayer::Overlay => LayerSurfaceTier::Overlay,
        }
    }

    fn layer_keyboard_interactivity(
        interactivity: WlrKeyboardInteractivity,
    ) -> LayerKeyboardInteractivity {
        match interactivity {
            WlrKeyboardInteractivity::None => LayerKeyboardInteractivity::None,
            WlrKeyboardInteractivity::Exclusive => LayerKeyboardInteractivity::Exclusive,
            WlrKeyboardInteractivity::OnDemand => LayerKeyboardInteractivity::OnDemand,
        }
    }

    fn layer_exclusive_zone(zone: WlrExclusiveZone) -> LayerExclusiveZone {
        match zone {
            WlrExclusiveZone::Neutral => LayerExclusiveZone::Neutral,
            WlrExclusiveZone::Exclusive(value) => LayerExclusiveZone::Exclusive(value),
            WlrExclusiveZone::DontCare => LayerExclusiveZone::DontCare,
        }
    }

    fn layer_surface_metadata(
        surface: &LayerSurface,
        namespace: String,
        layer: WlrLayer,
    ) -> LayerSurfaceMetadata {
        surface.with_cached_state(|state| LayerSurfaceMetadata {
            namespace,
            tier: layer_surface_tier(layer),
            keyboard_interactivity: layer_keyboard_interactivity(state.keyboard_interactivity),
            exclusive_zone: layer_exclusive_zone(state.exclusive_zone),
        })
    }

    fn default_xdg_toplevel_configure_snapshot() -> SmithayXdgToplevelConfigureSnapshot {
        SmithayXdgToplevelConfigureSnapshot {
            last_acked_serial: None,
            activated: false,
            fullscreen: false,
            maximized: false,
            pending_configure_count: 0,
        }
    }

    fn default_xdg_toplevel_metadata_snapshot() -> SmithayXdgToplevelMetadataSnapshot {
        SmithayXdgToplevelMetadataSnapshot {
            title: None,
            app_id: None,
            parent_surface_id: None,
            min_size: None,
            max_size: None,
            window_geometry: None,
        }
    }

    fn default_xdg_toplevel_request_snapshot() -> SmithayXdgToplevelRequestSnapshot {
        SmithayXdgToplevelRequestSnapshot {
            last_move_serial: None,
            last_resize_serial: None,
            last_resize_edge: None,
            last_window_menu_serial: None,
            last_window_menu_location: None,
            minimize_requested: false,
            last_request_kind: None,
            request_count: 0,
        }
    }

    fn titlebar_render_snapshot(
        policy: &SmithayWindowDecorationPolicySnapshot,
        metadata: &SmithayXdgToplevelMetadataSnapshot,
    ) -> Option<SmithayTitlebarRenderSnapshot> {
        if !policy.decorations_visible || !policy.titlebar_visible {
            return None;
        }

        let title = metadata
            .title
            .clone()
            .or_else(|| metadata.app_id.clone())
            .unwrap_or_else(|| "Window".into());

        Some(SmithayTitlebarRenderSnapshot {
            title,
            app_id: metadata.app_id.clone(),
            style: policy.titlebar_style.clone(),
        })
    }

    fn default_xdg_popup_configure_snapshot() -> SmithayXdgPopupConfigureSnapshot {
        SmithayXdgPopupConfigureSnapshot {
            last_acked_serial: None,
            pending_configure_count: 0,
            last_reposition_token: None,
            reactive: false,
            geometry: (0, 0, 0, 0),
            last_grab_serial: None,
            grab_requested: false,
            last_request_kind: None,
            request_count: 0,
        }
    }

    fn xdg_toplevel_configure_snapshot_for(
        surface: &WlSurface,
    ) -> SmithayXdgToplevelConfigureSnapshot {
        with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().ok())
                .map(|data| SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: data
                        .last_acked
                        .as_ref()
                        .map(|configure| u32::from(configure.serial)),
                    activated: data
                        .last_acked
                        .as_ref()
                        .map(|configure| {
                            configure
                                .state
                                .states
                                .contains(xdg_toplevel::State::Activated)
                        })
                        .unwrap_or(false),
                    fullscreen: data
                        .last_acked
                        .as_ref()
                        .map(|configure| {
                            configure
                                .state
                                .states
                                .contains(xdg_toplevel::State::Fullscreen)
                        })
                        .unwrap_or(false),
                    maximized: data
                        .last_acked
                        .as_ref()
                        .map(|configure| {
                            configure
                                .state
                                .states
                                .contains(xdg_toplevel::State::Maximized)
                        })
                        .unwrap_or(false),
                    pending_configure_count: data.pending_configures().len(),
                })
                .unwrap_or_else(default_xdg_toplevel_configure_snapshot)
        })
    }

    fn xdg_toplevel_metadata_snapshot_for(
        surface: &WlSurface,
    ) -> SmithayXdgToplevelMetadataSnapshot {
        with_states(surface, |states| {
            let mut surface_cached = states
                .cached_state
                .get::<smithay::wayland::shell::xdg::SurfaceCachedState>();
            let current = surface_cached.current();
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().ok())
                .map(|data| SmithayXdgToplevelMetadataSnapshot {
                    title: data.title.clone(),
                    app_id: data.app_id.clone(),
                    parent_surface_id: data.parent.as_ref().map(smithay_surface_id),
                    min_size: size_constraint_tuple(current.min_size),
                    max_size: size_constraint_tuple(current.max_size),
                    window_geometry: current.geometry.map(|geometry| {
                        (
                            geometry.loc.x,
                            geometry.loc.y,
                            geometry.size.w,
                            geometry.size.h,
                        )
                    }),
                })
                .unwrap_or_else(default_xdg_toplevel_metadata_snapshot)
        })
    }

    fn size_constraint_tuple(
        size: smithay::utils::Size<i32, smithay::utils::Logical>,
    ) -> Option<(i32, i32)> {
        ((size.w > 0) || (size.h > 0)).then_some((size.w, size.h))
    }

    fn layout_rect_contains(rect: LayoutRect, x: f64, y: f64) -> bool {
        let left = f64::from(rect.x);
        let top = f64::from(rect.y);
        let right = left + f64::from(rect.width);
        let bottom = top + f64::from(rect.height);
        x >= left && x < right && y >= top && y < bottom
    }

    fn titlebar_cursor_name(kind: TitlebarHitKind) -> &'static str {
        match kind {
            TitlebarHitKind::Move => "named:Grab",
            TitlebarHitKind::ResizeTop | TitlebarHitKind::ResizeBottom => "named:NsResize",
            TitlebarHitKind::ResizeLeft | TitlebarHitKind::ResizeRight => "named:EwResize",
            TitlebarHitKind::ResizeTopLeft | TitlebarHitKind::ResizeBottomRight => {
                "named:NwseResize"
            }
            TitlebarHitKind::ResizeTopRight | TitlebarHitKind::ResizeBottomLeft => {
                "named:NeswResize"
            }
        }
    }

    fn clamp_floating_rect(
        mut rect: LayoutRect,
        bounds: LayoutRect,
        min_height: f32,
    ) -> LayoutRect {
        rect.width = rect.width.clamp(160.0, bounds.width.max(160.0));
        rect.height = rect.height.clamp(min_height, bounds.height.max(min_height));
        rect.x = rect
            .x
            .clamp(bounds.x, bounds.x + (bounds.width - rect.width).max(0.0));
        rect.y = rect
            .y
            .clamp(bounds.y, bounds.y + (bounds.height - rect.height).max(0.0));
        rect
    }

    fn xdg_popup_configure_snapshot_for(
        surface: &PopupSurface,
    ) -> SmithayXdgPopupConfigureSnapshot {
        let (last_acked_serial, pending_configure_count, last_reposition_token, reactive, geometry) =
            with_states(surface.wl_surface(), |states| {
                let data = states
                    .data_map
                    .get::<XdgPopupSurfaceData>()
                    .and_then(|data| data.lock().ok());
                let last_acked = data.as_ref().and_then(|data| data.last_acked.as_ref());
                let geometry = last_acked
                    .map(|configure| configure.state.geometry)
                    .unwrap_or_default();
                (
                    last_acked.map(|configure| u32::from(configure.serial)),
                    data.as_ref()
                        .map(|data| data.pending_configures().len())
                        .unwrap_or(0),
                    last_acked.and_then(|configure| configure.reposition_token),
                    last_acked
                        .map(|configure| configure.state.positioner.reactive)
                        .unwrap_or(false),
                    geometry,
                )
            });

        SmithayXdgPopupConfigureSnapshot {
            last_acked_serial,
            pending_configure_count,
            last_reposition_token,
            reactive,
            geometry: (
                geometry.loc.x,
                geometry.loc.y,
                geometry.size.w,
                geometry.size.h,
            ),
            last_grab_serial: None,
            grab_requested: false,
            last_request_kind: None,
            request_count: 0,
        }
    }

    fn default_layer_surface_metadata() -> LayerSurfaceMetadata {
        LayerSurfaceMetadata {
            namespace: String::new(),
            tier: LayerSurfaceTier::Background,
            keyboard_interactivity: LayerKeyboardInteractivity::None,
            exclusive_zone: LayerExclusiveZone::Neutral,
        }
    }

    fn default_layer_surface_configure_snapshot() -> SmithayLayerSurfaceConfigureSnapshot {
        SmithayLayerSurfaceConfigureSnapshot {
            last_acked_serial: None,
            pending_configure_count: 0,
            last_configured_size: None,
        }
    }

    fn layer_surface_configure_snapshot_for(
        surface: &LayerSurface,
    ) -> SmithayLayerSurfaceConfigureSnapshot {
        surface.with_cached_state(|state| SmithayLayerSurfaceConfigureSnapshot {
            last_acked_serial: state
                .last_acked
                .as_ref()
                .map(|configure| u32::from(configure.serial)),
            pending_configure_count: usize::from(state.last_acked.is_none()),
            last_configured_size: state
                .last_acked
                .as_ref()
                .and_then(|configure| configure.state.size)
                .map(|size| (size.w, size.h)),
        })
    }

    fn selection_source_kind(source: &SelectionSource) -> String {
        selection_source_kind_from_debug_repr(&format!("{source:?}"))
    }

    fn cursor_image_snapshot(status: &CursorImageStatus) -> (String, Option<String>) {
        match status {
            CursorImageStatus::Hidden => ("hidden".into(), None),
            CursorImageStatus::Named(icon) => (format!("named:{icon:?}"), None),
            CursorImageStatus::Surface(surface) => {
                ("surface".into(), Some(smithay_surface_id(surface)))
            }
        }
    }

    fn selection_source_kind_from_debug_repr(debug_repr: &str) -> String {
        if debug_repr.contains("provider: DataDevice(") {
            "data-device".into()
        } else if debug_repr.contains("provider: Primary(") {
            "primary-selection".into()
        } else if debug_repr.contains("provider: WlrDataControl(") {
            "wlr-data-control".into()
        } else if debug_repr.contains("provider: ExtDataControl(") {
            "ext-data-control".into()
        } else {
            "client-selection".into()
        }
    }

    fn snapshot_into_discovery_event(snapshot: BackendSurfaceSnapshot) -> BackendDiscoveryEvent {
        match snapshot {
            BackendSurfaceSnapshot::Window {
                surface_id,
                window_id,
                output_id,
            } => BackendDiscoveryEvent::WindowSurfaceDiscovered {
                surface_id,
                window_id,
                output_id,
            },
            BackendSurfaceSnapshot::Popup {
                surface_id,
                output_id,
                parent_surface_id,
            } => BackendDiscoveryEvent::PopupSurfaceDiscovered {
                surface_id,
                output_id,
                parent_surface_id,
            },
            BackendSurfaceSnapshot::Layer {
                surface_id,
                output_id,
                metadata,
            } => BackendDiscoveryEvent::LayerSurfaceDiscovered {
                surface_id,
                output_id,
                metadata,
            },
            BackendSurfaceSnapshot::Unmanaged { surface_id } => {
                BackendDiscoveryEvent::UnmanagedSurfaceDiscovered { surface_id }
            }
        }
    }

    impl BufferHandler for SpidersSmithayState {
        fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
    }

    impl OutputHandler for SpidersSmithayState {
        fn output_bound(&mut self, output: Output, wl_output: WlOutput) {
            let output_id = OutputId::from(output.name());
            self.workspace_manager_state
                .output_bound(&output_id, &wl_output);
        }
    }

    impl WorkspaceHandler for SpidersSmithayState {
        fn workspace_manager_state(&mut self) -> &mut WorkspaceManagerState {
            &mut self.workspace_manager_state
        }

        fn activate_workspace(&mut self, workspace_id: &spiders_shared::ids::WorkspaceId) {
            self.queue_workspace_action(WmAction::ActivateWorkspace {
                workspace_id: workspace_id.clone(),
            });
        }

        fn assign_workspace(
            &mut self,
            workspace_id: &spiders_shared::ids::WorkspaceId,
            output_id: &OutputId,
        ) {
            self.queue_workspace_action(WmAction::AssignWorkspace {
                workspace_id: workspace_id.clone(),
                output_id: output_id.clone(),
            });
        }
    }

    impl CompositorHandler for SpidersSmithayState {
        fn compositor_state(&mut self) -> &mut CompositorState {
            &mut self.compositor_state
        }

        fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
            &client
                .get_data::<SmithayClientState>()
                .unwrap()
                .compositor_state
        }

        fn commit(&mut self, surface: &WlSurface) {
            on_commit_buffer_handler::<Self>(surface);
            self.track_committed_surface(surface);

            let root = root_surface(surface);
            if get_role(&root) == Some(XDG_TOPLEVEL_ROLE) {
                let needs_initial_configure = with_states(&root, |states| {
                    states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .and_then(|data| data.lock().ok())
                        .map(|data| !data.initial_configure_sent)
                        .unwrap_or(false)
                });

                if needs_initial_configure {
                    let surface_id = smithay_surface_id(&root);
                    let sent_count = self
                        .xdg_shell_state
                        .toplevel_surfaces()
                        .iter()
                        .filter(|surface| surface.wl_surface() == &root)
                        .map(|surface| {
                            let _ = surface.send_configure();
                            1usize
                        })
                        .sum::<usize>();
                    for _ in 0..sent_count {
                        self.note_xdg_toplevel_configure_sent(
                            surface_id.clone(),
                            true,
                            false,
                            false,
                        );
                    }
                }
            }
        }
    }

    impl ShmHandler for SpidersSmithayState {
        fn shm_state(&self) -> &ShmState {
            &self.shm_state
        }
    }

    impl SelectionHandler for SpidersSmithayState {
        type SelectionUserData = String;

        fn new_selection(
            &mut self,
            ty: SelectionTarget,
            source: Option<SelectionSource>,
            seat: Seat<Self>,
        ) {
            if seat.name() != self.seat_name {
                return;
            }

            let selection = source.map(|source| SmithaySelectionOfferSnapshot {
                mime_types: source.mime_types(),
                source_kind: selection_source_kind(&source),
            });

            match ty {
                SelectionTarget::Clipboard => {
                    self.clipboard_selection = selection;
                }
                SelectionTarget::Primary => {
                    self.primary_selection = selection;
                }
            }
        }
    }

    impl WaylandDndGrabHandler for SpidersSmithayState {}

    impl DataDeviceHandler for SpidersSmithayState {
        fn data_device_state(&mut self) -> &mut DataDeviceState {
            &mut self.data_device_state
        }
    }

    impl PrimarySelectionHandler for SpidersSmithayState {
        fn primary_selection_state(&mut self) -> &mut PrimarySelectionState {
            &mut self.primary_selection_state
        }
    }

    impl WlrDataControlHandler for SpidersSmithayState {
        fn data_control_state(&mut self) -> &mut WlrDataControlState {
            &mut self.wlr_data_control_state
        }
    }

    impl ExtDataControlHandler for SpidersSmithayState {
        fn data_control_state(&mut self) -> &mut ExtDataControlState {
            &mut self.ext_data_control_state
        }
    }

    impl XdgShellHandler for SpidersSmithayState {
        fn xdg_shell_state(&mut self) -> &mut XdgShellState {
            &mut self.xdg_shell_state
        }

        fn new_toplevel(&mut self, surface: ToplevelSurface) {
            self.track_toplevel_surface(surface.wl_surface());
            let surface_id = smithay_surface_id(surface.wl_surface());
            let policy = self
                .xdg_toplevel_decoration_policies
                .get(&surface_id)
                .cloned()
                .unwrap_or_default();
            self.apply_window_decoration_policy(&surface_id, &policy);
            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Activated);
            });
            let _ = surface.send_configure();
            self.note_xdg_toplevel_configure_sent(surface_id, true, false, false);
        }

        fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
            self.track_popup_surface(&surface);
            if !surface.is_initial_configure_sent() {
                if surface.send_configure().is_ok() {
                    let geometry = positioner.get_geometry();
                    self.record_xdg_popup_configure_sent(
                        smithay_surface_id(surface.wl_surface()),
                        None,
                        positioner.reactive,
                        (
                            geometry.loc.x,
                            geometry.loc.y,
                            geometry.size.w,
                            geometry.size.h,
                        ),
                    );
                }
            }
        }

        fn move_request(
            &mut self,
            surface: ToplevelSurface,
            _seat: wl_seat::WlSeat,
            serial: Serial,
        ) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            let mut snapshot = self.note_xdg_toplevel_request(surface_id.clone(), "move");
            snapshot.last_move_serial = Some(u32::from(serial));
            self.xdg_toplevel_requests.insert(surface_id, snapshot);
        }

        fn resize_request(
            &mut self,
            surface: ToplevelSurface,
            _seat: wl_seat::WlSeat,
            serial: Serial,
            edges: xdg_toplevel::ResizeEdge,
        ) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            let mut snapshot = self.note_xdg_toplevel_request(surface_id.clone(), "resize");
            snapshot.last_resize_serial = Some(u32::from(serial));
            snapshot.last_resize_edge = Some(format!("{edges:?}"));
            self.xdg_toplevel_requests.insert(surface_id, snapshot);
        }

        fn grab(&mut self, surface: PopupSurface, _seat: wl_seat::WlSeat, serial: Serial) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            let mut snapshot = self.note_xdg_popup_request(surface_id.clone(), "grab");
            snapshot.last_grab_serial = Some(u32::from(serial));
            snapshot.grab_requested = true;
            self.xdg_popup_configures.insert(surface_id, snapshot);
        }

        fn reposition_request(
            &mut self,
            surface: PopupSurface,
            positioner: PositionerState,
            token: u32,
        ) {
            surface.with_pending_state(|state| {
                state.positioner = positioner;
                state.geometry = positioner.get_geometry();
            });
            let surface_id = smithay_surface_id(surface.wl_surface());
            let previous = self.xdg_popup_configures.get(&surface_id).cloned();
            let geometry = positioner.get_geometry();
            let mut snapshot = self.note_xdg_popup_request(surface_id.clone(), "reposition");
            snapshot.pending_configure_count += 1;
            snapshot.last_reposition_token = Some(token);
            snapshot.reactive = positioner.reactive;
            snapshot.geometry = (
                geometry.loc.x,
                geometry.loc.y,
                geometry.size.w,
                geometry.size.h,
            );
            snapshot.last_grab_serial = previous
                .as_ref()
                .and_then(|snapshot| snapshot.last_grab_serial);
            snapshot.grab_requested = previous
                .as_ref()
                .map(|snapshot| snapshot.grab_requested)
                .unwrap_or(false);
            self.xdg_popup_configures
                .insert(surface_id.clone(), snapshot);
            let _ = surface.send_repositioned(token);
        }

        fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
            self.track_toplevel_surface_loss(&surface);
        }

        fn popup_destroyed(&mut self, surface: PopupSurface) {
            self.track_popup_surface_loss(&surface);
        }

        fn maximize_request(&mut self, surface: ToplevelSurface) {
            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Maximized);
            });
            let _ = surface.send_configure();
            self.note_xdg_toplevel_configure_sent(
                smithay_surface_id(surface.wl_surface()),
                true,
                false,
                true,
            );
        }

        fn unmaximize_request(&mut self, surface: ToplevelSurface) {
            surface.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Maximized);
            });
            let _ = surface.send_configure();
            self.note_xdg_toplevel_configure_sent(
                smithay_surface_id(surface.wl_surface()),
                true,
                false,
                false,
            );
        }

        fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<WlOutput>) {
            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Fullscreen);
            });
            let _ = surface.send_configure();
            self.note_xdg_toplevel_configure_sent(
                smithay_surface_id(surface.wl_surface()),
                true,
                true,
                false,
            );
        }

        fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
            surface.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Fullscreen);
            });
            let _ = surface.send_configure();
            self.note_xdg_toplevel_configure_sent(
                smithay_surface_id(surface.wl_surface()),
                true,
                false,
                false,
            );
        }

        fn minimize_request(&mut self, surface: ToplevelSurface) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            let mut snapshot = self.note_xdg_toplevel_request(surface_id.clone(), "minimize");
            snapshot.minimize_requested = true;
            self.xdg_toplevel_requests.insert(surface_id, snapshot);
        }

        fn show_window_menu(
            &mut self,
            surface: ToplevelSurface,
            _seat: wl_seat::WlSeat,
            serial: Serial,
            location: smithay::utils::Point<i32, smithay::utils::Logical>,
        ) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            let mut snapshot = self.note_xdg_toplevel_request(surface_id.clone(), "window-menu");
            snapshot.last_window_menu_serial = Some(u32::from(serial));
            snapshot.last_window_menu_location = Some((location.x, location.y));
            self.xdg_toplevel_requests.insert(surface_id, snapshot);
        }

        fn ack_configure(
            &mut self,
            surface: WlSurface,
            configure: smithay::wayland::shell::xdg::Configure,
        ) {
            if matches!(get_role(&surface), Some(XDG_TOPLEVEL_ROLE)) {
                let surface_id = smithay_surface_id(&surface);
                match configure {
                    smithay::wayland::shell::xdg::Configure::Toplevel(configure) => {
                        self.acknowledge_xdg_toplevel_configure(
                            surface_id,
                            u32::from(configure.serial),
                            configure
                                .state
                                .states
                                .contains(xdg_toplevel::State::Activated),
                            configure
                                .state
                                .states
                                .contains(xdg_toplevel::State::Fullscreen),
                            configure
                                .state
                                .states
                                .contains(xdg_toplevel::State::Maximized),
                        );
                    }
                    smithay::wayland::shell::xdg::Configure::Popup(_) => {}
                }
            } else if matches!(get_role(&surface), Some(XDG_POPUP_ROLE)) {
                let surface_id = smithay_surface_id(&surface);
                if let smithay::wayland::shell::xdg::Configure::Popup(configure) = configure {
                    let previous = self.xdg_popup_configures.get(&surface_id).cloned();
                    self.acknowledge_xdg_popup_configure(
                        surface_id.clone(),
                        u32::from(configure.serial),
                        configure.reposition_token,
                        configure.state.positioner.reactive,
                        (
                            configure.state.geometry.loc.x,
                            configure.state.geometry.loc.y,
                            configure.state.geometry.size.w,
                            configure.state.geometry.size.h,
                        ),
                    );
                    if let Some(snapshot) = self.xdg_popup_configures.get_mut(&surface_id) {
                        snapshot.last_grab_serial = previous
                            .as_ref()
                            .and_then(|snapshot| snapshot.last_grab_serial);
                        snapshot.grab_requested = previous
                            .as_ref()
                            .map(|snapshot| snapshot.grab_requested)
                            .unwrap_or(false);
                    }
                }
            }
        }

        fn app_id_changed(&mut self, surface: ToplevelSurface) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            self.xdg_toplevel_metadata.insert(
                surface_id,
                xdg_toplevel_metadata_snapshot_for(surface.wl_surface()),
            );
        }

        fn title_changed(&mut self, surface: ToplevelSurface) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            self.xdg_toplevel_metadata.insert(
                surface_id,
                xdg_toplevel_metadata_snapshot_for(surface.wl_surface()),
            );
        }

        fn parent_changed(&mut self, surface: ToplevelSurface) {
            let surface_id = smithay_surface_id(surface.wl_surface());
            self.xdg_toplevel_metadata.insert(
                surface_id,
                xdg_toplevel_metadata_snapshot_for(surface.wl_surface()),
            );
        }
    }

    impl XdgDecorationHandler for SpidersSmithayState {
        fn new_decoration(&mut self, toplevel: ToplevelSurface) {
            let surface_id = smithay_surface_id(toplevel.wl_surface());
            let policy = self
                .xdg_toplevel_decoration_policies
                .get(&surface_id)
                .cloned()
                .unwrap_or_default();
            self.apply_window_decoration_policy(&surface_id, &policy);
        }

        fn request_mode(
            &mut self,
            toplevel: ToplevelSurface,
            _mode: zxdg_toplevel_decoration_v1::Mode,
        ) {
            let surface_id = smithay_surface_id(toplevel.wl_surface());
            let policy = self
                .xdg_toplevel_decoration_policies
                .get(&surface_id)
                .cloned()
                .unwrap_or_default();
            self.apply_window_decoration_policy(&surface_id, &policy);
        }

        fn unset_mode(&mut self, toplevel: ToplevelSurface) {
            let surface_id = smithay_surface_id(toplevel.wl_surface());
            let policy = self
                .xdg_toplevel_decoration_policies
                .get(&surface_id)
                .cloned()
                .unwrap_or_default();
            self.apply_window_decoration_policy(&surface_id, &policy);
        }
    }

    impl WlrLayerShellHandler for SpidersSmithayState {
        fn shell_state(&mut self) -> &mut WlrLayerShellState {
            &mut self.layer_shell_state
        }

        fn new_layer_surface(
            &mut self,
            surface: LayerSurface,
            output: Option<WlOutput>,
            layer: WlrLayer,
            namespace: String,
        ) {
            self.track_layer_surface(&surface, output, layer, namespace);
            let serial = surface.send_configure();
            let surface_id = smithay_surface_id(surface.wl_surface());
            self.note_layer_configure_sent(surface_id.clone(), None);
            let mut snapshot = self
                .layer_configures
                .get(&surface_id)
                .cloned()
                .unwrap_or_else(default_layer_surface_configure_snapshot);
            snapshot.last_acked_serial = Some(u32::from(serial));
            self.layer_configures.insert(surface_id, snapshot);
        }

        fn ack_configure(&mut self, surface: WlSurface, configure: LayerSurfaceConfigure) {
            self.acknowledge_layer_configure(
                smithay_surface_id(&surface),
                u32::from(configure.serial),
                configure.state.size.map(|size| (size.w, size.h)),
            );
        }

        fn new_popup(&mut self, parent: LayerSurface, popup: PopupSurface) {
            self.track_layer_popup_surface(&parent, &popup);
        }

        fn layer_destroyed(&mut self, surface: LayerSurface) {
            self.track_surface_loss_by_id(smithay_surface_id(surface.wl_surface()));
        }
    }

    impl SeatHandler for SpidersSmithayState {
        type KeyboardFocus = WlSurface;
        type PointerFocus = WlSurface;
        type TouchFocus = WlSurface;

        fn seat_state(&mut self) -> &mut SeatState<Self> {
            &mut self.seat_state
        }

        fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
            if seat.name() != self.seat_name {
                return;
            }

            let client =
                focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
            self.focused_surface_id = focused.map(smithay_surface_id);
            self.clipboard_focus_client_id =
                client.as_ref().map(|client| format!("{:?}", client.id()));
            self.primary_focus_client_id = self.clipboard_focus_client_id.clone();
            let focused_window_id = self
                .focused_surface_id
                .as_ref()
                .and_then(|surface_id| self.focused_window_id(surface_id));
            let focused_output_id = self
                .focused_surface_id
                .as_ref()
                .and_then(|surface_id| self.focused_output_id(surface_id));
            self.pending_discovery_events
                .push(BackendDiscoveryEvent::SeatFocusChanged {
                    seat_name: self.current_seat_name().to_owned(),
                    window_id: focused_window_id,
                    output_id: focused_output_id,
                });
            smithay::wayland::selection::data_device::set_data_device_focus(
                &self.display_handle,
                seat,
                client.clone(),
            );
            set_primary_focus(&self.display_handle, seat, client);
        }

        fn cursor_image(
            &mut self,
            _seat: &Seat<Self>,
            image: smithay::input::pointer::CursorImageStatus,
        ) {
            let (cursor_image, cursor_surface_id) = cursor_image_snapshot(&image);
            self.cursor_image = cursor_image;
            self.cursor_surface_id = cursor_surface_id;
        }
    }

    delegate_compositor!(SpidersSmithayState);
    delegate_shm!(SpidersSmithayState);
    delegate_seat!(SpidersSmithayState);
    delegate_xdg_decoration!(SpidersSmithayState);
    delegate_xdg_shell!(SpidersSmithayState);
    delegate_layer_shell!(SpidersSmithayState);
    delegate_output!(SpidersSmithayState);
    delegate_presentation!(SpidersSmithayState);
    delegate_data_device!(SpidersSmithayState);
    delegate_primary_selection!(SpidersSmithayState);
    delegate_data_control!(SpidersSmithayState);
    delegate_ext_data_control!(SpidersSmithayState);
    crate::delegate_ext_workspace!(SpidersSmithayState);

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::net::UnixStream;
        use std::sync::Arc;

        use spiders_shared::ids::WorkspaceId;
        use spiders_shared::wm::{OutputSnapshot, ShellKind, WindowSnapshot, WorkspaceSnapshot};
        use wayland_client::protocol::{wl_compositor, wl_registry, wl_surface};
        use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle, WEnum};
        use wayland_protocols::xdg::decoration::zv1::client::{
            zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1,
        };
        use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

        #[derive(Debug, Default)]
        struct DecorationClientState {
            globals: Vec<(u32, String, u32)>,
            decoration_modes: Vec<zxdg_toplevel_decoration_v1::Mode>,
        }

        impl Dispatch<wl_registry::WlRegistry, ()> for DecorationClientState {
            fn event(
                state: &mut Self,
                _proxy: &wl_registry::WlRegistry,
                event: wl_registry::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                if let wl_registry::Event::Global {
                    name,
                    interface,
                    version,
                } = event
                {
                    state.globals.push((name, interface, version));
                }
            }
        }

        impl Dispatch<wl_compositor::WlCompositor, ()> for DecorationClientState {
            fn event(
                _state: &mut Self,
                _proxy: &wl_compositor::WlCompositor,
                _event: wl_compositor::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }

        impl Dispatch<wl_surface::WlSurface, ()> for DecorationClientState {
            fn event(
                _state: &mut Self,
                _proxy: &wl_surface::WlSurface,
                _event: wl_surface::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }

        impl Dispatch<xdg_wm_base::XdgWmBase, ()> for DecorationClientState {
            fn event(
                _state: &mut Self,
                proxy: &xdg_wm_base::XdgWmBase,
                event: xdg_wm_base::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                if let xdg_wm_base::Event::Ping { serial } = event {
                    proxy.pong(serial);
                }
            }
        }

        impl Dispatch<xdg_surface::XdgSurface, ()> for DecorationClientState {
            fn event(
                _state: &mut Self,
                proxy: &xdg_surface::XdgSurface,
                event: xdg_surface::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                if let xdg_surface::Event::Configure { serial } = event {
                    proxy.ack_configure(serial);
                }
            }
        }

        impl Dispatch<xdg_toplevel::XdgToplevel, ()> for DecorationClientState {
            fn event(
                _state: &mut Self,
                _proxy: &xdg_toplevel::XdgToplevel,
                _event: xdg_toplevel::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }

        impl Dispatch<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1, ()> for DecorationClientState {
            fn event(
                _state: &mut Self,
                _proxy: &zxdg_decoration_manager_v1::ZxdgDecorationManagerV1,
                _event: zxdg_decoration_manager_v1::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }

        impl Dispatch<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1, ()> for DecorationClientState {
            fn event(
                state: &mut Self,
                _proxy: &zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
                event: zxdg_toplevel_decoration_v1::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                if let zxdg_toplevel_decoration_v1::Event::Configure { mode } = event {
                    if let WEnum::Value(mode) = mode {
                        state.decoration_modes.push(mode);
                    }
                }
            }
        }

        delegate_noop!(DecorationClientState: ignore wayland_client::protocol::wl_callback::WlCallback);

        fn flush_decoration_roundtrip(
            conn: &Connection,
            display: &mut Display<SpidersSmithayState>,
            server_state: &mut SpidersSmithayState,
            queue: &mut EventQueue<DecorationClientState>,
            client_state: &mut DecorationClientState,
        ) {
            conn.flush().unwrap();
            display.dispatch_clients(server_state).unwrap();
            display.flush_clients().unwrap();

            if let Some(guard) = conn.prepare_read() {
                let _ = guard.read();
            } else {
                let _ = conn.backend().dispatch_inner_queue();
            }

            queue.dispatch_pending(client_state).unwrap();
        }

        #[test]
        fn smithay_state_initializes_seat_capabilities() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            assert!(state.seat.get_keyboard().is_some());
            assert!(state.seat.get_pointer().is_some());
            assert_eq!(state.seat_name, "test-seat");

            let snapshot = state.snapshot();
            assert_eq!(snapshot.seat.name, "test-seat");
            assert!(snapshot.seat.has_keyboard);
            assert!(snapshot.seat.has_pointer);
            assert!(!snapshot.seat.has_touch);
            assert!(snapshot.seat.focused_surface_id.is_none());
            assert!(snapshot.seat.focused_surface_role.is_none());
            assert!(snapshot.seat.focused_window_id.is_none());
            assert!(snapshot.seat.focused_output_id.is_none());
            assert_eq!(snapshot.seat.cursor_image, "default");
            assert!(snapshot.seat.cursor_surface_id.is_none());
            assert!(snapshot.outputs.known_output_ids.is_empty());
            assert!(snapshot.outputs.active_output_id.is_none());
            assert_eq!(snapshot.outputs.layer_surface_output_count, 0);
            assert_eq!(snapshot.outputs.active_output_attached_surface_count, 0);
            assert_eq!(snapshot.outputs.mapped_surface_count, 0);
        }

        #[test]
        fn smithay_state_binds_socket_source() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            let socket = state.bind_auto_socket_source().unwrap();

            assert!(!socket.socket_name().is_empty());
        }

        #[test]
        fn smithay_state_tracks_surface_events_by_id() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-11".into(),
                window_id: WindowId::from("smithay-window-1"),
                output_id: None,
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-11".into(),
                window_id: WindowId::from("smithay-window-1"),
                output_id: None,
            });
            state.track_surface_unmap_by_id("wl-surface-11".into());
            state.track_surface_loss_by_id("wl-surface-11".into());

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 3);
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::WindowSurfaceDiscovered { surface_id, .. }
                    if surface_id == "wl-surface-11"
            ));
            assert!(matches!(
                &events[1],
                BackendDiscoveryEvent::SurfaceUnmapped { surface_id }
                    if surface_id == "wl-surface-11"
            ));
            assert!(matches!(
                &events[2],
                BackendDiscoveryEvent::SurfaceLost { surface_id }
                    if surface_id == "wl-surface-11"
            ));
        }

        #[test]
        fn smithay_state_assigns_stable_window_ids_per_surface() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let first = state.window_id_for_surface("wl-surface-21");
            let second = state.window_id_for_surface("wl-surface-21");
            let third = state.window_id_for_surface("wl-surface-22");

            assert_eq!(first, second);
            assert_eq!(first, WindowId::from("smithay-window-1"));
            assert_eq!(third, WindowId::from("smithay-window-2"));
        }

        #[test]
        fn smithay_state_defaults_toplevel_decoration_policy_to_visible() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-decoration-default".into(),
                window_id: WindowId::from("smithay-window-decoration-default"),
                output_id: None,
            });

            let snapshot = state.snapshot();
            let toplevel = snapshot
                .known_surfaces
                .toplevels
                .iter()
                .find(|surface| surface.surface_id == "wl-surface-decoration-default")
                .unwrap();

            assert!(toplevel.decoration_policy.decorations_visible);
            assert!(toplevel.decoration_policy.titlebar_visible);
        }

        #[test]
        fn smithay_state_refreshes_toplevel_decoration_policy_snapshot() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-decoration-1".into(),
                window_id: WindowId::from("smithay-window-decoration-1"),
                output_id: None,
            });
            state.refresh_window_decoration_policies(&[(
                WindowId::from("smithay-window-decoration-1"),
                SmithayWindowDecorationPolicySnapshot {
                    decorations_visible: false,
                    titlebar_visible: false,
                    titlebar_style: TitlebarEffects {
                        background: Some("#111".into()),
                        ..TitlebarEffects::default()
                    },
                },
            )]);

            let snapshot = state.snapshot();
            let toplevel = snapshot
                .known_surfaces
                .toplevels
                .iter()
                .find(|surface| surface.surface_id == "wl-surface-decoration-1")
                .unwrap();

            assert!(!toplevel.decoration_policy.decorations_visible);
            assert!(!toplevel.decoration_policy.titlebar_visible);
            assert_eq!(
                toplevel
                    .decoration_policy
                    .titlebar_style
                    .background
                    .as_deref(),
                Some("#111")
            );
            assert!(toplevel.titlebar.is_none());
        }

        #[test]
        fn smithay_state_hides_titlebar_render_snapshot_when_decorations_are_disabled() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-titlebar-hidden".into(),
                window_id: WindowId::from("smithay-window-titlebar-hidden"),
                output_id: None,
            });
            state.set_test_toplevel_metadata_snapshot(
                "wl-surface-titlebar-hidden",
                SmithayXdgToplevelMetadataSnapshot {
                    title: Some("Terminal".into()),
                    app_id: Some("foot".into()),
                    ..default_xdg_toplevel_metadata_snapshot()
                },
            );
            state.refresh_window_decoration_policies(&[(
                WindowId::from("smithay-window-titlebar-hidden"),
                SmithayWindowDecorationPolicySnapshot {
                    decorations_visible: false,
                    titlebar_visible: false,
                    titlebar_style: TitlebarEffects::default(),
                },
            )]);

            let snapshot = state.snapshot();
            let toplevel = snapshot
                .known_surfaces
                .toplevels
                .iter()
                .find(|surface| surface.surface_id == "wl-surface-titlebar-hidden")
                .unwrap();

            assert!(toplevel.titlebar.is_none());
        }

        #[test]
        fn smithay_state_materializes_titlebar_render_snapshot_when_visible() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-titlebar-visible".into(),
                window_id: WindowId::from("smithay-window-titlebar-visible"),
                output_id: None,
            });
            state.set_test_toplevel_metadata_snapshot(
                "wl-surface-titlebar-visible",
                SmithayXdgToplevelMetadataSnapshot {
                    title: Some("Terminal".into()),
                    app_id: Some("foot".into()),
                    ..default_xdg_toplevel_metadata_snapshot()
                },
            );
            state.refresh_window_decoration_policies(&[(
                WindowId::from("smithay-window-titlebar-visible"),
                SmithayWindowDecorationPolicySnapshot {
                    decorations_visible: true,
                    titlebar_visible: true,
                    titlebar_style: TitlebarEffects {
                        background: Some("#222".into()),
                        ..TitlebarEffects::default()
                    },
                },
            )]);

            let snapshot = state.snapshot();
            let toplevel = snapshot
                .known_surfaces
                .toplevels
                .iter()
                .find(|surface| surface.surface_id == "wl-surface-titlebar-visible")
                .unwrap();

            assert_eq!(
                toplevel
                    .titlebar
                    .as_ref()
                    .map(|titlebar| titlebar.title.as_str()),
                Some("Terminal")
            );
            assert_eq!(
                toplevel
                    .titlebar
                    .as_ref()
                    .and_then(|titlebar| titlebar.app_id.as_deref()),
                Some("foot")
            );
            assert_eq!(
                toplevel
                    .titlebar
                    .as_ref()
                    .and_then(|titlebar| titlebar.style.background.as_deref()),
                Some("#222")
            );
        }

        #[test]
        fn floating_titlebar_interaction_updates_render_plan_geometry() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            let window_id = WindowId::from("w1");
            let base_item = TitlebarRenderItem {
                window_id: window_id.clone(),
                window_rect: LayoutRect {
                    x: 10.0,
                    y: 20.0,
                    width: 300.0,
                    height: 220.0,
                },
                titlebar_rect: LayoutRect {
                    x: 10.0,
                    y: 20.0,
                    width: 300.0,
                    height: 24.0,
                },
                title: "Terminal".into(),
                app_id: Some("foot".into()),
                focused: true,
                style: TitlebarEffects::default(),
            };

            state.refresh_workspace_state(&StateSnapshot {
                focused_window_id: Some(window_id.clone()),
                current_output_id: Some(OutputId::from("out-1")),
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
                outputs: vec![OutputSnapshot {
                    id: OutputId::from("out-1"),
                    name: "HDMI-A-1".into(),
                    logical_width: 800,
                    logical_height: 600,
                    scale: 1,
                    transform: OutputTransform::Normal,
                    enabled: true,
                    current_workspace_id: Some(WorkspaceId::from("ws-1")),
                }],
                workspaces: vec![WorkspaceSnapshot {
                    id: WorkspaceId::from("ws-1"),
                    name: "1".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["1".into()],
                    focused: true,
                    visible: true,
                    effective_layout: None,
                }],
                windows: vec![WindowSnapshot {
                    id: window_id.clone(),
                    shell: ShellKind::XdgToplevel,
                    app_id: Some("foot".into()),
                    title: Some("Terminal".into()),
                    class: None,
                    instance: None,
                    role: None,
                    window_type: None,
                    mapped: true,
                    floating: true,
                    floating_rect: None,
                    fullscreen: false,
                    focused: true,
                    urgent: false,
                    output_id: Some(OutputId::from("out-1")),
                    workspace_id: Some(WorkspaceId::from("ws-1")),
                    tags: vec!["1".into()],
                }],
                visible_window_ids: vec![window_id.clone()],
                tag_names: vec!["1".into()],
            });
            state.refresh_titlebar_render_plan(std::slice::from_ref(&base_item));

            state.update_pointer_location(100.0, 30.0);
            let hit = state.titlebar_hit_target_at_pointer().unwrap();
            assert_eq!(hit.kind, TitlebarHitKind::Move);
            assert!(state.begin_titlebar_interaction(&hit));

            state.update_pointer_location(140.0, 70.0);
            state.update_titlebar_interaction();
            state.refresh_titlebar_render_plan(std::slice::from_ref(&base_item));

            assert_eq!(state.current_titlebar_render_plan()[0].window_rect.x, 50.0);
            assert_eq!(state.current_titlebar_render_plan()[0].window_rect.y, 60.0);
        }

        #[test]
        fn titlebar_hit_target_reports_corner_resize_and_cursor_feedback() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.refresh_titlebar_render_plan(&[TitlebarRenderItem {
                window_id: WindowId::from("w1"),
                window_rect: LayoutRect {
                    x: 10.0,
                    y: 20.0,
                    width: 300.0,
                    height: 220.0,
                },
                titlebar_rect: LayoutRect {
                    x: 10.0,
                    y: 20.0,
                    width: 300.0,
                    height: 24.0,
                },
                title: "Terminal".into(),
                app_id: Some("foot".into()),
                focused: true,
                style: TitlebarEffects::default(),
            }]);

            state.update_pointer_location(12.0, 22.0);
            let hit = state.titlebar_hit_target_at_pointer().unwrap();
            assert_eq!(hit.kind, TitlebarHitKind::ResizeTopLeft);

            state.update_titlebar_cursor_feedback();
            assert_eq!(state.snapshot().seat.cursor_image, "named:NwseResize");
        }

        #[test]
        fn smithay_state_protocol_emits_client_side_decoration_mode_when_policy_disables_ssd() {
            let mut display = Display::<SpidersSmithayState>::new().unwrap();
            let mut handle = display.handle();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let (client_stream, server_stream) = UnixStream::pair().unwrap();
            client_stream.set_nonblocking(true).unwrap();
            server_stream.set_nonblocking(true).unwrap();
            handle
                .insert_client(server_stream, Arc::new(SmithayClientState::default()))
                .unwrap();

            let conn = Connection::from_socket(client_stream).unwrap();
            let mut queue = conn.new_event_queue::<DecorationClientState>();
            let qh = queue.handle();
            let registry = conn.display().get_registry(&qh, ());
            let mut client_state = DecorationClientState::default();

            flush_decoration_roundtrip(
                &conn,
                &mut display,
                &mut state,
                &mut queue,
                &mut client_state,
            );

            let compositor_name = client_state
                .globals
                .iter()
                .find(|(_, interface, _)| interface == "wl_compositor")
                .map(|(name, _, _)| *name)
                .unwrap();
            let wm_base_name = client_state
                .globals
                .iter()
                .find(|(_, interface, _)| interface == "xdg_wm_base")
                .map(|(name, _, _)| *name)
                .unwrap();
            let decoration_name = client_state
                .globals
                .iter()
                .find(|(_, interface, _)| interface == "zxdg_decoration_manager_v1")
                .map(|(name, _, _)| *name)
                .unwrap();

            let compositor =
                registry.bind::<wl_compositor::WlCompositor, _, _>(compositor_name, 1, &qh, ());
            let wm_base = registry.bind::<xdg_wm_base::XdgWmBase, _, _>(wm_base_name, 1, &qh, ());
            let decoration_manager = registry
                .bind::<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1, _, _>(
                    decoration_name,
                    1,
                    &qh,
                    (),
                );
            let surface = compositor.create_surface(&qh, ());
            let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
            let toplevel = xdg_surface.get_toplevel(&qh, ());
            let _decoration = decoration_manager.get_toplevel_decoration(&toplevel, &qh, ());

            surface.commit();
            flush_decoration_roundtrip(
                &conn,
                &mut display,
                &mut state,
                &mut queue,
                &mut client_state,
            );
            flush_decoration_roundtrip(
                &conn,
                &mut display,
                &mut state,
                &mut queue,
                &mut client_state,
            );

            let window_id = state.snapshot().known_surfaces.toplevels[0]
                .window_id
                .clone();
            state.refresh_window_decoration_policies(&[(
                window_id,
                SmithayWindowDecorationPolicySnapshot {
                    decorations_visible: false,
                    titlebar_visible: false,
                    titlebar_style: TitlebarEffects::default(),
                },
            )]);

            flush_decoration_roundtrip(
                &conn,
                &mut display,
                &mut state,
                &mut queue,
                &mut client_state,
            );
            flush_decoration_roundtrip(
                &conn,
                &mut display,
                &mut state,
                &mut queue,
                &mut client_state,
            );

            assert!(client_state
                .decoration_modes
                .iter()
                .any(|mode| *mode == zxdg_toplevel_decoration_v1::Mode::ClientSide));
        }

        #[test]
        fn smithay_state_releases_window_id_mapping_when_surface_is_lost() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let first = state.window_id_for_surface("wl-surface-31");
            state.toplevel_window_ids.remove("wl-surface-31");
            let second = state.window_id_for_surface("wl-surface-31");

            assert_eq!(first, WindowId::from("smithay-window-1"));
            assert_eq!(second, WindowId::from("smithay-window-2"));
        }

        #[test]
        fn smithay_state_tracks_xdg_toplevel_configure_snapshot() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-61".into(),
                window_id: WindowId::from("smithay-window-61"),
                output_id: None,
            });
            state.set_test_toplevel_configure_snapshot(
                "wl-surface-61",
                SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: Some(7),
                    activated: true,
                    fullscreen: false,
                    maximized: true,
                    pending_configure_count: 0,
                },
            );

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.toplevels.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.toplevels[0].configure,
                SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: Some(7),
                    activated: true,
                    fullscreen: false,
                    maximized: true,
                    pending_configure_count: 0,
                }
            );
        }

        #[test]
        fn smithay_state_tracks_xdg_toplevel_pending_configure_counts() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-61b".into(),
                window_id: WindowId::from("smithay-window-61b"),
                output_id: None,
            });
            state.record_test_xdg_toplevel_configure_sent("wl-surface-61b", true, false, false);
            state.record_test_xdg_toplevel_configure_sent("wl-surface-61b", true, true, false);

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.toplevels.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.toplevels[0].configure,
                SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: None,
                    activated: true,
                    fullscreen: true,
                    maximized: false,
                    pending_configure_count: 2,
                }
            );
        }

        #[test]
        fn smithay_state_initial_toplevel_configure_counts_as_pending() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-init-61".into(),
                window_id: WindowId::from("smithay-window-init-61"),
                output_id: None,
            });
            state.record_test_xdg_toplevel_configure_sent("wl-surface-init-61", true, false, false);

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.known_surfaces.toplevels[0].configure,
                SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: None,
                    activated: true,
                    fullscreen: false,
                    maximized: false,
                    pending_configure_count: 1,
                }
            );
        }

        #[test]
        fn smithay_state_xdg_toplevel_ack_reduces_pending_configure_count() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-61c".into(),
                window_id: WindowId::from("smithay-window-61c"),
                output_id: None,
            });
            state.record_test_xdg_toplevel_configure_sent("wl-surface-61c", true, false, true);
            state.record_test_xdg_toplevel_configure_sent("wl-surface-61c", true, false, false);
            state.record_test_xdg_toplevel_configure_acked(
                "wl-surface-61c",
                77,
                true,
                false,
                false,
            );

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.known_surfaces.toplevels[0].configure,
                SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: Some(77),
                    activated: true,
                    fullscreen: false,
                    maximized: false,
                    pending_configure_count: 1,
                }
            );
        }

        #[test]
        fn smithay_state_tracks_xdg_toplevel_metadata_snapshot() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-62".into(),
                window_id: WindowId::from("smithay-window-62"),
                output_id: None,
            });
            state.xdg_toplevel_metadata.insert(
                "wl-surface-62".into(),
                SmithayXdgToplevelMetadataSnapshot {
                    title: Some("terminal".into()),
                    app_id: Some("foot".into()),
                    parent_surface_id: Some("wl-surface-11".into()),
                    min_size: Some((640, 480)),
                    max_size: Some((1920, 1080)),
                    window_geometry: Some((10, 20, 1280, 720)),
                },
            );

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.toplevels.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.toplevels[0].metadata,
                SmithayXdgToplevelMetadataSnapshot {
                    title: Some("terminal".into()),
                    app_id: Some("foot".into()),
                    parent_surface_id: Some("wl-surface-11".into()),
                    min_size: Some((640, 480)),
                    max_size: Some((1920, 1080)),
                    window_geometry: Some((10, 20, 1280, 720)),
                }
            );
        }

        #[test]
        fn smithay_state_tracks_xdg_toplevel_request_snapshot() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-63".into(),
                window_id: WindowId::from("smithay-window-63"),
                output_id: None,
            });
            state.xdg_toplevel_requests.insert(
                "wl-surface-63".into(),
                SmithayXdgToplevelRequestSnapshot {
                    last_move_serial: Some(21),
                    last_resize_serial: Some(22),
                    last_resize_edge: Some("BottomRight".into()),
                    last_window_menu_serial: Some(23),
                    last_window_menu_location: Some((40, 50)),
                    minimize_requested: true,
                    last_request_kind: Some("window-menu".into()),
                    request_count: 4,
                },
            );

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.toplevels.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.toplevels[0].requests,
                SmithayXdgToplevelRequestSnapshot {
                    last_move_serial: Some(21),
                    last_resize_serial: Some(22),
                    last_resize_edge: Some("BottomRight".into()),
                    last_window_menu_serial: Some(23),
                    last_window_menu_location: Some((40, 50)),
                    minimize_requested: true,
                    last_request_kind: Some("window-menu".into()),
                    request_count: 4,
                }
            );
        }

        #[test]
        fn smithay_state_tracks_xdg_toplevel_request_sequence() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-63b".into(),
                window_id: WindowId::from("smithay-window-63b"),
                output_id: None,
            });

            let mut snapshot = state.note_xdg_toplevel_request("wl-surface-63b".into(), "move");
            snapshot.last_move_serial = Some(10);
            state
                .xdg_toplevel_requests
                .insert("wl-surface-63b".into(), snapshot);

            let mut snapshot = state.note_xdg_toplevel_request("wl-surface-63b".into(), "resize");
            snapshot.last_resize_serial = Some(11);
            snapshot.last_resize_edge = Some("Left".into());
            state
                .xdg_toplevel_requests
                .insert("wl-surface-63b".into(), snapshot);

            let mut snapshot =
                state.note_xdg_toplevel_request("wl-surface-63b".into(), "window-menu");
            snapshot.last_window_menu_serial = Some(12);
            snapshot.last_window_menu_location = Some((20, 30));
            state
                .xdg_toplevel_requests
                .insert("wl-surface-63b".into(), snapshot);

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.known_surfaces.toplevels[0].requests,
                SmithayXdgToplevelRequestSnapshot {
                    last_move_serial: Some(10),
                    last_resize_serial: Some(11),
                    last_resize_edge: Some("Left".into()),
                    last_window_menu_serial: Some(12),
                    last_window_menu_location: Some((20, 30)),
                    minimize_requested: false,
                    last_request_kind: Some("window-menu".into()),
                    request_count: 3,
                }
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_clipboard_selection() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.set_test_clipboard_selection(Some(SmithaySelectionOfferSnapshot {
                mime_types: vec!["text/plain".into(), "text/uri-list".into()],
                source_kind: "data-device".into(),
            }));

            let snapshot = state.snapshot();
            assert_eq!(snapshot.clipboard_selection.seat_name, "test-seat");
            assert_eq!(snapshot.clipboard_selection.target, "clipboard");
            assert!(snapshot.clipboard_selection.focused_client_id.is_none());
            assert_eq!(
                snapshot.clipboard_selection.selection,
                Some(SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into(), "text/uri-list".into()],
                    source_kind: "data-device".into(),
                })
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_selection_protocol_support() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.selection_protocols,
                SmithaySelectionProtocolSupportSnapshot {
                    data_device: true,
                    primary_selection: true,
                    wlr_data_control: true,
                    ext_data_control: true,
                }
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_clipboard_focus_client_id() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.set_test_clipboard_focus_client_id(Some("client-7"));

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.clipboard_selection.focused_client_id.as_deref(),
                Some("client-7")
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_focused_surface_id() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.set_test_focused_surface_id(Some("wl-surface-77"));

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.seat.focused_surface_id.as_deref(),
                Some("wl-surface-77")
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_focused_toplevel_role_and_window() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-78".into(),
                window_id: WindowId::from("smithay-window-78"),
                output_id: None,
            });
            state.set_test_focused_surface_id(Some("wl-surface-78"));

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.seat.focused_surface_role.as_deref(),
                Some("toplevel")
            );
            assert_eq!(
                snapshot.seat.focused_window_id,
                Some(WindowId::from("smithay-window-78"))
            );
            assert!(snapshot.seat.focused_output_id.is_none());
        }

        #[test]
        fn smithay_state_snapshot_reports_focused_popup_parent_window() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-79-parent".into(),
                window_id: WindowId::from("smithay-window-79"),
                output_id: None,
            });
            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-surface-79-popup".into(),
                output_id: None,
                parent_surface_id: "wl-surface-79-parent".into(),
            });
            state.track_test_popup_parent("wl-surface-79-popup", "wl-surface-79-parent");
            state.set_test_focused_surface_id(Some("wl-surface-79-popup"));

            let snapshot = state.snapshot();
            assert_eq!(snapshot.seat.focused_surface_role.as_deref(), Some("popup"));
            assert_eq!(
                snapshot.seat.focused_window_id,
                Some(WindowId::from("smithay-window-79"))
            );
            assert!(snapshot.seat.focused_output_id.is_none());
        }

        #[test]
        fn smithay_state_snapshot_reports_focused_output_for_layer_parented_popup() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-focus-1".into(),
                output_id: OutputId::from("out-3"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(10),
                },
            });
            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-popup-focus-1".into(),
                output_id: Some(OutputId::from("out-3")),
                parent_surface_id: "wl-layer-focus-1".into(),
            });
            state.track_test_popup_parent("wl-popup-focus-1", "wl-layer-focus-1");
            state.set_test_focused_surface_id(Some("wl-popup-focus-1"));

            let snapshot = state.snapshot();
            assert_eq!(snapshot.seat.focused_surface_role.as_deref(), Some("popup"));
            assert_eq!(
                snapshot.seat.focused_output_id,
                Some(OutputId::from("out-3"))
            );
        }

        #[test]
        fn smithay_state_enqueues_seat_focus_discovery_event() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-focus-layer-1".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(8),
                },
            });
            state.record_test_seat_focus_event(Some("wl-focus-layer-1"));

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 2);
            assert!(matches!(
                &events[1],
                BackendDiscoveryEvent::SeatFocusChanged {
                    seat_name,
                    window_id,
                    output_id,
                } if seat_name == "test-seat"
                    && window_id == &None
                    && output_id == &Some(OutputId::from("out-1"))
            ));
        }

        #[test]
        fn smithay_state_tracks_known_and_active_seat_names() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "seat-0").unwrap();

            let _ = state.take_discovery_events();
            state.register_seat_name("seat-1", false);
            state.activate_seat_name("seat-1");

            let snapshot = state.backend_topology_snapshot(1);
            assert_eq!(snapshot.seats.len(), 2);
            assert_eq!(state.snapshot().seat_name, "seat-1");
            assert_eq!(
                snapshot
                    .seats
                    .iter()
                    .find(|seat| seat.active)
                    .map(|seat| seat.seat_name.as_str()),
                Some("seat-1")
            );
        }

        #[test]
        fn smithay_state_reassigns_active_seat_when_active_seat_is_removed() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "seat-0").unwrap();

            let _ = state.take_discovery_events();
            state.register_seat_name("seat-1", false);
            state.activate_seat_name("seat-1");
            let _ = state.take_discovery_events();

            state.remove_seat_name("seat-1");

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 1);
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::SeatLost { seat_name } if seat_name == "seat-1"
            ));
            assert_eq!(state.snapshot().seat_name, "seat-0");
            assert_eq!(state.backend_topology_snapshot(2).seats.len(), 1);
        }

        #[test]
        fn smithay_state_clears_focus_and_cursor_when_focused_surface_unmaps() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-focus-unmap-1".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(8),
                },
            });
            let _ = state.take_discovery_events();
            state.set_test_focused_surface_id(Some("wl-focus-unmap-1"));
            state.set_test_cursor_image("surface", Some("wl-focus-unmap-1"));

            state.track_surface_unmap_by_id("wl-focus-unmap-1".into());

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 2);
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::SurfaceUnmapped { surface_id }
                    if surface_id == "wl-focus-unmap-1"
            ));
            assert!(matches!(
                &events[1],
                BackendDiscoveryEvent::SeatFocusChanged {
                    seat_name,
                    window_id,
                    output_id,
                } if seat_name == "test-seat" && window_id == &None && output_id == &None
            ));

            let snapshot = state.snapshot();
            assert!(snapshot.seat.focused_surface_id.is_none());
            assert!(snapshot.seat.cursor_surface_id.is_none());
            assert_eq!(snapshot.seat.cursor_image, "default");
        }

        #[test]
        fn smithay_state_clears_focus_when_focused_surface_is_lost() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-focus-lost-1".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(8),
                },
            });
            let _ = state.take_discovery_events();
            state.set_test_focused_surface_id(Some("wl-focus-lost-1"));

            state.track_surface_loss_by_id("wl-focus-lost-1".into());

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 2);
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::SurfaceLost { surface_id }
                    if surface_id == "wl-focus-lost-1"
            ));
            assert!(matches!(
                &events[1],
                BackendDiscoveryEvent::SeatFocusChanged {
                    seat_name,
                    window_id,
                    output_id,
                } if seat_name == "test-seat" && window_id == &None && output_id == &None
            ));
            assert!(state.snapshot().seat.focused_surface_id.is_none());
        }

        #[test]
        fn smithay_state_snapshot_reports_cursor_image() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.set_test_cursor_image("named:Pointer", None);

            let snapshot = state.snapshot();
            assert_eq!(snapshot.seat.cursor_image, "named:Pointer");
            assert!(snapshot.seat.cursor_surface_id.is_none());
        }

        #[test]
        fn smithay_state_snapshot_reports_known_and_active_outputs() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.register_output_id(OutputId::from("out-1"), true);
            state.register_output_id(OutputId::from("out-2"), false);
            let _ = state.take_discovery_events();

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.outputs.known_output_ids,
                vec![OutputId::from("out-1"), OutputId::from("out-2")]
            );
            assert_eq!(
                snapshot.outputs.active_output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(snapshot.outputs.known_outputs.len(), 2);
            assert_eq!(snapshot.outputs.known_outputs[0].name, "out-1");
            assert_eq!(snapshot.outputs.active_output_attached_surface_count, 0);
        }

        #[test]
        fn smithay_state_snapshot_reports_typed_output_metadata_when_registered() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.register_output_snapshot(
                OutputId::from("out-3"),
                "DP-2",
                Some((3440, 1440)),
                true,
            );

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.outputs.known_output_ids,
                vec![OutputId::from("out-3")]
            );
            assert_eq!(snapshot.outputs.known_outputs.len(), 1);
            assert_eq!(snapshot.outputs.known_outputs[0].name, "DP-2");
            assert_eq!(snapshot.outputs.known_outputs[0].logical_width, Some(3440));
            assert_eq!(snapshot.outputs.known_outputs[0].logical_height, Some(1440));
            assert_eq!(
                snapshot.outputs.active_output_id,
                Some(OutputId::from("out-3"))
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_layer_output_attachment_count() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.register_output_id(OutputId::from("out-1"), true);
            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-1".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(16),
                },
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.outputs.layer_surface_output_count, 1);
            assert_eq!(snapshot.outputs.active_output_attached_surface_count, 1);
            assert_eq!(snapshot.outputs.mapped_surface_count, 1);
        }

        #[test]
        fn smithay_state_snapshot_reports_mapped_surface_count() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-surface-map-1".into(),
            });
            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-surface-map-2".into(),
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.outputs.mapped_surface_count, 2);
        }

        #[test]
        fn smithay_state_enqueues_output_activation_discovery_event() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.register_output_id(OutputId::from("out-1"), false);
            state.register_output_id(OutputId::from("out-2"), true);
            let _ = state.take_discovery_events();
            state.activate_output_id(OutputId::from("out-1"));

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 1);
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::OutputActivated { output_id }
                    if output_id == &OutputId::from("out-1")
            ));
        }

        #[test]
        fn smithay_state_removes_output_and_enqueues_output_lost_event() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.register_output_id(OutputId::from("out-1"), false);
            state.register_output_id(OutputId::from("out-2"), true);
            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-output-lost-1".into(),
                output_id: OutputId::from("out-2"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(12),
                },
            });
            let _ = state.take_discovery_events();

            state.remove_output_id(&OutputId::from("out-2"));

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 1);
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::OutputLost { output_id }
                    if output_id == &OutputId::from("out-2")
            ));

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.outputs.known_output_ids,
                vec![OutputId::from("out-1")]
            );
            assert_eq!(
                snapshot.outputs.active_output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(snapshot.known_surfaces.layers.len(), 1);
            assert_eq!(snapshot.known_surfaces.layers[0].output_id, None);
        }

        #[test]
        fn smithay_state_uses_first_output_as_default_active_output() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.register_output_id(OutputId::from("out-1"), false);

            assert_eq!(
                state.snapshot().outputs.active_output_id,
                Some(OutputId::from("out-1"))
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_layer_configure_metadata() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-configure-1".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(16),
                },
            });
            state.layer_configures.insert(
                "wl-layer-configure-1".into(),
                SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: Some(42),
                    pending_configure_count: 0,
                    last_configured_size: Some((1280, 32)),
                },
            );

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.layers.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.layers[0].configure,
                SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: Some(42),
                    pending_configure_count: 0,
                    last_configured_size: Some((1280, 32)),
                }
            );
        }

        #[test]
        fn smithay_state_tracks_layer_pending_configure_counts() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-configure-3".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(12),
                },
            });
            state.record_test_layer_configure_sent("wl-layer-configure-3", Some((800, 24)));
            state.record_test_layer_configure_sent("wl-layer-configure-3", Some((800, 28)));

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.layers.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.layers[0].configure,
                SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 2,
                    last_configured_size: Some((800, 28)),
                }
            );
        }

        #[test]
        fn smithay_state_layer_ack_reduces_pending_configure_count() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-configure-4".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "overlay".into(),
                    tier: LayerSurfaceTier::Overlay,
                    keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                    exclusive_zone: LayerExclusiveZone::DontCare,
                },
            });
            state.record_test_layer_configure_sent("wl-layer-configure-4", Some((1920, 1080)));
            state.record_test_layer_configure_sent("wl-layer-configure-4", Some((1920, 900)));
            state.record_test_layer_configure_acked("wl-layer-configure-4", 55, Some((1920, 900)));

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.known_surfaces.layers[0].configure,
                SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: Some(55),
                    pending_configure_count: 1,
                    last_configured_size: Some((1920, 900)),
                }
            );
        }

        #[test]
        fn smithay_state_removes_layer_configure_metadata_when_surface_is_lost() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-configure-2".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "overlay".into(),
                    tier: LayerSurfaceTier::Overlay,
                    keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                    exclusive_zone: LayerExclusiveZone::DontCare,
                },
            });
            state.layer_configures.insert(
                "wl-layer-configure-2".into(),
                SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: Some(7),
                    pending_configure_count: 0,
                    last_configured_size: Some((1920, 1080)),
                },
            );

            state.track_test_surface_loss("wl-layer-configure-2");

            assert!(state.snapshot().known_surfaces.layers.is_empty());
            assert!(!state.layer_configures.contains_key("wl-layer-configure-2"));
        }

        #[test]
        fn cursor_image_snapshot_reports_hidden_and_named_status() {
            assert_eq!(
                cursor_image_snapshot(&CursorImageStatus::Hidden),
                ("hidden".into(), None)
            );
            assert_eq!(
                cursor_image_snapshot(&CursorImageStatus::default_named()),
                ("named:Default".into(), None)
            );
        }

        #[test]
        fn selection_source_kind_distinguishes_provider_debug_variants() {
            assert_eq!(
                selection_source_kind_from_debug_repr(
                    "SelectionSource { provider: DataDevice(WlDataSource@1) }"
                ),
                "data-device"
            );
            assert_eq!(
                selection_source_kind_from_debug_repr(
                    "SelectionSource { provider: Primary(ZwpPrimarySelectionSourceV1@2) }"
                ),
                "primary-selection"
            );
            assert_eq!(
                selection_source_kind_from_debug_repr(
                    "SelectionSource { provider: WlrDataControl(ZwlrDataControlSourceV1@3) }"
                ),
                "wlr-data-control"
            );
            assert_eq!(
                selection_source_kind_from_debug_repr(
                    "SelectionSource { provider: ExtDataControl(ExtDataControlSourceV1@4) }"
                ),
                "ext-data-control"
            );
            assert_eq!(
                selection_source_kind_from_debug_repr("SelectionSource { provider: Unknown }"),
                "client-selection"
            );
        }

        #[test]
        fn selection_handler_updates_clipboard_snapshot_for_matching_seat() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            let seat = state.seat.clone();

            SelectionHandler::new_selection(&mut state, SelectionTarget::Clipboard, None, seat);

            let snapshot = state.snapshot();
            assert_eq!(snapshot.clipboard_selection.target, "clipboard");
            assert!(snapshot.clipboard_selection.selection.is_none());
        }

        #[test]
        fn selection_handler_updates_primary_snapshot_for_matching_seat() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            let seat = state.seat.clone();

            SelectionHandler::new_selection(&mut state, SelectionTarget::Primary, None, seat);

            let snapshot = state.snapshot();
            assert_eq!(snapshot.primary_selection.target, "primary");
            assert!(snapshot.primary_selection.selection.is_none());
        }

        #[test]
        fn selection_handler_ignores_non_matching_seat() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            state.set_test_clipboard_selection(Some(SmithaySelectionOfferSnapshot {
                mime_types: vec!["text/plain".into()],
                source_kind: "data-device".into(),
            }));
            state.set_test_primary_selection(Some(SmithaySelectionOfferSnapshot {
                mime_types: vec!["text/plain".into()],
                source_kind: "primary-selection".into(),
            }));

            let mut other_seat_state = SeatState::new();
            let other_seat =
                other_seat_state.new_wl_seat(&state.display_handle, "other-seat".to_owned());

            SelectionHandler::new_selection(
                &mut state,
                SelectionTarget::Clipboard,
                None,
                other_seat.clone(),
            );
            SelectionHandler::new_selection(&mut state, SelectionTarget::Primary, None, other_seat);

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.clipboard_selection.selection,
                Some(SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into()],
                    source_kind: "data-device".into(),
                })
            );
            assert_eq!(
                snapshot.primary_selection.selection,
                Some(SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into()],
                    source_kind: "primary-selection".into(),
                })
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_primary_selection() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.set_test_primary_selection(Some(SmithaySelectionOfferSnapshot {
                mime_types: vec!["text/plain".into(), "text/html".into()],
                source_kind: "primary-selection".into(),
            }));

            let snapshot = state.snapshot();
            assert_eq!(snapshot.primary_selection.seat_name, "test-seat");
            assert_eq!(snapshot.primary_selection.target, "primary");
            assert!(snapshot.primary_selection.focused_client_id.is_none());
            assert_eq!(
                snapshot.primary_selection.selection,
                Some(SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into(), "text/html".into()],
                    source_kind: "primary-selection".into(),
                })
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_primary_focus_client_id() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.set_test_primary_focus_client_id(Some("client-11"));

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.primary_selection.focused_client_id.as_deref(),
                Some("client-11")
            );
        }

        #[test]
        fn smithay_state_tracks_xdg_popup_configure_snapshot() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-popup-62".into(),
                output_id: None,
                parent_surface_id: "unresolved-parent-wl-popup-62".into(),
            });
            state.xdg_popup_configures.insert(
                "wl-popup-62".into(),
                SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: Some(12),
                    pending_configure_count: 0,
                    last_reposition_token: Some(44),
                    reactive: true,
                    geometry: (10, 20, 300, 200),
                    last_grab_serial: Some(9),
                    grab_requested: true,
                    last_request_kind: Some("grab".into()),
                    request_count: 2,
                },
            );

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.popups.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.popups[0].configure,
                SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: Some(12),
                    pending_configure_count: 0,
                    last_reposition_token: Some(44),
                    reactive: true,
                    geometry: (10, 20, 300, 200),
                    last_grab_serial: Some(9),
                    grab_requested: true,
                    last_request_kind: Some("grab".into()),
                    request_count: 2,
                }
            );
        }

        #[test]
        fn smithay_state_tracks_xdg_popup_pending_configure_counts() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-popup-63".into(),
                output_id: None,
                parent_surface_id: "unresolved-parent-wl-popup-63".into(),
            });
            state.record_test_xdg_popup_configure_sent(
                "wl-popup-63",
                Some(10),
                true,
                (5, 10, 200, 120),
            );
            state.record_test_xdg_popup_configure_sent(
                "wl-popup-63",
                Some(11),
                true,
                (5, 10, 220, 140),
            );

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.popups.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.popups[0].configure,
                SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 2,
                    last_reposition_token: Some(11),
                    reactive: true,
                    geometry: (5, 10, 220, 140),
                    last_grab_serial: None,
                    grab_requested: false,
                    last_request_kind: Some("reposition".into()),
                    request_count: 2,
                }
            );
        }

        #[test]
        fn smithay_state_initial_popup_configure_counts_as_pending_without_request_sequence() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-popup-init-63".into(),
                output_id: None,
                parent_surface_id: "unresolved-parent-wl-popup-init-63".into(),
            });
            state.record_test_initial_xdg_popup_configure_sent(
                "wl-popup-init-63",
                false,
                (3, 4, 180, 110),
            );

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.known_surfaces.popups[0].configure,
                SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 1,
                    last_reposition_token: None,
                    reactive: false,
                    geometry: (3, 4, 180, 110),
                    last_grab_serial: None,
                    grab_requested: false,
                    last_request_kind: None,
                    request_count: 0,
                }
            );
        }

        #[test]
        fn smithay_state_xdg_popup_ack_reduces_pending_configure_count() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-popup-64".into(),
                output_id: None,
                parent_surface_id: "unresolved-parent-wl-popup-64".into(),
            });
            state.record_test_xdg_popup_configure_sent(
                "wl-popup-64",
                Some(21),
                true,
                (0, 0, 300, 180),
            );
            state.record_test_xdg_popup_configure_sent(
                "wl-popup-64",
                Some(22),
                true,
                (0, 0, 320, 200),
            );
            state.record_test_xdg_popup_configure_acked(
                "wl-popup-64",
                88,
                Some(22),
                true,
                (0, 0, 320, 200),
            );

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.known_surfaces.popups[0].configure,
                SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: Some(88),
                    pending_configure_count: 1,
                    last_reposition_token: Some(22),
                    reactive: true,
                    geometry: (0, 0, 320, 200),
                    last_grab_serial: None,
                    grab_requested: false,
                    last_request_kind: Some("reposition".into()),
                    request_count: 2,
                }
            );
        }

        #[test]
        fn smithay_state_tracks_xdg_popup_request_sequence() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_test_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-popup-65".into(),
                output_id: None,
                parent_surface_id: "unresolved-parent-wl-popup-65".into(),
            });
            state.record_test_xdg_popup_request("wl-popup-65", "grab", |snapshot| {
                snapshot.last_grab_serial = Some(13);
                snapshot.grab_requested = true;
            });
            state.record_test_xdg_popup_request("wl-popup-65", "reposition", |snapshot| {
                snapshot.last_reposition_token = Some(14);
                snapshot.geometry = (1, 2, 140, 90);
                snapshot.reactive = true;
            });

            let snapshot = state.snapshot();
            assert_eq!(
                snapshot.known_surfaces.popups[0].configure,
                SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 0,
                    last_reposition_token: Some(14),
                    reactive: true,
                    geometry: (1, 2, 140, 90),
                    last_grab_serial: Some(13),
                    grab_requested: true,
                    last_request_kind: Some("reposition".into()),
                    request_count: 2,
                }
            );
        }

        #[test]
        fn smithay_state_tracks_unmanaged_surface_snapshots() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-surface-90".into(),
            });

            let events = state.take_discovery_events();
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::UnmanagedSurfaceDiscovered { surface_id }
                    if surface_id == "wl-surface-90"
            ));
        }

        #[test]
        fn smithay_state_snapshot_reports_tracked_counts() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let before = state.snapshot();
            assert_eq!(before.seat_name, "test-seat");
            assert_eq!(before.tracked_surface_count, 0);
            assert_eq!(before.tracked_toplevel_count, 0);
            assert_eq!(before.pending_discovery_event_count, 0);
            assert_eq!(before.role_counts.toplevel, 0);
            assert_eq!(before.role_counts.popup, 0);
            assert_eq!(before.role_counts.unmanaged, 0);
            assert_eq!(before.role_counts.layer, 0);
            assert!(before.known_surfaces.all.is_empty());
            assert!(before.known_surfaces.toplevels.is_empty());
            assert!(before.known_surfaces.popups.is_empty());
            assert!(before.known_surfaces.unmanaged.is_empty());
            assert!(before.known_surfaces.layers.is_empty());

            let window_id = state.window_id_for_surface("wl-surface-101");
            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-101".into(),
                window_id,
                output_id: None,
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-surface-102".into(),
            });

            let after = state.snapshot();
            assert_eq!(after.tracked_surface_count, 2);
            assert_eq!(after.tracked_toplevel_count, 1);
            assert_eq!(after.pending_discovery_event_count, 2);
            assert_eq!(after.role_counts.toplevel, 1);
            assert_eq!(after.role_counts.popup, 0);
            assert_eq!(after.role_counts.unmanaged, 1);
            assert_eq!(after.role_counts.layer, 0);
            assert_eq!(after.known_surfaces.all.len(), 2);
            assert_eq!(after.known_surfaces.toplevels.len(), 1);
            assert_eq!(after.known_surfaces.unmanaged.len(), 1);

            let _ = state.take_discovery_events();
            let drained = state.snapshot();
            assert_eq!(drained.pending_discovery_event_count, 0);
            assert_eq!(drained.tracked_surface_count, 2);

            state.track_surface_unmap_by_id("wl-surface-101".into());
            let unmapped = state.snapshot();
            assert_eq!(unmapped.pending_discovery_event_count, 1);

            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-101".into(),
                window_id: WindowId::from("smithay-window-1"),
                output_id: None,
            });
            let remapped = state.snapshot();
            assert_eq!(remapped.pending_discovery_event_count, 2);
        }

        #[test]
        fn smithay_state_snapshot_reports_role_breakdown() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            state.register_output_id(OutputId::from("out-1"), true);

            let window_id = state.window_id_for_surface("wl-surface-201");
            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-201".into(),
                window_id,
                output_id: None,
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-surface-202".into(),
                output_id: None,
                parent_surface_id: "wl-surface-201".into(),
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-surface-203".into(),
                output_id: "out-1".into(),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(20),
                },
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-surface-204".into(),
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.tracked_surface_count, 4);
            assert_eq!(snapshot.role_counts.toplevel, 1);
            assert_eq!(snapshot.role_counts.popup, 1);
            assert_eq!(snapshot.role_counts.layer, 1);
            assert_eq!(snapshot.role_counts.unmanaged, 1);
            assert_eq!(snapshot.known_surfaces.all.len(), 4);
            assert_eq!(snapshot.known_surfaces.toplevels.len(), 1);
            assert_eq!(snapshot.known_surfaces.popups.len(), 1);
            assert_eq!(snapshot.known_surfaces.layers.len(), 1);
            assert_eq!(snapshot.known_surfaces.unmanaged.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.layers[0].output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(
                snapshot.known_surfaces.layers[0].metadata,
                LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(20),
                }
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_popup_parent_window_identity() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let parent_window_id = state.window_id_for_surface("wl-surface-301");
            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-301".into(),
                window_id: parent_window_id.clone(),
                output_id: None,
            });
            state.popup_parent_links.insert(
                "wl-surface-302".into(),
                SmithayPopupParentLink {
                    parent: SmithayPopupParentSnapshot::Resolved {
                        surface_id: "wl-surface-301".into(),
                        window_id: Some(parent_window_id.clone()),
                    },
                },
            );
            state.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-surface-302".into(),
                output_id: None,
                parent_surface_id: "wl-surface-301".into(),
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.popups.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.popups[0].parent,
                SmithayPopupParentSnapshot::Resolved {
                    surface_id: "wl-surface-301".into(),
                    window_id: Some(parent_window_id),
                }
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_unresolved_popup_parent() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.popup_parent_links.insert(
                "wl-surface-401".into(),
                SmithayPopupParentLink {
                    parent: SmithayPopupParentSnapshot::Unresolved,
                },
            );
            state.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-surface-401".into(),
                output_id: None,
                parent_surface_id: "unresolved-parent-wl-surface-401".into(),
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.popups.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.popups[0].parent,
                SmithayPopupParentSnapshot::Unresolved
            );
        }

        #[test]
        fn smithay_state_snapshot_reports_unified_known_surface_order() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            let window_id = state.window_id_for_surface("wl-surface-501");
            state.track_surface_snapshot(BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-surface-504".into(),
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-surface-502".into(),
                output_id: None,
                parent_surface_id: "unresolved-parent-wl-surface-502".into(),
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-surface-503".into(),
                output_id: "out-1".into(),
                metadata: LayerSurfaceMetadata {
                    namespace: "overlay".into(),
                    tier: LayerSurfaceTier::Overlay,
                    keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                    exclusive_zone: LayerExclusiveZone::DontCare,
                },
            });
            state.track_surface_snapshot(BackendSurfaceSnapshot::Window {
                surface_id: "wl-surface-501".into(),
                window_id,
                output_id: None,
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.all.len(), 4);
            assert!(matches!(
                &snapshot.known_surfaces.all[0],
                SmithayKnownSurface::Layer(surface) if surface.surface_id == "wl-surface-503"
            ));
            assert!(matches!(
                &snapshot.known_surfaces.all[1],
                SmithayKnownSurface::Popup(surface) if surface.surface_id == "wl-surface-502"
            ));
            assert!(matches!(
                &snapshot.known_surfaces.all[2],
                SmithayKnownSurface::Toplevel(surface) if surface.surface_id == "wl-surface-501"
            ));
            assert!(matches!(
                &snapshot.known_surfaces.all[3],
                SmithayKnownSurface::Unmanaged(surface) if surface.surface_id == "wl-surface-504"
            ));
        }

        #[test]
        fn smithay_state_unmaps_and_remaps_layer_surface_without_losing_output_attachment() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            state.register_output_id(OutputId::from("out-1"), true);

            state.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-21".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(20),
                },
            });
            let _ = state.take_discovery_events();

            state.track_surface_unmap_by_id("wl-layer-21".into());
            state.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-21".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(20),
                },
            });

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 2);
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::SurfaceUnmapped { surface_id } if surface_id == "wl-layer-21"
            ));
            assert!(matches!(
                &events[1],
                BackendDiscoveryEvent::LayerSurfaceDiscovered { surface_id, output_id, .. }
                    if surface_id == "wl-layer-21" && output_id == &OutputId::from("out-1")
            ));

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.layers.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.layers[0].output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(
                snapshot.known_surfaces.layers[0].metadata,
                LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(20),
                }
            );
            assert_eq!(
                state.layer_output_id("wl-layer-21"),
                Some(&OutputId::from("out-1"))
            );
        }

        #[test]
        fn smithay_state_snapshot_preserves_layer_policy_metadata() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            state.register_output_id(OutputId::from("out-1"), true);

            state.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-32".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "lockscreen".into(),
                    tier: LayerSurfaceTier::Overlay,
                    keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                    exclusive_zone: LayerExclusiveZone::DontCare,
                },
            });

            let snapshot = state.snapshot();
            assert_eq!(snapshot.known_surfaces.layers.len(), 1);
            assert_eq!(
                snapshot.known_surfaces.layers[0].metadata,
                LayerSurfaceMetadata {
                    namespace: "lockscreen".into(),
                    tier: LayerSurfaceTier::Overlay,
                    keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                    exclusive_zone: LayerExclusiveZone::DontCare,
                }
            );
        }

        #[test]
        fn smithay_state_removes_layer_output_attachment_when_layer_surface_is_lost() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            state.register_output_id(OutputId::from("out-1"), true);

            state.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-31".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "background".into(),
                    tier: LayerSurfaceTier::Background,
                    keyboard_interactivity: LayerKeyboardInteractivity::None,
                    exclusive_zone: LayerExclusiveZone::Neutral,
                },
            });
            let _ = state.take_discovery_events();

            state.track_surface_loss_by_id("wl-layer-31".into());

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 1);
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::SurfaceLost { surface_id } if surface_id == "wl-layer-31"
            ));
            assert!(state.layer_output_id("wl-layer-31").is_none());
            assert!(state.snapshot().known_surfaces.layers.is_empty());
        }

        #[test]
        fn smithay_state_assigns_layer_output_to_popup_parented_to_layer_surface() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            state.register_output_id(OutputId::from("out-1"), true);

            state.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-41".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(20),
                },
            });
            state.track_test_popup_parent("wl-popup-41", "wl-layer-41");
            state.track_surface_snapshot(BackendSurfaceSnapshot::Popup {
                surface_id: "wl-popup-41".into(),
                output_id: Some(OutputId::from("out-1")),
                parent_surface_id: "wl-layer-41".into(),
            });

            let events = state.take_discovery_events();
            assert!(matches!(
                &events[1],
                BackendDiscoveryEvent::PopupSurfaceDiscovered {
                    surface_id,
                    output_id,
                    parent_surface_id,
                } if surface_id == "wl-popup-41"
                    && output_id == &Some(OutputId::from("out-1"))
                    && parent_surface_id == "wl-layer-41"
            ));

            let snapshot = state.snapshot();
            let popup = snapshot
                .known_surfaces
                .popups
                .iter()
                .find(|popup| popup.surface_id == "wl-popup-41")
                .unwrap();
            assert_eq!(
                popup.parent,
                SmithayPopupParentSnapshot::Resolved {
                    surface_id: "wl-layer-41".into(),
                    window_id: None,
                }
            );
            assert_eq!(
                state.layer_output_id("wl-popup-41"),
                Some(&OutputId::from("out-1"))
            );
        }

        #[test]
        fn smithay_state_layer_popup_tracking_records_parent_and_output() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();

            state.track_surface_snapshot(BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-51".into(),
                output_id: OutputId::from("out-2"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(24),
                },
            });
            let _ = state.take_discovery_events();

            state.track_layer_popup_surface_for_test("wl-layer-51", "wl-popup-51");

            let events = state.take_discovery_events();
            assert_eq!(events.len(), 1);
            assert!(matches!(
                &events[0],
                BackendDiscoveryEvent::PopupSurfaceDiscovered {
                    surface_id,
                    output_id,
                    parent_surface_id,
                } if surface_id == "wl-popup-51"
                    && output_id == &Some(OutputId::from("out-2"))
                    && parent_surface_id == "wl-layer-51"
            ));

            let snapshot = state.snapshot();
            let popup = snapshot
                .known_surfaces
                .popups
                .iter()
                .find(|popup| popup.surface_id == "wl-popup-51")
                .unwrap();
            assert_eq!(
                popup.parent,
                SmithayPopupParentSnapshot::Resolved {
                    surface_id: "wl-layer-51".into(),
                    window_id: None,
                }
            );
        }
    }
}

#[cfg(feature = "smithay-winit")]
pub use imp::{
    SmithayClientState, SmithayClipboardSelectionSnapshot, SmithayKnownLayerSurface,
    SmithayKnownPopupSurface, SmithayKnownSurface, SmithayKnownSurfacesSnapshot,
    SmithayKnownToplevelSurface, SmithayKnownUnmanagedSurface,
    SmithayLayerSurfaceConfigureSnapshot, SmithayPopupParentSnapshot,
    SmithayRenderableToplevelSurface, SmithaySelectionOfferSnapshot, SmithayStateError,
    SmithayStateSnapshot, SmithaySurfaceRoleCounts, SmithayTitlebarRenderSnapshot,
    SmithayWindowDecorationPolicySnapshot, SmithayWindowRenderSnapshot,
    SmithayXdgPopupConfigureSnapshot, SmithayXdgToplevelConfigureSnapshot,
    SmithayXdgToplevelMetadataSnapshot, SmithayXdgToplevelRequestSnapshot, SpidersSmithayState,
};
