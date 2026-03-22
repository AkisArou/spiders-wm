mod bootstrap;
mod report;

use bootstrap::CliBootstrap;
use report::{
    BuildConfigReport, DiscoveryReport, ErrorReport, IpcActionReport, IpcMonitorReport,
    IpcQueryReport, IpcSmokeReport, OutputMode, SuccessCheckReport, emit,
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
                cache_dir_override: std::env::var_os("SPIDERS_WM_CACHE_DIR")
                    .map(std::path::PathBuf::from),
                authored_config_override: std::env::var_os("SPIDERS_WM_AUTHORED_CONFIG")
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
    let build_config = args.iter().any(|arg| arg == "build-config");
    let ipc_smoke = args.iter().any(|arg| arg == "ipc-smoke");
    let ipc_query = args.iter().any(|arg| arg == "ipc-query");
    let ipc_action = args.iter().any(|arg| arg == "ipc-action");
    let ipc_monitor = args.iter().any(|arg| arg == "ipc-monitor");
    let output_mode = if args.iter().any(|arg| arg == "--json") {
        OutputMode::Json
    } else {
        OutputMode::Text
    };
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
    } else if build_config {
        build_config_command(&cli, output_mode)
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
                    prepared_config: None,
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
                    prepared_config: None,
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
                    prepared_config: None,
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
                    prepared_config: None,
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
                    prepared_config: bootstrap.paths.prepared_config.display().to_string(),
                },
                || {
                    format!(
                        "spiders-cli placeholder (config runtime ready: {}, authored: {}, runtime: {})",
                        cli.ready,
                        bootstrap.paths.authored_config.display(),
                        bootstrap.paths.prepared_config.display()
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
                    prepared_config: None,
                    errors: None,
                    message: Some(error.to_string()),
                },
                || {
                    format!(
                        "spiders-cli placeholder (config runtime ready: {}, discovery error: {error})",
                        cli.ready
                    )
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
                    prepared_config: None,
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

    match bootstrap
        .service
        .load_config_with_cache_update(&bootstrap.paths)
    {
        Ok((config, prepared_config_update)) => {
            match bootstrap.service.validate_layout_modules(&config) {
                Ok(errors) if errors.is_empty() => {
                    emit(
                        output_mode,
                        &SuccessCheckReport {
                            status: "ok",
                            runtime_ready: cli.ready,
                            layouts: config.layouts.len(),
                            prepared_config: bootstrap.paths.prepared_config.display().to_string(),
                            prepared_config_update,
                        },
                        || {
                            format!(
                                "config ok (runtime ready: {}, layouts: {}, runtime: {}, cache: {})",
                                cli.ready,
                                config.layouts.len(),
                                bootstrap.paths.prepared_config.display(),
                                describe_prepared_config_update(prepared_config_update)
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
                            prepared_config: Some(
                                bootstrap.paths.prepared_config.display().to_string(),
                            ),
                            errors: Some(errors.clone()),
                            message: None,
                        },
                        || {
                            format!(
                                "config error (runtime ready: {}, runtime: {}): {}",
                                cli.ready,
                                bootstrap.paths.prepared_config.display(),
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
                            prepared_config: None,
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
            }
        }
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "load",
                    runtime_ready: cli.ready,
                    prepared_config: Some(bootstrap.paths.prepared_config.display().to_string()),
                    errors: None,
                    message: Some(error.to_string()),
                },
                || {
                    format!(
                        "config error (runtime ready: {}, runtime: {}): {error}",
                        cli.ready,
                        bootstrap.paths.prepared_config.display()
                    )
                },
            );
            std::process::ExitCode::from(1)
        }
    }
}

fn build_config_command(cli: &CliContext, output_mode: OutputMode) -> std::process::ExitCode {
    let bootstrap = match cli.bootstrap() {
        Ok(bootstrap) => bootstrap,
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "discovery",
                    runtime_ready: cli.ready,
                    prepared_config: None,
                    errors: None,
                    message: Some(error.to_string()),
                },
                || format!("prepared config discovery error: {error}"),
            );
            return std::process::ExitCode::from(1);
        }
    };

    let config = match bootstrap.service.load_authored_config(&bootstrap.paths) {
        Ok(config) => config,
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "load-authored",
                    runtime_ready: cli.ready,
                    prepared_config: Some(bootstrap.paths.prepared_config.display().to_string()),
                    errors: None,
                    message: Some(error.to_string()),
                },
                || format!("prepared config build error: {error}"),
            );
            return std::process::ExitCode::from(1);
        }
    };

    match bootstrap.service.validate_layout_modules(&config) {
        Ok(errors) if !errors.is_empty() => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "validation",
                    runtime_ready: cli.ready,
                    prepared_config: Some(bootstrap.paths.prepared_config.display().to_string()),
                    errors: Some(errors.clone()),
                    message: None,
                },
                || format!("prepared config build error: {}", errors.join("; ")),
            );
            std::process::ExitCode::from(1)
        }
        Ok(_) => match bootstrap
            .service
            .write_prepared_config(&bootstrap.paths, &config)
        {
            Ok(update) => {
                emit(
                    output_mode,
                    &BuildConfigReport {
                        status: "ok",
                        runtime_ready: cli.ready,
                        authored_config: bootstrap.paths.authored_config.display().to_string(),
                        prepared_config: bootstrap.paths.prepared_config.display().to_string(),
                        layouts: config.layouts.len(),
                        prepared_config_update: update,
                    },
                    || {
                        format!(
                            "prepared config built (layouts: {}, authored: {}, prepared: {}, refresh: {})",
                            config.layouts.len(),
                            bootstrap.paths.authored_config.display(),
                            bootstrap.paths.prepared_config.display(),
                            describe_prepared_config_update(Some(update))
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
                        phase: "write-runtime",
                        runtime_ready: cli.ready,
                        prepared_config: Some(
                            bootstrap.paths.prepared_config.display().to_string(),
                        ),
                        errors: None,
                        message: Some(error.to_string()),
                    },
                    || format!("prepared config write error: {error}"),
                );
                std::process::ExitCode::from(1)
            }
        },
        Err(error) => {
            emit(
                output_mode,
                &ErrorReport {
                    status: "error",
                    phase: "validation",
                    runtime_ready: cli.ready,
                    prepared_config: Some(bootstrap.paths.prepared_config.display().to_string()),
                    errors: None,
                    message: Some(error.to_string()),
                },
                || format!("prepared config validation error: {error}"),
            );
            std::process::ExitCode::from(1)
        }
    }
}

