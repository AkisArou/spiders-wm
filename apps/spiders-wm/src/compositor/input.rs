use crate::runtime::WmCommand;
use crate::state::SpidersWm;
use smithay::backend::input::{
    AbsolutePositionEvent, Event, InputBackend, InputEvent, KeyState, KeyboardKeyEvent, Keycode,
    PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
};
use smithay::backend::input::{Axis, AxisSource, ButtonState};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState, keysyms as xkb};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::SERIAL_COUNTER;
use smithay::wayland::pointer_constraints::{PointerConstraint, with_pointer_constraint};
use spiders_config::model::Binding;
use spiders_core::signal::WmSignal;

enum PointerFocusTarget<Layer, Window> {
    Layer(Layer),
    Window(Window),
    Clear,
}

impl SpidersWm {
    pub(crate) fn refresh_pointer_focus_and_constraints(&mut self) {
        let pointer = self.seat.get_pointer().expect("pointer missing");
        let location = self.pointer_location;
        let serial = SERIAL_COUNTER.next_serial();
        let time = self.start_time.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
        let under = self.surface_under(location);

        self.debug_protocol_event("pointer-refresh-under", None, || {
            format!("location={location:?} under_present={}", under.is_some())
        });

        pointer.motion(self, under.clone(), &MotionEvent { location, serial, time });
        pointer.frame(self);

        if let Some((surface, _)) = under {
            self.maybe_activate_pointer_constraint(&surface, &pointer);
        }
    }

