pub use spiders_wm_runtime::{
    ParsedBindingEntry, ParsedBindingsState, format_binding_token, parse_bindings_source,
};

use spiders_wm_runtime::{BindingKeyEvent, matches_binding_key_event, normalize_key_input};

pub fn matches_web_keyboard_event(
    entry: &ParsedBindingEntry,
    event: &web_sys::KeyboardEvent,
    mod_key: &str,
) -> bool {
    let Some(actual_key) = normalize_keyboard_event_key(event) else {
        return false;
    };

    matches_binding_key_event(
        entry,
        &BindingKeyEvent {
            key: actual_key,
            alt: event.alt_key(),
            ctrl: event.ctrl_key(),
            meta: event.meta_key(),
            shift: event.shift_key(),
        },
        mod_key,
    )
}

fn normalize_keyboard_event_key(event: &web_sys::KeyboardEvent) -> Option<String> {
    normalize_key_input(&event.code(), &event.key())
}
