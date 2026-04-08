use anyhow::{Context, Result};
use spiders_config::model::{Binding, Config};
use spiders_core::command::WmCommand;
use std::collections::BTreeSet;
use tracing::{debug, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt as _, GrabMode, KeyButMask, KeyPressEvent, ModMask, Window,
};
use x11rb::xcb_ffi::XCBConnection;
use xkbcommon::xkb;

pub(crate) const MOD_SHIFT_BIT: u32 = 1 << 0;
pub(crate) const MOD_LOCK_BIT: u32 = 1 << 1;
pub(crate) const MOD_CONTROL_BIT: u32 = 1 << 2;
pub(crate) const MOD1_BIT: u32 = 1 << 3;
pub(crate) const MOD2_BIT: u32 = 1 << 4;
pub(crate) const MOD3_BIT: u32 = 1 << 5;
pub(crate) const MOD4_BIT: u32 = 1 << 6;
pub(crate) const MOD5_BIT: u32 = 1 << 7;

pub(crate) struct KeyboardBindings {
    pub(crate) installed: Vec<InstalledBinding>,
    xkb_state: XkbBindingState,
}

#[cfg(test)]
impl KeyboardBindings {
    pub(crate) fn empty_for_tests() -> Self {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            "us",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .expect("test keymap");
        let state = xkb::State::new(&keymap);
        let modifier_masks = ModifierMasks::from_keymap(&keymap);
        let significant_modifiers = modifier_masks.shift
            | modifier_masks.control
            | modifier_masks.mod1
            | modifier_masks.mod4;

        Self {
            installed: Vec::new(),
            xkb_state: XkbBindingState { keymap, state, modifier_masks, significant_modifiers },
        }
    }
}

#[derive(Clone)]
struct BindingSpec {
    keysym: xkb::Keysym,
    required_modifiers: xkb::ModMask,
    required_x11_bits: u32,
    command: WmCommand,
}

#[derive(Clone)]
pub(crate) struct InstalledBinding {
    pub(crate) keycode: u8,
    pub(crate) required_modifiers: xkb::ModMask,
    pub(crate) grab_modifiers: ModMask,
    pub(crate) command: WmCommand,
}

struct XkbBindingState {
    keymap: xkb::Keymap,
    state: xkb::State,
    modifier_masks: ModifierMasks,
    significant_modifiers: xkb::ModMask,
}

#[derive(Clone, Copy, Default)]
struct ModifierMasks {
    shift: xkb::ModMask,
    lock: xkb::ModMask,
    control: xkb::ModMask,
    mod1: xkb::ModMask,
    mod2: xkb::ModMask,
    mod3: xkb::ModMask,
    mod4: xkb::ModMask,
    mod5: xkb::ModMask,
}

pub(crate) fn load_keyboard_bindings(
    connection: &XCBConnection,
    config: &Config,
) -> Result<KeyboardBindings> {
    let xkb_state = XkbBindingState::new(connection)?;
    let mod_key = config.options.mod_key.as_deref().unwrap_or("mod");
    let mut installed = Vec::new();

    for binding in &config.bindings {
        let Some(spec) = compile_binding_spec(&xkb_state.keymap, binding, mod_key) else {
            continue;
        };
        let mut compiled = compile_binding_variants(&xkb_state, &spec);

        if compiled.is_empty() {
            let key_token = binding.trigger.split('+').next_back().unwrap_or("<missing>").trim();
            warn!(key = %key_token, trigger = %binding.trigger, "spiders-wm-x could not resolve binding keysym to XKB keycode");
            continue;
        }

        installed.append(&mut compiled);
    }

    Ok(KeyboardBindings { installed, xkb_state })
}

