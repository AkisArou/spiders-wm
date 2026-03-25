mod compositor;
mod xdg_shell;

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::ImportDma;
use smithay::input::dnd::{DnDGrab, DndGrabHandler, GrabType, Source};
use smithay::input::pointer::Focus;
use smithay::input::{Seat, SeatHandler};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::selection::data_device::{WaylandDndGrabHandler, set_data_device_focus};
use smithay::{delegate_data_device, delegate_dmabuf, delegate_output, delegate_seat};

use crate::state::SpidersWm2;

impl SeatHandler for SpidersWm2 {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut smithay::input::SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
        set_data_device_focus(&self.display_handle, seat, client);
    }
}

impl DndGrabHandler for SpidersWm2 {}

impl WaylandDndGrabHandler for SpidersWm2 {
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        type_: GrabType,
    ) {
        match type_ {
            GrabType::Pointer => {
                let pointer = seat.get_pointer().expect("pointer missing");
                let start_data = pointer
                    .grab_start_data()
                    .expect("pointer grab data missing");
                let grab = DnDGrab::new_pointer(&self.display_handle, start_data, source, seat);
                pointer.set_grab(self, grab, serial, Focus::Keep);
            }
            GrabType::Touch => source.cancel(),
        }
    }
}

impl DmabufHandler for SpidersWm2 {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        let Some(backend) = self.backend.as_mut() else {
            notifier.failed();
            return;
        };

        if backend.renderer().import_dmabuf(&dmabuf, None).is_ok() {
            let _ = notifier.successful::<SpidersWm2>();
        } else {
            notifier.failed();
        }
    }
}

delegate_seat!(SpidersWm2);
delegate_data_device!(SpidersWm2);
delegate_dmabuf!(SpidersWm2);
delegate_output!(SpidersWm2);
