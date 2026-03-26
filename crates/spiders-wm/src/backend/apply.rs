use std::io::{Seek, Write};
use std::os::fd::AsFd;

use super::*;
use crate::backend::plan::TitlebarPlan;
use crate::backend::titlebar_renderer::render_titlebar_pixels;
use wayland_client::protocol::wl_shm;

impl RiverBackendState {
    fn write_titlebar_buffer(
        &self,
        width: i32,
        entry: &TitlebarPlan,
    ) -> Option<crate::runtime::registry::TitlebarBufferRecord> {
        let shm = self.shm.as_ref()?;
        let stride = width.checked_mul(4)?;
        let size = stride.checked_mul(entry.height)?;
        let mut file = tempfile::tempfile().ok()?;
        file.set_len(size as u64).ok()?;

        let pixels = render_titlebar_pixels(
            width,
            entry,
            self.config.options.titlebar_font.as_ref(),
        )?;

        for pixel in pixels {
            file.write_all(&pixel.to_le_bytes()).ok()?;
        }
        file.rewind().ok()?;

        let pool = shm.create_pool(file.as_fd(), size, self.queue_handle(), ());
        let buffer = pool.create_buffer(
            0,
            width,
            entry.height,
            stride,
            wl_shm::Format::Argb8888,
            self.queue_handle(),
            (),
        );

        Some(crate::runtime::registry::TitlebarBufferRecord {
            buffer,
            pool,
            file,
            width,
            height: entry.height,
            background: entry.background,
            border_bottom_width: entry.border_bottom_width,
            border_bottom_color: entry.border_bottom_color,
            title: entry.title.clone(),
            text_color: entry.text_color,
            text_align: entry.text_align,
            font_family: entry.font_family.clone(),
            font_size: entry.font_size,
            font_weight: entry.font_weight,
            letter_spacing: entry.letter_spacing,
            box_shadow: entry.box_shadow.clone(),
            padding_top: entry.padding_top,
            padding_right: entry.padding_right,
            padding_bottom: entry.padding_bottom,
            padding_left: entry.padding_left,
            corner_radius_top_left: entry.corner_radius_top_left,
            corner_radius_top_right: entry.corner_radius_top_right,
        })
    }

    pub(super) fn apply_manage_window_plan(&mut self, plan: &[ManageWindowPlan]) {
        for entry in plan {
            let Some(object_id) = self.window_object_id(&entry.window_id) else {
                continue;
            };
            let Some(window) = self.registry.windows.get_mut(&object_id) else {
                continue;
            };

            window.proxy.propose_dimensions(entry.width, entry.height);
            window.proxy.set_tiled(entry.tiled_edges);
            self.runtime_state
                .set_window_size(&entry.window_id, entry.width, entry.height);
        }
    }

    pub(super) fn apply_render_window_plan(&mut self, plan: &[RenderWindowPlan]) {
        for entry in plan {
            let Some(object_id) = self.window_object_id(&entry.window_id) else {
                continue;
            };
            let Some(window) = self.registry.windows.get_mut(&object_id) else {
                continue;
            };

            window.node.set_position(entry.x, entry.y);
            self.runtime_state.set_window_geometry(
                &entry.window_id,
                entry.x,
                entry.y,
                entry.width,
                entry.height,
            );
        }
    }

    pub(super) fn apply_clear_tiled_state_plan(&mut self, plan: &[ClearTiledStatePlan]) {
        for entry in plan {
            let Some(object_id) = self.window_object_id(&entry.window_id) else {
                continue;
            };
            let Some(window) = self.registry.windows.get_mut(&object_id) else {
                continue;
            };

            window.proxy.set_tiled(river_window_v1::Edges::None);
        }
    }

    pub(super) fn apply_offscreen_window_plan(&mut self, plan: &[OffscreenWindowPlan]) {
        for entry in plan {
            let Some(object_id) = self.window_object_id(&entry.window_id) else {
                continue;
            };
            let Some(window) = self.registry.windows.get_mut(&object_id) else {
                continue;
            };

            window.node.set_position(entry.x, entry.y);
        }
    }

    pub(super) fn apply_border_plan(&mut self, plan: &[BorderPlan]) {
        for entry in plan {
            let Some(object_id) = self.window_object_id(&entry.window_id) else {
                continue;
            };
            let Some(window) = self.registry.windows.get_mut(&object_id) else {
                continue;
            };

            window.proxy.set_borders(
                entry.edges,
                entry.width,
                entry.red,
                entry.green,
                entry.blue,
                entry.alpha,
            );
        }
    }