pub(crate) fn install_key_grabs<C: Connection>(
    connection: &C,
    root: Window,
    bindings: &[InstalledBinding],
) -> Result<()> {
    let mut seen = BTreeSet::new();

    for binding in bindings {
        let key = (binding.keycode, u16::from(binding.grab_modifiers));
        if !seen.insert(key) {
            continue;
        }

        connection
            .grab_key(
                false,
                root,
                binding.grab_modifiers,
                binding.keycode,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check()
            .context("failed to install X11 key grab")?;
    }

    if !bindings.is_empty() {
        connection.flush().context("failed to flush X11 key grabs")?;
    }

    Ok(())
}

pub(crate) fn uninstall_key_grabs<C: Connection>(
    connection: &C,
    root: Window,
    bindings: &[InstalledBinding],
) -> Result<()> {
    let mut seen = BTreeSet::new();

    for binding in bindings {
        let key = (binding.keycode, u16::from(binding.grab_modifiers));
        if !seen.insert(key) {
            continue;
        }

        connection
            .ungrab_key(binding.keycode, root, binding.grab_modifiers)?
            .check()
            .context("failed to remove X11 key grab")?;
    }

    if !bindings.is_empty() {
        connection.flush().context("failed to flush X11 key ungrabs")?;
    }

    Ok(())
}

pub(crate) fn binding_for_key_event(
    bindings: &mut KeyboardBindings,
    event: &KeyPressEvent,
) -> Option<WmCommand> {
    let keycode = xkb::Keycode::from(event.detail);
    bindings.xkb_state.update_from_event_state(event.state);

    let keysym = bindings.xkb_state.state.key_get_one_sym(keycode);
    if keysym.raw() == xkb::keysyms::KEY_NoSymbol {
        return None;
    }

    let active_modifiers = bindings.xkb_state.binding_modifiers_for_key(keycode);
    let matched = bindings.installed.iter().find(|binding| {
        binding.keycode == event.detail && binding.required_modifiers == active_modifiers
    });

    if let Some(binding) = matched {
        debug!(
            keycode = event.detail,
            keysym = %xkb::keysym_get_name(keysym),
            active_modifiers,
            command = ?binding.command,
            "x11 binding matched key event"
        );
        Some(binding.command.clone())
    } else {
        debug!(
            keycode = event.detail,
            keysym = %xkb::keysym_get_name(keysym),
            active_modifiers,
            "x11 key event did not match any binding"
        );
        None
    }
}

impl XkbBindingState {
    fn new(connection: &XCBConnection) -> Result<Self> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let mut major = xkb::x11::MIN_MAJOR_XKB_VERSION;
        let mut minor = xkb::x11::MIN_MINOR_XKB_VERSION;
        let mut base_event = 0;
        let mut base_error = 0;
        let setup_ok = xkb::x11::setup_xkb_extension(
            connection,
            xkb::x11::MIN_MAJOR_XKB_VERSION,
            xkb::x11::MIN_MINOR_XKB_VERSION,
            xkb::x11::SetupXkbExtensionFlags::NoFlags,
            &mut major,
            &mut minor,
            &mut base_event,
            &mut base_error,
        );
        if !setup_ok {
            anyhow::bail!("failed to initialize XKB extension over X11 connection");
        }

        let device_id = xkb::x11::get_core_keyboard_device_id(connection);
        if device_id < 0 {
            anyhow::bail!("failed to resolve X11 core keyboard device id for XKB");
        }

        let keymap = xkb::x11::keymap_new_from_device(
            &context,
            connection,
            device_id,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        );
        let state = xkb::x11::state_new_from_device(&keymap, connection, device_id);
        let modifier_masks = ModifierMasks::from_keymap(&keymap);
        let significant_modifiers = modifier_masks.shift
            | modifier_masks.control
            | modifier_masks.mod1
            | modifier_masks.mod4;

        Ok(Self { keymap, state, modifier_masks, significant_modifiers })
    }

    fn update_from_event_state(&mut self, state: KeyButMask) {
        let depressed_mods = self.modifier_masks.depressed_from_x11_state(state);
        let locked_mods = self.modifier_masks.locked_from_x11_state(state);
        let depressed_layout = self.state.serialize_layout(xkb::STATE_LAYOUT_DEPRESSED);
        let latched_layout = self.state.serialize_layout(xkb::STATE_LAYOUT_LATCHED);
        let locked_layout = self.state.serialize_layout(xkb::STATE_LAYOUT_LOCKED);

        self.state.update_mask(
            depressed_mods,
            0,
            locked_mods,
            depressed_layout,
            latched_layout,
            locked_layout,
        );
    }

    fn binding_modifiers_for_key(&self, keycode: xkb::Keycode) -> xkb::ModMask {
        let _ = keycode;
        self.state.serialize_mods(xkb::STATE_MODS_EFFECTIVE) & self.significant_modifiers
    }
}

impl ModifierMasks {
    fn from_keymap(keymap: &xkb::Keymap) -> Self {
        Self {
            shift: modifier_mask_for_name(keymap, "Shift"),
            lock: modifier_mask_for_name(keymap, "Lock"),
            control: modifier_mask_for_name(keymap, "Control"),
            mod1: modifier_mask_for_name(keymap, "Mod1"),
            mod2: modifier_mask_for_name(keymap, "Mod2"),
            mod3: modifier_mask_for_name(keymap, "Mod3"),
            mod4: modifier_mask_for_name(keymap, "Mod4"),
            mod5: modifier_mask_for_name(keymap, "Mod5"),
        }
    }

