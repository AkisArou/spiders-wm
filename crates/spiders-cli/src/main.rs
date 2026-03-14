mod bootstrap;
mod report;

use bootstrap::CliBootstrap;
use report::{
    emit, BootstrapFailureReport, BootstrapReport, DiscoveryReport, ErrorReport, IpcActionReport,
    IpcMonitorReport, IpcQueryReport, IpcSmokeReport, OutputMode, SuccessCheckReport,
};

#[derive(Debug, Clone)]
struct CliContext {
    ready: bool,
    options: spiders_config::model::ConfigDiscoveryOptions,
}

impl CliContext {
    fn new() -> Self {
        Self {
            ready: spiders_config::crate_ready(),
            options: spiders_config::model::ConfigDiscoveryOptions {
                home_dir: std::env::var_os("SPIDERS_WM_HOME").map(std::path::PathBuf::from),
                config_dir_override: std::env::var_os("SPIDERS_WM_CONFIG_DIR")
                    .map(std::path::PathBuf::from),
                data_dir_override: std::env::var_os("SPIDERS_WM_DATA_DIR")
                    .map(std::path::PathBuf::from),
                authored_config_override: std::env::var_os("SPIDERS_WM_AUTHORED_CONFIG")
                    .map(std::path::PathBuf::from),
                runtime_config_override: std::env::var_os("SPIDERS_WM_RUNTIME_CONFIG")
                    .map(std::path::PathBuf::from),
            },
        }
    }

    fn bootstrap(&self) -> Result<CliBootstrap, spiders_config::model::LayoutConfigError> {
        bootstrap::build_bootstrap(self.options.clone())
    }
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let check_config = args.iter().any(|arg| arg == "check-config");
    let bootstrap_trace = args.iter().any(|arg| arg == "bootstrap-trace");
    let ipc_smoke = args.iter().any(|arg| arg == "ipc-smoke");
    let ipc_query = args.iter().any(|arg| arg == "ipc-query");
    let ipc_action = args.iter().any(|arg| arg == "ipc-action");
    let ipc_monitor = args.iter().any(|arg| arg == "ipc-monitor");
    let output_mode = if args.iter().any(|arg| arg == "--json") {
        OutputMode::Json
    } else {
        OutputMode::Text
    };
    let events_path = arg_value(&args, "--events");
    let transcript_path = arg_value(&args, "--transcript");
    let socket_path = arg_value(&args, "--socket")
        .map(std::path::PathBuf::from)
        .or_else(default_ipc_socket_path);
    let query_name = arg_value(&args, "--query");
    let action_name = arg_value(&args, "--action");
    let topic_names = arg_values(&args, "--topic");

    let cli = CliContext::new();

    if ipc_smoke {
        ipc_smoke_command(output_mode)
    } else if ipc_query {
        ipc_query_command(output_mode, socket_path, query_name)
    } else if ipc_action {
        ipc_action_command(output_mode, socket_path, action_name)
    } else if ipc_monitor {
        ipc_monitor_command(output_mode, socket_path, topic_names)
    } else if bootstrap_trace {
        bootstrap_trace_command(&cli, output_mode, events_path, transcript_path)
    } else if check_config {
        check_config_command(&cli, output_mode)
    } else {
        print_discovery(&cli, output_mode)
    }
}

fn ipc_monitor_command(
    output_mode: OutputMode,
    socket_path: Option<std::path::PathBuf>,
    topic_names: Vec<&str>,
) -> std::process::ExitCode {
    match run_ipc_monitor(socket_path, topic_names) {
        Ok(report) => {
            emit(output_mode, &report, || {
                format!(
                    "ipc monitor ok (socket: {}, topics: {}, events: {})",
                    report.socket_path,
                    report.topics.join(","),
                    report.events.len()
                )
            });
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "ipc-monitor",
                    runtime_ready: spiders_ipc::crate_ready(),
                    runtime_config: None,
                    errors: None,
                    message: Some(error),
                },
                || "ipc monitor error".into(),
            );
            std::process::ExitCode::from(1)
        }
    }
}

fn ipc_query_command(
    output_mode: OutputMode,
    socket_path: Option<std::path::PathBuf>,
    query_name: Option<&str>,
) -> std::process::ExitCode {
    match run_ipc_query(socket_path, query_name.unwrap_or("state")) {
        Ok(report) => {
            emit(output_mode, &report, || {
                format!(
                    "ipc query ok (socket: {}, query: {}, request: {})",
                    report.socket_path,
                    query_label(&report.query),
                    report.request_id
                )
            });
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "ipc-query",
                    runtime_ready: spiders_ipc::crate_ready(),
                    runtime_config: None,
                    errors: None,
                    message: Some(error),
                },
                || "ipc query error".into(),
            );
            std::process::ExitCode::from(1)
        }
    }
}