    pub(super) fn apply_appearance_plan(&mut self, plan: &[AppearancePlan]) {
        for entry in plan {
            let Some(object_id) = self.window_object_id(&entry.window_id) else {
                continue;
            };
            let Some(window) = self.registry.windows.get_mut(&object_id) else {
                continue;
            };

            match entry.decoration_mode {
                DecorationMode::ClientSide => window.proxy.use_csd(),
                DecorationMode::CompositorTitlebar => window.proxy.use_ssd(),
                DecorationMode::NoTitlebar => {
                    // Best effort "no titlebar": for SSD-capable clients ask the client not to
                    // draw decorations, and do not create compositor titlebar surfaces. If the
                    // client only supports CSD, river cannot suppress its client-drawn chrome.
                    if window.supports_ssd {
                        window.proxy.use_ssd();
                    } else {
                        window.proxy.use_csd();
                    }
                }
            }
        }
    }

    pub(super) fn apply_titlebar_plan(&mut self, plan: &[TitlebarPlan]) {
        let planned = plan
            .iter()
            .filter_map(|entry| self.window_object_id(&entry.window_id).map(|id| (id, entry)))
            .collect::<Vec<_>>();

        for (object_id, entry) in planned {
            let width = self
                .runtime_state
                .windows
                .get(&entry.window_id)
                .map(|window| window.width)
                .unwrap_or(0)
                .max(1);
            let Some(needs_buffer) = self.registry.titlebars.get(&object_id).map(|titlebar| {
                titlebar.buffer.as_ref().is_none_or(|buffer| {
                    buffer.width != width
                        || buffer.height != entry.height
                        || buffer.background != entry.background
                        || buffer.border_bottom_width != entry.border_bottom_width
                        || buffer.border_bottom_color != entry.border_bottom_color
                        || buffer.title != entry.title
                        || buffer.text_color != entry.text_color
                        || buffer.text_align != entry.text_align
                        || buffer.font_family != entry.font_family
                        || buffer.font_size != entry.font_size
                        || buffer.font_weight != entry.font_weight
                        || buffer.letter_spacing != entry.letter_spacing
                        || buffer.box_shadow != entry.box_shadow
                        || buffer.padding_top != entry.padding_top
                        || buffer.padding_right != entry.padding_right
                        || buffer.padding_bottom != entry.padding_bottom
                        || buffer.padding_left != entry.padding_left
                        || buffer.corner_radius_top_left != entry.corner_radius_top_left
                        || buffer.corner_radius_top_right != entry.corner_radius_top_right
                })
            }) else {
                continue;
            };

            if needs_buffer {
                let next_buffer = self.write_titlebar_buffer(width, entry);
                let Some(titlebar) = self.registry.titlebars.get_mut(&object_id) else {
                    continue;
                };
                if let Some(old) = titlebar.buffer.take() {
                    old.buffer.destroy();
                    old.pool.destroy();
                }
                titlebar.buffer = next_buffer;
            }

            let Some(titlebar) = self.registry.titlebars.get_mut(&object_id) else {
                continue;
            };
            let Some(buffer) = titlebar.buffer.as_ref() else {
                continue;
            };

            titlebar.decoration.set_offset(entry.offset_x, -entry.height + entry.offset_y);
            titlebar.decoration.sync_next_commit();
            titlebar.surface.attach(Some(&buffer.buffer), 0, 0);
            titlebar.surface.damage_buffer(0, 0, width, entry.height);
            titlebar.surface.commit();
        }
    }

    pub(super) fn apply_window_mode_plan(&mut self, plan: &[WindowModePlan]) {
        for entry in plan {
            let Some(object_id) = self.window_object_id(&entry.window_id) else {
                continue;
            };
            let Some(window) = self.registry.windows.get_mut(&object_id) else {
                continue;
            };

            match &entry.mode {
                spiders_shared::types::WindowMode::Tiled => {
                    window.proxy.set_tiled(
                        river_window_v1::Edges::Top
                            | river_window_v1::Edges::Bottom
                            | river_window_v1::Edges::Left
                            | river_window_v1::Edges::Right,
                    );
                    window.proxy.inform_not_fullscreen();
                }
                spiders_shared::types::WindowMode::Floating { .. } => {
                    window.proxy.set_tiled(river_window_v1::Edges::None);
                    window.proxy.inform_not_fullscreen();
                    window.proxy.propose_dimensions(entry.width, entry.height);
                    window.node.set_position(entry.x, entry.y);
                }
                spiders_shared::types::WindowMode::Fullscreen => {
                    window.proxy.set_tiled(river_window_v1::Edges::None);
                    window.proxy.inform_fullscreen();
                    window.proxy.propose_dimensions(entry.width, entry.height);
                    window.node.set_position(entry.x, entry.y);
                }
            }

            self.runtime_state
                .set_window_mode(&entry.window_id, entry.mode.clone());
            self.runtime_state
                .set_window_geometry(&entry.window_id, entry.x, entry.y, entry.width, entry.height);
        }
    }

