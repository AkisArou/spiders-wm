# Runtime API Sketch

This document turns the runtime-boundary discussion into a concrete Rust sketch.

The recommendation is:

- Keep a trait-free runtime API as the primary boundary.
- Let the runtime return typed effects.
- Add an optional host-trait convenience layer on top of the same effect API.

That keeps `crates/spiders-wm-runtime` testable and reusable while still allowing `apps/spiders-wm` and `apps/spiders-wm-www` to opt into a thinner integration style.

## Recommended Shape

Canonical API:

```rust
use spiders_core::api::CompositorEvent;
use spiders_core::command::WmCommand;
use spiders_core::resize::LayoutAdjustmentState;
use spiders_core::wm::WmModel;
use spiders_core::{OutputId, WindowId, WorkspaceId};

#[derive(Debug, Clone)]
pub struct RuntimeBootstrap {
    pub default_workspace: String,
    pub output_id: OutputId,
    pub output_name: String,
}

#[derive(Debug, Clone)]
pub enum RuntimeInput {
    Command(WmCommand),
    OutputConnected {
        output_id: OutputId,
        output_name: String,
        width: u32,
        height: u32,
    },
    WindowMapped {
        window_id: WindowId,
        workspace_id: WorkspaceId,
        app_id: Option<String>,
        title: Option<String>,
    },
    WindowUnmapped {
        window_id: WindowId,
    },
    WindowDestroyed {
        window_id: WindowId,
    },
    FocusRequested {
        window_id: Option<WindowId>,
    },
    PointerEntered {
        window_id: Option<WindowId>,
    },
}

#[derive(Debug, Clone)]
pub enum RuntimeEffect {
    SpawnProgram {
        command: String,
    },
    ExitRequested,
    CloseWindow {
        window_id: WindowId,
    },
    FocusWindow {
        window_id: Option<WindowId>,
    },
    RaiseWindow {
        window_id: WindowId,
    },
    RelayoutRequested,
    ConfigReloadRequested,
    PublishEvent(CompositorEvent),
}

#[derive(Debug, Clone)]
pub struct RuntimeOutput {
    pub state_changed: bool,
    pub effects: Vec<RuntimeEffect>,
}

pub struct Runtime {
    model: WmModel,
    layout_adjustments: LayoutAdjustmentState,
}

impl Runtime {
    pub fn new(bootstrap: RuntimeBootstrap) -> Self {
        let mut model = WmModel::default();
        model.upsert_output(
            bootstrap.output_id.clone(),
            bootstrap.output_name,
            0,
            0,
            Some(bootstrap.default_workspace.clone().into()),
        );
        model.upsert_workspace(
            bootstrap.default_workspace.clone().into(),
            bootstrap.default_workspace,
        );

        Self {
            model,
            layout_adjustments: LayoutAdjustmentState::default(),
        }
    }

    pub fn apply(&mut self, input: RuntimeInput) -> RuntimeOutput {
        let mut effects = Vec::new();
        let mut state_changed = false;

        match input {
            RuntimeInput::Command(command) => {
                state_changed |= self.apply_command(command, &mut effects);
            }
            RuntimeInput::WindowMapped {
                window_id,
                workspace_id,
                app_id,
                title,
            } => {
                self.model.insert_window(window_id.clone(), Some(workspace_id), self.model.current_output_id().cloned());
                self.model.set_window_mapped(window_id.clone(), true);
                self.model.set_window_identity(window_id.clone(), title, app_id);
                effects.push(RuntimeEffect::RelayoutRequested);
                effects.push(RuntimeEffect::FocusWindow { window_id: Some(window_id) });
                state_changed = true;
            }
            RuntimeInput::WindowUnmapped { window_id } => {
                self.model.set_window_mapped(window_id, false);
                effects.push(RuntimeEffect::RelayoutRequested);
                state_changed = true;
            }
            RuntimeInput::WindowDestroyed { window_id } => {
                self.model.remove_window(&window_id);
                effects.push(RuntimeEffect::RelayoutRequested);
                state_changed = true;
            }
            RuntimeInput::OutputConnected { .. }
            | RuntimeInput::FocusRequested { .. }
            | RuntimeInput::PointerEntered { .. } => {}
        }

        RuntimeOutput {
            state_changed,
            effects,
        }
    }

    pub fn model(&self) -> &WmModel {
        &self.model
    }

    pub fn layout_adjustments(&self) -> &LayoutAdjustmentState {
        &self.layout_adjustments
    }

    fn apply_command(&mut self, command: WmCommand, effects: &mut Vec<RuntimeEffect>) -> bool {
        match command {
            WmCommand::Spawn { command } => {
                effects.push(RuntimeEffect::SpawnProgram { command });
                false
            }
            WmCommand::SpawnTerminal => {
                effects.push(RuntimeEffect::SpawnProgram {
                    command: "foot".to_string(),
                });
                false
            }
            WmCommand::Quit => {
                effects.push(RuntimeEffect::ExitRequested);
                false
            }
            WmCommand::ReloadConfig => {
                effects.push(RuntimeEffect::ConfigReloadRequested);
                false
            }
            WmCommand::CloseFocusedWindow => {
                if let Some(window_id) = self.model.focused_window_id().cloned() {
                    effects.push(RuntimeEffect::CloseWindow { window_id });
                }
                false
            }
            WmCommand::FocusNextWindow
            | WmCommand::FocusPreviousWindow
            | WmCommand::FocusDirection { .. }
            | WmCommand::FocusWindow { .. }
            | WmCommand::SelectWorkspace { .. }
            | WmCommand::SelectNextWorkspace
            | WmCommand::SelectPreviousWorkspace
            | WmCommand::AssignFocusedWindowToWorkspace { .. }
            | WmCommand::ToggleAssignFocusedWindowToWorkspace { .. }
            | WmCommand::ToggleFloating
            | WmCommand::ToggleFullscreen
            | WmCommand::SwapDirection { .. }
            | WmCommand::ResizeDirection { .. }
            | WmCommand::ResizeTiledDirection { .. } => {
                effects.push(RuntimeEffect::RelayoutRequested);
                true
            }
            WmCommand::SetLayout { .. }
            | WmCommand::CycleLayout { .. }
            | WmCommand::ViewWorkspace { .. }
            | WmCommand::ActivateWorkspace { .. }
            | WmCommand::ToggleViewWorkspace { .. }
            | WmCommand::AssignWorkspace { .. }
            | WmCommand::FocusMonitorLeft
            | WmCommand::FocusMonitorRight
            | WmCommand::SendMonitorLeft
            | WmCommand::SendMonitorRight
            | WmCommand::SetFloatingWindowGeometry { .. }
            | WmCommand::MoveDirection { .. } => false,
        }
    }
}
```