fn ipc_action_command(
    output_mode: OutputMode,
    socket_path: Option<std::path::PathBuf>,
    action_name: Option<&str>,
) -> std::process::ExitCode {
    match run_ipc_action(socket_path, action_name.unwrap_or("reload-config")) {
        Ok(report) => {
            emit(output_mode, &report, || {
                format!(
                    "ipc action ok (socket: {}, action: {}, request: {})",
                    report.socket_path,
                    action_label(&report.action),
                    report.request_id
                )
            });
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "ipc-action",
                    runtime_ready: spiders_ipc::crate_ready(),
                    runtime_config: None,
                    errors: None,
                    message: Some(error),
                },
                || "ipc action error".into(),
            );
            std::process::ExitCode::from(1)
        }
    }
}

fn ipc_smoke_command(output_mode: OutputMode) -> std::process::ExitCode {
    match run_ipc_smoke() {
        Ok(report) => {
            let event_suffix = report
                .event_line
                .as_ref()
                .map(|_| ", event: yes")
                .unwrap_or("");
            emit(output_mode, &report, || {
                format!(
                    "ipc smoke ok (client: {}, request: {}, response: {}{})",
                    report.client_id, report.request_kind, report.response_kind, event_suffix
                )
            });
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "ipc-smoke",
                    runtime_ready: spiders_ipc::crate_ready(),
                    runtime_config: None,
                    errors: None,
                    message: Some(error.to_string()),
                },
                || format!("ipc smoke error: {error}"),
            );
            std::process::ExitCode::from(1)
        }
    }
}

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
}

fn arg_values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    args.windows(2)
        .filter(|window| window[0] == flag)
        .map(|window| window[1].as_str())
        .collect()
}

fn print_discovery(cli: &CliContext, output_mode: OutputMode) -> std::process::ExitCode {
    match cli.bootstrap() {
        Ok(bootstrap) => {
            emit(
                output_mode,
                &DiscoveryReport {
                    status: "ok",
                    runtime_ready: cli.ready,
                    authored_config: bootstrap.paths.authored_config.display().to_string(),
                    runtime_config: bootstrap.paths.runtime_config.display().to_string(),
                },
                || {
                    format!(
                        "spiders-cli placeholder (config runtime ready: {}, authored: {}, runtime: {})",
                        cli.ready,
                        bootstrap.paths.authored_config.display(),
                        bootstrap.paths.runtime_config.display()
                    )
                },
            );
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "discovery",
                    runtime_ready: cli.ready,
                    runtime_config: None,
                    errors: None,
                    message: Some(error.to_string()),
                },
                || {
                    format!("spiders-cli placeholder (config runtime ready: {}, discovery error: {error})", cli.ready)
                },
            );
            std::process::ExitCode::from(1)
        }
    }
}