    fn clamp_pointer_to_outputs(
        &self,
        location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) -> smithay::utils::Point<f64, smithay::utils::Logical> {
        let Some(bounds) = self.output_union_geometry() else {
            return location;
        };
        let min_x = bounds.loc.x as f64;
        let min_y = bounds.loc.y as f64;
        let max_x = (bounds.loc.x + bounds.size.w.saturating_sub(1)) as f64;
        let max_y = (bounds.loc.y + bounds.size.h.saturating_sub(1)) as f64;

        (location.x.clamp(min_x, max_x), location.y.clamp(min_y, max_y)).into()
    }

    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                self.handle_keyboard_key(event.key_code(), event.state(), serial, time);
            }
            InputEvent::PointerMotion { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                self.handle_pointer_motion(
                    serial,
                    event.time_msec(),
                    event.delta(),
                    event.delta_unaccel(),
                    |state, pointer| {
                        state.clamp_pointer_to_outputs(pointer.current_location() + event.delta())
                    },
                );
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let location = event.position_transformed(
                    self.current_output_geometry()
                        .map(|geometry| geometry.size)
                        .unwrap_or_default(),
                );
                let delta = location - self.pointer_location;
                self.handle_pointer_motion(serial, event.time_msec(), delta, delta, |state, _| {
                    state.clamp_pointer_to_outputs(location)
                });
            }
            InputEvent::PointerButton { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.seat.get_pointer().expect("pointer missing");

                if event.state() == ButtonState::Pressed && !pointer.is_grabbed() {
                    let interacted_window_id = self.window_id_under(pointer.current_location());
                    let seat_id = self.active_backend_seat_name().to_string();
                    let events = {
                        let mut runtime = self.runtime();
                        runtime.handle_signal(
                            &mut crate::runtime::NoopHost,
                            WmSignal::InteractedWindowChanged {
                                seat_id: seat_id.into(),
                                interacted_window_id,
                            },
                        )
                    };
                    self.broadcast_runtime_events(events);
                    match pointer_focus_target(
                        self.layer_surface_under(pointer.current_location())
                            .map(|(surface, _)| surface),
                        self.window_under(pointer.current_location()),
                    ) {
                        PointerFocusTarget::Layer(surface) => {
                            self.set_focus(Some(surface), serial);
                        }
                        PointerFocusTarget::Window(window) => {
                            let surface = window
                                .toplevel()
                                .expect("window missing toplevel")
                                .wl_surface()
                                .clone();
                            let debug_window_id = self
                                .window_id_for_surface(&surface)
                                .as_ref()
                                .map(ToString::to_string);
                            self.debug_protocol_event(
                                "pointer-focus-request",
                                debug_window_id.as_deref(),
                                || format!("location={:?}", pointer.current_location()),
                            );
                            self.raise_window_element(&window);
                            self.set_focus(Some(surface), serial);
                        }
                        PointerFocusTarget::Clear => {
                            self.debug_protocol_event("pointer-focus-clear", None, || {
                                format!("location={:?}", pointer.current_location())
                            });
                            self.set_focus(Option::<WlSurface>::None, serial);
                        }
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

    pub(crate) fn handle_keyboard_key(
        &mut self,
        keycode: Keycode,
        state: KeyState,
        serial: smithay::utils::Serial,
        time: u32,
    ) {
        let keyboard = self.seat.get_keyboard().expect("keyboard missing");
        let bindings = self.config.bindings.clone();

        let command = keyboard
            .input(self, keycode, state, serial, time, |_, modifiers, handle| {
                let keysym = handle.modified_sym();
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

    fn handle_pointer_motion<F>(
        &mut self,
        serial: smithay::utils::Serial,
        time_msec: u32,
        delta: smithay::utils::Point<f64, smithay::utils::Logical>,
        delta_unaccel: smithay::utils::Point<f64, smithay::utils::Logical>,
        resolve_location: F,
    ) where
        F: FnOnce(
            &Self,
            &smithay::input::pointer::PointerHandle<Self>,
        ) -> smithay::utils::Point<f64, smithay::utils::Logical>,
    {
        let pointer = self.seat.get_pointer().expect("pointer missing");
        let current_location = pointer.current_location();
        let unconstrained_location = resolve_location(self, &pointer);
        let under_before_motion = self.surface_under(current_location);
        let constraint_state = under_before_motion.as_ref().and_then(|(surface, origin)| {
            pointer_constraint_state_for_surface(self, &pointer, surface, *origin, current_location)
        });

        if matches!(constraint_state, Some(ActivePointerConstraint::Locked)) {
            if let Some((surface, origin)) = under_before_motion {
                pointer.relative_motion(
                    self,
                    Some((surface.clone(), origin)),
                    &relative_motion_event(delta, delta_unaccel, time_msec),
                );
                pointer.frame(self);
                self.maybe_activate_pointer_constraint(&surface, &pointer);
            }
            return;
        }

        let location = constraint_state
            .and_then(|constraint| match constraint {
                ActivePointerConstraint::Confined { surface_origin, region } => {
                    Some(confined_pointer_location(
                        unconstrained_location,
                        surface_origin,
                        region.as_ref(),
                    ))
                }
                ActivePointerConstraint::Locked => None,
            })
            .unwrap_or(unconstrained_location);
        self.pointer_location = location;
        let hovered_window_id = self.window_id_under(location);
        let seat_id = self.active_backend_seat_name().to_string();
        let events = {
            let mut runtime = self.runtime();
            runtime.handle_signal(
                &mut crate::runtime::NoopHost,
                WmSignal::HoveredWindowChanged { seat_id: seat_id.into(), hovered_window_id },
            )
        };
        self.broadcast_runtime_events(events);
        let under = self.surface_under(location);
        self.debug_protocol_event("pointer-motion-under", None, || {
            format!(
                "current_location={current_location:?} new_location={location:?} under_present={}",
                under.is_some()
            )
        });

        pointer.motion(self, under.clone(), &MotionEvent { location, serial, time: time_msec });
        pointer.relative_motion(
            self,
            under,
            &relative_motion_event(delta, delta_unaccel, time_msec),
        );
        pointer.frame(self);
        if let Some((surface, _)) = self.surface_under(location) {
            self.maybe_activate_pointer_constraint(&surface, &pointer);
        }
    }
}

enum ActivePointerConstraint {
    Locked,
    Confined {
        surface_origin: smithay::utils::Point<f64, smithay::utils::Logical>,
        region: Option<smithay::wayland::compositor::RegionAttributes>,
    },
}

fn pointer_constraint_state_for_surface(
    _state: &SpidersWm,
    pointer: &smithay::input::pointer::PointerHandle<SpidersWm>,
    surface: &WlSurface,
    surface_origin: smithay::utils::Point<f64, smithay::utils::Logical>,
    pointer_location: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> Option<ActivePointerConstraint> {
    with_pointer_constraint(surface, pointer, |constraint| {
        let constraint = constraint?;
        if !constraint.is_active() {
            return None;
        }

        if let Some(region) = constraint.region() {
            let pos_within_surface = pointer_location - surface_origin;
            if !region.contains(pos_within_surface.to_i32_round()) {
                return None;
            }
        }

        match &*constraint {
            PointerConstraint::Locked(_) => Some(ActivePointerConstraint::Locked),
            PointerConstraint::Confined(confined) => Some(ActivePointerConstraint::Confined {
                surface_origin,
                region: confined.region().cloned(),
            }),
        }
    })
}

fn confined_pointer_location(
    unconstrained_location: smithay::utils::Point<f64, smithay::utils::Logical>,
    surface_origin: smithay::utils::Point<f64, smithay::utils::Logical>,
    region: Option<&smithay::wayland::compositor::RegionAttributes>,
) -> smithay::utils::Point<f64, smithay::utils::Logical> {
    let Some(region) = region else {
        return unconstrained_location;
    };

    let relative = unconstrained_location - surface_origin;
    if region.contains(relative.to_i32_round()) {
        unconstrained_location
    } else {
        clamp_relative_to_region(surface_origin, relative, region)
    }
}

fn clamp_relative_to_region(
    surface_origin: smithay::utils::Point<f64, smithay::utils::Logical>,
    relative: smithay::utils::Point<f64, smithay::utils::Logical>,
    region: &smithay::wayland::compositor::RegionAttributes,
) -> smithay::utils::Point<f64, smithay::utils::Logical> {
    let Some(clamped_relative) = nearest_point_in_region(region, relative) else {
        return surface_origin + relative;
    };

    surface_origin + clamped_relative
}

fn nearest_point_in_region(
    region: &smithay::wayland::compositor::RegionAttributes,
    relative: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> Option<smithay::utils::Point<f64, smithay::utils::Logical>> {
    let additive_rects = region.rects.iter().filter_map(|(kind, rect)| {
        matches!(kind, smithay::wayland::compositor::RectangleKind::Add).then_some(*rect)
    });

    additive_rects
        .filter_map(|rect| nearest_point_in_rect(rect, relative))
        .filter(|candidate| region.contains(candidate.to_i32_round()))
        .min_by(|left, right| {
            squared_distance(*left, relative)
                .partial_cmp(&squared_distance(*right, relative))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn nearest_point_in_rect(
    rect: smithay::utils::Rectangle<i32, smithay::utils::Logical>,
    point: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> Option<smithay::utils::Point<f64, smithay::utils::Logical>> {
    if rect.size.w <= 0 || rect.size.h <= 0 {
        return None;
    }

    let min_x = rect.loc.x as f64;
    let min_y = rect.loc.y as f64;
    let max_x = (rect.loc.x + rect.size.w.saturating_sub(1)) as f64;
    let max_y = (rect.loc.y + rect.size.h.saturating_sub(1)) as f64;

    Some((point.x.clamp(min_x, max_x), point.y.clamp(min_y, max_y)).into())
}

fn squared_distance(
    left: smithay::utils::Point<f64, smithay::utils::Logical>,
    right: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> f64 {
    let dx = left.x - right.x;
    let dy = left.y - right.y;
    dx * dx + dy * dy
}

fn pointer_focus_target<Layer, Window>(
    layer_surface: Option<Layer>,
    window: Option<Window>,
) -> PointerFocusTarget<Layer, Window> {
    if let Some(layer_surface) = layer_surface {
        PointerFocusTarget::Layer(layer_surface)
    } else if let Some(window) = window {
        PointerFocusTarget::Window(window)
    } else {
        PointerFocusTarget::Clear
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
    if modifiers.alt
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
    use smithay::utils::{Logical, Point, Rectangle, Size};
    use spiders_config::model::Binding;

    fn clamp_pointer_to_bounds(
        bounds: Rectangle<i32, Logical>,
        location: Point<f64, Logical>,
    ) -> Point<f64, Logical> {
        let min_x = bounds.loc.x as f64;
        let min_y = bounds.loc.y as f64;
        let max_x = (bounds.loc.x + bounds.size.w.saturating_sub(1)) as f64;
        let max_y = (bounds.loc.y + bounds.size.h.saturating_sub(1)) as f64;

        (location.x.clamp(min_x, max_x), location.y.clamp(min_y, max_y)).into()
    }

    #[test]
    fn clamp_pointer_to_bounds_limits_to_union_edges() {
        let bounds = Rectangle::new(Point::from((100, 20)), Size::from((300, 200)));

        assert_eq!(
            clamp_pointer_to_bounds(bounds, Point::from((50.0, 500.0))),
            Point::from((100.0, 219.0)),
        );
        assert_eq!(
            clamp_pointer_to_bounds(bounds, Point::from((450.0, 10.0))),
            Point::from((399.0, 20.0)),
        );
    }

    #[test]
    fn pointer_focus_target_prefers_layer_surface_over_window() {
        let target = pointer_focus_target(Some("layer"), Some("window"));

        assert!(matches!(target, PointerFocusTarget::Layer("layer")));
    }

    #[test]
    fn pointer_focus_target_falls_back_to_window() {
        let target = pointer_focus_target::<&str, _>(None, Some("window"));

        assert!(matches!(target, PointerFocusTarget::Window("window")));
    }

    #[test]
    fn pointer_focus_target_clears_when_nothing_is_under_pointer() {
        let target = pointer_focus_target::<&str, &str>(None, None);

        assert!(matches!(target, PointerFocusTarget::Clear));
    }

    #[test]
    fn relative_motion_event_preserves_both_delta_variants() {
        let event = relative_motion_event((1.25, -0.5).into(), (2.0, -1.0).into(), 42);

        assert_eq!(event.delta, (1.25, -0.5).into());
        assert_eq!(event.delta_unaccel, (2.0, -1.0).into());
        assert_eq!(event.utime, 42);
    }

    #[test]
    fn relative_motion_event_uses_msec_timestamp_as_microsecond_field_source() {
        let event = relative_motion_event((0.0, 0.0).into(), (0.0, 0.0).into(), 7);

        assert_eq!(event.utime, 7);
    }

    #[test]
    fn absolute_motion_delta_uses_previous_pointer_location() {
        let previous = Point::<f64, Logical>::from((10.0, 15.0));
        let next = Point::<f64, Logical>::from((18.0, 9.0));

        assert_eq!(next - previous, Point::from((8.0, -6.0)));
    }

    #[test]
    fn confined_pointer_location_clamps_to_added_region_bounds() {
        let region = smithay::wayland::compositor::RegionAttributes {
            rects: vec![(
                smithay::wayland::compositor::RectangleKind::Add,
                Rectangle::new((10, 20).into(), (30, 40).into()),
            )],
        };

        let location =
            confined_pointer_location((100.0, 100.0).into(), (0.0, 0.0).into(), Some(&region));

        assert_eq!(location, (39.0, 59.0).into());
    }

    #[test]
    fn confined_pointer_location_respects_subtracted_hole() {
        let region = smithay::wayland::compositor::RegionAttributes {
            rects: vec![
                (
                    smithay::wayland::compositor::RectangleKind::Add,
                    Rectangle::new((0, 0).into(), (100, 100).into()),
                ),
                (
                    smithay::wayland::compositor::RectangleKind::Subtract,
                    Rectangle::new((40, 40).into(), (20, 20).into()),
                ),
            ],
        };

        assert!(!region.contains((50, 50)));
    }

    #[test]
    fn nearest_point_in_region_returns_none_when_region_has_no_additive_rects() {
        let region = smithay::wayland::compositor::RegionAttributes {
            rects: vec![(
                smithay::wayland::compositor::RectangleKind::Subtract,
                Rectangle::new((0, 0).into(), (10, 10).into()),
            )],
        };

        assert_eq!(nearest_point_in_region(&region, (5.0, 5.0).into()), None);
    }

    #[test]
    fn confined_pointer_location_passes_through_without_region() {
        let location = confined_pointer_location((25.0, 30.0).into(), (5.0, 5.0).into(), None);

        assert_eq!(location, (25.0, 30.0).into());
    }

    #[test]
    fn binding_command_matches_authored_binding() {
        let bindings = vec![Binding {
            trigger: "alt+Return".to_string(),
            command: WmCommand::Spawn { command: "foot".to_string() },
        }];
        let modifiers = ModifiersState { alt: true, ..ModifiersState::default() };

        assert_eq!(
            binding_command(&bindings, modifiers, Keysym::Return),
            Some(WmCommand::Spawn { command: "foot".to_string() })
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
        let bindings = vec![Binding {
            trigger: "mod4+Return".to_string(),
            command: WmCommand::Spawn { command: "foot".to_string() },
        }];
        let modifiers = ModifiersState { logo: true, ..ModifiersState::default() };

        assert_eq!(
            binding_command(&bindings, modifiers, Keysym::Return),
            Some(WmCommand::Spawn { command: "foot".to_string() })
        );
    }

    #[test]
    fn bootstrap_shortcut_command_preserves_minimal_fallbacks() {
        let modifiers = ModifiersState { alt: true, shift: true, ..ModifiersState::default() };

        assert_eq!(
            bootstrap_shortcut_command(
                ModifiersState { alt: true, ..ModifiersState::default() },
                Keysym::Return
            ),
            None
        );
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

fn relative_motion_event(
    delta: smithay::utils::Point<f64, smithay::utils::Logical>,
    delta_unaccel: smithay::utils::Point<f64, smithay::utils::Logical>,
    time_msec: u32,
) -> RelativeMotionEvent {
    RelativeMotionEvent { delta, delta_unaccel, utime: time_msec.into() }
}
