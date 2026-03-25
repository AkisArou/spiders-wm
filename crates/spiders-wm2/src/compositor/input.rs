use smithay::backend::input::{
    AbsolutePositionEvent, Event, InputBackend, InputEvent, KeyState, KeyboardKeyEvent,
    PointerAxisEvent, PointerButtonEvent,
};
use smithay::backend::input::{Axis, AxisSource, ButtonState};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState, keysyms as xkb};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::SERIAL_COUNTER;
use tracing::{debug, info};

use crate::state::SpidersWm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    None,
    SpawnFoot,
    CloseFocusedWindow,
}

impl SpidersWm {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                let state = event.state();
                let keycode = event.key_code();
                let keyboard = self.seat.get_keyboard().expect("keyboard missing");

                let action = keyboard
                    .input(
                        self,
                        keycode,
                        state,
                        serial,
                        time,
                        |_, modifiers, handle| {
                            let keysym = handle.modified_sym();
                            debug!(
                                ?state,
                                ?modifiers,
                                ?keycode,
                                raw_keysym = keysym.raw(),
                                "keyboard event"
                            );

                            if state == KeyState::Pressed {
                                shortcut_action(*modifiers, keysym)
                                    .map(FilterResult::Intercept)
                                    .unwrap_or(FilterResult::Forward)
                            } else {
                                FilterResult::Forward
                            }
                        },
                    )
                    .unwrap_or(KeyAction::None);

                match action {
                    KeyAction::None => {}
                    KeyAction::SpawnFoot => {
                        info!("Alt+Enter matched; spawning terminal");
                        self.spawn_foot()
                    }
                    KeyAction::CloseFocusedWindow => self.close_focused_window(),
                }
            }
            InputEvent::PointerMotion { .. } => {}
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().expect("output missing");
                let output_geo = self
                    .space
                    .output_geometry(output)
                    .expect("output geometry missing");
                let location =
                    event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.seat.get_pointer().expect("pointer missing");
                let under = self.surface_under(location);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.seat.get_pointer().expect("pointer missing");
                let keyboard = self.seat.get_keyboard().expect("keyboard missing");

                if event.state() == ButtonState::Pressed && !pointer.is_grabbed() {
                    if let Some((window, _)) = self.space.element_under(pointer.current_location())
                    {
                        let window = window.clone();
                        let surface = window
                            .toplevel()
                            .expect("window missing toplevel")
                            .wl_surface()
                            .clone();
                        self.space.raise_element(&window, true);
                        keyboard.set_focus(self, Some(surface.clone()), serial);
                        self.focused_surface = Some(surface);
                    } else {
                        keyboard.set_focus(self, Option::<WlSurface>::None, serial);
                        self.focused_surface = None;
                    }
                }

                pointer.button(
                    self,
                    &ButtonEvent {
                        button: event.button_code(),
                        state: event.state(),
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0
                });
                let vertical_amount = event.amount(Axis::Vertical).unwrap_or_else(|| {
                    event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0
                });

                let mut frame = AxisFrame::new(event.time_msec()).source(event.source());
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                }

                if event.source() == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().expect("pointer missing");
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}

fn shortcut_action(modifiers: ModifiersState, keysym: Keysym) -> Option<KeyAction> {
    if modifiers.alt && matches!(keysym.raw(), xkb::KEY_Return | xkb::KEY_KP_Enter) {
        Some(KeyAction::SpawnFoot)
    } else if modifiers.alt && (keysym == Keysym::q || keysym.raw() == xkb::KEY_q) {
        Some(KeyAction::CloseFocusedWindow)
    } else {
        None
    }
}