fn check_config_command(cli: &CliContext, output_mode: OutputMode) -> std::process::ExitCode {
    let bootstrap = match cli.bootstrap() {
        Ok(bootstrap) => bootstrap,
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "discovery",
                    runtime_ready: cli.ready,
                    runtime_config: None,
                    errors: None,
                    message: Some(error.to_string()),
                },
                || {
                    format!(
                        "config error (runtime ready: {}, discovery): {error}",
                        cli.ready
                    )
                },
            );
            return std::process::ExitCode::from(1);
        }
    };

    match bootstrap.service.load_config(&bootstrap.paths) {
        Ok(config) => match bootstrap.service.validate_layout_modules(&config) {
            Ok(errors) if errors.is_empty() => {
                emit(
                    output_mode,
                    &SuccessCheckReport {
                        status: "ok",
                        runtime_ready: cli.ready,
                        layouts: config.layouts.len(),
                        runtime_config: bootstrap.paths.runtime_config.display().to_string(),
                    },
                    || {
                        format!(
                            "config ok (runtime ready: {}, layouts: {}, runtime: {})",
                            cli.ready,
                            config.layouts.len(),
                            bootstrap.paths.runtime_config.display()
                        )
                    },
                );
                std::process::ExitCode::SUCCESS
            }
            Ok(errors) => {
                emit(
                    output_mode,
                    &ErrorReport {
                        status: "error",
                        phase: "validation",
                        runtime_ready: cli.ready,
                        runtime_config: Some(bootstrap.paths.runtime_config.display().to_string()),
                        errors: Some(errors.clone()),
                        message: None,
                    },
                    || {
                        format!(
                            "config error (runtime ready: {}, runtime: {}): {}",
                            cli.ready,
                            bootstrap.paths.runtime_config.display(),
                            errors.join("; ")
                        )
                    },
                );
                std::process::ExitCode::from(1)
            }
            Err(error) => {
                emit(
                    output_mode,
                    &ErrorReport {
                        status: "error",
                        phase: "validation",
                        runtime_ready: cli.ready,
                        runtime_config: None,
                        errors: None,
                        message: Some(error.to_string()),
                    },
                    || {
                        format!(
                            "config error (runtime ready: {}, validation): {error}",
                            cli.ready
                        )
                    },
                );
                std::process::ExitCode::from(1)
            }
        },
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "load",
                    runtime_ready: cli.ready,
                    runtime_config: Some(bootstrap.paths.runtime_config.display().to_string()),
                    errors: None,
                    message: Some(error.to_string()),
                },
                || {
                    format!(
                        "config error (runtime ready: {}, runtime: {}): {error}",
                        cli.ready,
                        bootstrap.paths.runtime_config.display()
                    )
                },
            );
            std::process::ExitCode::from(1)
        }
    }
}

fn bootstrap_trace_command(
    cli: &CliContext,
    output_mode: OutputMode,
    events_path: Option<&str>,
    transcript_path: Option<&str>,
) -> std::process::ExitCode {
    let bootstrap = match cli.bootstrap() {
        Ok(bootstrap) => bootstrap,
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "discovery",
                    runtime_ready: cli.ready,
                    runtime_config: None,
                    errors: None,
                    message: Some(error.to_string()),
                },
                || format!("bootstrap trace discovery error: {error}"),
            );
            return std::process::ExitCode::from(1);
        }
    };

    let config = match bootstrap.service.load_config(&bootstrap.paths) {
        Ok(config) => config,
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "load",
                    runtime_ready: cli.ready,
                    runtime_config: Some(bootstrap.paths.runtime_config.display().to_string()),
                    errors: None,
                    message: Some(error.to_string()),
                },
                || format!("bootstrap trace config load error: {error}"),
            );
            return std::process::ExitCode::from(1);
        }
    };

    let authored_config = bootstrap.paths.authored_config.display().to_string();
    let runtime_config = bootstrap.paths.runtime_config.display().to_string();

    if let Some(script_path) = transcript_path.or(events_path) {
        let contents = match std::fs::read_to_string(script_path) {
            Ok(contents) => contents,
            Err(error) => {
                emit(
                    output_mode,
                    &ErrorReport {
                        status: "error",
                        phase: "script",
                        runtime_ready: cli.ready,
                        runtime_config: Some(runtime_config.clone()),
                        errors: None,
                        message: Some(error.to_string()),
                    },
                    || format!("bootstrap trace script read error: {error}"),
                );
                return std::process::ExitCode::from(1);
            }
        };

        let script = match spiders_compositor::BootstrapScript::from_json_str(&contents) {
            Ok(script) => script,
            Err(error) => {
                emit(
                    output_mode,
                    &ErrorReport {
                        status: "error",
                        phase: "script",
                        runtime_ready: cli.ready,
                        runtime_config: Some(runtime_config.clone()),
                        errors: None,
                        message: Some(error.to_string()),
                    },
                    || format!("bootstrap trace script parse error: {error}"),
                );
                return std::process::ExitCode::from(1);
            }
        };

        let state = synthetic_bootstrap_state();
        let mut controller = match spiders_compositor::CompositorController::initialize_with_script(
            bootstrap.service,
            config,
            state,
            &script,
        ) {
            Ok(controller) => controller,
            Err(error) => {
                emit(
                    output_mode,
                    &ErrorReport {
                        status: "error",
                        phase: "bootstrap",
                        runtime_ready: cli.ready,
                        runtime_config: Some(runtime_config.clone()),
                        errors: None,
                        message: Some(error.to_string()),
                    },
                    || format!("bootstrap trace runner error: {error}"),
                );
                return std::process::ExitCode::from(1);
            }
        };

        if let Err(trace) = controller.apply_bootstrap_script(script) {
            emit(
                output_mode,
                &BootstrapFailureReport {
                    status: "error",
                    runtime_ready: cli.ready,
                    authored_config: authored_config.clone(),
                    runtime_config: runtime_config.clone(),
                    controller_phase: controller.phase(),
                    error: trace.error.clone(),
                    failed_event: trace.failed_event,
                    applied_events: trace.applied_events.len(),
                    diagnostics: trace.diagnostics,
                },
                || {
                    format!(
                        "bootstrap trace failed after {} events: {}",
                        trace.applied_events.len(),
                        trace.error
                    )
                },
            );
            return std::process::ExitCode::from(1);
        }

        return emit_bootstrap_trace(
            cli,
            output_mode,
            authored_config,
            runtime_config,
            controller.report(),
        );
    }

    let state = synthetic_bootstrap_state();
    let controller = match spiders_compositor::CompositorController::initialize(
        bootstrap.service,
        config,
        state,
    ) {
        Ok(controller) => controller,
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "bootstrap",
                    runtime_ready: cli.ready,
                    runtime_config: Some(runtime_config.clone()),
                    errors: None,
                    message: Some(error.to_string()),
                },
                || format!("bootstrap trace runner error: {error}"),
            );
            return std::process::ExitCode::from(1);
        }
    };

    emit_bootstrap_trace(
        cli,
        output_mode,
        authored_config,
        runtime_config,
        controller.report(),
    )
}

