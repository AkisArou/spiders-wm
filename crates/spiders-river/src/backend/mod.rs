use std::collections::HashMap;
use std::os::fd::AsFd;
use std::io::{Seek, SeekFrom, Write};
use std::process::Command;

use anyhow::{Result, anyhow};
use spiders_config::authoring_layout::AuthoringLayoutService;
use spiders_config::model::{Binding, Config};
use spiders_config::model::ConfigPaths;
use spiders_config::model::InputConfig;
use spiders_scene::pipeline::SceneCache;
use spiders_shared::api::{FocusDirection, WmAction};
use spiders_runtime_js::{build_authoring_layout_service, DefaultLayoutRuntime};
use wayland_backend::client::ObjectId;
use wayland_client::protocol::{wl_output, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use tracing::{info, warn};
use xkbcommon::xkb;

use crate::actions::{
    activate_workspace, active_workspace_window_ids, compute_horizontal_tiles,
    configured_mode_for_window, configured_workspace_for_window, focus_target_in_direction,
};
use crate::action_bridge::{RiverCommand, bridge_action};
use crate::protocol::river_input_management::{river_input_device_v1, river_input_manager_v1};
use crate::protocol::river_layer_shell::river_layer_shell_v1;
use crate::protocol::river_libinput_config::{
    river_libinput_config_v1, river_libinput_device_v1, river_libinput_result_v1,
};
use crate::protocol::river_window_management_v1::{
    river_node_v1, river_output_v1, river_pointer_binding_v1, river_seat_v1,
    river_window_manager_v1, river_window_v1,
};
use crate::protocol::river_xkb_bindings::{river_xkb_binding_v1, river_xkb_bindings_v1};
use crate::protocol::river_xkb_config::{
    river_xkb_config_v1, river_xkb_keyboard_v1, river_xkb_keymap_v1,
};
use crate::protocol::{
    RIVER_INPUT_MANAGEMENT_GLOBAL, RIVER_LAYER_SHELL_GLOBAL, RIVER_LIBINPUT_CONFIG_GLOBAL,
    RIVER_WINDOW_MANAGEMENT_GLOBAL, RIVER_XKB_BINDINGS_GLOBAL, RIVER_XKB_CONFIG_GLOBAL,
    RiverProtocolSupport,
};
use crate::model::{SeatPointerOpState, WindowState, WmState};
use crate::runtime::{
    BindingTargetKind, InputDeviceKind, InputDeviceRecord, LibinputDeviceRecord, OutputRecord,
    ParsedBinding, PointerBindingRecord, RiverRegistry, SeatRecord, WindowRecord,
    WlOutputRecord, WlSeatRecord, XkbBindingRecord, XkbKeyboardRecord,
};

mod apply;
mod dispatch;
mod planner;
mod plan;
mod transient;

use self::apply::{effective_bindings, parse_binding};
use self::plan::{
    ActivateWorkspacePlan, BorderPlan, ClearTiledStatePlan, CloseWindowPlan, CommandPlan,
    FocusPlan, ManageWindowPlan, MoveFocusedWindowToWorkspacePlan, MoveWindowToTopPlan,
    MoveWindowInWorkspacePlan, OffscreenWindowPlan, PointerRenderPlan, RenderWindowPlan,
    ResizeWindowPlan, WindowModePlan,
};
use self::transient::BackendTransientState;

#[derive(Debug)]
pub struct RiverConnection {
    connection: Connection,
    event_queue: EventQueue<RiverBackendState>,
    state: RiverBackendState,
}

impl RiverConnection {
    pub fn connect(paths: ConfigPaths, config: Config, runtime_state: WmState) -> Result<Self> {
        let connection = Connection::connect_to_env()?;
        let display = connection.display();
        let mut event_queue = connection.new_event_queue();
        let qh = event_queue.handle();

        let _registry = display.get_registry(&qh, ());

        let mut state = RiverBackendState::new(paths, config, runtime_state);
        event_queue.roundtrip(&mut state)?;

        if !state.protocol_support.supports_minimum_viable_wm() {
            return Err(anyhow!(
                "river_window_manager_v1 not available; is river running?"
            ));
        }

        if !state.protocol_support.xkb_bindings {
            return Err(anyhow!(
                "river_xkb_bindings_v1 not available; milestone keybindings require xkb bindings support"
            ));
        }

        Ok(Self {
            connection,
            event_queue,
            state,
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn protocol_support(&self) -> &RiverProtocolSupport {
        &self.state.protocol_support
    }

    pub fn state(&self) -> &WmState {
        &self.state.runtime_state
    }

    pub fn blocking_dispatch(&mut self) -> Result<usize> {
        Ok(self.event_queue.blocking_dispatch(&mut self.state)?)
    }

    pub fn roundtrip(&mut self) -> Result<usize> {
        Ok(self.event_queue.roundtrip(&mut self.state)?)
    }

    pub fn is_running(&self) -> bool {
        self.state.running
    }
}

#[derive(Debug)]
struct RiverBackendState {
    config: Config,
    layout_service: AuthoringLayoutService<DefaultLayoutRuntime>,
    scene_cache: SceneCache,
    protocol_support: RiverProtocolSupport,
    runtime_state: WmState,
    running: bool,
    wm: Option<river_window_manager_v1::RiverWindowManagerV1>,
    layer_shell: Option<river_layer_shell_v1::RiverLayerShellV1>,
    xkb_bindings: Option<river_xkb_bindings_v1::RiverXkbBindingsV1>,
    input_manager: Option<river_input_manager_v1::RiverInputManagerV1>,
    xkb_config: Option<river_xkb_config_v1::RiverXkbConfigV1>,
    libinput_config: Option<river_libinput_config_v1::RiverLibinputConfigV1>,
    registry: RiverRegistry,
    transient: BackendTransientState,
    next_output_serial: usize,
    next_window_serial: usize,
    next_seat_serial: usize,
}

impl RiverBackendState {
    fn prewarm_scene_cache(&mut self) {
        self.scene_cache.clear();

        let snapshot = self.runtime_state.as_state_snapshot();
        let base_workspace = snapshot.current_workspace().cloned();

        for layout in &self.config.layouts {
            let workspace = base_workspace.clone().unwrap_or(spiders_shared::wm::WorkspaceSnapshot {
                id: spiders_tree::WorkspaceId::from("warmup"),
                name: "warmup".into(),
                output_id: None,
                active_workspaces: Vec::new(),
                focused: true,
                visible: true,
                effective_layout: None,
            });
            let mut workspace = workspace;
            workspace.effective_layout = Some(spiders_shared::wm::LayoutRef {
                name: layout.name.clone(),
            });

            let prepared = match self.layout_service.prepare_for_workspace(&self.config, &workspace) {
                Ok(Some(prepared)) => prepared,
                Ok(None) => continue,
                Err(error) => {
                    warn!(layout = %layout.name, %error, "failed to prepare layout for scene cache prewarm");
                    continue;
                }
            };

            let source = prepared.stylesheets.combined_source();
            if source.trim().is_empty() {
                continue;
            }

            if let Err(error) = self.scene_cache.precompile_layout(layout.name.clone(), &source) {
                warn!(layout = %layout.name, %error, "failed to precompile stylesheet for scene cache");
            }
        }
    }

    fn seat_name(&self, seat_id: &ObjectId) -> Option<&str> {
        self.registry
            .seats
            .get(seat_id)
            .map(|seat| seat.state_name.as_str())
    }

    fn seat_focused_state_window_id(&self, seat_id: &ObjectId) -> Option<spiders_tree::WindowId> {
        let seat_name = self.seat_name(seat_id)?;
        self.runtime_state
            .seats
            .get(seat_name)
            .and_then(|seat| seat.focused_window_id.clone())
    }

    fn seat_interacted_object_id(&self, seat_id: &ObjectId) -> Option<ObjectId> {
        let state_window_id = self
            .seat_name(seat_id)
            .and_then(|seat_name| self.runtime_state.seats.get(seat_name))
            .and_then(|seat| seat.interacted_window_id.clone())?;

        self.registry
            .window_ids_by_state
            .get(&state_window_id)
            .cloned()
    }

    fn window_object_id(&self, state_id: &spiders_tree::WindowId) -> Option<ObjectId> {
        self.registry.window_ids_by_state.get(state_id).cloned()
    }

    fn apply_window_rules(&mut self, window_id: &spiders_tree::WindowId) {
        let Some(window) = self.runtime_state.windows.get(window_id).cloned() else {
            return;
        };

        if let Some(workspace_id) = configured_workspace_for_window(&self.config, &window)
            && self.runtime_state.workspaces.contains_key(&workspace_id)
        {
            self.runtime_state.set_window_workspace(window_id, &workspace_id);
        }

        if let Some(mode) = configured_mode_for_window(&self.config, &window) {
            self.runtime_state.set_window_mode(window_id, mode);
        }
    }

    fn configured_input<'a>(&'a self, device_name: &str) -> Option<&'a InputConfig> {
        self.config.inputs.iter().find(|input| input.name == device_name)
    }

    fn apply_xkb_keymap_for_device(
        &mut self,
        input_device_id: &ObjectId,
        config: &InputConfig,
        qh: &QueueHandle<Self>,
    ) {
        let Some(xkb_config) = self.xkb_config.as_ref() else {
            return;
        };
        let device_name = self
            .registry
            .input_devices
            .get(input_device_id)
            .and_then(|device| device.name.clone())
            .unwrap_or_else(|| format!("device:{input_device_id:?}"));

        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let Some(keymap) = xkb::Keymap::new_from_names(
            &context,
            "",
            config.xkb_model.as_deref().unwrap_or(""),
            config.xkb_layout.as_deref().unwrap_or(""),
            config.xkb_variant.as_deref().unwrap_or(""),
            config.xkb_options.clone(),
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        ) else {
            return;
        };
        let keymap_text = keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
        let Ok(mut file) = tempfile::tempfile() else {
            return;
        };
        if file.write_all(keymap_text.as_bytes()).is_err() || file.seek(SeekFrom::Start(0)).is_err() {
            return;
        }

        let keymap_proxy = xkb_config.create_keymap(
            file.as_fd(),
            crate::protocol::river_xkb_config::river_xkb_config_v1::KeymapFormat::TextV1,
            qh,
            (),
        );
        self.transient
            .pending_xkb_keymaps
            .insert(keymap_proxy.id(), input_device_id.clone());
        self.transient.pending_xkb_keymap_context.insert(
            keymap_proxy.id(),
            format!(
                "device={} layout={:?} model={:?} variant={:?} options={:?}",
                device_name, config.xkb_layout, config.xkb_model, config.xkb_variant, config.xkb_options
            ),
        );
        self.transient
            .xkb_keymap_proxies
            .insert(keymap_proxy.id(), keymap_proxy);
    }

    fn track_input_result<T: Proxy>(&mut self, proxy: &T, description: String) {
        self.transient
            .pending_input_results
            .insert(proxy.id(), description);
    }

    fn apply_input_config_for_device(&mut self, input_device_id: &ObjectId, qh: &QueueHandle<Self>) {
        let Some(device) = self.registry.input_devices.get(input_device_id).cloned() else {
            return;
        };
        let Some(device_name) = device.name.as_deref() else {
            return;
        };
        let Some(config) = self.configured_input(device_name).cloned() else {
            return;
        };

        if matches!(device.kind, Some(InputDeviceKind::Keyboard)) {
            if let Some(rate) = config.repeat_rate {
                let delay = config.repeat_delay.unwrap_or(600);
                device.proxy.set_repeat_info(rate as i32, delay as i32);
            } else if let Some(delay) = config.repeat_delay {
                device.proxy.set_repeat_info(25, delay as i32);
            }

            if config.xkb_layout.is_some()
                || config.xkb_model.is_some()
                || config.xkb_variant.is_some()
                || config.xkb_options.is_some()
            {
                self.apply_xkb_keymap_for_device(input_device_id, &config, qh);
            } else if let Some(layout_name) = config.xkb_layout.as_deref() {
                for keyboard in self.registry.xkb_keyboards.values() {
                    if keyboard.input_device_id.as_ref() == Some(input_device_id) {
                        keyboard.proxy.set_layout_by_name(layout_name.to_owned());
                    }
                }
            }
        }

        let libinput_devices = self.registry.libinput_devices.values().cloned().collect::<Vec<_>>();
        for libinput in &libinput_devices {
            if libinput.input_device_id.as_ref() != Some(input_device_id) {
                continue;
            }

            if let Some(enabled) = config.tap {
                let state = if enabled {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::TapState::Enabled
                } else {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::TapState::Disabled
                };
                let result = libinput.proxy.set_tap(state, qh, ());
                self.track_input_result(&result, format!("set tap={} for {}", enabled, device_name));
            }
            if let Some(enabled) = config.natural_scroll {
                let state = if enabled {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::NaturalScrollState::Enabled
                } else {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::NaturalScrollState::Disabled
                };
                let result = libinput.proxy.set_natural_scroll(state, qh, ());
                self.track_input_result(
                    &result,
                    format!("set natural_scroll={} for {}", enabled, device_name),
                );
            }
            if let Some(enabled) = config.left_handed {
                let state = if enabled {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::LeftHandedState::Enabled
                } else {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::LeftHandedState::Disabled
                };
                let result = libinput.proxy.set_left_handed(state, qh, ());
                self.track_input_result(
                    &result,
                    format!("set left_handed={} for {}", enabled, device_name),
                );
            }
            if let Some(enabled) = config.middle_emulation {
                let state = if enabled {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::MiddleEmulationState::Enabled
                } else {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::MiddleEmulationState::Disabled
                };
                let result = libinput.proxy.set_middle_emulation(state, qh, ());
                self.track_input_result(
                    &result,
                    format!("set middle_emulation={} for {}", enabled, device_name),
                );
            }
            if let Some(enabled) = config.dwt {
                let state = if enabled {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::DwtState::Enabled
                } else {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::DwtState::Disabled
                };
                let result = libinput.proxy.set_dwt(state, qh, ());
                self.track_input_result(&result, format!("set dwt={} for {}", enabled, device_name));
            }
            if let Some(enabled) = config.drag_lock {
                let state = if enabled {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::DragLockState::EnabledSticky
                } else {
                    crate::protocol::river_libinput_config::river_libinput_device_v1::DragLockState::Disabled
                };
                let result = libinput.proxy.set_drag_lock(state, qh, ());
                self.track_input_result(
                    &result,
                    format!("set drag_lock={} for {}", enabled, device_name),
                );
            }
            if let Some(profile) = config.accel_profile.as_deref() {
                let profile = match profile {
                    "flat" => Some(
                        crate::protocol::river_libinput_config::river_libinput_device_v1::AccelProfile::Flat,
                    ),
                    "adaptive" => Some(
                        crate::protocol::river_libinput_config::river_libinput_device_v1::AccelProfile::Adaptive,
                    ),
                    "none" => Some(
                        crate::protocol::river_libinput_config::river_libinput_device_v1::AccelProfile::None,
                    ),
                    _ => None,
                };
                if let Some(profile) = profile {
                    let result = libinput.proxy.set_accel_profile(profile, qh, ());
                    self.track_input_result(
                        &result,
                        format!("set accel_profile={profile:?} for {}", device_name),
                    );
                }
            }
            if let Some(pointer_accel) = config.pointer_accel {
                let speed = pointer_accel.clamp(-1.0, 1.0).to_ne_bytes().to_vec();
                let result = libinput.proxy.set_accel_speed(speed, qh, ());
                self.track_input_result(
                    &result,
                    format!("set pointer_accel={} for {}", pointer_accel, device_name),
                );
            }
        }
    }

    fn queue_seat_command(&mut self, seat_id: &ObjectId, command: RiverCommand) {
        self.transient
            .seat_command_mailbox
            .entry(seat_id.clone())
            .or_default()
            .push_back(command);
    }

    fn new(paths: ConfigPaths, config: Config, runtime_state: WmState) -> Self {
        let mut state = Self {
            config,
            layout_service: build_authoring_layout_service(&paths),
            scene_cache: SceneCache::new(),
            protocol_support: RiverProtocolSupport::default(),
            runtime_state,
            running: true,
            wm: None,
            layer_shell: None,
            xkb_bindings: None,
            input_manager: None,
            xkb_config: None,
            libinput_config: None,
            registry: RiverRegistry::default(),
            transient: BackendTransientState::default(),
            next_output_serial: 0,
            next_window_serial: 0,
            next_seat_serial: 0,
        };

        state.prewarm_scene_cache();
        state
    }

    fn next_output_id(&mut self) -> spiders_tree::OutputId {
        self.next_output_serial += 1;
        format!("river-output-{}", self.next_output_serial).into()
    }

    fn next_window_id(&mut self) -> spiders_tree::WindowId {
        self.next_window_serial += 1;
        format!("river-window-{}", self.next_window_serial).into()
    }

    fn next_seat_name(&mut self) -> String {
        self.next_seat_serial += 1;
        format!("seat-{}", self.next_seat_serial)
    }

    fn output_name_for_global(&self, global_name: u32) -> String {
        self.registry
            .wl_outputs_by_global
            .get(&global_name)
            .and_then(|record| record.logical_name.clone())
            .unwrap_or_else(|| format!("wl-output-{global_name}"))
    }

    fn seat_name_for_global(&self, global_name: u32) -> String {
        self.registry
            .wl_seats_by_global
            .get(&global_name)
            .and_then(|record| record.logical_name.clone())
            .unwrap_or_else(|| format!("wl-seat-{global_name}"))
    }

    fn rename_seat(&mut self, previous_name: &str, next_name: String) {
        if previous_name == next_name {
            return;
        }
        self.runtime_state.remove_seat(previous_name);
        self.runtime_state.insert_seat(next_name);
    }

    fn destroy_protocol_objects(&mut self) {
        for seat in self.registry.seats.values_mut() {
            for binding in seat.xkb_bindings.drain().map(|(_, binding)| binding) {
                binding.proxy.destroy();
            }
            for binding in seat.pointer_bindings.drain().map(|(_, binding)| binding) {
                binding.proxy.destroy();
            }
            seat.proxy.destroy();
        }

        for window in self.registry.windows.values() {
            window.node.destroy();
            window.proxy.destroy();
        }
        for output in self.registry.outputs.values() {
            output.proxy.destroy();
        }
        if let Some(xkb) = self.xkb_bindings.take() {
            xkb.destroy();
        }
        if let Some(libinput) = self.libinput_config.take() {
            libinput.stop();
        }
        if let Some(xkb_config) = self.xkb_config.take() {
            xkb_config.stop();
        }
        if let Some(input_manager) = self.input_manager.take() {
            input_manager.stop();
        }
        if let Some(layer_shell) = self.layer_shell.take() {
            layer_shell.destroy();
        }
        if let Some(wm) = self.wm.take() {
            wm.destroy();
        }
    }

    fn remove_outputs(&mut self) {
        let removed = self
            .registry
            .outputs
            .iter()
            .filter(|(id, _)| self.transient.pending_output_removals.contains(*id))
            .map(|(id, output)| (id.clone(), output.state_id.clone(), output.proxy.clone()))
            .collect::<Vec<_>>();

        for (id, state_id, proxy) in removed {
            self.transient.pending_output_removals.remove(&id);
            self.transient.output_global_links.remove(&id);
            self.registry.output_ids_by_state.remove(&state_id);
            self.registry.outputs.remove(&id);
            self.runtime_state.remove_output(&state_id);
            proxy.destroy();
        }
    }

    fn remove_windows(&mut self) {
        let removed = self
            .registry
            .windows
            .iter()
            .filter(|(_, window)| {
                self.runtime_state
                    .windows
                    .get(&window.state_id)
                    .is_some_and(|state| state.closed)
            })
            .map(|(id, window)| {
                (
                    id.clone(),
                    window.state_id.clone(),
                    window.proxy.clone(),
                    window.node.clone(),
                )
            })
            .collect::<Vec<_>>();

        for (id, state_id, proxy, node) in removed {
            self.transient.window_pointer_move_requests.remove(&id);
            self.transient.window_pointer_resize_requests.remove(&id);
            self.registry.window_ids_by_state.remove(&state_id);
            for seat in self.registry.seats.values_mut() {
                if self
                    .runtime_state
                    .seats
                    .get(&seat.state_name)
                    .is_some_and(|seat_state| match &seat_state.pointer_op {
                        SeatPointerOpState::Move { window_id, .. }
                        | SeatPointerOpState::Resize { window_id, .. } => window_id == &state_id,
                        SeatPointerOpState::None => false,
                    })
                {
                    self.runtime_state
                        .set_seat_pointer_op(&seat.state_name, SeatPointerOpState::None);
                }
            }
            for seat in self.runtime_state.seats.values_mut() {
                if seat.focused_window_id.as_ref() == Some(&state_id) {
                    seat.focused_window_id = None;
                }
                if seat.hovered_window_id.as_ref() == Some(&state_id) {
                    seat.hovered_window_id = None;
                }
                if seat.interacted_window_id.as_ref() == Some(&state_id) {
                    seat.interacted_window_id = None;
                }
            }
            self.registry.windows.remove(&id);
            self.runtime_state.remove_window(&state_id);
            node.destroy();
            proxy.destroy();
        }
    }

    fn remove_seats(&mut self) {
        let removed = self
            .registry
            .seats
            .iter()
            .filter(|(id, _)| self.transient.pending_seat_removals.contains(*id))
            .map(|(id, seat)| {
                (
                    id.clone(),
                    seat.state_name.clone(),
                    seat.proxy.clone(),
                    seat.xkb_bindings
                        .values()
                        .map(|binding| binding.proxy.clone())
                        .collect::<Vec<_>>(),
                    seat.pointer_bindings
                        .values()
                        .map(|binding| binding.proxy.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();

        for (id, seat_name, proxy, xkb_bindings, pointer_bindings) in removed {
            self.transient.pending_seat_removals.remove(&id);
            self.transient.seat_global_links.remove(&id);
            self.transient.seat_command_mailbox.remove(&id);
            self.transient.initialized_seat_bindings.remove(&id);
            self.registry.seats.remove(&id);
            self.runtime_state.remove_seat(&seat_name);
            for binding in xkb_bindings {
                binding.destroy();
            }
            for binding in pointer_bindings {
                binding.destroy();
            }
            proxy.destroy();
        }
    }

    fn initialize_seat_bindings(&mut self, qh: &QueueHandle<Self>) {
        let Some(xkb_bindings) = self.xkb_bindings.clone() else {
            return;
        };

        let parsed_bindings = effective_bindings(&self.config)
            .iter()
            .filter_map(parse_binding)
            .collect::<Vec<_>>();

        for seat in self.registry.seats.values_mut() {
            if self.transient.initialized_seat_bindings.contains(&seat.proxy.id()) {
                continue;
            }

            for binding in &parsed_bindings {
                match binding.kind {
                    BindingTargetKind::Key => {
                        if let Some(keysym) = binding.key {
                            let proxy = xkb_bindings.get_xkb_binding(
                                &seat.proxy,
                                keysym,
                                binding.modifiers,
                                qh,
                                seat.proxy.id(),
                            );
                            proxy.enable();
                            seat.xkb_bindings.insert(
                                proxy.id(),
                                XkbBindingRecord {
                                    proxy,
                                    action: binding.action.clone(),
                                },
                            );
                        }
                    }
                    BindingTargetKind::Pointer => {
                        if let Some(button) = binding.button {
                            let proxy = seat.proxy.get_pointer_binding(
                                button,
                                binding.modifiers,
                                qh,
                                seat.proxy.id(),
                            );
                            proxy.enable();
                            seat.pointer_bindings.insert(
                                proxy.id(),
                                PointerBindingRecord {
                                    proxy,
                                    action: binding.action.clone(),
                                },
                            );
                        }
                    }
                }
            }

            self.transient.initialized_seat_bindings.insert(seat.proxy.id());
        }
    }

    fn handle_window_requests(&mut self) {
        let pending = self
            .registry
            .windows
            .keys()
            .cloned()
            .map(|id| {
                (
                    id.clone(),
                    self.transient.window_pointer_move_requests.get(&id).cloned(),
                    self.transient.window_pointer_resize_requests.get(&id).cloned(),
                )
            })
            .collect::<Vec<_>>();

        for (window_id, move_request, resize_request) in pending {
            if let Some(seat_id) = move_request
                && let Some(window) = self.registry.windows.get(&window_id).cloned()
                && let Some(window_state) = self.runtime_state.windows.get(&window.state_id).cloned()
                && let Some(seat) = self.registry.seats.get_mut(&seat_id)
            {
                seat.pointer_move(&mut self.runtime_state, &window, &window_state);
                self.transient.window_pointer_move_requests.remove(&window_id);
            }

            if let Some((seat_id, edges)) = resize_request
                && let Some(window) = self.registry.windows.get(&window_id).cloned()
                && let Some(window_state) = self.runtime_state.windows.get(&window.state_id).cloned()
                && let Some(seat) = self.registry.seats.get_mut(&seat_id)
            {
                seat.pointer_resize(&mut self.runtime_state, &window, &window_state, edges);
                self.transient.window_pointer_resize_requests.remove(&window_id);
            }
        }
    }

    fn active_workspace_window_state_ids(&self) -> Vec<spiders_tree::WindowId> {
        let state_window_stack = self.runtime_state.window_stack.iter().cloned().collect::<Vec<_>>();

        active_workspace_window_ids(&self.runtime_state, &state_window_stack)
    }

    fn active_workspace_window_ids(&self) -> Vec<ObjectId> {
        let active_state_ids = self.active_workspace_window_state_ids();

        active_state_ids
            .into_iter()
            .filter_map(|state_id| self.window_object_id(&state_id))
            .collect()
    }

    fn initialize_new_windows(&mut self) {
        let output_geometry = self.current_output_geometry();

        let new_window_ids = self
            .runtime_state
            .windows
            .values()
            .filter(|window| window.is_new)
            .map(|window| window.id.clone())
            .collect::<Vec<_>>();

        for window_id in new_window_ids {
            let (x, y, width, height) = output_geometry;
            self.runtime_state
                .set_window_geometry(&window_id, x, y, width, height);
            self.runtime_state.set_window_new(&window_id, false);
        }
    }

    fn current_output_geometry(&self) -> (i32, i32, i32, i32) {
        self.runtime_state
            .current_output_id
            .as_ref()
            .and_then(|output_id| self.runtime_state.outputs.get(output_id))
            .filter(|output| output.enabled)
            .map(|output| {
                let width = output.logical_width.max(1) as i32;
                let height = output.logical_height.max(1) as i32;
                (output.logical_x, output.logical_y, width, height)
            })
            .or_else(|| {
                self.runtime_state
                    .outputs
                    .values()
                    .find(|output| output.enabled)
                    .map(|output| {
                    let width = output.logical_width.max(1) as i32;
                    let height = output.logical_height.max(1) as i32;
                    (output.logical_x, output.logical_y, width, height)
                })
            })
            .unwrap_or((0, 0, 1024, 768))
    }

    fn execute_command_plan(&mut self, seat_id: &ObjectId, plan: CommandPlan) {
        match plan {
            CommandPlan::Spawn { command } => {
                let _ = Command::new("sh").arg("-lc").arg(command).spawn();
            }
            CommandPlan::ActivateWorkspace(plan) => {
                activate_workspace(&mut self.runtime_state, &plan.workspace_id);
                self.apply_focus_plan(seat_id, &plan.focus);
            }
            CommandPlan::MoveFocusedWindowToWorkspace(plan) => {
                self.apply_move_focused_window_to_workspace_plan(seat_id, &plan);
            }
            CommandPlan::MoveWindowInWorkspace(plan) => {
                self.apply_move_window_in_workspace_plan(seat_id, &plan);
            }
            CommandPlan::SetWindowMode(plan) => {
                self.apply_window_mode_plan(&[plan]);
            }
            CommandPlan::FocusOutput { output_id } => {
                self.runtime_state.focus_output(&output_id);
            }
            CommandPlan::FocusWindow { stack, focus } => {
                self.runtime_state.move_window_to_top(&stack.window_id);
                self.apply_focus_plan(seat_id, &focus);
            }
            CommandPlan::CloseFocusedWindow => {
                if let Some(plan) = self.plan_close_focused_window(seat_id) {
                    self.apply_close_window_plan(&plan);
                }
            }
            CommandPlan::FocusDirection { stack, focus } => {
                self.runtime_state.move_window_to_top(&stack.window_id);
                self.apply_focus_plan(seat_id, &focus);
            }
            CommandPlan::Noop => {}
        }
    }

    fn handle_manage_start(&mut self, qh: &QueueHandle<Self>) {
        self.remove_outputs();
        self.remove_windows();
        self.remove_seats();
        self.initialize_new_windows();
        self.initialize_seat_bindings(qh);
        self.handle_window_requests();
        let mode_plans = self.plan_window_mode_updates();
        self.apply_window_mode_plan(&mode_plans);

        let seat_ids = self.registry.seats.keys().cloned().collect::<Vec<_>>();
        for seat_id in seat_ids {
            let interacted = self.seat_interacted_object_id(&seat_id);
            if let Some(seat_name) = self.seat_name(&seat_id).map(str::to_owned) {
                self.runtime_state.set_seat_interacted_window(&seat_name, None);
            }
            if let Some(window_id) = interacted {
                if let Some(state_id) = self
                    .registry
                    .windows
                    .get(&window_id)
                    .map(|window| window.state_id.clone())
                {
                    self.runtime_state.move_window_to_top(&state_id);
                }
            }

            self.focus_top_window_for_seat(&seat_id);

            let commands = self
                .transient
                .seat_command_mailbox
                .remove(&seat_id)
                .map(|commands| commands.into_iter().collect::<Vec<_>>())
                .unwrap_or_default();

            for command in commands {
                let plan = self.plan_command(&seat_id, command);
                self.execute_command_plan(&seat_id, plan);
            }

            if let Some(seat) = self.registry.seats.get_mut(&seat_id) {
                let pointer_released = self
                    .runtime_state
                    .seats
                    .get(&seat.state_name)
                    .is_some_and(|seat_state| seat_state.pointer_op_release);
                if pointer_released {
                    seat.op_end(&mut self.runtime_state);
                    self.runtime_state
                        .set_seat_pointer_release(&seat.state_name, false);
                } else {
                    let plans = seat
                        .op_manage(&self.runtime_state, &self.registry.windows)
                        .into_iter()
                        .collect::<Vec<_>>();
                    self.apply_resize_window_plan(&plans);
                }
            }
        }

        if !self.has_active_pointer_op() {
            self.apply_tiled_manage_layout();
        }

        if let Some(wm) = self.wm.as_ref() {
            wm.manage_finish();
        }
    }

    fn handle_render_start(&mut self) {
        if !self.has_active_pointer_op() {
            self.apply_tiled_render_layout();
        }

        self.apply_window_borders();

        let plan = self.plan_pointer_render_ops();
        self.apply_pointer_render_plan(&plan);

        if let Some(wm) = self.wm.as_ref() {
            wm.render_finish();
        }
    }
}


#[cfg(test)]
    mod tests {
        use super::*;
    use spiders_config::model::{ConfigOptions, WindowRule};
    use spiders_tree::LayoutRect;

    #[test]
    fn falls_back_to_milestone_bindings_when_config_is_empty() {
        let bindings = effective_bindings(&Config::default());

        assert_eq!(bindings.len(), 26);
        assert_eq!(bindings[0].trigger, "Alt+Enter");
        assert_eq!(bindings[1].trigger, "Alt+q");
        assert_eq!(bindings[2].trigger, "Alt+h");
        assert_eq!(bindings[3].trigger, "Alt+l");
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+1"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+9"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+1"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+9"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+h"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+j"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+k"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+l"));
    }

    #[test]
    fn explicit_config_bindings_override_defaults() {
        let config = Config {
            bindings: vec![Binding {
                trigger: "Alt+Enter".into(),
                action: WmAction::Spawn {
                    command: "alacritty".into(),
                },
            }],
            ..Config::default()
        };

        let bindings = effective_bindings(&config);

        assert_eq!(bindings.len(), 26);
        assert_eq!(bindings[0].trigger, "Alt+Enter");
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+q"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+h"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+l"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+1"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+9"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+1"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+9"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+h"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+j"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+k"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+l"));
    }

    #[test]
    fn unparseable_config_bindings_still_get_milestone_defaults() {
        let config = Config {
            bindings: vec![Binding {
                trigger: "Weird+Thing+Unsupported".into(),
                action: WmAction::Spawn {
                    command: "foo".into(),
                },
            }],
            ..Config::default()
        };

        let bindings = effective_bindings(&config);

        assert!(
            bindings
                .iter()
                .any(|binding| binding.trigger == "Alt+Enter")
        );
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+q"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+1"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+9"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+1"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+9"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+h"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Alt+Shift+j"));
    }

    #[test]
    fn milestone_bindings_follow_config_mod_key() {
        let config = Config {
            options: ConfigOptions {
                mod_key: Some("Super".into()),
                ..ConfigOptions::default()
            },
            ..Config::default()
        };

        let bindings = effective_bindings(&config);

        assert!(bindings.iter().any(|binding| binding.trigger == "Super+Enter"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Super+1"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Super+Shift+1"));
        assert!(bindings.iter().any(|binding| binding.trigger == "Super+Shift+h"));
        assert!(!bindings.iter().any(|binding| binding.trigger == "Alt+Enter"));
    }

    #[test]
    fn configured_workspace_rule_matches_exact_window_metadata() {
        let config = Config {
            rules: vec![WindowRule {
                app_id: Some("firefox".into()),
                title: Some("Mozilla Firefox".into()),
                workspaces: vec!["2".into()],
                ..WindowRule::default()
            }],
            ..Config::default()
        };
        let window = WindowState {
            id: "win-1".into(),
            app_id: Some("firefox".into()),
            title: Some("Mozilla Firefox".into()),
            class: Some("Navigator".into()),
            instance: Some("navigator".into()),
            role: Some("browser".into()),
            window_type: Some("normal".into()),
            identifier: Some("window-1".into()),
            unreliable_pid: Some(42),
            output_id: None,
            workspace_ids: vec!["1".into()],
            is_new: false,
            closed: false,
            mapped: true,
            mode: spiders_shared::wm::WindowMode::Tiled,
            focused: false,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            last_floating_rect: None,
        };

        let workspace_id = configured_workspace_for_window(&config, &window);

        assert_eq!(workspace_id, Some("2".into()));
    }

    #[test]
    fn configured_mode_rule_prefers_fullscreen_over_floating() {
        let config = Config {
            rules: vec![WindowRule {
                app_id: Some("firefox".into()),
                floating: Some(true),
                fullscreen: Some(true),
                ..WindowRule::default()
            }],
            ..Config::default()
        };
        let window = WindowState {
            id: "win-1".into(),
            app_id: Some("firefox".into()),
            title: None,
            class: None,
            instance: None,
            role: None,
            window_type: None,
            identifier: None,
            unreliable_pid: None,
            output_id: None,
            workspace_ids: vec!["1".into()],
            is_new: false,
            closed: false,
            mapped: true,
            mode: spiders_shared::wm::WindowMode::Tiled,
            focused: false,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            last_floating_rect: None,
        };

        let mode = configured_mode_for_window(&config, &window);

        assert_eq!(mode, Some(spiders_shared::wm::WindowMode::Fullscreen));
    }

    #[test]
    fn fullscreen_toggle_can_restore_last_floating_rect() {
        let mut state = WmState::from_config(&Config::default());
        state.insert_window("win-1".into());
        state.set_window_mode(
            &"win-1".into(),
            spiders_shared::wm::WindowMode::Floating {
                rect: Some(LayoutRect {
                    x: 10.0,
                    y: 20.0,
                    width: 300.0,
                    height: 200.0,
                }),
            },
        );
        state.set_window_geometry(&"win-1".into(), 10, 20, 300, 200);
        state.set_window_mode(&"win-1".into(), spiders_shared::wm::WindowMode::Fullscreen);

        let window = state.windows.get(&"win-1".into()).unwrap();

        assert_eq!(
            window.last_floating_rect,
            Some(LayoutRect {
                x: 10.0,
                y: 20.0,
                width: 300.0,
                height: 200.0,
            })
        );
    }

}
