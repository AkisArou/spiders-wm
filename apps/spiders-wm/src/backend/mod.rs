pub mod drm;
pub mod output;
pub mod session;
pub mod tty;
pub mod winit;

use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::WinitGraphicsBackend;

use self::drm::TtyDrmBackendState;
use self::output::TtyOutputState;
use self::session::BackendSession;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Winit,
    Tty,
}

impl BackendKind {
    pub fn from_env_or_args() -> Result<Self, String> {
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--backend" {
                let Some(value) = args.next() else {
                    return Err("missing value for --backend".into());
                };
                return Self::parse(&value);
            }

            if let Some(value) = arg.strip_prefix("--backend=") {
                return Self::parse(value);
            }
        }

        if let Ok(value) = std::env::var("SPIDERS_WM_BACKEND") {
            return Self::parse(&value);
        }

        Ok(Self::Winit)
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "winit" => Ok(Self::Winit),
            "tty" => Ok(Self::Tty),
            other => Err(format!("unsupported backend '{other}', expected 'winit' or 'tty'")),
        }
    }
}

pub enum BackendState {
    Winit(WinitGraphicsBackend<GlesRenderer>),
    #[allow(dead_code)]
    Tty(TtyBackendState),
}

#[allow(dead_code)]
pub struct TtyBackendState {
    pub session: BackendSession,
    pub outputs: Vec<TtyOutputState>,
    pub drm: TtyDrmBackendState,
}
