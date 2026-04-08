mod compositor;
mod layer_shell;
mod fractional_scale;
mod pointer_constraints;
mod relative_pointer;
mod virtual_keyboard;
mod xdg_activation;
mod xdg_decoration;
mod xdg_shell;

pub(crate) use compositor::ClientState;
pub(crate) use xdg_activation::pending_activation_is_valid;
pub(crate) use virtual_keyboard::VirtualKeyboardManagerState;

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::ImportDma;
use smithay::input::dnd::{DnDGrab, DndGrabHandler, GrabType, Source};
use smithay::input::pointer::Focus;
use smithay::input::{Seat, SeatHandler};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::DataDeviceHandler;
use smithay::wayland::selection::data_device::{WaylandDndGrabHandler, set_data_device_focus};
use smithay::{delegate_data_device, delegate_dmabuf, delegate_fractional_scale, delegate_layer_shell, delegate_output, delegate_pointer_constraints, delegate_relative_pointer, delegate_seat, delegate_xdg_activation, delegate_xdg_decoration};

use crate::backend::BackendState;
use crate::state::SpidersWm;

impl SeatHandler for SpidersWm {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut smithay::input::SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        image: smithay::input::pointer::CursorImageStatus,
    ) {
        self.cursor_image_status = image;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
        set_data_device_focus(&self.display_handle, seat, client);
    }
}

impl DndGrabHandler for SpidersWm {}

impl SelectionHandler for SpidersWm {
    type SelectionUserData = ();
}

impl DataDeviceHandler for SpidersWm {
    fn data_device_state(
        &mut self,
    ) -> &mut smithay::wayland::selection::data_device::DataDeviceState {
        &mut self.data_device_state
    }
}

impl OutputHandler for SpidersWm {}

impl WaylandDndGrabHandler for SpidersWm {
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
                let start_data = pointer.grab_start_data().expect("pointer grab data missing");
                let grab = DnDGrab::new_pointer(&self.display_handle, start_data, source, seat);
                pointer.set_grab(self, grab, serial, Focus::Keep);
            }
            GrabType::Touch => source.cancel(),
        }
    }
}

impl DmabufHandler for SpidersWm {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        let Some(BackendState::Winit(backend)) = self.backend.as_mut() else {
            notifier.failed();
            return;
        };

        if backend.renderer().import_dmabuf(&dmabuf, None).is_ok() {
            let _ = notifier.successful::<SpidersWm>();
        } else {
            notifier.failed();
        }
    }
}

delegate_seat!(SpidersWm);
delegate_data_device!(SpidersWm);
delegate_dmabuf!(SpidersWm);
delegate_fractional_scale!(SpidersWm);
delegate_layer_shell!(SpidersWm);
delegate_output!(SpidersWm);
delegate_pointer_constraints!(SpidersWm);
delegate_relative_pointer!(SpidersWm);
delegate_xdg_activation!(SpidersWm);
delegate_xdg_decoration!(SpidersWm);
