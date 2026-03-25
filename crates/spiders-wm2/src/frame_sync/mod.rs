//! Frame-perfect synchronization for tiled window relayouts.
//!
//! This module encapsulates all frame synchronization logic to ensure that layout
//! changes become visible atomically on screen without intermediate frames.
//!
//! # Public API
//!
//! The minimal public API exposed to the rest of the compositor:
//! - `Transaction`: Reference-counted transaction with timeout-based completion
//! - `WindowSnapshot`: Captured window state for transitions
//! - `ClosingWindow`: Close animation overlay (tied to transaction)
//! - `ResizingWindow`: Resize animation overlay (tied to transaction)
//! - `ClosePathQueue`: Deduplicating queue for close+relayout coordination
//!
//! # Design
//!
//! All frame synchronization is handled internally through:
//! - Transaction reference counting for commit blocking
//! - Deadline timers as safety fallback (300ms timeout)
//! - Window snapshots for visual continuity during transitions
//! - Close path queueing to serialize overlapping operations
//!
//! The rest of the compositor interacts with frame_sync only through these public types.

mod close_path;
mod runtime;
mod snapshots;
mod transaction;

pub use runtime::{FrameSyncState, WindowFrameSyncState};
pub use close_path::ClosePathQueue;
pub use snapshots::{ClosingWindow, ResizingWindow, WindowSnapshot, Wm2RenderElements};
pub use transaction::Transaction;