    fn depressed_from_x11_state(self, state: KeyButMask) -> xkb::ModMask {
        let bits = relevant_key_state_bits(state);
        let mut mask = 0;

        if bits & MOD_SHIFT_BIT != 0 {
            mask |= self.shift;
        }
        if bits & MOD_CONTROL_BIT != 0 {
            mask |= self.control;
        }
        if bits & MOD1_BIT != 0 {
            mask |= self.mod1;
        }
        if bits & MOD3_BIT != 0 {
            mask |= self.mod3;
        }
        if bits & MOD4_BIT != 0 {
            mask |= self.mod4;
        }
        if bits & MOD5_BIT != 0 {
            mask |= self.mod5;
        }

        mask
    }

    fn locked_from_x11_state(self, state: KeyButMask) -> xkb::ModMask {
        let bits = relevant_key_state_bits(state);
        let mut mask = 0;

        if bits & MOD_LOCK_BIT != 0 {
            mask |= self.lock;
        }
        if bits & MOD2_BIT != 0 {
            mask |= self.mod2;
        }

        mask
    }

    fn x11_mask_from_xkb_mask(self, mask: xkb::ModMask) -> u32 {
        let mut x11_mask = 0;

        if self.shift != 0 && mask & self.shift != 0 {
            x11_mask |= MOD_SHIFT_BIT;
        }
        if self.lock != 0 && mask & self.lock != 0 {
            x11_mask |= MOD_LOCK_BIT;
        }
        if self.control != 0 && mask & self.control != 0 {
            x11_mask |= MOD_CONTROL_BIT;
        }
        if self.mod1 != 0 && mask & self.mod1 != 0 {
            x11_mask |= MOD1_BIT;
        }
        if self.mod2 != 0 && mask & self.mod2 != 0 {
            x11_mask |= MOD2_BIT;
        }
        if self.mod3 != 0 && mask & self.mod3 != 0 {
            x11_mask |= MOD3_BIT;
        }
        if self.mod4 != 0 && mask & self.mod4 != 0 {
            x11_mask |= MOD4_BIT;
        }
        if self.mod5 != 0 && mask & self.mod5 != 0 {
            x11_mask |= MOD5_BIT;
        }

        x11_mask
    }
}

fn compile_binding_variants(
    xkb_state: &XkbBindingState,
    spec: &BindingSpec,
) -> Vec<InstalledBinding> {
    let mut installed = Vec::new();
    let mut seen = BTreeSet::new();

    xkb_state.keymap.key_for_each(|keymap, keycode| {
        if !xkb::keycode_is_legal_x11(keycode.raw()) {
            return;
        }

        let layout_count = keymap.num_layouts_for_key(keycode);
        for layout in 0..layout_count {
            let level_count = keymap.num_levels_for_key(keycode, layout);
            for level in 0..level_count {
                let syms = keymap.key_get_syms_by_level(keycode, layout, level);
                if !syms.iter().copied().any(|candidate| candidate == spec.keysym) {
                    continue;
                }

                let mut level_masks = [0_u32; 16];
                let mut level_mask_count =
                    keymap.key_get_mods_for_level(keycode, layout, level, &mut level_masks);
                if level_mask_count == 0 {
                    level_masks[0] = 0;
                    level_mask_count = 1;
                }

                for level_mask in level_masks.iter().copied().take(level_mask_count) {
                    let symbol_x11_bits =
                        xkb_state.modifier_masks.x11_mask_from_xkb_mask(level_mask);
                    for grab_bits in expand_modifier_masks(spec.required_x11_bits | symbol_x11_bits)
                    {
                        let key = (
                            keycode.raw() as u8,
                            grab_bits as u16,
                            spec.required_modifiers,
                            spec.keysym.raw(),
                        );
                        if !seen.insert(key) {
                            continue;
                        }

                        installed.push(InstalledBinding {
                            keycode: keycode.raw() as u8,
                            required_modifiers: spec.required_modifiers,
                            grab_modifiers: ModMask::from(grab_bits as u16),
                            command: spec.command.clone(),
                        });
                    }
                }
            }
        }
    });

    installed
}

