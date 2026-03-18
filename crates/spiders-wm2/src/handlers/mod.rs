mod compositor;
mod xdg_shell;

use smithay::{
    delegate_data_device, delegate_output, delegate_seat,
    input::{
        SeatHandler, SeatState,
        dnd::{DnDGrab, DndGrabHandler, GrabType},
        pointer::Focus,
    },
    reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface},
    wayland::{
        output::OutputHandler,
        selection::{
            SelectionHandler,
            data_device::{
                DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
            },
        },
    },
};

use crate::runtime::SpidersWm2;

impl SeatHandler for SpidersWm2 {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.runtime.smithay.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &smithay::input::Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }

    fn focus_changed(
        &mut self,
        seat: &smithay::input::Seat<Self>,
        focused: Option<&Self::KeyboardFocus>,
    ) {
        let client =
            focused.and_then(|surface| self.runtime.display_handle.get_client(surface.id()).ok());

        set_data_device_focus(&self.runtime.display_handle, seat, client);
    }
}

delegate_seat!(SpidersWm2);

impl SelectionHandler for SpidersWm2 {
    type SelectionUserData = ();
}

impl DataDeviceHandler for SpidersWm2 {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.runtime.smithay.data_device_state
    }
}

impl DndGrabHandler for SpidersWm2 {}

impl WaylandDndGrabHandler for SpidersWm2 {
    fn dnd_requested<S: smithay::input::dnd::Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        seat: smithay::input::Seat<Self>,
        serial: smithay::utils::Serial,
        type_: smithay::input::dnd::GrabType,
    ) {
        match type_ {
            GrabType::Pointer => {
                let pointer = seat.get_pointer().unwrap();
                let start_data = pointer.grab_start_data().unwrap();
                let grab =
                    DnDGrab::new_pointer(&self.runtime.display_handle, start_data, source, seat);
                pointer.set_grab(self, grab, serial, Focus::Keep);
            }
            GrabType::Touch => {
                source.cancel();
            }
        }
    }
}

delegate_data_device!(SpidersWm2);

impl OutputHandler for SpidersWm2 {}

delegate_output!(SpidersWm2);

// mod compositor;
// mod xdg_shell;
//
// use smithay::{
//     delegate_data_device, delegate_output, delegate_seat,
//     input::{
//         dnd::{DnDGrab, DndGrabHandler, GrabType, Source},
//         pointer::Focus,
//         Seat, SeatHandler, SeatState,
//     },
//     reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
//     utils::Serial,
//     wayland::{
//         output::OutputHandler,
//         selection::{
//             data_device::{
//                 set_data_device_focus, DataDeviceHandler, DataDeviceState,
//                 WaylandDndGrabHandler,
//             },
//             SelectionHandler,
//         },
//     },
// };
//
// use crate::state::SpidersWm2;
//
// impl SeatHandler for SpidersWm2 {
//     type KeyboardFocus = WlSurface;
//     type PointerFocus = WlSurface;
//     type TouchFocus = WlSurface;
//
//     fn seat_state(&mut self) -> &mut SeatState<Self> {
//         &mut self.seat_state
//     }
//
//     fn cursor_image(&mut self, _seat: &Seat<Self>, _image: smithay::input::pointer::CursorImageStatus) {}
//
//     fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
//         let client = focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
//         set_data_device_focus(&self.display_handle, seat, client);
//     }
// }
//
// delegate_seat!(SpidersWm2);
//
// impl SelectionHandler for SpidersWm2 {
//     type SelectionUserData = ();
// }
//
// impl DataDeviceHandler for SpidersWm2 {
//     fn data_device_state(&mut self) -> &mut DataDeviceState {
//         &mut self.data_device_state
//     }
// }
//
// impl DndGrabHandler for SpidersWm2 {}
//
// impl WaylandDndGrabHandler for SpidersWm2 {
//     fn dnd_requested<S: Source>(
//         &mut self,
//         source: S,
//         _icon: Option<WlSurface>,
//         seat: Seat<Self>,
//         serial: Serial,
//         type_: GrabType,
//     ) {
//         match type_ {
//             GrabType::Pointer => {
//                 let pointer = seat.get_pointer().unwrap();
//                 let start_data = pointer.grab_start_data().unwrap();
//                 let grab = DnDGrab::new_pointer(&self.display_handle, start_data, source, seat);
//                 pointer.set_grab(self, grab, serial, Focus::Keep);
//             }
//             GrabType::Touch => {
//                 source.cancel();
//             }
//         }
//     }
// }
//
// delegate_data_device!(SpidersWm2);
//
// impl OutputHandler for SpidersWm2 {}
//
// delegate_output!(SpidersWm2);
