mod bootstrap;
mod report;

use bootstrap::CliBootstrap;
use report::{emit, DiscoveryReport, ErrorReport, OutputMode, SuccessCheckReport};

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
    let output_mode = if args.iter().any(|arg| arg == "--json") {
        OutputMode::Json
    } else {
        OutputMode::Text
    };

    let cli = CliContext::new();

    if check_config {
        check_config_command(&cli, output_mode)
    } else {
        print_discovery(&cli, output_mode)
    }
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