fn compile_binding_spec(
    keymap: &xkb::Keymap,
    binding: &Binding,
    mod_key: &str,
) -> Option<BindingSpec> {
    let bind = binding.trigger.split('+').map(|token| token.trim().to_string()).collect::<Vec<_>>();
    let command = binding.command.clone();
    let key_token = bind.last()?;
    let keysym = resolve_binding_keysym(key_token).or_else(|| {
        warn!(key = %key_token, trigger = %binding.trigger, "spiders-wm-x could not resolve binding key to XKB keysym");
        None
    })?;
    let modifiers = compile_binding_modifiers(&bind[..bind.len().saturating_sub(1)], mod_key);

    Some(BindingSpec {
        keysym,
        required_modifiers: modifiers.xkb_mask(keymap),
        required_x11_bits: modifiers.x11_bits(),
        command,
    })
}

fn resolve_binding_keysym(token: &str) -> Option<xkb::Keysym> {
    let primary = binding_keysym_name(token);
    let keysym = xkb::keysym_from_name(&primary, xkb::KEYSYM_NO_FLAGS);
    if keysym.raw() != xkb::keysyms::KEY_NoSymbol {
        return Some(keysym);
    }

    if primary != token {
        let fallback = xkb::keysym_from_name(token, xkb::KEYSYM_NO_FLAGS);
        if fallback.raw() != xkb::keysyms::KEY_NoSymbol {
            return Some(fallback);
        }
    }

    let fallback = xkb::keysym_from_name(token, xkb::KEYSYM_CASE_INSENSITIVE);
    (fallback.raw() != xkb::keysyms::KEY_NoSymbol).then_some(fallback)
}

fn binding_keysym_name(token: &str) -> String {
    if token.eq_ignore_ascii_case("space") {
        "space".to_string()
    } else if token.eq_ignore_ascii_case("comma") {
        "comma".to_string()
    } else if token.eq_ignore_ascii_case("period") {
        "period".to_string()
    } else {
        token.to_string()
    }
}

#[derive(Default)]
struct BindingModifiers {
    shift: bool,
    control: bool,
    alt: bool,
    super_: bool,
}

impl BindingModifiers {
    fn xkb_mask(&self, keymap: &xkb::Keymap) -> xkb::ModMask {
        let mut mask = 0;

        if self.shift {
            mask |= modifier_mask_for_name(keymap, "Shift");
        }
        if self.control {
            mask |= modifier_mask_for_name(keymap, "Control");
        }
        if self.alt {
            mask |= modifier_mask_for_name(keymap, "Mod1");
        }
        if self.super_ {
            mask |= modifier_mask_for_name(keymap, "Mod4");
        }

        mask
    }

    fn x11_bits(&self) -> u32 {
        let mut bits = 0;

        if self.shift {
            bits |= MOD_SHIFT_BIT;
        }
        if self.control {
            bits |= MOD_CONTROL_BIT;
        }
        if self.alt {
            bits |= MOD1_BIT;
        }
        if self.super_ {
            bits |= MOD4_BIT;
        }

        bits
    }
}

fn compile_binding_modifiers(bind: &[String], mod_key: &str) -> BindingModifiers {
    let mut modifiers = BindingModifiers::default();

    for token in bind {
        let resolved = if token.eq_ignore_ascii_case("mod") { mod_key } else { token.as_str() };
        match resolved {
            resolved if resolved.eq_ignore_ascii_case("shift") => modifiers.shift = true,
            resolved
                if resolved.eq_ignore_ascii_case("ctrl")
                    || resolved.eq_ignore_ascii_case("control") =>
            {
                modifiers.control = true
            }
            resolved
                if resolved.eq_ignore_ascii_case("alt")
                    || resolved.eq_ignore_ascii_case("mod1") =>
            {
                modifiers.alt = true
            }
            resolved
                if resolved.eq_ignore_ascii_case("super")
                    || resolved.eq_ignore_ascii_case("logo")
                    || resolved.eq_ignore_ascii_case("mod4") =>
            {
                modifiers.super_ = true
            }
            _ => {}
        }
    }

    modifiers
}

fn modifier_mask_for_name(keymap: &xkb::Keymap, name: &str) -> xkb::ModMask {
    let index = keymap.mod_get_index(name);
    if index == xkb::MOD_INVALID { 0 } else { 1 << index }
}