    pub(super) fn apply_focus_plan(&mut self, seat_id: &ObjectId, plan: &FocusPlan) {
        match plan {
            FocusPlan::FocusWindow { window_id } => {
                let Some(object_id) = self.window_object_id(window_id) else {
                    return;
                };
                let Some(window) = self.registry.windows.get(&object_id).cloned() else {
                    return;
                };
                let Some(seat) = self.registry.seats.get_mut(seat_id) else {
                    return;
                };

                seat.proxy.focus_window(&window.proxy);
                self.runtime_state.focus_window(window_id);
                self.runtime_state
                    .set_seat_focused_window(&seat.state_name, Some(window_id.clone()));
            }
            FocusPlan::ClearFocus => {
                let Some(seat) = self.registry.seats.get_mut(seat_id) else {
                    return;
                };
                seat.proxy.clear_focus();
                self.runtime_state
                    .set_seat_focused_window(&seat.state_name, None);
                self.runtime_state.focused_window_id = None;
            }
        }
    }

    pub(super) fn apply_close_window_plan(&mut self, plan: &CloseWindowPlan) {
        self.runtime_state.request_window_close(&plan.window_id);
    }

    pub(super) fn apply_move_focused_window_to_workspace_plan(
        &mut self,
        seat_id: &ObjectId,
        plan: &MoveFocusedWindowToWorkspacePlan,
    ) {
        self.runtime_state
            .set_window_workspace(&plan.window_id, &plan.workspace_id);
        self.runtime_state.focused_window_id = None;

        if let Some(seat) = self.registry.seats.get(seat_id) {
            self.runtime_state
                .set_seat_focused_window(&seat.state_name, None);
        }

        self.apply_focus_plan(seat_id, &plan.focus);
    }

    pub(super) fn apply_move_window_in_workspace_plan(
        &mut self,
        seat_id: &ObjectId,
        plan: &MoveWindowInWorkspacePlan,
    ) {
        if self
            .runtime_state
            .swap_windows_in_stack(&plan.window_id, &plan.target_window_id)
        {
            self.apply_focus_plan(seat_id, &plan.focus);
        }
    }

    pub(super) fn apply_resize_window_plan(&mut self, plan: &[ResizeWindowPlan]) {
        for entry in plan {
            let Some(object_id) = self.window_object_id(&entry.window_id) else {
                continue;
            };
            let Some(window) = self.registry.windows.get(&object_id) else {
                continue;
            };

            window
                .proxy
                .propose_dimensions(entry.width.max(1), entry.height.max(1));
        }
    }

    pub(super) fn apply_pointer_render_plan(&mut self, plan: &[PointerRenderPlan]) {
        for entry in plan {
            let Some(object_id) = self.window_object_id(&entry.window_id) else {
                continue;
            };
            let Some(window) = self.registry.windows.get_mut(&object_id) else {
                continue;
            };

            window.node.set_position(entry.x, entry.y);
            let (width, height) = self
                .runtime_state
                .windows
                .get(&entry.window_id)
                .map(|window| (window.width, window.height))
                .unwrap_or((0, 0));
            self.runtime_state
                .set_window_geometry(&entry.window_id, entry.x, entry.y, width, height);
        }
    }
}

impl SeatRecord {
    pub(super) fn new(proxy: river_seat_v1::RiverSeatV1, state_name: String) -> Self {
        Self {
            proxy,
            state_name,
            xkb_bindings: HashMap::new(),
            pointer_bindings: HashMap::new(),
        }
    }

