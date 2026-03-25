#![allow(dead_code)]

//! Pure compositor model types.
//!
//! This module is the receiving architecture for future state extraction from the
//! Smithay-facing compositor shell. It intentionally avoids Smithay types so layout,
//! config, runtime, and scene integration can depend on stable data structures.

pub(crate) mod output;
pub(crate) mod seat;
pub(crate) mod window;
pub(crate) mod wm;
pub(crate) mod workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct WorkspaceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct OutputId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SeatId(pub String);

impl From<&str> for WorkspaceId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<&str> for OutputId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<&str> for SeatId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}