use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    input::{
        keyboard::{FilterResult, Keysym, keysyms as xkb},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::Resource,
    utils::SERIAL_COUNTER,
};

use crate::{runtime::SpidersWm2, state::WorkspaceId, wm};

enum KeyboardAction {
    SwitchWorkspace(WorkspaceId),
}

fn keyboard_action_for_keysym(keysym: Keysym) -> Option<KeyboardAction> {
    if keysym == Keysym::from(xkb::KEY_1) {
        Some(KeyboardAction::SwitchWorkspace(WorkspaceId(1)))
    } else if keysym == Keysym::from(xkb::KEY_2) {
        Some(KeyboardAction::SwitchWorkspace(WorkspaceId(2)))
    } else if keysym == Keysym::from(xkb::KEY_3) {
        Some(KeyboardAction::SwitchWorkspace(WorkspaceId(3)))
    } else if keysym == Keysym::from(xkb::KEY_4) {
        Some(KeyboardAction::SwitchWorkspace(WorkspaceId(4)))
    } else if keysym == Keysym::from(xkb::KEY_5) {
        Some(KeyboardAction::SwitchWorkspace(WorkspaceId(5)))
    } else if keysym == Keysym::from(xkb::KEY_6) {
        Some(KeyboardAction::SwitchWorkspace(WorkspaceId(6)))
    } else if keysym == Keysym::from(xkb::KEY_7) {
        Some(KeyboardAction::SwitchWorkspace(WorkspaceId(7)))
    } else if keysym == Keysym::from(xkb::KEY_8) {
        Some(KeyboardAction::SwitchWorkspace(WorkspaceId(8)))
    } else if keysym == Keysym::from(xkb::KEY_9) {
        Some(KeyboardAction::SwitchWorkspace(WorkspaceId(9)))
    } else {
        None
    }
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
                        |_, _, handle| {
                            if let KeyState::Pressed = key_state {
                                if let Some(action) =
                                    keyboard_action_for_keysym(handle.modified_sym())
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
                    }
                }
            }

            InputEvent::PointerMotion { .. } => {}

            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.runtime.smithay.space.outputs().next().unwrap();
                let output_geo = self.runtime.smithay.space.output_geometry(output).unwrap();
                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

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

                    if let Some(surface) = focused_surface.clone() {
                        if let Some(window_id) = self.app.bindings.window_for_surface(&surface.id())
                        {
                            wm::focus_window(&mut self.app.wm, window_id);
                        }
                    }

                    self.focus_window_surface(focused_surface, serial);
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