fn emit_bootstrap_trace(
    cli: &CliContext,
    output_mode: OutputMode,
    authored_config: String,
    runtime_config: String,
    report: spiders_compositor::ControllerReport,
) -> std::process::ExitCode {
    let current_workspace = report.diagnostics.current_workspace.clone();
    let focused_window = report.diagnostics.focused_window.clone();
    emit(
        output_mode,
        &BootstrapReport {
            status: "ok",
            runtime_ready: cli.ready,
            authored_config,
            runtime_config,
            controller_phase: report.phase,
            active_seat: report.diagnostics.active_seat,
            active_output: report.diagnostics.active_output.map(|id| id.to_string()),
            current_workspace: current_workspace.clone(),
            focused_window: focused_window.clone(),
            seat_names: report.diagnostics.seat_names,
            output_ids: report.diagnostics.output_ids,
            surface_ids: report.diagnostics.surface_ids,
            mapped_surface_ids: report.diagnostics.mapped_surface_ids,
            seat_count: report.diagnostics.seat_count,
            output_count: report.diagnostics.output_count,
            surface_count: report.diagnostics.surface_count,
            mapped_surface_count: report.diagnostics.mapped_surface_count,
            applied_events: report.applied_events,
            startup: report.startup,
        },
        || {
            format!(
                "bootstrap trace ok (workspace: {}, focused: {}, seats: {}, outputs: {}, surfaces: {}, mapped: {})",
                current_workspace.as_deref().unwrap_or("none"),
                focused_window.as_deref().unwrap_or("none"),
                report.diagnostics.seat_count,
                report.diagnostics.output_count,
                report.diagnostics.surface_count,
                report.diagnostics.mapped_surface_count
            )
        },
    );

    std::process::ExitCode::SUCCESS
}