fn describe_prepared_config_update(
    update: Option<spiders_shared::runtime::RuntimeRefreshSummary>,
) -> String {
    match update {
        Some(update) if update.is_noop() => "noop".into(),
        Some(update) => format!(
            "refreshed={}, pruned={}",
            update.refreshed_files, update.pruned_files
        ),
        None => "unchanged".into(),
    }
}

fn synthetic_bootstrap_state() -> spiders_shared::wm::StateSnapshot {
    use spiders_tree::{OutputId, WindowId, WorkspaceId};
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
            logical_x: 0,
            logical_y: 0,
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
            active_workspaces: vec!["bootstrap".into()],
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
            mode: spiders_shared::wm::WindowMode::Tiled,
            focused: true,
            urgent: false,
            output_id: Some(OutputId::from("bootstrap-output")),
            workspace_id: Some(WorkspaceId::from("bootstrap-workspace")),
            workspaces: vec!["bootstrap".into()],
        }],
        visible_window_ids: vec![WindowId::from("bootstrap-window")],
        workspace_names: vec!["bootstrap".into()],
    }
}

fn run_ipc_smoke() -> Result<IpcSmokeReport, String> {
    use spiders_ipc::{
        IpcClientMessage, IpcEnvelope, IpcServerHandleResult, IpcServerState, IpcSubscriptionTopic,
        decode_request_line, encode_request_line, encode_response_line,
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
    use spiders_ipc::{IpcClientMessage, IpcEnvelope, connect, recv_response, send_request};

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
    use spiders_ipc::{IpcClientMessage, IpcEnvelope, connect, recv_response, send_request};

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
        IpcClientMessage, IpcEnvelope, IpcServerMessage, connect, decode_response_line,
        send_request,
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
        "workspace-names" => Ok(QueryRequest::WorkspaceNames),
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
    if let Some(value) = name.strip_prefix("view-workspace:") {
        return Ok(WmAction::ViewWorkspace {
            workspace: parse_workspace_shortcut(value, "view-workspace")?,
        });
    }
    if let Some(value) = name.strip_prefix("toggle-view-workspace:") {
        return Ok(WmAction::ToggleViewWorkspace {
            workspace: parse_workspace_shortcut(value, "toggle-view-workspace")?,
        });
    }
    if let Some(value) = name.strip_prefix("activate-workspace:") {
        return Ok(WmAction::ActivateWorkspace {
            workspace_id: value.into(),
        });
    }
    if let Some(value) = name.strip_prefix("assign-workspace:") {
        let (workspace_id, output_id) = value
            .split_once('@')
            .ok_or_else(|| "assign-workspace expects <workspace-id>@<output-id>".to_string())?;
        return Ok(WmAction::AssignWorkspace {
            workspace_id: workspace_id.into(),
            output_id: output_id.into(),
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

fn parse_workspace_shortcut(value: &str, action_name: &str) -> Result<u8, String> {
    let workspace = value
        .parse::<u8>()
        .map_err(|_| format!("{action_name} expects a workspace number between 1 and 9"))?;

    if (1..=9).contains(&workspace) {
        Ok(workspace)
    } else {
        Err(format!(
            "{action_name} expects a workspace number between 1 and 9"
        ))
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
        "workspaces" => Ok(spiders_ipc::IpcSubscriptionTopic::Workspaces),
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
        QueryRequest::WorkspaceNames => "workspace-names",
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
        WmAction::ViewWorkspace { .. } => "view-workspace",
        WmAction::ToggleViewWorkspace { .. } => "toggle-view-workspace",
        WmAction::ActivateWorkspace { .. } => "activate-workspace",
        WmAction::AssignWorkspace { .. } => "assign-workspace",
        WmAction::FocusMonitorLeft => "focus-monitor-left",
        WmAction::FocusMonitorRight => "focus-monitor-right",
        WmAction::SendMonitorLeft => "send-monitor-left",
        WmAction::SendMonitorRight => "send-monitor-right",
        WmAction::AssignFocusedWindowToWorkspace { .. } => "assign-focused-window-to-workspace",
        WmAction::ToggleAssignFocusedWindowToWorkspace { .. } => {
            "toggle-assign-focused-window-to-workspace"
        }
        WmAction::FocusWindow { .. } => "focus-window",
        WmAction::SetFloatingWindowGeometry { .. } => "set-floating-window-geometry",
        WmAction::FocusDirection { .. } => "focus-direction",
        WmAction::SwapDirection { .. } => "swap-direction",
        WmAction::ResizeDirection { .. } => "resize-direction",
        WmAction::ResizeTiledDirection { .. } => "resize-tiled-direction",
        WmAction::MoveDirection { .. } => "move-direction",
    }
}

fn topic_label(topic: &spiders_ipc::IpcSubscriptionTopic) -> &'static str {
    match topic {
        spiders_ipc::IpcSubscriptionTopic::All => "all",
        spiders_ipc::IpcSubscriptionTopic::Focus => "focus",
        spiders_ipc::IpcSubscriptionTopic::Windows => "windows",
        spiders_ipc::IpcSubscriptionTopic::Workspaces => "workspaces",
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
        QueryRequest::WorkspaceNames => QueryResponse::WorkspaceNames(state.workspace_names),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixListener;

    use spiders_ipc::{IpcEnvelope, IpcServerMessage, encode_response_line};
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
                    QueryResponse::WorkspaceNames(vec!["1".into(), "2".into()]),
                )))
                .unwrap();
                stream.write_all(line.as_bytes()).unwrap();
                drop(stream);
                socket_path
            }
        });

        let report = run_ipc_query(Some(socket_path.clone()), "workspace-names").unwrap();

        assert_eq!(
            report.query,
            spiders_shared::api::QueryRequest::WorkspaceNames
        );
        assert_eq!(
            report.response,
            QueryResponse::WorkspaceNames(vec!["1".into(), "2".into()])
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
    fn parse_ipc_action_supports_workspace_activate_and_assign() {
        assert_eq!(
            parse_action_request("activate-workspace:ws-2").unwrap(),
            spiders_shared::api::WmAction::ActivateWorkspace {
                workspace_id: "ws-2".into(),
            }
        );
        assert_eq!(
            parse_action_request("assign-workspace:ws-2@out-2").unwrap(),
            spiders_shared::api::WmAction::AssignWorkspace {
                workspace_id: "ws-2".into(),
                output_id: "out-2".into(),
            }
        );
    }

    #[test]
    fn parse_ipc_action_rejects_invalid_workspace_assign_format() {
        assert_eq!(
            parse_action_request("assign-workspace:ws-2"),
            Err("assign-workspace expects <workspace-id>@<output-id>".into())
        );
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
