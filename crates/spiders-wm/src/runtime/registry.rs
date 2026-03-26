use std::collections::{BTreeMap, HashMap};
use std::fs::File;

use spiders_shared::command::WmCommand;
use spiders_scene::{BoxShadowValue, ColorValue, FontFamilyValue, FontWeightValue, TextAlignValue};
use spiders_tree::{OutputId, WindowId};
use wayland_backend::client::ObjectId;
use wayland_client::protocol::{wl_buffer, wl_shm_pool, wl_surface};

use crate::protocol::river_window_management_v1::{
    river_decoration_v1, river_node_v1, river_output_v1, river_pointer_binding_v1,
    river_seat_v1, river_window_v1,
};
use crate::protocol::river_xkb_bindings::river_xkb_binding_v1;

#[derive(Debug, Default)]
pub struct RiverRegistry {
    pub outputs: HashMap<ObjectId, OutputRecord>,
    pub output_ids_by_state: HashMap<OutputId, ObjectId>,
    pub windows: HashMap<ObjectId, WindowRecord>,
    pub window_ids_by_state: HashMap<WindowId, ObjectId>,
    pub titlebars: HashMap<ObjectId, TitlebarRecord>,
    pub seats: HashMap<ObjectId, SeatRecord>,
    pub input_devices: HashMap<ObjectId, InputDeviceRecord>,
    pub xkb_keyboards: HashMap<ObjectId, XkbKeyboardRecord>,
    pub libinput_devices: HashMap<ObjectId, LibinputDeviceRecord>,
    pub wl_outputs_by_global: BTreeMap<u32, WlOutputRecord>,
    pub wl_seats_by_global: BTreeMap<u32, WlSeatRecord>,
}

#[derive(Debug, Clone)]
pub struct OutputRecord {
    pub proxy: river_output_v1::RiverOutputV1,
    pub state_id: spiders_tree::OutputId,
}

#[derive(Debug, Clone)]
pub struct WindowRecord {
    pub proxy: river_window_v1::RiverWindowV1,
    pub node: river_node_v1::RiverNodeV1,
    pub state_id: spiders_tree::WindowId,
    pub supports_ssd: bool,
}

#[derive(Debug)]
pub struct TitlebarRecord {
    pub decoration: river_decoration_v1::RiverDecorationV1,
    pub surface: wl_surface::WlSurface,
    pub buffer: Option<TitlebarBufferRecord>,
}

#[derive(Debug)]
pub struct TitlebarBufferRecord {
    pub buffer: wl_buffer::WlBuffer,
    pub pool: wl_shm_pool::WlShmPool,
    pub file: File,
    pub width: i32,
    pub height: i32,
    pub background: ColorValue,
    pub border_bottom_width: i32,
    pub border_bottom_color: ColorValue,
    pub title: String,
    pub text_color: ColorValue,
    pub text_align: TextAlignValue,
    pub font_family: Option<FontFamilyValue>,
    pub font_size: i32,
    pub font_weight: FontWeightValue,
    pub letter_spacing: i32,
    pub box_shadow: Option<Vec<BoxShadowValue>>,
    pub padding_top: i32,
    pub padding_right: i32,
    pub padding_bottom: i32,
    pub padding_left: i32,
    pub corner_radius_top_left: i32,
    pub corner_radius_top_right: i32,
}

#[derive(Debug, Clone)]
pub struct XkbBindingRecord {
    pub proxy: river_xkb_binding_v1::RiverXkbBindingV1,
    pub trigger: String,
    pub action: WmCommand,
}

#[derive(Debug, Clone)]
pub struct PointerBindingRecord {
    pub proxy: river_pointer_binding_v1::RiverPointerBindingV1,
    pub trigger: String,
    pub action: WmCommand,
}

#[derive(Debug, Clone)]
pub struct SeatRecord {
    pub proxy: river_seat_v1::RiverSeatV1,
    pub state_name: String,
    pub xkb_bindings: HashMap<ObjectId, XkbBindingRecord>,
    pub pointer_bindings: HashMap<ObjectId, PointerBindingRecord>,
}

#[derive(Debug, Clone)]
pub struct WlOutputRecord {
    pub logical_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WlSeatRecord {
    pub logical_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceKind {
    Keyboard,
    Pointer,
    Touch,
    Tablet,
}

#[derive(Debug, Clone)]
pub struct InputDeviceRecord {
    pub proxy: crate::protocol::river_input_management::river_input_device_v1::RiverInputDeviceV1,
    pub name: Option<String>,
    pub kind: Option<InputDeviceKind>,
}

#[derive(Debug, Clone)]
pub struct XkbKeyboardRecord {
    pub proxy: crate::protocol::river_xkb_config::river_xkb_keyboard_v1::RiverXkbKeyboardV1,
    pub input_device_id: Option<ObjectId>,
}

#[derive(Debug, Clone)]
pub struct LibinputDeviceRecord {
    pub proxy:
        crate::protocol::river_libinput_config::river_libinput_device_v1::RiverLibinputDeviceV1,
    pub input_device_id: Option<ObjectId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingTargetKind {
    Key,
    Pointer,
}

#[derive(Debug, Clone)]
pub struct ParsedBinding {
    pub trigger: String,
    pub kind: BindingTargetKind,
    pub modifiers: river_seat_v1::Modifiers,
    pub key: Option<u32>,
    pub button: Option<u32>,
    pub action: WmCommand,
}