    pub(super) fn pointer_move(
        &mut self,
        state: &mut WmState,
        window: &WindowRecord,
        window_state: &WindowState,
    ) {
        self.proxy.op_start_pointer();
        state.set_seat_pointer_op(
            &self.state_name,
            SeatPointerOpState::Move {
                window_id: window.state_id.clone(),
                start_x: window_state.x,
                start_y: window_state.y,
            },
        );
        state.set_seat_pointer_delta(&self.state_name, 0, 0);
    }

    pub(super) fn pointer_resize(
        &mut self,
        state: &mut WmState,
        window: &WindowRecord,
        window_state: &WindowState,
        edges: river_window_v1::Edges,
    ) {
        self.proxy.op_start_pointer();
        window.proxy.inform_resize_start();
        state.set_seat_pointer_op(
            &self.state_name,
            SeatPointerOpState::Resize {
                window_id: window.state_id.clone(),
                start_x: window_state.x,
                start_y: window_state.y,
                start_width: window_state.width,
                start_height: window_state.height,
                edges,
            },
        );
        state.set_seat_pointer_delta(&self.state_name, 0, 0);
    }

    pub(super) fn op_end(&mut self, state: &mut WmState) {
        self.proxy.op_end();
        state.set_seat_pointer_op(&self.state_name, SeatPointerOpState::None);
        state.set_seat_pointer_delta(&self.state_name, 0, 0);
    }

    pub(super) fn op_manage(
        &mut self,
        state: &WmState,
        windows: &HashMap<ObjectId, WindowRecord>,
    ) -> Option<ResizeWindowPlan> {
        if let Some(seat_state) = state.seats.get(&self.state_name)
            && let SeatPointerOpState::Resize {
            window_id,
            start_width,
            start_height,
            edges,
            ..
        } = &seat_state.pointer_op
            && let Some(window) = windows.values().find(|window| &window.state_id == window_id)
        {
            let mut width = *start_width;
            let mut height = *start_height;
            if edges.contains(river_window_v1::Edges::Left) {
                width -= seat_state.pointer_op_dx;
            }
            if edges.contains(river_window_v1::Edges::Right) {
                width += seat_state.pointer_op_dx;
            }
            if edges.contains(river_window_v1::Edges::Top) {
                height -= seat_state.pointer_op_dy;
            }
            if edges.contains(river_window_v1::Edges::Bottom) {
                height += seat_state.pointer_op_dy;
            }
            let _ = window;
            return Some(ResizeWindowPlan {
                window_id: window_id.clone(),
                width: width.max(1),
                height: height.max(1),
            });
        }

        None
    }
}

pub(super) fn parse_binding(binding: &Binding) -> Option<ParsedBinding> {
    let parts = binding
        .trigger
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let target = parts.last().copied()?;
    let modifiers = parts[..parts.len().saturating_sub(1)]
        .iter()
        .try_fold(river_seat_v1::Modifiers::None, |mods, part| {
            parse_modifier(part).map(|modifier| mods | modifier)
        })?;

    if let Some(button) = parse_pointer_button(target) {
        return Some(ParsedBinding {
            trigger: binding.trigger.clone(),
            kind: BindingTargetKind::Pointer,
            modifiers,
            key: None,
            button: Some(button),
            action: binding.action.clone(),
        });
    }

    parse_keysym(target).map(|key| ParsedBinding {
        trigger: binding.trigger.clone(),
        kind: BindingTargetKind::Key,
        modifiers,
        key: Some(key),
        button: None,
        action: binding.action.clone(),
    })
}

fn workspace_assign_prefix(config: &Config, mod_key: &str) -> String {
    for binding in &config.bindings {
        let WmCommand::AssignFocusedWindowToWorkspace { .. } = binding.action else {
            continue;
        };

        let tokens = binding
            .trigger
            .split('+')
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();

        if tokens.len() < 2 {
            continue;
        }

        let Some(last) = tokens.last().copied() else {
            continue;
        };

        if last.parse::<u8>().ok().is_some_and(|workspace| (1..=9).contains(&workspace)) {
            return tokens[..tokens.len() - 1].join("+");
        }
    }

    format!("{mod_key}+Shift")
}

