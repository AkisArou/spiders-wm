#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Text,
    Json,
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

    fn discover_paths(
        &self,
    ) -> Result<spiders_config::model::ConfigPaths, spiders_config::model::LayoutConfigError> {
        spiders_config::model::ConfigPaths::discover(self.options.clone())
    }

    fn build_service(
        &self,
        paths: &spiders_config::model::ConfigPaths,
    ) -> spiders_config::service::ConfigRuntimeService<
        spiders_config::loader::RuntimeProjectLayoutSourceLoader,
        spiders_config::runtime::BoaLayoutRuntime<
            spiders_config::loader::RuntimeProjectLayoutSourceLoader,
        >,
    > {
        let resolver = spiders_config::loader::RuntimePathResolver::new(
            paths
                .authored_config
                .parent()
                .and_then(|dir| dir.parent())
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::path::PathBuf::from(".")),
            paths
                .runtime_config
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::path::PathBuf::from(".")),
        );
        let loader = spiders_config::loader::RuntimeProjectLayoutSourceLoader::new(resolver);
        let runtime = spiders_config::runtime::BoaLayoutRuntime::with_loader(loader.clone());

        spiders_config::service::ConfigRuntimeService::new(loader, runtime)
    }
}

fn print_discovery(cli: &CliContext, output_mode: OutputMode) -> std::process::ExitCode {
    match cli.discover_paths() {
        Ok(paths) => {
            match output_mode {
                OutputMode::Text => println!(
                    "spiders-cli placeholder (config runtime ready: {}, authored: {}, runtime: {})",
                    cli.ready,
                    paths.authored_config.display(),
                    paths.runtime_config.display()
                ),
                OutputMode::Json => println!(
                    "{{\"status\":\"ok\",\"runtime_ready\":{},\"authored_config\":\"{}\",\"runtime_config\":\"{}\"}}",
                    cli.ready,
                    paths.authored_config.display(),
                    paths.runtime_config.display()
                ),
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            match output_mode {
                OutputMode::Text => println!(
                    "spiders-cli placeholder (config runtime ready: {}, discovery error: {error})",
                    cli.ready
                ),
                OutputMode::Json => println!(
                    "{{\"status\":\"error\",\"phase\":\"discovery\",\"runtime_ready\":{},\"message\":\"{}\"}}",
                    cli.ready,
                    escape_json(&error.to_string())
                ),
            }
            std::process::ExitCode::from(1)
        }
    }
}

fn check_config_command(cli: &CliContext, output_mode: OutputMode) -> std::process::ExitCode {
    let paths = match cli.discover_paths() {
        Ok(paths) => paths,
        Err(error) => {
            match output_mode {
                OutputMode::Text => println!(
                    "config error (runtime ready: {}, discovery): {error}",
                    cli.ready
                ),
                OutputMode::Json => println!(
                    "{{\"status\":\"error\",\"phase\":\"discovery\",\"runtime_ready\":{},\"message\":\"{}\"}}",
                    cli.ready,
                    escape_json(&error.to_string())
                ),
            }
            return std::process::ExitCode::from(1);
        }
    };

    let service = cli.build_service(&paths);
    match service.load_config(&paths) {
        Ok(config) => match service.validate_layout_modules(&config) {
            Ok(errors) if errors.is_empty() => {
                match output_mode {
                    OutputMode::Text => println!(
                        "config ok (runtime ready: {}, layouts: {}, runtime: {})",
                        cli.ready,
                        config.layouts.len(),
                        paths.runtime_config.display()
                    ),
                    OutputMode::Json => println!(
                        "{{\"status\":\"ok\",\"runtime_ready\":{},\"layouts\":{},\"runtime_config\":\"{}\"}}",
                        cli.ready,
                        config.layouts.len(),
                        paths.runtime_config.display()
                    ),
                }
                std::process::ExitCode::SUCCESS
            }
            Ok(errors) => {
                match output_mode {
                    OutputMode::Text => println!(
                        "config error (runtime ready: {}, runtime: {}): {}",
                        cli.ready,
                        paths.runtime_config.display(),
                        errors.join("; ")
                    ),
                    OutputMode::Json => println!(
                        "{{\"status\":\"error\",\"phase\":\"validation\",\"runtime_ready\":{},\"runtime_config\":\"{}\",\"errors\":[{}]}}",
                        cli.ready,
                        paths.runtime_config.display(),
                        errors
                            .iter()
                            .map(|error| format!("\"{}\"", escape_json(error)))
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                }
                std::process::ExitCode::from(1)
            }
            Err(error) => {
                match output_mode {
                    OutputMode::Text => println!(
                        "config error (runtime ready: {}, validation): {error}",
                        cli.ready
                    ),
                    OutputMode::Json => println!(
                        "{{\"status\":\"error\",\"phase\":\"validation\",\"runtime_ready\":{},\"message\":\"{}\"}}",
                        cli.ready,
                        escape_json(&error.to_string())
                    ),
                }
                std::process::ExitCode::from(1)
            }
        },
        Err(error) => {
            match output_mode {
                OutputMode::Text => println!(
                    "config error (runtime ready: {}, runtime: {}): {error}",
                    cli.ready,
                    paths.runtime_config.display()
                ),
                OutputMode::Json => println!(
                    "{{\"status\":\"error\",\"phase\":\"load\",\"runtime_ready\":{},\"runtime_config\":\"{}\",\"message\":\"{}\"}}",
                    cli.ready,
                    paths.runtime_config.display(),
                    escape_json(&error.to_string())
                ),
            }
            std::process::ExitCode::from(1)
        }
    }
}

fn escape_json(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}
