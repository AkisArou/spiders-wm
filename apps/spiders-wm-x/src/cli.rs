use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RunMode {
    #[default]
    Bootstrap,
    DumpState,
    Observe,
    Manage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CliOptions {
    pub(crate) help: bool,
    pub(crate) run_mode: RunMode,
    pub(crate) event_limit: Option<usize>,
    pub(crate) idle_timeout_ms: Option<u64>,
}

impl CliOptions {
    pub(crate) fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut options = Self::default();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--dump-state" => set_run_mode(&mut options, RunMode::DumpState, &arg)?,
                "--observe" => set_run_mode(&mut options, RunMode::Observe, &arg)?,
                "--manage" => set_run_mode(&mut options, RunMode::Manage, &arg)?,
                "--event-limit" => {
                    let value = args.next().ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing value for `--event-limit`; expected a positive integer"
                        )
                    })?;
                    let parsed = value.parse::<usize>().map_err(|_| {
                        anyhow::anyhow!(
                            "invalid value `{value}` for `--event-limit`; expected a positive integer"
                        )
                    })?;
                    if parsed == 0 {
                        bail!("invalid value `0` for `--event-limit`; expected a positive integer");
                    }
                    options.event_limit = Some(parsed);
                }
                "--idle-timeout-ms" => {
                    let value = args.next().ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing value for `--idle-timeout-ms`; expected a positive integer"
                        )
                    })?;
                    let parsed = value.parse::<u64>().map_err(|_| {
                        anyhow::anyhow!(
                            "invalid value `{value}` for `--idle-timeout-ms`; expected a positive integer"
                        )
                    })?;
                    if parsed == 0 {
                        bail!(
                            "invalid value `0` for `--idle-timeout-ms`; expected a positive integer"
                        );
                    }
                    options.idle_timeout_ms = Some(parsed);
                }
                "-h" | "--help" => options.help = true,
                other => bail!("unsupported argument `{other}`; use --help for supported options"),
            }
        }

        if options.run_mode != RunMode::Observe
            && (options.event_limit.is_some() || options.idle_timeout_ms.is_some())
        {
            bail!("`--event-limit` and `--idle-timeout-ms` require `--observe`");
        }

        Ok(options)
    }
}

fn set_run_mode(options: &mut CliOptions, next_mode: RunMode, flag: &str) -> Result<()> {
    if options.run_mode != RunMode::Bootstrap && options.run_mode != next_mode {
        bail!("`{flag}` cannot be combined with another run mode flag");
    }

    options.run_mode = next_mode;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dump_state_mode() {
        let options = CliOptions::parse(["--dump-state".to_string()]).unwrap();

        assert_eq!(options.run_mode, RunMode::DumpState);
        assert_eq!(options.event_limit, None);
    }

    #[test]
    fn parses_observe_mode_with_limit() {
        let options = CliOptions::parse([
            "--observe".to_string(),
            "--event-limit".to_string(),
            "5".to_string(),
        ])
        .unwrap();

        assert_eq!(options.run_mode, RunMode::Observe);
        assert_eq!(options.event_limit, Some(5));
        assert_eq!(options.idle_timeout_ms, None);
    }

    #[test]
    fn rejects_unknown_flags() {
        let error = CliOptions::parse(["--unknown".to_string()]).unwrap_err();

        assert!(error.to_string().contains("unsupported argument `--unknown`"));
    }

    #[test]
    fn rejects_mixed_run_modes() {
        let error =
            CliOptions::parse(["--dump-state".to_string(), "--manage".to_string()]).unwrap_err();

        assert!(error.to_string().contains("cannot be combined with another run mode flag"));
    }

    #[test]
    fn parses_manage_mode() {
        let options = CliOptions::parse(["--manage".to_string()]).unwrap();

        assert_eq!(options.run_mode, RunMode::Manage);
    }

    #[test]
    fn rejects_observe_only_options_without_observe() {
        let error = CliOptions::parse(["--event-limit".to_string(), "4".to_string()]).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("`--event-limit` and `--idle-timeout-ms` require `--observe`")
        );
    }

    #[test]
    fn parses_observe_idle_timeout() {
        let options = CliOptions::parse([
            "--observe".to_string(),
            "--idle-timeout-ms".to_string(),
            "250".to_string(),
        ])
        .unwrap();

        assert_eq!(options.run_mode, RunMode::Observe);
        assert_eq!(options.idle_timeout_ms, Some(250));
    }
}