## Why This Is The Base API

This shape keeps the boundary honest:

- The runtime owns state reduction.
- The runtime emits host-meaningful intents instead of directly touching smithay or browser state.
- The apps stay thin because they only translate `RuntimeEffect` into platform work.
- Tests can assert both state transitions and emitted effects without mocks or trait objects.

## Optional Host Trait Layer

The implemented version of this layer now lives in `spiders-wm-runtime` as `WmEnvironment`.
It keeps the same intent described below, but the concrete API is environment-oriented rather
than a direct effect-to-method mirror: environments activate workspaces, move windows between
workspaces, focus windows, toggle window state, and handle process or quit requests.

The host-trait form should be a convenience wrapper, not the only API.

```rust
use spiders_core::WindowId;

pub trait RuntimeHost {
    fn spawn_program(&mut self, command: &str);
    fn request_exit(&mut self);
    fn request_close_window(&mut self, window_id: &WindowId);
    fn focus_window(&mut self, window_id: Option<&WindowId>);
    fn raise_window(&mut self, window_id: &WindowId);
    fn request_relayout(&mut self);
    fn request_config_reload(&mut self);
    fn publish_event(&mut self, event: &spiders_core::api::CompositorEvent);
}

impl Runtime {
    pub fn apply_with_host<H: RuntimeHost>(
        &mut self,
        input: RuntimeInput,
        host: &mut H,
    ) -> RuntimeOutput {
        let output = self.apply(input);

        for effect in &output.effects {
            match effect {
                RuntimeEffect::SpawnProgram { command } => host.spawn_program(command),
                RuntimeEffect::ExitRequested => host.request_exit(),
                RuntimeEffect::CloseWindow { window_id } => {
                    host.request_close_window(window_id)
                }
                RuntimeEffect::FocusWindow { window_id } => {
                    host.focus_window(window_id.as_ref())
                }
                RuntimeEffect::RaiseWindow { window_id } => host.raise_window(window_id),
                RuntimeEffect::RelayoutRequested => host.request_relayout(),
                RuntimeEffect::ConfigReloadRequested => host.request_config_reload(),
                RuntimeEffect::PublishEvent(event) => host.publish_event(event),
            }
        }

        output
    }
}
```

