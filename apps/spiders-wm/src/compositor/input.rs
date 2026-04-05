use smithay::backend::input::{
    AbsolutePositionEvent, Event, InputBackend, InputEvent, KeyState, KeyboardKeyEvent,
    PointerAxisEvent, PointerButtonEvent,
};
use smithay::backend::input::{Axis, AxisSource, ButtonState};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState, keysyms as xkb};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::SERIAL_COUNTER;
use spiders_config::model::Binding;
use spiders_core::signal::WmSignal;
use tracing::debug;

use crate::runtime::WmCommand;
use crate::state::SpidersWm;

impl SpidersWm {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                let state = event.state();
                let keycode = event.key_code();
                let keyboard = self.seat.get_keyboard().expect("keyboard missing");
                let bindings = self.config.bindings.clone();

                let command = keyboard
                    .input(self, keycode, state, serial, time, |_, modifiers, handle| {
                        let keysym = handle.modified_sym();
                        debug!(
                            ?state,
                            ?modifiers,
                            ?keycode,
                            raw_keysym = keysym.raw(),
                            "keyboard event"
                        );

                        if state == KeyState::Pressed {
                            binding_command(&bindings, *modifiers, keysym)
                                .or_else(|| {
                                    if bindings.is_empty() {
                                        bootstrap_shortcut_command(*modifiers, keysym)
                                    } else {
                                        None
                                    }
                                })
                                .map(Some)
                                .map(FilterResult::Intercept)
                                .unwrap_or(FilterResult::Forward)
                        } else {
                            FilterResult::Forward
                        }
                    })
                    .unwrap_or(None);