fn synthetic_bootstrap_state() -> spiders_shared::wm::StateSnapshot {
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    StateSnapshot {
        focused_window_id: Some(WindowId::from("bootstrap-window")),
        current_output_id: Some(OutputId::from("bootstrap-output")),
        current_workspace_id: Some(WorkspaceId::from("bootstrap-workspace")),
        outputs: vec![OutputSnapshot {
            id: OutputId::from("bootstrap-output"),
            name: "BOOT-1".into(),
            logical_width: 1280,
            logical_height: 720,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: Some(WorkspaceId::from("bootstrap-workspace")),
        }],
        workspaces: vec![WorkspaceSnapshot {
            id: WorkspaceId::from("bootstrap-workspace"),
            name: "bootstrap".into(),
            output_id: Some(OutputId::from("bootstrap-output")),
            active_tags: vec!["bootstrap".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef {
                name: "master-stack".into(),
            }),
        }],
        windows: vec![WindowSnapshot {
            id: WindowId::from("bootstrap-window"),
            shell: ShellKind::XdgToplevel,
            app_id: Some("bootstrap".into()),
            title: Some("Bootstrap".into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            floating: false,
            fullscreen: false,
            focused: true,
            urgent: false,
            output_id: Some(OutputId::from("bootstrap-output")),
            workspace_id: Some(WorkspaceId::from("bootstrap-workspace")),
            tags: vec!["bootstrap".into()],
        }],
        visible_window_ids: vec![WindowId::from("bootstrap-window")],
        tag_names: vec!["bootstrap".into()],
    }
}

fn run_ipc_smoke() -> Result<IpcSmokeReport, String> {
    use spiders_ipc::{
        decode_request_line, encode_request_line, encode_response_line, IpcClientMessage,
        IpcEnvelope, IpcServerHandleResult, IpcServerState, IpcSubscriptionTopic,
    };
    use spiders_shared::api::CompositorEvent;

    let mut server = IpcServerState::new();
    let client_id = server.add_client();
    let state = synthetic_bootstrap_state();

    let request = IpcEnvelope::new(IpcClientMessage::subscribe([IpcSubscriptionTopic::Focus]))
        .with_request_id("smoke-1");
    let request_line = encode_request_line(&request).map_err(|error| error.to_string())?;
    let decoded_request = decode_request_line(&request_line).map_err(|error| error.to_string())?;

    let response = match server
        .handle_request(client_id, decoded_request)
        .map_err(|error| error.to_string())?
    {
        IpcServerHandleResult::Response { response, .. } => response,
        IpcServerHandleResult::Query {
            request_id, query, ..
        } => server
            .query_response(client_id, request_id, fallback_query_response(query))
            .map_err(|error| error.to_string())?,
        IpcServerHandleResult::Action { request_id, .. } => server
            .action_accepted(client_id, request_id)
            .map_err(|error| error.to_string())?,
    };

    let response_line = encode_response_line(&response).map_err(|error| error.to_string())?;
    let event_line = server
        .broadcast_event(CompositorEvent::FocusChange {
            focused_window_id: state.focused_window_id.clone(),
            current_output_id: state.current_output_id.clone(),
            current_workspace_id: state.current_workspace_id.clone(),
        })
        .into_iter()
        .find(|(id, _)| *id == client_id)
        .map(|(_, response)| encode_response_line(&response).map_err(|error| error.to_string()))
        .transpose()?;

    Ok(IpcSmokeReport {
        status: "ok",
        client_id,
        request_kind: "subscribe",
        response_kind: match response.message {
            spiders_ipc::IpcServerMessage::Subscribed { .. } => "subscribed",
            spiders_ipc::IpcServerMessage::Query(_) => "query",
            spiders_ipc::IpcServerMessage::ActionAccepted => "action-accepted",
            spiders_ipc::IpcServerMessage::Event { .. } => "event",
            spiders_ipc::IpcServerMessage::Unsubscribed { .. } => "unsubscribed",
            spiders_ipc::IpcServerMessage::Error { .. } => "error",
        },
        request_line,
        response_line,
        event_line,
    })
}

fn run_ipc_query(
    socket_path: Option<std::path::PathBuf>,
    query_name: &str,
) -> Result<IpcQueryReport, String> {
    use spiders_ipc::{connect, recv_response, send_request, IpcClientMessage, IpcEnvelope};

    let socket_path = socket_path.ok_or_else(|| "missing IPC socket path".to_string())?;
    let query = parse_query_request(query_name)?;
    let request_id = "cli-query-1".to_string();
    let request = IpcEnvelope::new(IpcClientMessage::Query(query)).with_request_id(&request_id);
    let mut stream = connect(&socket_path).map_err(|error| error.to_string())?;

    send_request(&mut stream, &request).map_err(|error| error.to_string())?;

    match recv_response(&stream)
        .map_err(|error| error.to_string())?
        .message
    {
        spiders_ipc::IpcServerMessage::Query(response) => Ok(IpcQueryReport {
            status: "ok",
            socket_path: socket_path.display().to_string(),
            request_id,
            query,
            response,
        }),
        message => Err(format!("unexpected IPC query response: {message:?}")),
    }
}

fn run_ipc_action(
    socket_path: Option<std::path::PathBuf>,
    action_name: &str,
) -> Result<IpcActionReport, String> {
    use spiders_ipc::{connect, recv_response, send_request, IpcClientMessage, IpcEnvelope};

    let socket_path = socket_path.ok_or_else(|| "missing IPC socket path".to_string())?;
    let action = parse_action_request(action_name)?;
    let request_id = "cli-action-1".to_string();
    let request =
        IpcEnvelope::new(IpcClientMessage::Action(action.clone())).with_request_id(&request_id);
    let mut stream = connect(&socket_path).map_err(|error| error.to_string())?;

    send_request(&mut stream, &request).map_err(|error| error.to_string())?;

    match recv_response(&stream)
        .map_err(|error| error.to_string())?
        .message
    {
        spiders_ipc::IpcServerMessage::ActionAccepted => Ok(IpcActionReport {
            status: "ok",
            socket_path: socket_path.display().to_string(),
            request_id,
            action,
            response_kind: "action-accepted",
        }),
        message => Err(format!("unexpected IPC action response: {message:?}")),
    }
}

fn run_ipc_monitor(
    socket_path: Option<std::path::PathBuf>,
    topic_names: Vec<&str>,
) -> Result<IpcMonitorReport, String> {
    use spiders_ipc::{
        connect, decode_response_line, send_request, IpcClientMessage, IpcEnvelope,
        IpcServerMessage,
    };
    use std::io::BufRead;

    let socket_path = socket_path.ok_or_else(|| "missing IPC socket path".to_string())?;
    let topics = parse_subscription_topics(&topic_names)?;
    let request_id = "cli-monitor-1".to_string();
    let request =
        IpcEnvelope::new(IpcClientMessage::subscribe(topics.clone())).with_request_id(&request_id);
    let mut stream = connect(&socket_path).map_err(|error| error.to_string())?;
    let mut reader =
        std::io::BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);

    send_request(&mut stream, &request).map_err(|error| error.to_string())?;

    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .map_err(|error| error.to_string())?;
    let subscribed = decode_response_line(&first_line).map_err(|error| error.to_string())?;
    let subscribed_topics = match subscribed.message {
        IpcServerMessage::Subscribed { topics } => topics,
        message => return Err(format!("unexpected IPC monitor response: {message:?}")),
    };

    let mut events = Vec::new();

    loop {
        let mut line = String::new();

        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => match decode_response_line(&line)
                .map_err(|error| error.to_string())?
                .message
            {
                IpcServerMessage::Event { event, .. } => events.push(event),
                IpcServerMessage::Error { message } => return Err(message),
                _ => {}
            },
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                break;
            }
            Err(error) => return Err(error.to_string()),
        }
    }

    Ok(IpcMonitorReport {
        status: "ok",
        socket_path: socket_path.display().to_string(),
        request_id,
        topics: topics.iter().map(topic_label).map(str::to_string).collect(),
        subscribed_topics: subscribed_topics
            .iter()
            .map(topic_label)
            .map(str::to_string)
            .collect(),
        events,
    })
}