fn expand_modifier_masks(base: u32) -> Vec<u32> {
    [base, base | MOD_LOCK_BIT, base | MOD2_BIT, base | MOD_LOCK_BIT | MOD2_BIT]
        .into_iter()
        .collect()
}

fn relevant_key_state_bits(state: KeyButMask) -> u32 {
    u16::from(state) as u32
        & (MOD_SHIFT_BIT
            | MOD_LOCK_BIT
            | MOD_CONTROL_BIT
            | MOD1_BIT
            | MOD2_BIT
            | MOD3_BIT
            | MOD4_BIT
            | MOD5_BIT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_config::model::Binding;
    use spiders_core::command::WmCommand;

    #[test]
    fn resolve_binding_keysym_handles_common_aliases() {
        assert_eq!(
            resolve_binding_keysym("Return").map(|keysym| xkb::keysym_get_name(keysym)),
            Some("Return".to_string())
        );
        assert_eq!(
            resolve_binding_keysym("space").map(|keysym| xkb::keysym_get_name(keysym)),
            Some("space".to_string())
        );
        assert_eq!(
            resolve_binding_keysym("comma").map(|keysym| xkb::keysym_get_name(keysym)),
            Some("comma".to_string())
        );
    }

    #[test]
    fn compile_binding_modifiers_resolves_mod_aliases() {
        let modifiers = compile_binding_modifiers(
            &["mod".to_string(), "shift".to_string(), "ctrl".to_string()],
            "super",
        );

        assert!(modifiers.super_);
        assert!(modifiers.shift);
        assert!(modifiers.control);
        assert!(!modifiers.alt);
        assert_eq!(modifiers.x11_bits(), MOD_SHIFT_BIT | MOD_CONTROL_BIT | MOD4_BIT);
    }

    #[test]
    fn compile_binding_modifiers_is_case_insensitive() {
        let modifiers = compile_binding_modifiers(
            &["Alt".to_string(), "Shift".to_string(), "Control".to_string()],
            "super",
        );

        assert!(modifiers.alt);
        assert!(modifiers.shift);
        assert!(modifiers.control);
        assert!(!modifiers.super_);
        assert_eq!(modifiers.x11_bits(), MOD_SHIFT_BIT | MOD_CONTROL_BIT | MOD1_BIT);
    }

    #[test]
    fn expand_modifier_masks_includes_lock_variants() {
        let masks = expand_modifier_masks(MOD4_BIT | MOD_SHIFT_BIT);

        assert_eq!(
            masks,
            vec![
                MOD4_BIT | MOD_SHIFT_BIT,
                MOD4_BIT | MOD_SHIFT_BIT | MOD_LOCK_BIT,
                MOD4_BIT | MOD_SHIFT_BIT | MOD2_BIT,
                MOD4_BIT | MOD_SHIFT_BIT | MOD_LOCK_BIT | MOD2_BIT,
            ]
        );
    }

    #[test]
    fn compile_binding_spec_builds_expected_keysym_and_masks() {
        let keymap = test_keymap();
        let entry = Binding { trigger: "mod+shift+Return".to_string(), command: WmCommand::Quit };

        let spec = compile_binding_spec(&keymap, &entry, "super").expect("binding spec");

        assert_eq!(xkb::keysym_get_name(spec.keysym), "Return");
        assert_eq!(spec.required_x11_bits, MOD_SHIFT_BIT | MOD4_BIT);
        assert_ne!(spec.required_modifiers, 0);
        assert_eq!(spec.command, WmCommand::Quit);
    }

    #[test]
    fn compile_binding_spec_accepts_capitalized_modifiers() {
        let keymap = test_keymap();
        let entry = Binding { trigger: "Alt+Return".to_string(), command: WmCommand::Quit };

        let spec = compile_binding_spec(&keymap, &entry, "super").expect("binding spec");

        assert_eq!(xkb::keysym_get_name(spec.keysym), "Return");
        assert_eq!(spec.required_x11_bits, MOD1_BIT);
        assert_ne!(spec.required_modifiers, 0);
    }

    #[test]
    fn compile_binding_spec_returns_none_for_missing_command() {
        let keymap = test_keymap();
        let entry = Binding { trigger: "".to_string(), command: WmCommand::Quit };

        assert!(compile_binding_spec(&keymap, &entry, "super").is_none());
    }

    fn test_keymap() -> xkb::Keymap {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        xkb::Keymap::new_from_names(&context, "", "", "us", "", None, xkb::KEYMAP_COMPILE_NO_FLAGS)
            .expect("test keymap")
    }
}
