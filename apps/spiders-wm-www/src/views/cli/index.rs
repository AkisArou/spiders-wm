use std::cell::RefCell;
use std::rc::Rc;

use leptos::html;
use leptos::prelude::*;
use spiders_cli_core::{
    CliConfigCommand, CliTopLevelCommand, CliWmCommand, complete_tokens, parse_cli_tokens,
};
use wasm_bindgen_futures::spawn_local;

use crate::app_state::{AppState, load_config_from_buffers};
use crate::browser_ipc;
use crate::components::{Panel, PanelBar};
use crate::editor_files::{EditorFileId, WORKSPACE_FS_ROOT, file_by_id};
use spiders_ipc_core::{
    DebugRequest, IpcClientMessage, IpcEnvelope, IpcResponse, IpcServerMessage,
};

mod wasm {
    use js_sys::Function;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "/src/xterm_host_bundle.js")]
    extern "C" {
        #[wasm_bindgen(catch, js_name = createXtermTerminal)]
        fn create_xterm_terminal_js(
            host: &web_sys::HtmlElement,
            on_command: &Function,
            on_tab_complete: &Function,
        ) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch, js_name = writeXtermLines)]
        fn write_xterm_lines_js(handle: &JsValue, lines: JsValue) -> Result<(), JsValue>;

        #[wasm_bindgen(catch, js_name = clearXtermTerminal)]
        fn clear_xterm_terminal_js(handle: &JsValue) -> Result<(), JsValue>;

        #[wasm_bindgen(catch, js_name = disposeXtermTerminal)]
        fn dispose_xterm_terminal_js(handle: &JsValue) -> Result<(), JsValue>;

        #[wasm_bindgen(catch, js_name = replaceXtermInput)]
        fn replace_xterm_input_js(handle: &JsValue, input: &str) -> Result<(), JsValue>;

    }

    pub struct XtermHandle {
        handle: JsValue,
        _command_callback: Closure<dyn Fn(String)>,
        _tab_complete_callback: Closure<dyn Fn(String)>,
    }

    impl XtermHandle {
        pub fn write_lines(&self, lines: &[String]) -> Result<(), String> {
            write_xterm_lines_js(
                &self.handle,
                serde_wasm_bindgen::to_value(lines).map_err(|error| error.to_string())?,
            )
            .map_err(js_error_message)
        }

        pub fn clear(&self) -> Result<(), String> {
            clear_xterm_terminal_js(&self.handle).map_err(js_error_message)
        }

        pub fn replace_input(&self, input: &str) -> Result<(), String> {
            replace_xterm_input_js(&self.handle, input).map_err(js_error_message)
        }

    }

    impl Drop for XtermHandle {
        fn drop(&mut self) {
            let _ = dispose_xterm_terminal_js(&self.handle);
        }
    }

    pub fn mount_xterm_terminal(
        host: web_sys::HtmlElement,
        on_command: impl Fn(String) + 'static,
        on_tab_complete: impl Fn(String) + 'static,
    ) -> Result<XtermHandle, String> {
        let command_callback = Closure::wrap(Box::new(on_command) as Box<dyn Fn(String)>);
        let tab_complete_callback =
            Closure::wrap(Box::new(on_tab_complete) as Box<dyn Fn(String)>);
        let handle = create_xterm_terminal_js(
            &host,
            command_callback.as_ref().unchecked_ref(),
            tab_complete_callback.as_ref().unchecked_ref(),
        )
        .map_err(js_error_message)?;
        Ok(XtermHandle {
            handle,
            _command_callback: command_callback,
            _tab_complete_callback: tab_complete_callback,
        })
    }

    fn js_error_message(error: JsValue) -> String {
        error.as_string().unwrap_or_else(|| "xterm bridge failed".to_string())
    }
}

use wasm::{XtermHandle, mount_xterm_terminal};