fn default_ipc_socket_path() -> Option<std::path::PathBuf> {
    std::env::var_os("SPIDERS_WM_IPC_SOCKET").map(std::path::PathBuf::from)
}

fn parse_query_request(name: &str) -> Result<spiders_shared::api::QueryRequest, String> {
    use spiders_shared::api::QueryRequest;

    match name {
        "state" => Ok(QueryRequest::State),
        "focused-window" => Ok(QueryRequest::FocusedWindow),
        "current-output" => Ok(QueryRequest::CurrentOutput),
        "current-workspace" => Ok(QueryRequest::CurrentWorkspace),
        "monitor-list" => Ok(QueryRequest::MonitorList),
        "tag-names" => Ok(QueryRequest::TagNames),
        _ => Err(format!("unsupported IPC query '{name}'")),
    }
}

fn parse_action_request(name: &str) -> Result<spiders_shared::api::WmAction, String> {
    use spiders_shared::api::WmAction;

    if let Some(value) = name.strip_prefix("set-layout:") {
        return Ok(WmAction::SetLayout {
            name: value.to_string(),
        });
    }
    if let Some(value) = name.strip_prefix("view-tag:") {
        return Ok(WmAction::ViewTag {
            tag: value.to_string(),
        });
    }
    if let Some(value) = name.strip_prefix("toggle-view-tag:") {
        return Ok(WmAction::ToggleViewTag {
            tag: value.to_string(),
        });
    }
    if let Some(value) = name.strip_prefix("spawn:") {
        return Ok(WmAction::Spawn {
            command: value.to_string(),
        });
    }

    match name {
        "reload-config" => Ok(WmAction::ReloadConfig),
        "cycle-layout-next" => Ok(WmAction::CycleLayout {
            direction: Some(spiders_shared::api::LayoutCycleDirection::Next),
        }),
        "cycle-layout-previous" => Ok(WmAction::CycleLayout {
            direction: Some(spiders_shared::api::LayoutCycleDirection::Previous),
        }),
        "toggle-floating" => Ok(WmAction::ToggleFloating),
        "toggle-fullscreen" => Ok(WmAction::ToggleFullscreen),
        "focus-left" => Ok(WmAction::FocusDirection {
            direction: spiders_shared::api::FocusDirection::Left,
        }),
        "focus-right" => Ok(WmAction::FocusDirection {
            direction: spiders_shared::api::FocusDirection::Right,
        }),
        "focus-up" => Ok(WmAction::FocusDirection {
            direction: spiders_shared::api::FocusDirection::Up,
        }),
        "focus-down" => Ok(WmAction::FocusDirection {
            direction: spiders_shared::api::FocusDirection::Down,
        }),
        "close-focused-window" => Ok(WmAction::CloseFocusedWindow),
        _ => Err(format!("unsupported IPC action '{name}'")),
    }
}