                if let Some(command) = command {
                    self.execute_wm_command_with_serial(command, serial);
                }
            }
            InputEvent::PointerMotion { .. } => {}
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output_geo = self.current_output_geometry().expect("output geometry missing");
                let location =
                    event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.seat.get_pointer().expect("pointer missing");
                let hovered_window_id = self.window_id_under(location);
                let events = {
                    let mut runtime = self.runtime();
                    runtime.handle_signal(
                        &mut crate::runtime::NoopHost,
                        WmSignal::HoveredWindowChanged {
                            seat_id: "winit".into(),
                            hovered_window_id,
                        },
                    )
                };
                self.broadcast_runtime_events(events);
                let under = self.surface_under(location);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent { location, serial, time: event.time_msec() },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.seat.get_pointer().expect("pointer missing");

                if event.state() == ButtonState::Pressed && !pointer.is_grabbed() {
                    if event.button_code() == 0x110
                        && let Some((window_id, command)) =
                            self.titlebar_action_at(pointer.current_location())
                    {
                        if let Some(surface) = self.surface_for_window_id(window_id.clone()) {
                            self.set_focus(Some(surface), serial);
                        }
                        self.execute_wm_command_with_serial(command, serial);
                        return;
                    }

                    let interacted_window_id = self.window_id_under(pointer.current_location());
                    let events = {
                        let mut runtime = self.runtime();
                        runtime.handle_signal(
                            &mut crate::runtime::NoopHost,
                            WmSignal::InteractedWindowChanged {
                                seat_id: "winit".into(),
                                interacted_window_id,
                            },
                        )
                    };
                    self.broadcast_runtime_events(events);
                    if let Some(window) = self.window_under(pointer.current_location()) {
                        let surface = window
                            .toplevel()
                            .expect("window missing toplevel")
                            .wl_surface()
                            .clone();
                        self.raise_window_element(&window);
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

fn binding_command(
    bindings: &[Binding],
    modifiers: ModifiersState,
    keysym: Keysym,
) -> Option<WmCommand> {
    bindings
        .iter()
        .find(|binding| binding_matches(&binding.trigger, modifiers, keysym))
        .map(|binding| binding.command.clone())
}

fn binding_matches(trigger: &str, modifiers: ModifiersState, keysym: Keysym) -> bool {
    let Some((binding_modifiers, binding_key)) = parse_trigger(trigger) else {
        return false;
    };

    let Some(keysym_key) = normalize_key_token(keysym) else {
        return false;
    };

    binding_modifiers == modifier_tokens(modifiers) && binding_key == keysym_key
}

fn parse_trigger(trigger: &str) -> Option<(Vec<String>, String)> {
    let mut parts = trigger
        .split('+')
        .filter(|part| !part.is_empty())
        .map(normalize_trigger_token)
        .collect::<Vec<_>>();
    let key = parts.pop()?;
    Some((parts, key))
}

fn normalize_trigger_token(token: &str) -> String {
    let trimmed = token.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "control" | "ctrl" => "ctrl".to_string(),
        "mod4" | "logo" | "super" => "super".to_string(),
        other if other.len() == 1 => other.to_string(),
        other => other.to_string(),
    }
}

fn modifier_tokens(modifiers: ModifiersState) -> Vec<String> {
    let mut tokens = Vec::new();
    if modifiers.ctrl {
        tokens.push("ctrl".to_string());
    }
    if modifiers.alt {
        tokens.push("alt".to_string());
    }
    if modifiers.shift {
        tokens.push("shift".to_string());
    }
    if modifiers.logo {
        tokens.push("super".to_string());
    }
    tokens
}

fn normalize_key_token(keysym: Keysym) -> Option<String> {
    let raw = keysym.raw();
    match raw {
        xkb::KEY_Return | xkb::KEY_KP_Enter => Some("return".to_string()),
        xkb::KEY_Tab | xkb::KEY_ISO_Left_Tab => Some("tab".to_string()),
        xkb::KEY_Left => Some("left".to_string()),
        xkb::KEY_Right => Some("right".to_string()),
        xkb::KEY_Up => Some("up".to_string()),
        xkb::KEY_Down => Some("down".to_string()),
        xkb::KEY_space => Some("space".to_string()),
        xkb::KEY_comma => Some("comma".to_string()),
        xkb::KEY_period => Some("period".to_string()),
        xkb::KEY_0..=xkb::KEY_9 => char::from_u32(raw).map(|c| c.to_string()),
        xkb::KEY_a..=xkb::KEY_z | xkb::KEY_A..=xkb::KEY_Z => {
            char::from_u32(raw).map(|c| c.to_ascii_lowercase().to_string())
        }
        _ => None,
    }
}

fn bootstrap_shortcut_command(modifiers: ModifiersState, keysym: Keysym) -> Option<WmCommand> {
    if modifiers.alt && matches!(keysym.raw(), xkb::KEY_Return | xkb::KEY_KP_Enter) {
        Some(WmCommand::SpawnTerminal)
    } else if modifiers.alt
        && modifiers.shift
        && matches!(keysym.raw(), xkb::KEY_Tab | xkb::KEY_ISO_Left_Tab)
    {
        Some(WmCommand::FocusPreviousWindow)
    } else if modifiers.alt && keysym.raw() == xkb::KEY_Tab {
        Some(WmCommand::FocusNextWindow)
    } else if modifiers.alt && modifiers.shift && matches!(keysym.raw(), xkb::KEY_W | xkb::KEY_w) {
        Some(WmCommand::SelectPreviousWorkspace)
    } else if modifiers.alt && (keysym == Keysym::w || keysym.raw() == xkb::KEY_w) {
        Some(WmCommand::SelectNextWorkspace)
    } else if modifiers.alt && matches!(keysym.raw(), xkb::KEY_1..=xkb::KEY_9) {
        Some(WmCommand::SelectWorkspace {
            workspace_id: (keysym.raw() - xkb::KEY_0).to_string().into(),
        })
    } else if modifiers.alt && (keysym == Keysym::q || keysym.raw() == xkb::KEY_q) {
        Some(WmCommand::CloseFocusedWindow)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_config::model::Binding;

    #[test]
    fn binding_command_matches_authored_binding() {
        let bindings =
            vec![Binding { trigger: "alt+Return".to_string(), command: WmCommand::SpawnTerminal }];
        let modifiers = ModifiersState { alt: true, ..ModifiersState::default() };

        assert_eq!(
            binding_command(&bindings, modifiers, Keysym::Return),
            Some(WmCommand::SpawnTerminal)
        );
    }

    #[test]
    fn binding_command_normalizes_shifted_letter_triggers() {
        let bindings = vec![Binding {
            trigger: "alt+shift+N".to_string(),
            command: WmCommand::Spawn { command: "notes".to_string() },
        }];
        let modifiers = ModifiersState { alt: true, shift: true, ..ModifiersState::default() };

        assert_eq!(
            binding_command(&bindings, modifiers, Keysym::N),
            Some(WmCommand::Spawn { command: "notes".to_string() })
        );
    }

    #[test]
    fn binding_command_supports_super_aliases() {
        let bindings =
            vec![Binding { trigger: "mod4+Return".to_string(), command: WmCommand::SpawnTerminal }];
        let modifiers = ModifiersState { logo: true, ..ModifiersState::default() };

        assert_eq!(
            binding_command(&bindings, modifiers, Keysym::Return),
            Some(WmCommand::SpawnTerminal)
        );
    }

    #[test]
    fn bootstrap_shortcut_command_preserves_minimal_fallbacks() {
        let modifiers = ModifiersState { alt: true, shift: true, ..ModifiersState::default() };

        assert_eq!(
            bootstrap_shortcut_command(modifiers, Keysym::Tab),
            Some(WmCommand::FocusPreviousWindow)
        );
        assert_eq!(
            bootstrap_shortcut_command(modifiers, Keysym::W),
            Some(WmCommand::SelectPreviousWorkspace)
        );
    }
}
