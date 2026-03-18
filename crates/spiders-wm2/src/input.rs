// use smithay::{
//     backend::input::{
//         AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
//         KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
//     },
//     input::{
//         keyboard::FilterResult,
//         pointer::{AxisFrame, ButtonEvent, MotionEvent},
//     },
//     utils::SERIAL_COUNTER,
// };
//
// use crate::state::SpidersWm2;
//
// impl SpidersWm2 {
//     pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
//         match event {
//             InputEvent::Keyboard { event, .. } => {
//                 let serial = SERIAL_COUNTER.next_serial();
//                 let time = Event::time_msec(&event);
//
//                 self.seat.get_keyboard().unwrap().input::<(), _>(
//                     self,
//                     event.key_code(),
//                     event.state(),
//                     serial,
//                     time,
//                     |_, _, _| FilterResult::Forward,
//                 );
//             }
//             InputEvent::PointerMotion { .. } => {}
//             InputEvent::PointerMotionAbsolute { event, .. } => {
//                 let output = self.space.outputs().next().unwrap();
//                 let output_geo = self.space.output_geometry(output).unwrap();
//                 let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();
//
//                 let serial = SERIAL_COUNTER.next_serial();
//                 let pointer = self.seat.get_pointer().unwrap();
//                 let under = self.surface_under(pos);
//
//                 pointer.motion(
//                     self,
//                     under,
//                     &MotionEvent {
//                         location: pos,
//                         serial,
//                         time: event.time_msec(),
//                     },
//                 );
//                 pointer.frame(self);
//             }
//             InputEvent::PointerButton { event, .. } => {
//                 let pointer = self.seat.get_pointer().unwrap();
//                 let serial = SERIAL_COUNTER.next_serial();
//                 let button = event.button_code();
//                 let button_state = event.state();
//
//                 if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
//                     let focused = self
//                         .space
//                         .element_under(pointer.current_location())
//                         .map(|(window, _location)| window.clone());
//
//                     self.focus_window(focused, serial);
//                 }
//
//                 pointer.button(
//                     self,
//                     &ButtonEvent {
//                         button,
//                         state: button_state,
//                         serial,
//                         time: event.time_msec(),
//                     },
//                 );
//                 pointer.frame(self);
//             }
//             InputEvent::PointerAxis { event, .. } => {
//                 let source = event.source();
//
//                 let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
//                     event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0
//                 });
//                 let vertical_amount = event.amount(Axis::Vertical).unwrap_or_else(|| {
//                     event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0
//                 });
//                 let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
//                 let vertical_amount_discrete = event.amount_v120(Axis::Vertical);
//
//                 let mut frame = AxisFrame::new(event.time_msec()).source(source);
//                 if horizontal_amount != 0.0 {
//                     frame = frame.value(Axis::Horizontal, horizontal_amount);
//                     if let Some(discrete) = horizontal_amount_discrete {
//                         frame = frame.v120(Axis::Horizontal, discrete as i32);
//                     }
//                 }
//                 if vertical_amount != 0.0 {
//                     frame = frame.value(Axis::Vertical, vertical_amount);
//                     if let Some(discrete) = vertical_amount_discrete {
//                         frame = frame.v120(Axis::Vertical, discrete as i32);
//                     }
//                 }
//
//                 if source == AxisSource::Finger {
//                     if event.amount(Axis::Horizontal) == Some(0.0) {
//                         frame = frame.stop(Axis::Horizontal);
//                     }
//                     if event.amount(Axis::Vertical) == Some(0.0) {
//                         frame = frame.stop(Axis::Vertical);
//                     }
//                 }
//
//                 let pointer = self.seat.get_pointer().unwrap();
//                 pointer.axis(self, frame);
//                 pointer.frame(self);
//             }
//             _ => {}
//         }
//     }
// }
