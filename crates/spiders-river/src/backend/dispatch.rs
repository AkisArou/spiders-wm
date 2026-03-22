use super::*;

impl Dispatch<wl_registry::WlRegistry, ()> for RiverBackendState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                RIVER_WINDOW_MANAGEMENT_GLOBAL => {
                    let wm = registry.bind::<river_window_manager_v1::RiverWindowManagerV1, _, _>(
                        name,
                        version.min(3),
                        qh,
                        (),
                    );
                    state.protocol_support.window_management = true;
                    state.wm = Some(wm);
                }
                RIVER_LAYER_SHELL_GLOBAL => {
                    let layer_shell = registry
                        .bind::<river_layer_shell_v1::RiverLayerShellV1, _, _>(
                            name,
                            version.min(1),
                            qh,
                            (),
                        );
                    state.protocol_support.layer_shell = true;
                    state.layer_shell = Some(layer_shell);
                }
                RIVER_XKB_BINDINGS_GLOBAL => {
                    let xkb_bindings = registry
                        .bind::<river_xkb_bindings_v1::RiverXkbBindingsV1, _, _>(
                            name,
                            version.min(2),
                            qh,
                            (),
                        );
                    state.protocol_support.xkb_bindings = true;
                    state.xkb_bindings = Some(xkb_bindings);
                }
                RIVER_INPUT_MANAGEMENT_GLOBAL => {
                    let input_manager = registry
                        .bind::<river_input_manager_v1::RiverInputManagerV1, _, _>(
                            name,
                            version.min(1),
                            qh,
                            (),
                        );
                    state.protocol_support.input_management = true;
                    state.input_manager = Some(input_manager);
                }
                RIVER_XKB_CONFIG_GLOBAL => {
                    let xkb_config = registry.bind::<river_xkb_config_v1::RiverXkbConfigV1, _, _>(
                        name,
                        version.min(1),
                        qh,
                        (),
                    );
                    state.protocol_support.xkb_config = true;
                    state.xkb_config = Some(xkb_config);
                }
                RIVER_LIBINPUT_CONFIG_GLOBAL => {
                    let libinput_config = registry
                        .bind::<river_libinput_config_v1::RiverLibinputConfigV1, _, _>(
                            name,
                            version.min(1),
                            qh,
                            (),
                        );
                    state.protocol_support.libinput_config = true;
                    state.libinput_config = Some(libinput_config);
                }
                "wl_output" => {
                    let _ =
                        registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, name);
                    state
                        .registry
                        .wl_outputs_by_global
                        .insert(name, WlOutputRecord { logical_name: None });
                }
                "wl_seat" => {
                    let _ = registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(2), qh, name);
                    state
                        .registry
                        .wl_seats_by_global
                        .insert(name, WlSeatRecord { logical_name: None });
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => {
                state.registry.wl_outputs_by_global.remove(&name);
                state.registry.wl_seats_by_global.remove(&name);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for RiverBackendState {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        global_name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            if let Some(record) = state.registry.wl_outputs_by_global.get_mut(global_name) {
                record.logical_name = Some(name.clone());
            }

            let output_ids = state
                .registry
                .outputs
                .values()
                .filter(|record| {
                    state.transient.output_global_links.get(&record.proxy.id()) == Some(global_name)
                })
                .map(|record| record.state_id.clone())
                .collect::<Vec<_>>();

            for output_id in output_ids {
                state
                    .runtime_state
                    .set_output_name(&output_id, name.clone());
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, u32> for RiverBackendState {
    fn event(
        state: &mut Self,
        _proxy: &wl_seat::WlSeat,
        event: wl_seat::Event,
        global_name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Name { name } = event {
            if let Some(record) = state.registry.wl_seats_by_global.get_mut(global_name) {
                record.logical_name = Some(name.clone());
            }

            let seat_ids = state
                .registry
                .seats
                .iter()
                .filter(|(id, _)| state.transient.seat_global_links.get(*id) == Some(global_name))
                .map(|(id, record)| (id.clone(), record.state_name.clone()))
                .collect::<Vec<_>>();

            for (seat_id, previous_name) in seat_ids {
                state.rename_seat(&previous_name, name.clone());
                if let Some(record) = state.registry.seats.get_mut(&seat_id) {
                    record.state_name = name.clone();
                }
            }
        }
    }
}

impl Dispatch<river_window_manager_v1::RiverWindowManagerV1, ()> for RiverBackendState {
    wayland_client::event_created_child!(RiverBackendState, river_window_manager_v1::RiverWindowManagerV1, [
        river_window_manager_v1::EVT_WINDOW_OPCODE => (river_window_v1::RiverWindowV1, ()),
        river_window_manager_v1::EVT_OUTPUT_OPCODE => (river_output_v1::RiverOutputV1, ()),
        river_window_manager_v1::EVT_SEAT_OPCODE => (river_seat_v1::RiverSeatV1, ()),
    ]);

    fn event(
        state: &mut Self,
        _wm: &river_window_manager_v1::RiverWindowManagerV1,
        event: river_window_manager_v1::Event,
        _: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            river_window_manager_v1::Event::Unavailable => {
                state.running = false;
                state.destroy_protocol_objects();
            }
            river_window_manager_v1::Event::Finished => {
                state.running = false;
                if let Some(wm) = state.wm.take() {
                    wm.destroy();
                }
            }
            river_window_manager_v1::Event::Window { id } => {
                let window_id = state.next_window_id();
                let node = id.get_node(qh, ());
                state.runtime_state.insert_window(window_id.clone());
                state
                    .registry
                    .window_ids_by_state
                    .insert(window_id.clone(), id.id());
                state.registry.windows.insert(
                    id.id(),
                    WindowRecord {
                        proxy: id,
                        node,
                        state_id: window_id,
                    },
                );
            }
            river_window_manager_v1::Event::Output { id } => {
                let output_id = state.next_output_id();
                let output_name = output_id.as_str().to_owned();
                state
                    .runtime_state
                    .insert_output(output_id.clone(), output_name);
                if state.runtime_state.current_output_id.is_none() {
                    state.runtime_state.focus_output(&output_id);
                }
                state
                    .registry
                    .output_ids_by_state
                    .insert(output_id.clone(), id.id());
                state.registry.outputs.insert(
                    id.id(),
                    OutputRecord {
                        proxy: id,
                        state_id: output_id,
                    },
                );
            }
            river_window_manager_v1::Event::Seat { id } => {
                let seat_name = state.next_seat_name();
                state.runtime_state.insert_seat(seat_name.clone());
                state.registry.seats.insert(id.id(), SeatRecord::new(id, seat_name));
            }
            river_window_manager_v1::Event::ManageStart => state.handle_manage_start(qh),
            river_window_manager_v1::Event::RenderStart => state.handle_render_start(),
            river_window_manager_v1::Event::SessionLocked => {
                state.runtime_state.focused_window_id = None;
            }
            river_window_manager_v1::Event::SessionUnlocked => {}
        }
    }
}

impl Dispatch<river_window_v1::RiverWindowV1, ()> for RiverBackendState {
    fn event(
        state: &mut Self,
        proxy: &river_window_v1::RiverWindowV1,
        event: river_window_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(window) = state.registry.windows.get(&proxy.id()).cloned() else {
            return;
        };

        match event {
            river_window_v1::Event::Closed => {
                state.runtime_state.set_window_closed(&window.state_id, true);
            }
            river_window_v1::Event::Dimensions { width, height } => {
                let (x, y) = state
                    .runtime_state
                    .windows
                    .get(&window.state_id)
                    .map(|window| (window.x, window.y))
                    .unwrap_or((0, 0));
                state
                    .runtime_state
                    .set_window_geometry(&window.state_id, x, y, width, height);
            }
            river_window_v1::Event::AppId { app_id } => {
                state
                    .runtime_state
                    .set_window_app_id(&window.state_id, app_id);
                state.apply_window_rules(&window.state_id);
            }
            river_window_v1::Event::Title { title } => {
                state
                    .runtime_state
                    .set_window_title(&window.state_id, title);
                state.apply_window_rules(&window.state_id);
            }
            river_window_v1::Event::Identifier { identifier } => {
                state
                    .runtime_state
                    .set_window_identifier(&window.state_id, Some(identifier));
            }
            river_window_v1::Event::UnreliablePid { unreliable_pid } => {
                state
                    .runtime_state
                    .set_window_unreliable_pid(
                        &window.state_id,
                        u32::try_from(unreliable_pid).ok(),
                    );
            }
            river_window_v1::Event::PointerMoveRequested { seat } => {
                state.transient.window_pointer_move_requests.insert(proxy.id(), seat.id());
            }
            river_window_v1::Event::PointerResizeRequested { seat, edges } => {
                if let Ok(edges) = edges.into_result() {
                    state
                        .transient
                        .window_pointer_resize_requests
                        .insert(proxy.id(), (seat.id(), edges));
                }
            }
            river_window_v1::Event::FullscreenRequested { output } => {
                if let Some(output) = output
                    && let Some(output_record) = state.registry.outputs.get(&output.id())
                {
                    state
                        .runtime_state
                        .set_window_output(&window.state_id, Some(output_record.state_id.clone()));
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<river_output_v1::RiverOutputV1, ()> for RiverBackendState {
    fn event(
        state: &mut Self,
        proxy: &river_output_v1::RiverOutputV1,
        event: river_output_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let output_id = proxy.id();
        let Some(existing) = state.registry.outputs.get(&output_id).cloned() else {
            return;
        };

        match event {
            river_output_v1::Event::Removed => {
                state.transient.pending_output_removals.insert(output_id);
            }
            river_output_v1::Event::WlOutput { name } => {
                let resolved_name = state.output_name_for_global(name);
                state.transient.output_global_links.insert(output_id, name);
                state
                    .runtime_state
                    .set_output_name(&existing.state_id, resolved_name);
            }
            river_output_v1::Event::Position { x, y } => {
                state
                    .runtime_state
                    .set_output_position(&existing.state_id, x, y);
            }
            river_output_v1::Event::Dimensions { width, height } => {
                if width > 0 && height > 0 {
                    state.runtime_state.set_output_dimensions(
                        &existing.state_id,
                        width as u32,
                        height as u32,
                    );
                }
            }
        }
    }
}

impl Dispatch<river_seat_v1::RiverSeatV1, ()> for RiverBackendState {
    fn event(
        state: &mut Self,
        proxy: &river_seat_v1::RiverSeatV1,
        event: river_seat_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let seat_id = proxy.id();
        let Some(existing) = state.registry.seats.get(&seat_id).cloned() else {
            return;
        };

        match event {
            river_seat_v1::Event::Removed => {
                state.transient.pending_seat_removals.insert(seat_id);
            }
            river_seat_v1::Event::WlSeat { name } => {
                let resolved_name = state.seat_name_for_global(name);
                state.rename_seat(&existing.state_name, resolved_name.clone());
                if let Some(seat) = state.registry.seats.get_mut(&seat_id) {
                    seat.state_name = resolved_name;
                }
                state.transient.seat_global_links.insert(seat_id, name);
            }
            river_seat_v1::Event::PointerEnter { window } => {
                let hovered_window_id = state
                    .registry
                    .windows
                    .get(&window.id())
                    .map(|window| window.state_id.clone());
                if let Some(seat_name) = state.seat_name(&seat_id).map(str::to_owned) {
                    state
                        .runtime_state
                        .set_seat_hovered_window(&seat_name, hovered_window_id);
                }
            }
            river_seat_v1::Event::PointerLeave => {
                if let Some(seat_name) = state.seat_name(&seat_id).map(str::to_owned) {
                    state.runtime_state.set_seat_hovered_window(&seat_name, None);
                }
            }
            river_seat_v1::Event::WindowInteraction { window } => {
                let interacted_window_id = state
                    .registry
                    .windows
                    .get(&window.id())
                    .map(|window| window.state_id.clone());
                if let Some(seat_name) = state.seat_name(&seat_id).map(str::to_owned) {
                    state
                        .runtime_state
                        .set_seat_interacted_window(&seat_name, interacted_window_id);
                }
            }
            river_seat_v1::Event::OpDelta { dx, dy } => {
                if let Some(seat_name) = state.seat_name(&seat_id).map(str::to_owned) {
                    state.runtime_state.set_seat_pointer_delta(&seat_name, dx, dy);
                }
            }
            river_seat_v1::Event::OpRelease => {
                if let Some(seat_name) = state.seat_name(&seat_id).map(str::to_owned) {
                    state.runtime_state.set_seat_pointer_release(&seat_name, true);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<river_xkb_binding_v1::RiverXkbBindingV1, ObjectId> for RiverBackendState {
    fn event(
        state: &mut Self,
        proxy: &river_xkb_binding_v1::RiverXkbBindingV1,
        event: river_xkb_binding_v1::Event,
        seat_id: &ObjectId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(seat) = state.registry.seats.get(seat_id) else {
            return;
        };
        let Some(binding) = seat.xkb_bindings.get(&proxy.id()) else {
            return;
        };

        let action = binding.action.clone();
        let trigger = binding.trigger.clone();

        match event {
            river_xkb_binding_v1::Event::Pressed => {
                let command = bridge_action(&action);
                tracing::debug!(
                    trigger = %trigger,
                    action = ?action,
                    command = ?command,
                    "received keybinding press"
                );
                if let RiverCommand::Unsupported { action } = &command {
                    tracing::warn!(action = *action, "received keybinding for unsupported action");
                }
                state.queue_seat_command(seat_id, command);
            }
            river_xkb_binding_v1::Event::Released | river_xkb_binding_v1::Event::StopRepeat => {}
        }
    }
}

impl Dispatch<river_pointer_binding_v1::RiverPointerBindingV1, ObjectId> for RiverBackendState {
    fn event(
        state: &mut Self,
        proxy: &river_pointer_binding_v1::RiverPointerBindingV1,
        event: river_pointer_binding_v1::Event,
        seat_id: &ObjectId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(seat) = state.registry.seats.get(seat_id) else {
            return;
        };
        let Some(binding) = seat.pointer_bindings.get(&proxy.id()) else {
            return;
        };

        let action = binding.action.clone();
        let trigger = binding.trigger.clone();

        match event {
            river_pointer_binding_v1::Event::Pressed => {
                let command = bridge_action(&action);
                tracing::debug!(
                    trigger = %trigger,
                    action = ?action,
                    command = ?command,
                    "received pointer binding press"
                );
                if let RiverCommand::Unsupported { action } = &command {
                    tracing::warn!(action = *action, "received pointer binding for unsupported action");
                }
                state.queue_seat_command(seat_id, command);
            }
            river_pointer_binding_v1::Event::Released => {}
        }
    }
}

impl Dispatch<river_layer_shell_v1::RiverLayerShellV1, ()> for RiverBackendState {
    fn event(
        _state: &mut Self,
        _proxy: &river_layer_shell_v1::RiverLayerShellV1,
        _event: river_layer_shell_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<river_xkb_bindings_v1::RiverXkbBindingsV1, ()> for RiverBackendState {
    fn event(
        _state: &mut Self,
        _proxy: &river_xkb_bindings_v1::RiverXkbBindingsV1,
        _event: river_xkb_bindings_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<river_input_manager_v1::RiverInputManagerV1, ()> for RiverBackendState {
    wayland_client::event_created_child!(RiverBackendState, river_input_manager_v1::RiverInputManagerV1, [
        river_input_manager_v1::EVT_INPUT_DEVICE_OPCODE => (river_input_device_v1::RiverInputDeviceV1, ()),
    ]);

    fn event(
        state: &mut Self,
        _proxy: &river_input_manager_v1::RiverInputManagerV1,
        event: river_input_manager_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let river_input_manager_v1::Event::InputDevice { id } = event {
            state.registry.input_devices.insert(
                id.id(),
                InputDeviceRecord {
                    proxy: id,
                    name: None,
                    kind: None,
                },
            );
        }
    }
}

impl Dispatch<river_xkb_config_v1::RiverXkbConfigV1, ()> for RiverBackendState {
    wayland_client::event_created_child!(RiverBackendState, river_xkb_config_v1::RiverXkbConfigV1, [
        river_xkb_config_v1::EVT_XKB_KEYBOARD_OPCODE => (river_xkb_keyboard_v1::RiverXkbKeyboardV1, ()),
    ]);

    fn event(
        state: &mut Self,
        _proxy: &river_xkb_config_v1::RiverXkbConfigV1,
        event: river_xkb_config_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let river_xkb_config_v1::Event::XkbKeyboard { id } = event {
            state.registry.xkb_keyboards.insert(
                id.id(),
                XkbKeyboardRecord {
                    proxy: id,
                    input_device_id: None,
                },
            );
        }
    }
}

impl Dispatch<river_libinput_config_v1::RiverLibinputConfigV1, ()> for RiverBackendState {
    wayland_client::event_created_child!(RiverBackendState, river_libinput_config_v1::RiverLibinputConfigV1, [
        river_libinput_config_v1::EVT_LIBINPUT_DEVICE_OPCODE => (river_libinput_device_v1::RiverLibinputDeviceV1, ()),
    ]);

    fn event(
        state: &mut Self,
        _proxy: &river_libinput_config_v1::RiverLibinputConfigV1,
        event: river_libinput_config_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let river_libinput_config_v1::Event::LibinputDevice { id } = event {
            state.registry.libinput_devices.insert(
                id.id(),
                LibinputDeviceRecord {
                    proxy: id,
                    input_device_id: None,
                },
            );
        }
    }
}

impl Dispatch<river_node_v1::RiverNodeV1, ()> for RiverBackendState {
    fn event(
        _state: &mut Self,
        _proxy: &river_node_v1::RiverNodeV1,
        _event: river_node_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<river_input_device_v1::RiverInputDeviceV1, ()> for RiverBackendState {
    fn event(
        state: &mut Self,
        proxy: &river_input_device_v1::RiverInputDeviceV1,
        event: river_input_device_v1::Event,
        _: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(device) = state.registry.input_devices.get_mut(&proxy.id()) else {
            return;
        };
        match event {
            river_input_device_v1::Event::Removed => {
                state.registry.input_devices.remove(&proxy.id());
                proxy.destroy();
            }
            river_input_device_v1::Event::Type { _type } => {
                device.kind = match _type.into_result().ok() {
                    Some(river_input_device_v1::Type::Keyboard) => Some(InputDeviceKind::Keyboard),
                    Some(river_input_device_v1::Type::Pointer) => Some(InputDeviceKind::Pointer),
                    Some(river_input_device_v1::Type::Touch) => Some(InputDeviceKind::Touch),
                    Some(river_input_device_v1::Type::Tablet) => Some(InputDeviceKind::Tablet),
                    None => None,
                };
                state.apply_input_config_for_device(&proxy.id(), qh);
            }
            river_input_device_v1::Event::Name { name } => {
                device.name = Some(name);
                state.apply_input_config_for_device(&proxy.id(), qh);
            }
        }
    }
}

impl Dispatch<river_xkb_keyboard_v1::RiverXkbKeyboardV1, ()> for RiverBackendState {
    fn event(
        state: &mut Self,
        proxy: &river_xkb_keyboard_v1::RiverXkbKeyboardV1,
        event: river_xkb_keyboard_v1::Event,
        _: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(keyboard) = state.registry.xkb_keyboards.get_mut(&proxy.id()) else {
            return;
        };
        match event {
            river_xkb_keyboard_v1::Event::Removed => {
                state.registry.xkb_keyboards.remove(&proxy.id());
                proxy.destroy();
            }
            river_xkb_keyboard_v1::Event::InputDevice { device } => {
                keyboard.input_device_id = Some(device.id());
                state.apply_input_config_for_device(&device.id(), qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<river_xkb_keymap_v1::RiverXkbKeymapV1, ()> for RiverBackendState {
    fn event(
        state: &mut Self,
        proxy: &river_xkb_keymap_v1::RiverXkbKeymapV1,
        event: river_xkb_keymap_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            river_xkb_keymap_v1::Event::Success => {
                let Some(input_device_id) = state.transient.pending_xkb_keymaps.remove(&proxy.id()) else {
                    return;
                };
                if let Some(context) = state.transient.pending_xkb_keymap_context.remove(&proxy.id()) {
                    info!(target: "spiders_river::input", "applied xkb keymap: {context}");
                }
                for keyboard in state.registry.xkb_keyboards.values() {
                    if keyboard.input_device_id.as_ref() == Some(&input_device_id) {
                        keyboard.proxy.set_keymap(proxy);
                    }
                }
                state.transient.xkb_keymap_proxies.remove(&proxy.id());
                proxy.destroy();
            }
            river_xkb_keymap_v1::Event::Failure { error_msg } => {
                state.transient.pending_xkb_keymaps.remove(&proxy.id());
                let context = state
                    .transient
                    .pending_xkb_keymap_context
                    .remove(&proxy.id())
                    .unwrap_or_else(|| "unknown xkb keymap request".into());
                warn!(target: "spiders_river::input", "failed to create xkb keymap: {context}: {error_msg}");
                state.transient.xkb_keymap_proxies.remove(&proxy.id());
                proxy.destroy();
            }
        }
    }
}

impl Dispatch<river_libinput_device_v1::RiverLibinputDeviceV1, ()> for RiverBackendState {
    fn event(
        state: &mut Self,
        proxy: &river_libinput_device_v1::RiverLibinputDeviceV1,
        event: river_libinput_device_v1::Event,
        _: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(device) = state.registry.libinput_devices.get_mut(&proxy.id()) else {
            return;
        };
        match event {
            river_libinput_device_v1::Event::Removed => {
                state.registry.libinput_devices.remove(&proxy.id());
                proxy.destroy();
            }
            river_libinput_device_v1::Event::InputDevice { device: input_device } => {
                device.input_device_id = Some(input_device.id());
                state.apply_input_config_for_device(&input_device.id(), qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<river_libinput_result_v1::RiverLibinputResultV1, ()> for RiverBackendState {
    fn event(
        state: &mut Self,
        proxy: &river_libinput_result_v1::RiverLibinputResultV1,
        event: river_libinput_result_v1::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let context = state
            .transient
            .pending_input_results
            .remove(&proxy.id())
            .unwrap_or_else(|| "unknown input config request".into());

        match event {
            river_libinput_result_v1::Event::Success => {
                info!(target: "spiders_river::input", "applied input config: {context}");
            }
            river_libinput_result_v1::Event::Unsupported => {
                warn!(target: "spiders_river::input", "unsupported input config: {context}");
            }
            river_libinput_result_v1::Event::Invalid => {
                warn!(target: "spiders_river::input", "invalid input config: {context}");
            }
        }
    }
}