fn parse_subscription_topics(
    names: &[&str],
) -> Result<Vec<spiders_ipc::IpcSubscriptionTopic>, String> {
    if names.is_empty() {
        return Ok(vec![spiders_ipc::IpcSubscriptionTopic::All]);
    }

    names
        .iter()
        .map(|name| parse_subscription_topic(name))
        .collect()
}

fn parse_subscription_topic(name: &str) -> Result<spiders_ipc::IpcSubscriptionTopic, String> {
    match name {
        "all" => Ok(spiders_ipc::IpcSubscriptionTopic::All),
        "focus" => Ok(spiders_ipc::IpcSubscriptionTopic::Focus),
        "windows" => Ok(spiders_ipc::IpcSubscriptionTopic::Windows),
        "tags" => Ok(spiders_ipc::IpcSubscriptionTopic::Tags),
        "layout" => Ok(spiders_ipc::IpcSubscriptionTopic::Layout),
        "config" => Ok(spiders_ipc::IpcSubscriptionTopic::Config),
        _ => Err(format!("unsupported IPC topic '{name}'")),
    }
}

fn query_label(query: &spiders_shared::api::QueryRequest) -> &'static str {
    use spiders_shared::api::QueryRequest;

    match query {
        QueryRequest::State => "state",
        QueryRequest::FocusedWindow => "focused-window",
        QueryRequest::CurrentOutput => "current-output",
        QueryRequest::CurrentWorkspace => "current-workspace",
        QueryRequest::MonitorList => "monitor-list",
        QueryRequest::TagNames => "tag-names",
    }
}

fn action_label(action: &spiders_shared::api::WmAction) -> &'static str {
    use spiders_shared::api::WmAction;

    match action {
        WmAction::ReloadConfig => "reload-config",
        WmAction::ToggleFloating => "toggle-floating",
        WmAction::ToggleFullscreen => "toggle-fullscreen",
        WmAction::CloseFocusedWindow => "close-focused-window",
        WmAction::Spawn { .. } => "spawn",
        WmAction::SetLayout { .. } => "set-layout",
        WmAction::CycleLayout { .. } => "cycle-layout",
        WmAction::ViewTag { .. } => "view-tag",
        WmAction::ToggleViewTag { .. } => "toggle-view-tag",
        WmAction::FocusDirection { .. } => "focus-direction",
    }
}

fn topic_label(topic: &spiders_ipc::IpcSubscriptionTopic) -> &'static str {
    match topic {
        spiders_ipc::IpcSubscriptionTopic::All => "all",
        spiders_ipc::IpcSubscriptionTopic::Focus => "focus",
        spiders_ipc::IpcSubscriptionTopic::Windows => "windows",
        spiders_ipc::IpcSubscriptionTopic::Tags => "tags",
        spiders_ipc::IpcSubscriptionTopic::Layout => "layout",
        spiders_ipc::IpcSubscriptionTopic::Config => "config",
    }
}

