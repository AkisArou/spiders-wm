use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    input::{
        keyboard::{keysyms as xkb, FilterResult, Keysym, ModifiersState},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::Resource,
    utils::SERIAL_COUNTER,
};

use crate::{actions, command::RuntimeCommand, model::WorkspaceId, runtime::SpidersWm2};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

enum KeyboardAction {
    SwitchWorkspace(WorkspaceId),
    MoveFocusedWindowToWorkspace(WorkspaceId),
    FocusNextWindow,
    FocusPreviousWindow,
    SwapFocusedWindowWithNext,
    SwapFocusedWindowWithPrevious,
    ToggleFloatingFocusedWindow,
    ToggleFullscreenFocusedWindow,
    CloseFocusedWindow,
    ReloadConfig,
}

fn keyboard_action_for_keysym(
    modifiers: &ModifiersState,
    keysym: Keysym,
) -> Option<KeyboardAction> {
    if !modifiers.alt {
        return None;
    }

    let workspace_action = if keysym == Keysym::from(xkb::KEY_1) {
        Some(WorkspaceId::from("1"))
    } else if keysym == Keysym::from(xkb::KEY_2) {
        Some(WorkspaceId::from("2"))
    } else if keysym == Keysym::from(xkb::KEY_3) {
        Some(WorkspaceId::from("3"))
    } else if keysym == Keysym::from(xkb::KEY_4) {
        Some(WorkspaceId::from("4"))
    } else if keysym == Keysym::from(xkb::KEY_5) {
        Some(WorkspaceId::from("5"))
    } else if keysym == Keysym::from(xkb::KEY_6) {
        Some(WorkspaceId::from("6"))
    } else if keysym == Keysym::from(xkb::KEY_7) {
        Some(WorkspaceId::from("7"))
    } else if keysym == Keysym::from(xkb::KEY_8) {
        Some(WorkspaceId::from("8"))
    } else if keysym == Keysym::from(xkb::KEY_9) {
        Some(WorkspaceId::from("9"))
    } else {
        None
    };

    if let Some(workspace_id) = workspace_action {
        if modifiers.shift {
            return Some(KeyboardAction::MoveFocusedWindowToWorkspace(workspace_id));
        }

        return Some(KeyboardAction::SwitchWorkspace(workspace_id));
    }

    if keysym == Keysym::from(xkb::KEY_j) {
        if modifiers.shift {
            return Some(KeyboardAction::SwapFocusedWindowWithNext);
        }

        return Some(KeyboardAction::FocusNextWindow);
    }

    if keysym == Keysym::from(xkb::KEY_k) {
        if modifiers.shift {
            return Some(KeyboardAction::SwapFocusedWindowWithPrevious);
        }

        return Some(KeyboardAction::FocusPreviousWindow);
    }

    if keysym == Keysym::from(xkb::KEY_q) {
        return Some(KeyboardAction::CloseFocusedWindow);
    }

    if modifiers.shift && keysym == Keysym::from(xkb::KEY_r) {
        return Some(KeyboardAction::ReloadConfig);
    }

    if keysym == Keysym::from(xkb::KEY_space) {
        return Some(KeyboardAction::ToggleFloatingFocusedWindow);
    }

    if keysym == Keysym::from(xkb::KEY_f) {
        return Some(KeyboardAction::ToggleFullscreenFocusedWindow);
    }

    None
}

impl SpidersWm2 {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                let key_state = event.state();

                let action = self
                    .runtime
                    .smithay
                    .seat
                    .get_keyboard()
                    .unwrap()
                    .input::<KeyboardAction, _>(
                        self,
                        event.key_code(),
                        event.state(),
                        serial,
                        time,
                        |_, modifiers, handle| {
                            if let KeyState::Pressed = key_state {
                                if let Some(action) =
                                    keyboard_action_for_keysym(modifiers, handle.modified_sym())
                                {
                                    return FilterResult::Intercept(action);
                                }
                            }

                            FilterResult::Forward
                        },
                    );

                if let Some(action) = action {
                    match action {
                        KeyboardAction::SwitchWorkspace(workspace_id) => {
                            self.switch_workspace(workspace_id);
                        }
                        KeyboardAction::MoveFocusedWindowToWorkspace(workspace_id) => {
                            self.move_focused_window_to_workspace(workspace_id);
                        }
                        KeyboardAction::FocusNextWindow => {
                            self.focus_next_window();
                        }
                        KeyboardAction::FocusPreviousWindow => {
                            self.focus_previous_window();
                        }
                        KeyboardAction::SwapFocusedWindowWithNext => {
                            self.swap_focused_window_with_next();
                        }
                        KeyboardAction::SwapFocusedWindowWithPrevious => {
                            self.swap_focused_window_with_previous();
                        }
                        KeyboardAction::ToggleFloatingFocusedWindow => {
                            self.toggle_floating_focused_window();
                        }
                        KeyboardAction::ToggleFullscreenFocusedWindow => {
                            self.toggle_fullscreen_focused_window();
                        }
                        KeyboardAction::CloseFocusedWindow => {
                            self.close_focused_window();
                        }
                        KeyboardAction::ReloadConfig => {
                            self.handle_runtime_command(RuntimeCommand::ReloadConfig);
                        }
                    }
                }
            }
            InputEvent::PointerMotion { .. } => {}
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.runtime.smithay.space.outputs().next().unwrap();
                let output_geo = self.runtime.smithay.space.output_geometry(output).unwrap();
                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                self.update_floating_drag(pos);
                self.update_floating_resize(pos);

                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.runtime.smithay.seat.get_pointer().unwrap();
                let under = self.surface_under(pos);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );

                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.runtime.smithay.seat.get_pointer().unwrap();
                let keyboard = self.runtime.smithay.seat.get_keyboard().unwrap();
                let serial = SERIAL_COUNTER.next_serial();
                let button = event.button_code();
                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    let focused_surface = self
                        .runtime
                        .smithay
                        .space
                        .element_under(pointer.current_location())
                        .and_then(|(window, _location)| {
                            window
                                .toplevel()
                                .map(|toplevel| toplevel.wl_surface().clone())
                        });

                    let mut focus_changed = false;

                    if let Some(surface) = focused_surface.clone() {
                        if let Some(window_id) = self.app.bindings.window_for_surface(&surface.id())
                        {
                            focus_changed = actions::focus_window(&mut self.app.wm, window_id);
                        }
                    }

                    if focus_changed {
                        self.refresh_active_workspace();
                    } else {
                        self.focus_window_surface(focused_surface.clone(), serial);
                    }

                    let modifiers = keyboard.modifier_state();

                    if modifiers.alt && button == BTN_LEFT {
                        if let Some(surface) = focused_surface.clone() {
                            self.begin_floating_drag(surface, pointer.current_location());
                        }
                    }

                    if modifiers.alt && button == BTN_RIGHT {
                        if let Some(surface) = focused_surface {
                            self.begin_floating_resize(surface, pointer.current_location());
                        }
                    }
                }

                if ButtonState::Released == button_state && button == BTN_LEFT {
                    self.end_floating_drag();
                }

                if ButtonState::Released == button_state && button == BTN_RIGHT {
                    self.end_floating_resize();
                }

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time_msec(),
                    },
                );

                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0
                });

                let vertical_amount = event.amount(Axis::Vertical).unwrap_or_else(|| {
                    event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0
                });

                let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);

                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);

                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }

                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);

                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }

                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.runtime.smithay.seat.get_pointer().unwrap();

                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}
