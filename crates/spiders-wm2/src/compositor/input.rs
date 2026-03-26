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

use crate::runtime::{RuntimeCommand, WmCommand};
use crate::state::SpidersWm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    None,
    SpawnFoot,
    FocusNextWindow,
    FocusPreviousWindow,
    SelectNextWorkspace,
    SelectPreviousWorkspace,
    SelectWorkspace(u8),
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

                if let Some(command) = action.into_wm_command() {
                    if command == WmCommand::SpawnTerminal {
                        info!("Alt+Enter matched; spawning terminal");
                    }
                    self.execute_wm_command_with_serial(command, serial);
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
                let hovered_window_id = self.window_id_under(location);
                let _ = self.runtime().execute(RuntimeCommand::SyncHoveredWindow {
                    seat_id: "winit".into(),
                    hovered_window_id,
                });
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

                if event.state() == ButtonState::Pressed && !pointer.is_grabbed() {
                    let interacted_window_id = self.window_id_under(pointer.current_location());
                    let _ = self.runtime().execute(RuntimeCommand::SyncInteractedWindow {
                        seat_id: "winit".into(),
                        interacted_window_id,
                    });
                    if let Some((window, _)) = self.space.element_under(pointer.current_location())
                    {
                        let window = window.clone();
                        let surface = window
                            .toplevel()
                            .expect("window missing toplevel")
                            .wl_surface()
                            .clone();
                        self.space.raise_element(&window, true);
                        self.set_focus(Some(surface), serial);
                    } else {
                        self.set_focus(Option::<WlSurface>::None, serial);
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

impl KeyAction {
    fn into_wm_command(self) -> Option<WmCommand> {
        match self {
            KeyAction::None => None,
            KeyAction::SpawnFoot => Some(WmCommand::SpawnTerminal),
            KeyAction::FocusNextWindow => Some(WmCommand::FocusNextWindow),
            KeyAction::FocusPreviousWindow => Some(WmCommand::FocusPreviousWindow),
            KeyAction::SelectNextWorkspace => Some(WmCommand::SelectNextWorkspace),
            KeyAction::SelectPreviousWorkspace => Some(WmCommand::SelectPreviousWorkspace),
            KeyAction::SelectWorkspace(index) => {
                Some(WmCommand::SelectWorkspaceNamed(index.to_string()))
            }
            KeyAction::CloseFocusedWindow => Some(WmCommand::CloseFocusedWindow),
        }
    }
}

fn shortcut_action(modifiers: ModifiersState, keysym: Keysym) -> Option<KeyAction> {
    if modifiers.alt && matches!(keysym.raw(), xkb::KEY_Return | xkb::KEY_KP_Enter) {
        Some(KeyAction::SpawnFoot)
    } else if modifiers.alt
        && modifiers.shift
        && matches!(keysym.raw(), xkb::KEY_Tab | xkb::KEY_ISO_Left_Tab)
    {
        Some(KeyAction::FocusPreviousWindow)
    } else if modifiers.alt && keysym.raw() == xkb::KEY_Tab {
        Some(KeyAction::FocusNextWindow)
    } else if modifiers.alt && modifiers.shift && matches!(keysym.raw(), xkb::KEY_W | xkb::KEY_w) {
        Some(KeyAction::SelectPreviousWorkspace)
    } else if modifiers.alt && (keysym == Keysym::w || keysym.raw() == xkb::KEY_w) {
        Some(KeyAction::SelectNextWorkspace)
    } else if modifiers.alt && matches!(keysym.raw(), xkb::KEY_1..=xkb::KEY_9) {
        Some(KeyAction::SelectWorkspace((keysym.raw() - xkb::KEY_0) as u8))
    } else if modifiers.alt && (keysym == Keysym::q || keysym.raw() == xkb::KEY_q) {
        Some(KeyAction::CloseFocusedWindow)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_action_maps_previous_traversal_keys() {
        let modifiers = ModifiersState {
            alt: true,
            shift: true,
            ..ModifiersState::default()
        };

        assert_eq!(shortcut_action(modifiers, Keysym::Tab), Some(KeyAction::FocusPreviousWindow));
        assert_eq!(shortcut_action(modifiers, Keysym::W), Some(KeyAction::SelectPreviousWorkspace));
    }

    #[test]
    fn key_actions_translate_to_wm_commands() {
        assert_eq!(KeyAction::SpawnFoot.into_wm_command(), Some(WmCommand::SpawnTerminal));
        assert_eq!(KeyAction::FocusNextWindow.into_wm_command(), Some(WmCommand::FocusNextWindow));
        assert_eq!(
            KeyAction::SelectWorkspace(4).into_wm_command(),
            Some(WmCommand::SelectWorkspaceNamed("4".to_string()))
        );
    }
}