#[component]
pub fn CliView() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let terminal_mount = NodeRef::<html::Div>::new();
    let terminal_error = RwSignal::new(None::<String>);
    let transcript = RwSignal::new(Vec::<String>::new());
    let suggestions = RwSignal::new(Vec::<String>::new());
    let terminal_handle = Rc::new(RefCell::new(None::<XtermHandle>));

    Effect::new({
        let terminal_mount = terminal_mount.clone();
        let terminal_handle = Rc::clone(&terminal_handle);
        move |_| {
            let Some(host) = terminal_mount.get() else {
                return;
            };
            if terminal_handle.borrow().is_some() {
                return;
            }

            let transcript_signal = transcript;
            let terminal_handle_for_complete = Rc::clone(&terminal_handle);
            let suggestions_signal = suggestions;
            let app_state = app_state;
            match mount_xterm_terminal(
                host.into(),
                move |command| {
                    run_terminal_command(app_state, transcript_signal, suggestions_signal, command);
                },
                move |input| {
                    handle_tab_completion(
                        Rc::clone(&terminal_handle_for_complete),
                        transcript_signal,
                        suggestions_signal,
                        input,
                    );
                },
            ) {
                Ok(handle) => {
                    *terminal_handle.borrow_mut() = Some(handle);
                    terminal_error.set(None);
                }
                Err(error) => terminal_error.set(Some(error)),
            }
        }
    });

    Effect::new({
        let terminal_handle = Rc::clone(&terminal_handle);
        move |_| {
            let lines = transcript.get();
            let terminal_binding = terminal_handle.borrow();
            let Some(handle) = terminal_binding.as_ref() else {
                return;
            };

            if let Err(error) = handle.clear().and_then(|_| handle.write_lines(&lines)) {
                terminal_error.set(Some(error));
            }
        }
    });

    Effect::new({
        move |_| {
            let transcript_signal = transcript;
            let subscription = browser_ipc::subscribe_system_events(move |response| {
                transcript_signal.update(|lines| {
                    lines.push(format_ipc_response("event", &response));
                });
            });
            std::mem::forget(subscription);
        }
    });

    view! {
        <section class="grid grid-cols-1 gap-2 w-full min-w-0 h-full min-h-0 xl:grid-cols-[minmax(0,1.5fr)_20rem]">
            <Panel>
                <PanelBar>
                    <div>"cli://terminal"</div>
                </PanelBar>
                <Show
                    when=move || terminal_error.get().is_none()
                    fallback=move || {
                        view! {
                            <div class="p-3 text-sm text-terminal-error">
                                {move || terminal_error.get().unwrap_or_else(|| "xterm unavailable".to_string())}
                            </div>
                        }
                    }
                >
                    <div node_ref=terminal_mount class="h-full min-h-[24rem] w-full bg-terminal-bg-panel" />
                </Show>
            </Panel>

            <div class="grid gap-2 min-h-0 xl:grid-rows-[auto_minmax(0,1fr)]">
                <Panel>
                    <PanelBar>
                        <div>"cli://commands"</div>
                    </PanelBar>
                    <div class="grid gap-2 p-2 text-sm text-terminal-muted">
                        <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1">
                            "Commands: config discover | config check | wm query state | wm monitor focus | wm command close-focused-window | wm debug dump wm-state | completions zsh"
                        </div>
                        <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1">
                            "Tab completes from shared cli metadata. Suggestions below are powered by spiders-cli-core."
                        </div>
                    </div>
                </Panel>

                <Panel>
                    <PanelBar>
                        <div>"cli://suggestions"</div>
                    </PanelBar>
                    <div class="overflow-auto p-2 min-h-0 text-sm text-terminal-muted">
                        <div class="grid gap-1">
                            {move || {
                                suggestions
                                    .get()
                                    .into_iter()
                                    .map(|line| {
                                        view! {
                                            <div class="border border-terminal-border bg-terminal-bg-panel px-2 py-1">
                                                {line}
                                            </div>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </div>
                    </div>
                </Panel>
            </div>
        </section>
    }
}

fn run_terminal_command(
    app_state: AppState,
    transcript: RwSignal<Vec<String>>,
    suggestions: RwSignal<Vec<String>>,
    command: String,
) {
    transcript.update(|lines| lines.push(format!("> {command}")));
    suggestions.set(Vec::new());

    if command.trim() == "clear" {
        transcript.set(Vec::new());
        return;
    }

    if command.trim() == "help" {
        transcript.update(|lines| {
            lines.push("config discover".to_string());
            lines.push("config check".to_string());
            lines.push("config build".to_string());
            lines.push("wm query <state|focused-window|current-output|current-workspace|monitor-list|workspace-names>".to_string());
            lines.push("wm command <close-focused-window|toggle-floating|toggle-fullscreen|reload-config|cycle-layout-next|...>".to_string());
            lines.push("wm monitor [all|focus|windows|workspaces|layout|config]".to_string());
            lines.push("wm debug dump <wm-state|debug-profile|scene-snapshot|frame-sync|seats>".to_string());
            lines.push("wm smoke".to_string());
            lines.push("completions <zsh|bash|fish>".to_string());
        });
        return;
    }

    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let parsed = match parse_cli_tokens(&tokens) {
        Ok(parsed) => parsed,
        Err(error) => {
            transcript.update(|lines| lines.push(format!("< parse error: {error:?}")));
            suggestions.set(
                complete_tokens(&tokens)
                    .into_iter()
                    .map(|candidate| format!("{}: {}", candidate.value, candidate.help))
                    .collect(),
            );
            return;
        }
    };

    let request = match parsed.command {
        CliTopLevelCommand::Config(config) => {
            run_browser_config_command(app_state, transcript, config);
            None
        }
        CliTopLevelCommand::Completions { shell } => {
            transcript.update(|lines| {
                lines.push(format!(
                    "< completions {shell:?}: use checked-in files under crates/cli/completions/ or native spiders-cli completions {shell:?}"
                ));
            });
            None
        }
        CliTopLevelCommand::Wm(CliWmCommand::Query { query }) => Some((
            format!("wm query {}", query.name()),
            IpcEnvelope::new(IpcClientMessage::Query(query.to_runtime())),
        )),
        CliTopLevelCommand::Wm(CliWmCommand::Command { command }) => Some((
            format!("wm command {}", format_runtime_command(&command)),
            IpcEnvelope::new(IpcClientMessage::Command(command)),
        )),
        CliTopLevelCommand::Wm(CliWmCommand::Monitor { topics }) => Some((
            "wm monitor".to_string(),
            IpcEnvelope::new(IpcClientMessage::subscribe(
                if topics.is_empty() {
                    vec![spiders_cli_core::CliTopic::All.to_runtime()]
                } else {
                    topics.into_iter().map(spiders_cli_core::CliTopic::to_runtime).collect()
                },
            )),
        )),
        CliTopLevelCommand::Wm(CliWmCommand::DebugDump { kind }) => Some((
            format!("wm debug dump {}", kind.name()),
            IpcEnvelope::new(IpcClientMessage::Debug(DebugRequest::Dump { kind: kind.to_runtime() })),
        )),
        CliTopLevelCommand::Wm(CliWmCommand::Smoke) => {
            transcript.update(|lines| {
                lines.push("< wm smoke is native-cli only right now; use wm query/command/monitor in browser".to_string());
            });
            None
        }
    };

    let Some((label, request)) = request else {
        return;
    };

    let client = browser_ipc::system_client();
    spawn_local(async move {
        match client.request(request) {
            Ok(future) => match future.await {
                Ok(response) => transcript.update(|lines| {
                    lines.push(format_ipc_response(&label, &response));
                }),
                Err(error) => transcript.update(|lines| {
                    lines.push(format!("< {label}: error {error:?}"));
                }),
            },
            Err(error) => transcript.update(|lines| {
                lines.push(format!("< {label}: error {error:?}"));
            }),
        }
    });
}

fn handle_tab_completion(
    terminal_handle: Rc<RefCell<Option<XtermHandle>>>,
    transcript: RwSignal<Vec<String>>,
    suggestions: RwSignal<Vec<String>>,
    input: String,
) {
    let ends_with_space = input.ends_with(' ');
    let mut tokens = input.split_whitespace().collect::<Vec<_>>();
    if ends_with_space || input.is_empty() {
        tokens.push("");
    }

    let candidates = complete_tokens(&tokens);
    suggestions.set(
        candidates
            .iter()
            .map(|candidate| format!("{}: {}", candidate.value, candidate.help))
            .collect(),
    );

    if candidates.len() == 1 {
        let mut next_tokens = input.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        if input.trim().is_empty() {
            next_tokens = vec![candidates[0].value.clone()];
        } else if ends_with_space {
            next_tokens.push(candidates[0].value.clone());
        } else if let Some(last) = next_tokens.last_mut() {
            *last = candidates[0].value.clone();
        }
        let completed = format!("{} ", next_tokens.join(" "));

        if let Some(handle) = terminal_handle.borrow().as_ref()
            && let Err(error) = handle.replace_input(&completed)
        {
            transcript.update(|lines| lines.push(format!("< completion error: {error}")));
        }
    }
}

fn run_browser_config_command(
    app_state: AppState,
    transcript: RwSignal<Vec<String>>,
    command: CliConfigCommand,
) {
    match command {
        CliConfigCommand::Discover => {
            let authored = file_by_id(EditorFileId::Config).path;
            let prepared = format!("{WORKSPACE_FS_ROOT}/.cache/config.js");
            transcript.update(|lines| {
                lines.push(format!("< authored config: {authored}"));
                lines.push(format!("< runtime root: {WORKSPACE_FS_ROOT}"));
                lines.push(format!("< prepared config (browser preview): {prepared}"));
            });
        }
        CliConfigCommand::Check => {
            let buffers = app_state.editor_buffers.get_untracked();
            spawn_local(async move {
                match load_config_from_buffers(&buffers).await {
                    Ok(config) => transcript.update(|lines| {
                        lines.push(format!(
                            "< config ok: workspaces={}, layouts={}, bindings={}, autostart={}"
                            ,
                            config.workspaces.len(),
                            config.layouts.len(),
                            config.bindings.len(),
                            config.autostart.len() + config.autostart_once.len(),
                        ));
                    }),
                    Err(error) => transcript.update(|lines| {
                        lines.push(format!("< config error: {error}"));
                    }),
                }
            });
        }
        CliConfigCommand::Build => {
            let buffers = app_state.editor_buffers.get_untracked();
            let app_state_copy = app_state;
            spawn_local(async move {
                match load_config_from_buffers(&buffers).await {
                    Ok(config) => {
                        app_state_copy.apply_loaded_config(config.clone());
                        transcript.update(|lines| {
                            lines.push(format!(
                                "< browser preview config refreshed: layouts={}, workspaces={}, bindings={}"
                                ,
                                config.layouts.len(),
                                config.workspaces.len(),
                                config.bindings.len(),
                            ));
                            lines.push(
                                "< note: browser build refreshes preview/runtime inputs, not a native on-disk prepared cache artifact>".to_string(),
                            );
                        });
                    }
                    Err(error) => {
                        app_state_copy.apply_config_error();
                        transcript.update(|lines| {
                            lines.push(format!("< config build failed: {error}"));
                        });
                    }
                }
            });
        }
    }
}

fn format_runtime_command(command: &spiders_core::command::WmCommand) -> String {
    match command {
        spiders_core::command::WmCommand::Spawn { command } => format!("spawn:{command}"),
        spiders_core::command::WmCommand::SetLayout { name } => format!("set-layout:{name}"),
        spiders_core::command::WmCommand::SelectWorkspace { workspace_id } => {
            format!("select-workspace:{workspace_id}")
        }
        spiders_core::command::WmCommand::ReloadConfig => "reload-config".to_string(),
        spiders_core::command::WmCommand::FocusNextWindow => "focus-next-window".to_string(),
        spiders_core::command::WmCommand::FocusPreviousWindow => {
            "focus-previous-window".to_string()
        }
        spiders_core::command::WmCommand::SelectNextWorkspace => {
            "select-next-workspace".to_string()
        }
        spiders_core::command::WmCommand::SelectPreviousWorkspace => {
            "select-previous-workspace".to_string()
        }
        spiders_core::command::WmCommand::CycleLayout { direction } => match direction {
            Some(spiders_core::command::LayoutCycleDirection::Next) => {
                "cycle-layout-next".to_string()
            }
            Some(spiders_core::command::LayoutCycleDirection::Previous) => {
                "cycle-layout-previous".to_string()
            }
            None => "cycle-layout".to_string(),
        },
        spiders_core::command::WmCommand::ToggleFloating => "toggle-floating".to_string(),
        spiders_core::command::WmCommand::ToggleFullscreen => "toggle-fullscreen".to_string(),
        spiders_core::command::WmCommand::FocusDirection { direction } => match direction {
            spiders_core::command::FocusDirection::Left => "focus-left".to_string(),
            spiders_core::command::FocusDirection::Right => "focus-right".to_string(),
            spiders_core::command::FocusDirection::Up => "focus-up".to_string(),
            spiders_core::command::FocusDirection::Down => "focus-down".to_string(),
        },
        spiders_core::command::WmCommand::CloseFocusedWindow => "close-focused-window".to_string(),
        _ => format!("{command:?}"),
    }
}

fn format_ipc_response(label: &str, response: &IpcResponse) -> String {
    match &response.message {
        IpcServerMessage::Query(query) => format!("< {label}: {query:?}"),
        IpcServerMessage::Debug(debug) => format!("< {label}: {debug:?}"),
        IpcServerMessage::Event { topics, event } => format!("< {label}: event {topics:?} {event:?}"),
        IpcServerMessage::CommandAccepted => format!("< {label}: command accepted"),
        IpcServerMessage::Subscribed { topics } => format!("< {label}: subscribed {topics:?}"),
        IpcServerMessage::Unsubscribed { topics } => format!("< {label}: unsubscribed {topics:?}"),
        IpcServerMessage::Error { message } => format!("< {label}: error {message}"),
    }
}
