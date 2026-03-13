mod bootstrap;
mod report;

use bootstrap::CliBootstrap;
use report::{
    emit, BootstrapFailureReport, BootstrapReport, DiscoveryReport, ErrorReport, OutputMode,
    SuccessCheckReport,
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
    let output_mode = if args.iter().any(|arg| arg == "--json") {
        OutputMode::Json
    } else {
        OutputMode::Text
    };
    let events_path = arg_value(&args, "--events");
    let transcript_path = arg_value(&args, "--transcript");

    let cli = CliContext::new();

    if bootstrap_trace {
        bootstrap_trace_command(&cli, output_mode, events_path, transcript_path)
    } else if check_config {
        check_config_command(&cli, output_mode)
    } else {
        print_discovery(&cli, output_mode)
    }
}

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
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