That gives us two integration modes from one runtime:

- explicit effect matching when the app wants maximum control
- host-trait dispatch when the app wants thinner glue

## Minimal Usage: `apps/spiders-wm` Without Host Trait

This is the most explicit compositor integration. It replaces a large `match WmCommand` executor with `RuntimeInput` plus effect dispatch.

```rust
use spiders_wm_runtime::{Runtime, RuntimeEffect, RuntimeInput};

impl SpidersWm {
    pub fn execute_wm_command(&mut self, command: WmCommand) {
        let output = self.runtime.apply(RuntimeInput::Command(command));

        for effect in output.effects {
            match effect {
                RuntimeEffect::SpawnProgram { command } => self.spawn_command(&command),
                RuntimeEffect::ExitRequested => self.loop_signal.stop(),
                RuntimeEffect::CloseWindow { window_id } => {
                    if let Some(surface) = self.surface_for_window_id(window_id) {
                        self.capture_close_snapshot(&surface);
                        if let Some(record) = self.managed_window_for_surface(&surface) {
                            if let Some(toplevel) = record.window.toplevel() {
                                toplevel.send_close();
                            }
                        }
                    }
                }
                RuntimeEffect::FocusWindow { window_id } => {
                    let surface = window_id.and_then(|id| self.surface_for_window_id(id));
                    self.set_focus_with_new_serial(surface);
                }
                RuntimeEffect::RaiseWindow { window_id } => {
                    if let Some(record) = self.managed_window_for_id(&window_id) {
                        self.raise_window_element(&record.window);
                    }
                }
                RuntimeEffect::RelayoutRequested => self.schedule_relayout(),
                RuntimeEffect::ConfigReloadRequested => self.reload_config(),
                RuntimeEffect::PublishEvent(event) => self.broadcast_ipc_event(event),
            }
        }
    }
}
```

Why this is a good fit for `apps/spiders-wm`:

- smithay details stay in the app
- frame-sync and snapshot overlays stay in the app
- the runtime decides policy, but not Wayland timing

## Minimal Usage: `apps/spiders-wm-www` Without Host Trait

This is the same runtime boundary, but the web shell chooses different effect semantics.

```rust
use spiders_wm_runtime::{RuntimeEffect, RuntimeInput};

impl PreviewSessionState {
    pub fn apply_command(&mut self, command: WmCommand) {
        let output = self.runtime.apply(RuntimeInput::Command(command));

        for effect in output.effects {
            match effect {
                RuntimeEffect::SpawnProgram { command } => {
                    self.push_log(format!("spawn ignored in preview: {command}"));
                }
                RuntimeEffect::ExitRequested => {
                    self.push_log("quit ignored in web preview".to_string());
                }
                RuntimeEffect::CloseWindow { window_id } => {
                    self.remove_preview_window(&window_id);
                }
                RuntimeEffect::FocusWindow { window_id } => {
                    self.set_preview_focus(window_id);
                }
                RuntimeEffect::RaiseWindow { .. } => {}
                RuntimeEffect::RelayoutRequested => self.recompute_preview_layout(),
                RuntimeEffect::ConfigReloadRequested => self.push_log("reload requested".to_string()),
                RuntimeEffect::PublishEvent(_) => {}
            }
        }
    }
}
```

Why this is a good fit for `apps/spiders-wm-www`:

- preview can ignore or reinterpret real-host effects
- browser-only logging and recompute policy stays in the web shell
- the runtime still owns command reduction and shared state invariants

## Minimal Usage: `apps/spiders-wm` With Host Trait

If the compositor wants thinner call sites, wrap app behavior in a host adapter.

