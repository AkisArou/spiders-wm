#[cfg(feature = "libseat")]
use smithay::backend::session::libseat::LibSeatSession;

#[allow(dead_code)]
pub enum BackendSession {
    Nested(NestedSessionState),
    Tty(TtySessionState),
}

impl BackendSession {
    pub fn seat_name(&self) -> &str {
        match self {
            Self::Nested(state) => &state.seat_name,
            Self::Tty(state) => &state.seat_name,
        }
    }
}

#[allow(dead_code)]
pub struct NestedSessionState {
    pub seat_name: String,
    pub active: bool,
}

#[allow(dead_code)]
pub struct TtySessionState {
    pub seat_name: String,
    pub active: bool,
    pub handle: TtySessionHandle,
}

#[allow(dead_code)]
pub enum TtySessionHandle {
    Placeholder,
    #[cfg(feature = "libseat")]
    LibSeat(LibSeatSession),
}
