use std::os::unix::io::OwnedFd;
use std::sync::{Arc, Mutex};

use crate::state::SpidersWm;
use smithay::backend::input::KeyState;
use smithay::input::Seat;
use smithay::input::keyboard::{ModifiersState, xkb};
use smithay::reexports::wayland_protocols_misc::zwp_virtual_keyboard_v1::server::{
    zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1, zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use smithay::utils::SERIAL_COUNTER;
use tracing::debug;

const MANAGER_VERSION: u32 = 1;

#[derive(Debug)]
pub(crate) struct VirtualKeyboardManagerState {
    global: GlobalId,
}

pub(crate) struct VirtualKeyboardManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

struct VirtualKeyboardXkb {
    state: xkb::State,
}

impl std::fmt::Debug for VirtualKeyboardXkb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualKeyboardXkb").field("state", &self.state.get_raw_ptr()).finish()
    }
}

// xkbcommon state remains on the compositor thread.
unsafe impl Send for VirtualKeyboardXkb {}

#[derive(Debug, Default)]
struct VirtualKeyboardState {
    initialized: bool,
    mods: ModifiersState,
    xkb: Option<VirtualKeyboardXkb>,
}

#[derive(Debug, Clone, Default)]
struct VirtualKeyboardHandle {
    inner: Arc<Mutex<VirtualKeyboardState>>,
}

pub(crate) struct VirtualKeyboardUserData {
    handle: VirtualKeyboardHandle,
    seat: Seat<SpidersWm>,
}

impl std::fmt::Debug for VirtualKeyboardManagerGlobalData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualKeyboardManagerGlobalData").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for VirtualKeyboardUserData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualKeyboardUserData")
            .field("handle", &self.handle)
            .finish_non_exhaustive()
    }
}

impl VirtualKeyboardManagerState {
    pub(crate) fn new<F>(display: &DisplayHandle, filter: F) -> Self
    where
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let data = VirtualKeyboardManagerGlobalData { filter: Box::new(filter) };
        let global = display
            .create_global::<SpidersWm, ZwpVirtualKeyboardManagerV1, _>(MANAGER_VERSION, data);
        Self { global }
    }

    pub(crate) fn global(&self) -> GlobalId {
        self.global.clone()
    }
}