fn milestone_bindings_with_mod_key(config: &Config, mod_key: &str) -> Vec<Binding> {
    let assign_prefix = workspace_assign_prefix(config, mod_key);

    let mut bindings = vec![
        Binding {
            trigger: format!("{mod_key}+Enter"),
            action: WmCommand::Spawn {
                command: "foot".into(),
            },
        },
        Binding {
            trigger: format!("{mod_key}+q"),
            action: WmCommand::CloseFocusedWindow,
        },
        Binding {
            trigger: format!("{mod_key}+h"),
            action: WmCommand::FocusDirection {
                direction: FocusDirection::Left,
            },
        },
        Binding {
            trigger: format!("{mod_key}+l"),
            action: WmCommand::FocusDirection {
                direction: FocusDirection::Right,
            },
        },
    ];

    bindings.extend((1..=9).map(|workspace| Binding {
        trigger: format!("{mod_key}+{workspace}"),
        action: WmCommand::ActivateWorkspace {
            workspace_id: workspace.to_string().into(),
        },
    }));

    bindings.extend((1..=9).map(|workspace| Binding {
        trigger: format!("{assign_prefix}+{workspace}"),
        action: WmCommand::AssignFocusedWindowToWorkspace { workspace },
    }));

    bindings.extend([
        Binding {
            trigger: format!("{mod_key}+Shift+h"),
            action: WmCommand::MoveDirection {
                direction: FocusDirection::Left,
            },
        },
        Binding {
            trigger: format!("{mod_key}+Shift+j"),
            action: WmCommand::MoveDirection {
                direction: FocusDirection::Down,
            },
        },
        Binding {
            trigger: format!("{mod_key}+Shift+k"),
            action: WmCommand::MoveDirection {
                direction: FocusDirection::Up,
            },
        },
        Binding {
            trigger: format!("{mod_key}+Shift+l"),
            action: WmCommand::MoveDirection {
                direction: FocusDirection::Right,
            },
        },
    ]);

    bindings
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BindingSignature {
    kind: BindingTargetKind,
    modifiers: river_seat_v1::Modifiers,
    key: Option<u32>,
    button: Option<u32>,
}

pub(super) fn effective_bindings(config: &Config) -> Vec<Binding> {
    let mut bindings = config.bindings.clone();
    let mod_key = config.options.mod_key.as_deref().unwrap_or("Alt");
    let mut existing_signatures = bindings
        .iter()
        .filter_map(parse_binding)
        .map(|binding| BindingSignature {
            kind: binding.kind,
            modifiers: binding.modifiers,
            key: binding.key,
            button: binding.button,
        })
        .collect::<std::collections::HashSet<_>>();

    for binding in milestone_bindings_with_mod_key(config, mod_key) {
        let Some(parsed) = parse_binding(&binding) else {
            continue;
        };
        let signature = BindingSignature {
            kind: parsed.kind,
            modifiers: parsed.modifiers,
            key: parsed.key,
            button: parsed.button,
        };

        if !existing_signatures.insert(signature) {
            continue;
        }

        bindings.push(binding);
    }

    bindings
}

fn parse_modifier(part: &str) -> Option<river_seat_v1::Modifiers> {
    match part.to_ascii_lowercase().as_str() {
        "shift" => Some(river_seat_v1::Modifiers::Shift),
        "ctrl" | "control" => Some(river_seat_v1::Modifiers::Ctrl),
        "alt" => Some(river_seat_v1::Modifiers::Mod1),
        "super" | "logo" | "mod" | "mod4" => Some(river_seat_v1::Modifiers::Mod4),
        "mod3" => Some(river_seat_v1::Modifiers::Mod3),
        "mod5" => Some(river_seat_v1::Modifiers::Mod5),
        _ => None,
    }
}

fn parse_keysym(target: &str) -> Option<u32> {
    match target.to_ascii_lowercase().as_str() {
        "space" => Some(0x20),
        "return" | "enter" => Some(0xff0d),
        "tab" => Some(0xff09),
        "escape" | "esc" => Some(0xff1b),
        _ if target.len() == 1 => target.chars().next().map(|ch| ch as u32),
        _ if target.starts_with('F') || target.starts_with('f') => {
            let suffix = &target[1..];
            suffix.parse::<u32>().ok().and_then(|n| match n {
                1..=12 => Some(0xffbe + (n - 1)),
                _ => None,
            })
        }
        _ => None,
    }
}

fn parse_pointer_button(target: &str) -> Option<u32> {
    match target.to_ascii_lowercase().as_str() {
        "btn_left" | "button1" | "mouseleft" | "leftclick" => Some(0x110),
        "btn_right" | "button3" | "mouseright" | "rightclick" => Some(0x111),
        "btn_middle" | "button2" | "mousemiddle" | "middleclick" => Some(0x112),
        _ => None,
    }
}