fn fallback_query_response(
    query: spiders_shared::api::QueryRequest,
) -> spiders_shared::api::QueryResponse {
    use spiders_shared::api::{QueryRequest, QueryResponse};

    let state = synthetic_bootstrap_state();
    let focused_window = state.focused_window_id.as_ref().and_then(|window_id| {
        state
            .windows
            .iter()
            .find(|window| &window.id == window_id)
            .cloned()
    });

    match query {
        QueryRequest::State => QueryResponse::State(state),
        QueryRequest::FocusedWindow => QueryResponse::FocusedWindow(focused_window),
        QueryRequest::CurrentOutput => {
            QueryResponse::CurrentOutput(state.current_output().cloned())
        }
        QueryRequest::CurrentWorkspace => {
            QueryResponse::CurrentWorkspace(state.current_workspace().cloned())
        }
        QueryRequest::MonitorList => QueryResponse::MonitorList(state.outputs),
        QueryRequest::TagNames => QueryResponse::TagNames(state.tag_names),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixListener;

    use spiders_ipc::{encode_response_line, IpcEnvelope, IpcServerMessage};
    use spiders_shared::api::{CompositorEvent, QueryResponse};

    #[test]
    fn ipc_smoke_report_contains_subscription_and_event_lines() {
        let report = run_ipc_smoke().unwrap();

        assert_eq!(report.status, "ok");
        assert_eq!(report.request_kind, "subscribe");
        assert_eq!(report.response_kind, "subscribed");
        assert!(report.request_line.ends_with('\n'));
        assert!(report.response_line.ends_with('\n'));
        assert!(report.event_line.is_some());
    }

    #[test]
    fn ipc_query_uses_real_socket_transport() {
        let socket_path = unique_socket_path("ipc-query");
        let listener = UnixListener::bind(&socket_path).unwrap();

        let handle = std::thread::spawn({
            let socket_path = socket_path.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                use std::io::BufRead;
                reader.read_line(&mut request).unwrap();
                let line = encode_response_line(&IpcEnvelope::new(IpcServerMessage::Query(
                    QueryResponse::TagNames(vec!["1".into(), "2".into()]),
                )))
                .unwrap();
                stream.write_all(line.as_bytes()).unwrap();
                drop(stream);
                socket_path
            }
        });

        let report = run_ipc_query(Some(socket_path.clone()), "tag-names").unwrap();

        assert_eq!(report.query, spiders_shared::api::QueryRequest::TagNames);
        assert_eq!(
            report.response,
            QueryResponse::TagNames(vec!["1".into(), "2".into()])
        );

        let path = handle.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn ipc_action_uses_real_socket_transport() {
        let socket_path = unique_socket_path("ipc-action");
        let listener = UnixListener::bind(&socket_path).unwrap();

        let handle = std::thread::spawn({
            let socket_path = socket_path.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                use std::io::BufRead;
                reader.read_line(&mut request).unwrap();
                let line =
                    encode_response_line(&IpcEnvelope::new(IpcServerMessage::ActionAccepted))
                        .unwrap();
                stream.write_all(line.as_bytes()).unwrap();
                drop(stream);
                socket_path
            }
        });

        let report = run_ipc_action(Some(socket_path.clone()), "reload-config").unwrap();

        assert_eq!(report.action, spiders_shared::api::WmAction::ReloadConfig);
        assert_eq!(report.response_kind, "action-accepted");

        let path = handle.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn ipc_monitor_reads_subscribed_events_until_socket_closes() {
        let socket_path = unique_socket_path("ipc-monitor");
        let listener = UnixListener::bind(&socket_path).unwrap();

        let handle = std::thread::spawn({
            let socket_path = socket_path.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                use std::io::BufRead;
                reader.read_line(&mut request).unwrap();
                let subscribed =
                    encode_response_line(&IpcEnvelope::new(IpcServerMessage::Subscribed {
                        topics: vec![spiders_ipc::IpcSubscriptionTopic::Focus],
                    }))
                    .unwrap();
                let event = encode_response_line(&IpcEnvelope::new(IpcServerMessage::event(
                    CompositorEvent::FocusChange {
                        focused_window_id: synthetic_bootstrap_state().focused_window_id,
                        current_output_id: synthetic_bootstrap_state().current_output_id,
                        current_workspace_id: synthetic_bootstrap_state().current_workspace_id,
                    },
                )))
                .unwrap();
                stream.write_all(subscribed.as_bytes()).unwrap();
                stream.write_all(event.as_bytes()).unwrap();
                drop(stream);
                socket_path
            }
        });

        let report = run_ipc_monitor(Some(socket_path.clone()), vec!["focus"]).unwrap();

        assert_eq!(report.topics, vec!["focus"]);
        assert_eq!(report.subscribed_topics, vec!["focus"]);
        assert_eq!(report.events.len(), 1);

        let path = handle.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    fn unique_socket_path(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("spiders-cli-{label}-{nanos}.sock"))
    }
}