impl GlobalDispatch<ZwpVirtualKeyboardManagerV1, VirtualKeyboardManagerGlobalData, SpidersWm>
    for VirtualKeyboardManagerState
{
    fn bind(
        _: &mut SpidersWm,
        _: &DisplayHandle,
        _: &Client,
        resource: New<ZwpVirtualKeyboardManagerV1>,
        _: &VirtualKeyboardManagerGlobalData,
        data_init: &mut DataInit<'_, SpidersWm>,
    ) {
        data_init.init(resource, ());
    }

    fn can_view(client: Client, global_data: &VirtualKeyboardManagerGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, (), SpidersWm> for VirtualKeyboardManagerState {
    fn request(
        _state: &mut SpidersWm,
        _client: &Client,
        _resource: &ZwpVirtualKeyboardManagerV1,
        request: zwp_virtual_keyboard_manager_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, SpidersWm>,
    ) {
        match request {
            zwp_virtual_keyboard_manager_v1::Request::CreateVirtualKeyboard { seat, id } => {
                let seat =
                    Seat::<SpidersWm>::from_resource(&seat).expect("virtual keyboard seat missing");
                debug!("virtual keyboard created");
                data_init.init(
                    id,
                    VirtualKeyboardUserData { handle: VirtualKeyboardHandle::default(), seat },
                );
            }
            _ => unreachable!(),
        }
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, VirtualKeyboardUserData, SpidersWm>
    for VirtualKeyboardManagerState
{
    fn request(
        state: &mut SpidersWm,
        _client: &Client,
        virtual_keyboard: &ZwpVirtualKeyboardV1,
        request: zwp_virtual_keyboard_v1::Request,
        data: &VirtualKeyboardUserData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, SpidersWm>,
    ) {
        match request {
            zwp_virtual_keyboard_v1::Request::Keymap { format, fd, size } => {
                debug!(format, size, "virtual keyboard keymap request");
                update_keymap(state, data, format, fd, size as usize, virtual_keyboard);
            }
            zwp_virtual_keyboard_v1::Request::Key { time, key, state: key_state } => {
                if !data.handle.inner.lock().unwrap().initialized {
                    virtual_keyboard.post_error(
                        zwp_virtual_keyboard_v1::Error::NoKeymap,
                        "`key` sent before keymap.",
                    );
                    return;
                }

                let state_value =
                    if key_state == 1 { KeyState::Pressed } else { KeyState::Released };
                debug!(?state_value, key, time, "virtual keyboard key request");
                state.handle_keyboard_key(
                    key.saturating_add(8).into(),
                    state_value,
                    SERIAL_COUNTER.next_serial(),
                    time,
                );
            }
            zwp_virtual_keyboard_v1::Request::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            } => {
                if !data.handle.inner.lock().unwrap().initialized {
                    virtual_keyboard.post_error(
                        zwp_virtual_keyboard_v1::Error::NoKeymap,
                        "`modifiers` sent before keymap.",
                    );
                    return;
                }

                let keyboard = data.seat.get_keyboard().expect("keyboard missing");
                debug!(
                    mods_depressed,
                    mods_latched, mods_locked, group, "virtual keyboard modifiers request"
                );
                let mods = {
                    let mut virtual_state = data.handle.inner.lock().unwrap();
                    let Some(xkb) = virtual_state.xkb.as_mut() else {
                        virtual_keyboard.post_error(
                            zwp_virtual_keyboard_v1::Error::NoKeymap,
                            "`modifiers` sent before keymap.",
                        );
                        return;
                    };
                    xkb.state.update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group);
                    let mut mods = ModifiersState::default();
                    mods.update_with(&xkb.state);
                    virtual_state.mods = mods;
                    mods
                };

                if keyboard.set_modifier_state(mods) != 0 {
                    keyboard.advertise_modifier_state(state);
                }
            }
            zwp_virtual_keyboard_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        _state: &mut SpidersWm,
        _client_id: ClientId,
        _virtual_keyboard: &ZwpVirtualKeyboardV1,
        _data: &VirtualKeyboardUserData,
    ) {
    }
}

fn update_keymap(
    state: &mut SpidersWm,
    data: &VirtualKeyboardUserData,
    format: u32,
    fd: OwnedFd,
    size: usize,
    virtual_keyboard: &ZwpVirtualKeyboardV1,
) {
    if format
        != smithay::reexports::wayland_server::protocol::wl_keyboard::KeymapFormat::XkbV1 as u32
    {
        return;
    }

    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let compiled_keymap = match unsafe {
        xkb::Keymap::new_from_fd(
            &context,
            fd,
            size,
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
    } {
        Ok(Some(compiled_keymap)) => compiled_keymap,
        Ok(None) => {
            virtual_keyboard
                .post_error(zwp_virtual_keyboard_v1::Error::NoKeymap, "failed to compile keymap");
            return;
        }
        Err(_) => {
            virtual_keyboard
                .post_error(zwp_virtual_keyboard_v1::Error::NoKeymap, "could not map keymap");
            return;
        }
    };
    let keymap = compiled_keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
    if keymap.is_empty() {
        virtual_keyboard
            .post_error(zwp_virtual_keyboard_v1::Error::NoKeymap, "compiled keymap was empty");
        return;
    }

    let keyboard = data.seat.get_keyboard().expect("keyboard missing");
    if let Err(error) = keyboard.set_keymap_from_string(state, keymap) {
        virtual_keyboard.post_error(
            zwp_virtual_keyboard_v1::Error::NoKeymap,
            &format!("failed to apply keymap: {error}"),
        );
        return;
    }

    let mut virtual_state = data.handle.inner.lock().unwrap();
    virtual_state.initialized = true;
    virtual_state.mods = keyboard.modifier_state();
    virtual_state.xkb = Some(VirtualKeyboardXkb { state: xkb::State::new(&compiled_keymap) });
}

smithay::reexports::wayland_server::delegate_global_dispatch!(
    SpidersWm: [ZwpVirtualKeyboardManagerV1: VirtualKeyboardManagerGlobalData] => VirtualKeyboardManagerState
);

smithay::reexports::wayland_server::delegate_dispatch!(
    SpidersWm: [ZwpVirtualKeyboardManagerV1: ()] => VirtualKeyboardManagerState
);

smithay::reexports::wayland_server::delegate_dispatch!(
    SpidersWm: [ZwpVirtualKeyboardV1: VirtualKeyboardUserData] => VirtualKeyboardManagerState
);