```rust
use spiders_wm_runtime::{RuntimeHost, RuntimeInput};

struct WmHost<'a> {
    wm: &'a mut SpidersWm,
}

impl RuntimeHost for WmHost<'_> {
    fn spawn_program(&mut self, command: &str) {
        self.wm.spawn_command(command);
    }

    fn request_exit(&mut self) {
        self.wm.loop_signal.stop();
    }

    fn request_close_window(&mut self, window_id: &WindowId) {
        if let Some(surface) = self.wm.surface_for_window_id(window_id.clone()) {
            self.wm.capture_close_snapshot(&surface);
            if let Some(record) = self.wm.managed_window_for_surface(&surface) {
                if let Some(toplevel) = record.window.toplevel() {
                    toplevel.send_close();
                }
            }
        }
    }

    fn focus_window(&mut self, window_id: Option<&WindowId>) {
        let surface = window_id.cloned().and_then(|id| self.wm.surface_for_window_id(id));
        self.wm.set_focus_with_new_serial(surface);
    }

    fn raise_window(&mut self, window_id: &WindowId) {
        if let Some(record) = self.wm.managed_window_for_id(window_id) {
            self.wm.raise_window_element(&record.window);
        }
    }

    fn request_relayout(&mut self) {
        self.wm.schedule_relayout();
    }

    fn request_config_reload(&mut self) {
        self.wm.reload_config();
    }

    fn publish_event(&mut self, event: &spiders_core::api::CompositorEvent) {
        self.wm.broadcast_ipc_event(event.clone());
    }
}

impl SpidersWm {
    pub fn execute_wm_command(&mut self, command: WmCommand) {
        let mut host = WmHost { wm: self };
        host.wm.runtime.apply_with_host(RuntimeInput::Command(command), &mut host);
    }
}
```

This is thinner, but the effect-only API still exists underneath it for tests and more specialized flows.

## Minimal Usage: `apps/spiders-wm-www` With Host Trait

The web shell can do the same thing with preview-specific semantics.

```rust
use spiders_wm_runtime::{RuntimeHost, RuntimeInput};

struct WebPreviewHost<'a> {
    session: &'a mut PreviewSessionState,
}

impl RuntimeHost for WebPreviewHost<'_> {
    fn spawn_program(&mut self, command: &str) {
        self.session
            .push_log(format!("spawn ignored in preview: {command}"));
    }

    fn request_exit(&mut self) {
        self.session
            .push_log("quit ignored in web preview".to_string());
    }

    fn request_close_window(&mut self, window_id: &WindowId) {
        self.session.remove_preview_window(window_id);
    }

    fn focus_window(&mut self, window_id: Option<&WindowId>) {
        self.session.set_preview_focus(window_id.cloned());
    }

    fn raise_window(&mut self, _window_id: &WindowId) {}

    fn request_relayout(&mut self) {
        self.session.recompute_preview_layout();
    }

    fn request_config_reload(&mut self) {
        self.session.push_log("reload requested".to_string());
    }

    fn publish_event(&mut self, _event: &spiders_core::api::CompositorEvent) {}
}

impl PreviewSessionState {
    pub fn apply_command(&mut self, command: WmCommand) {
        let mut host = WebPreviewHost { session: self };
        host.session
            .runtime
            .apply_with_host(RuntimeInput::Command(command), &mut host);
    }
}
```

## Recommendation For This Repo

Use this layering:

1. Build `RuntimeInput -> RuntimeOutput<RuntimeEffect>` first.
2. Keep `RuntimeHost` as an optional adapter over emitted effects.
3. Move command reduction and lifecycle/focus policy into the runtime.
4. Keep smithay transactions, browser logging, preview relayout timing, and JS evaluation in the apps.

That gives `apps/spiders-wm` and `apps/spiders-wm-www` a shared runtime brain without forcing the wrong abstraction too early.

## Immediate Refactor Target

The first code move implied by this sketch is:

- extract the real command reducer from `apps/spiders-wm/src/runtime/command.rs` into `crates/spiders-wm-runtime`
- keep `apps/spiders-wm` responsible only for translating `RuntimeEffect` into smithay work
- keep `apps/spiders-wm-www` responsible only for translating `RuntimeEffect` into preview/web behavior

That is the smallest step that materially improves the boundary.
