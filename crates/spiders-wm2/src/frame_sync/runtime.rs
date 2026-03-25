//! Runtime frame-sync state and policy.
//!
//! This file is the behavioral contract that future layout systems should preserve.
//! The current tiling planner is temporary, but the frame-sync rules here are intended
//! to remain stable as this compositor moves to CSS-driven layout.
//!
//! # Frozen Responsibilities
//!
//! `WindowFrameSyncState` owns per-window transition state:
//! - pending target location for the next visible state
//! - configure-to-commit transaction matching
//! - cached snapshots used for static close/resize continuity
//! - resize overlay lifecycle
//! - frame callback eligibility during transitional states
//!
//! `FrameSyncState` owns global transition policy:
//! - closing window overlays
//! - deduplicated relayout queueing
//! - relayout deferral until transitions are idle
//! - snapshot refresh and overlay retirement orchestration
//!
//! # Invariants
//!
//! - A relayout that needs a configure must not become visible until the matching
//!   configure commit releases its transaction.
//! - A mapped window may be temporarily unmapped while a resize overlay holds the
//!   previous visible content on screen.
//! - A window with either visible content or a pending map target must still receive
//!   frame callbacks.
//! - Deferred relayouts must run exactly once after active transitions drain.
//! - Callers outside `frame_sync` should use the highest-level API available rather
//!   than manipulating pending transaction queues or matched-commit flags directly.

use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::{Space, Window};
use smithay::utils::{Logical, Point, Serial, Size};
use tracing::warn;

use super::{ClosePathQueue, ClosingWindow, RenderElements, ResizingWindow, Transaction, WindowSnapshot};

/// Result of processing a surface commit against frame-sync state.
///
/// `first_map` indicates the first real commit for a new window. In that case the
/// compositor may need to trigger a follow-up relayout and focus update.
pub struct WindowCommitUpdate {
    pub first_map: bool,
    pub pending_location: Option<Point<i32, Logical>>,
}

/// Frame-sync decision for a single relayout step.
///
/// The caller still performs Smithay-facing operations such as `map_element`,
/// `unmap_elem`, and `send_configure`, but the decision of whether those actions
/// are required lives here.
pub struct WindowRelayoutAction {
    pub unmap_window: bool,
    pub map_now: Option<Point<i32, Logical>>,
}

/// Per-window frame-sync state.
///
/// This is the primary boundary that the rest of the compositor should interact with
/// when a window enters or leaves a transitional state.
pub struct WindowFrameSyncState {
    pending_location: Option<Point<i32, Logical>>,
    matched_configure_commit: bool,
    snapshot: Option<WindowSnapshot>,
    resize_overlay: Option<ResizingWindow>,
    snapshot_dirty: bool,
    transaction_for_next_configure: Option<Transaction>,
    pending_transactions: Vec<(Serial, Transaction)>,
}

impl Default for WindowFrameSyncState {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowFrameSyncState {
    /// Creates empty frame-sync state for a newly tracked window.
    pub fn new() -> Self {
        Self {
            pending_location: None,
            matched_configure_commit: false,
            snapshot: None,
            resize_overlay: None,
            snapshot_dirty: true,
            transaction_for_next_configure: None,
            pending_transactions: Vec::new(),
        }
    }

    pub fn pending_location(&self) -> Option<Point<i32, Logical>> {
        self.pending_location
    }

    /// Returns whether the window should still receive a frame callback.
    ///
    /// This stays true for mapped windows and for windows that are waiting for a
    /// pending location to become visible.
    pub fn needs_frame_callback(&self, mapped: bool) -> bool {
        mapped || self.pending_location.is_some()
    }

    pub fn set_pending_location(&mut self, location: Point<i32, Logical>) {
        self.pending_location = Some(location);
    }

    pub fn take_pending_location(&mut self) -> Option<Point<i32, Logical>> {
        self.pending_location.take()
    }

    pub fn snapshot_owned(&self) -> Option<WindowSnapshot> {
        self.snapshot.clone()
    }

    pub fn mark_snapshot_dirty(&mut self) {
        self.snapshot_dirty = true;
    }

    pub fn has_resize_overlay(&self) -> bool {
        self.resize_overlay.is_some()
    }

    pub fn resize_overlay(&self) -> Option<&ResizingWindow> {
        self.resize_overlay.as_ref()
    }

    pub fn clear_resize_overlay(&mut self) {
        self.resize_overlay = None;
    }

    pub fn maybe_finish_resize_overlay(&mut self) -> Option<Point<i32, Logical>> {
        let finished = self
            .resize_overlay
            .as_ref()
            .is_some_and(ResizingWindow::is_finished);
        if !finished {
            return None;
        }

        self.resize_overlay = None;
        self.pending_location
    }

    /// Captures a fresh snapshot after the visible content changes.
    pub fn refresh_snapshot(
        &mut self,
        renderer: &mut GlesRenderer,
        window: &Window,
        mapped: bool,
    ) {
        if !(mapped && self.snapshot_dirty) {
            return;
        }

        match WindowSnapshot::capture(renderer, window) {
            Ok(Some(snapshot)) => {
                self.snapshot = Some(snapshot);
                self.snapshot_dirty = false;
            }
            Ok(None) => {}
            Err(err) => {
                warn!(%err, "failed to refresh window snapshot");
            }
        }
    }

    pub fn maybe_prepare_resize_overlay(
        &mut self,
        window: &Window,
        current_location: Option<Point<i32, Logical>>,
        target_location: Point<i32, Logical>,
        target_size: Size<i32, Logical>,
        transaction: &Transaction,
    ) -> bool {
        let (Some(snapshot), Some(_)) = (self.snapshot.as_ref(), current_location) else {
            return false;
        };

        self.resize_overlay = Some(snapshot.into_resizing_window(
            target_location,
            window.geometry().loc,
            window.geometry().size,
            target_size,
            transaction.monitor(),
        ));
        true
    }

    /// Computes the frame-sync side effects of a relayout for one window.
    ///
    /// This does not mutate Smithay objects directly. It only updates frame-sync
    /// bookkeeping and reports whether the caller must unmap the real window or may
    /// map it immediately.
    pub fn plan_relayout(
        &mut self,
        window: &Window,
        mapped: bool,
        current_location: Option<Point<i32, Logical>>,
        target_location: Point<i32, Logical>,
        target_size: Size<i32, Logical>,
        needs_configure: bool,
        transaction: &Transaction,
    ) -> WindowRelayoutAction {
        let unmap_window = if needs_configure && mapped {
            self.maybe_prepare_resize_overlay(
                window,
                current_location,
                target_location,
                target_size,
                transaction,
            )
        } else {
            false
        };

        if needs_configure {
            self.set_pending_location(target_location);
            self.stash_transaction_for_next_configure(transaction.clone());
        } else {
            self.clear_resize_overlay();
            self.set_pending_location(target_location);
        }

        let map_now = (mapped && !self.has_resize_overlay()).then_some(target_location);

        WindowRelayoutAction {
            unmap_window,
            map_now,
        }
    }

    /// Records the serial for a configure that has just been sent.
    ///
    /// The transaction previously staged by `plan_relayout` becomes associated with
    /// this serial until the matching commit arrives.
    pub fn register_sent_configure(&mut self, serial: Serial) {
        if let Some(transaction) = self.transaction_for_next_configure.take() {
            self.pending_transactions.push((serial, transaction));
        }
    }

    /// Matches an acked configure commit to its pending transaction.
    ///
    /// If a transaction is returned, the commit is considered the one that should
    /// release the blocked relayout when the transaction completes.
    pub fn match_configure_commit(&mut self, commit_serial: Serial) -> Option<Transaction> {
        let transaction = self.take_pending_transaction(commit_serial);
        if transaction.is_some() {
            self.note_matched_configure_commit();
        }
        transaction
    }

    fn stash_transaction_for_next_configure(&mut self, transaction: Transaction) {
        self.transaction_for_next_configure = Some(transaction);
    }

    fn take_pending_transaction(&mut self, commit_serial: Serial) -> Option<Transaction> {
        let mut transaction = None;
        while let Some((serial, _)) = self.pending_transactions.first() {
            if commit_serial.is_no_older_than(serial) {
                let (_, pending) = self.pending_transactions.remove(0);
                transaction = Some(pending);
            } else {
                break;
            }
        }
        transaction
    }

    fn note_matched_configure_commit(&mut self) {
        self.matched_configure_commit = true;
    }

    /// Consumes transient commit bookkeeping and reports how the compositor should
    /// handle the committed surface.
    pub fn consume_commit_update(&mut self, mapped: bool) -> WindowCommitUpdate {
        let first_map = !mapped && self.pending_location.is_none();
        let matched_configure_commit = self.matched_configure_commit;
        self.matched_configure_commit = false;
        let pending_location = if first_map || matched_configure_commit {
            self.pending_location.take()
        } else {
            self.pending_location
        };

        if matched_configure_commit {
            self.resize_overlay = None;
        }

        WindowCommitUpdate {
            first_map,
            pending_location,
        }
    }
}

#[derive(Default)]
/// Global frame-sync runtime state shared across all managed windows.
pub struct FrameSyncState {
    closing_windows: Vec<ClosingWindow>,
    close_path_queue: ClosePathQueue,
}

impl FrameSyncState {
    /// Queues a closing snapshot overlay that remains visible until its transaction releases.
    pub fn push_closing_window(&mut self, window: ClosingWindow) {
        self.closing_windows.push(window);
    }

    /// Returns whether a relayout should be deferred and queues it if necessary.
    pub fn should_defer_relayout<'a, I>(&mut self, window_states: I) -> bool
    where
        I: IntoIterator<Item = &'a WindowFrameSyncState>,
    {
        if self.has_active_transitions(window_states) {
            self.queue_relayout();
            true
        } else {
            false
        }
    }

    /// Returns whether a previously queued relayout may run now.
    pub fn take_ready_relayout<'a, I>(&mut self, window_states: I) -> bool
    where
        I: IntoIterator<Item = &'a WindowFrameSyncState>,
    {
        !self.has_active_transitions(window_states) && self.take_queued_relayout()
    }

    /// Reports whether any active transition is still preventing an immediate relayout.
    pub fn has_active_transitions<'a, I>(&self, window_states: I) -> bool
    where
        I: IntoIterator<Item = &'a WindowFrameSyncState>,
    {
        !self.closing_windows.is_empty()
            || window_states.into_iter().any(WindowFrameSyncState::has_resize_overlay)
    }

    pub fn queue_relayout(&mut self) {
        self.close_path_queue.queue_relayout();
    }

    pub fn take_queued_relayout(&mut self) -> bool {
        self.close_path_queue.take_relayout()
    }

    pub fn advance_closing_windows(&mut self) {
        self.closing_windows.retain(|window| !window.is_finished());
    }

    /// Applies completed resize-overlay remaps to the compositor space.
    pub fn advance_resize_overlays(
        &mut self,
        space: &mut Space<Window>,
        windows: impl IntoIterator<Item = (Window, Point<i32, Logical>)>,
    ) {
        for (window, location) in windows {
            space.map_element(window, location, false);
        }
    }

    /// Collects windows whose resize overlays finished and are ready to be remapped.
    pub fn finished_resize_overlay_mappings<'a, I>(&mut self, windows: I) -> Vec<(Window, Point<i32, Logical>)>
    where
        I: IntoIterator<Item = (&'a Window, &'a mut WindowFrameSyncState)>,
    {
        windows
            .into_iter()
            .filter_map(|(window, frame_sync)| {
                frame_sync
                    .maybe_finish_resize_overlay()
                    .map(|location| (window.clone(), location))
            })
            .collect()
    }

    /// Refreshes cached snapshots for windows whose visible content changed.
    pub fn refresh_window_snapshots<'a, I>(
        &mut self,
        renderer: &mut GlesRenderer,
        windows: I,
    )
    where
        I: IntoIterator<Item = (&'a Window, bool, &'a mut WindowFrameSyncState)>,
    {
        for (window, mapped, frame_sync) in windows {
            frame_sync.refresh_snapshot(renderer, window, mapped);
        }
    }

    /// Builds transition render elements for resize and close overlays.
    pub fn render_elements<'a, I>(&self, window_states: I) -> Vec<RenderElements>
    where
        I: IntoIterator<Item = &'a WindowFrameSyncState>,
    {
        let mut elements: Vec<RenderElements> = window_states
            .into_iter()
            .filter_map(WindowFrameSyncState::resize_overlay)
            .map(ResizingWindow::render_element)
            .collect();

        elements.extend(
            self.closing_windows
            .iter()
            .map(ClosingWindow::render_element)
        );
        elements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_commit_without_pending_layout_requests_initial_map() {
        let mut frame_sync = WindowFrameSyncState::default();

        let update = frame_sync.consume_commit_update(false);

        assert!(update.first_map);
        assert!(update.pending_location.is_none());
    }

    #[test]
    fn matched_configure_commit_consumes_pending_location_once() {
        let mut frame_sync = WindowFrameSyncState::default();
        let pending = Point::from((320, 24));
        frame_sync.set_pending_location(pending);
        frame_sync.note_matched_configure_commit();

        let update = frame_sync.consume_commit_update(true);
        assert!(!update.first_map);
        assert_eq!(update.pending_location, Some(pending));

        let second_update = frame_sync.consume_commit_update(true);
        assert!(second_update.pending_location.is_none());
    }

    #[test]
    fn pending_transactions_match_latest_acked_configure() {
        let mut frame_sync = WindowFrameSyncState::default();
        let first = Transaction::new();
        let second = Transaction::new();
        let serial1 = Serial::from(5_u32);
        let serial2 = Serial::from(7_u32);

        frame_sync.stash_transaction_for_next_configure(first.clone());
        frame_sync.register_sent_configure(serial1);
        frame_sync.stash_transaction_for_next_configure(second.clone());
        frame_sync.register_sent_configure(serial2);

        let matched_first = frame_sync
            .match_configure_commit(serial1)
            .expect("expected matching transaction");
        assert!(!matched_first.is_completed());

        let matched_second = frame_sync
            .match_configure_commit(serial2)
            .expect("expected matching transaction");
        assert!(!matched_second.is_completed());
        assert!(frame_sync.match_configure_commit(serial2).is_none());
    }

    #[test]
    fn frame_callback_needed_when_window_is_pending_map() {
        let mut frame_sync = WindowFrameSyncState::default();
        assert!(!frame_sync.needs_frame_callback(false));

        frame_sync.set_pending_location(Point::from((12, 34)));
        assert!(frame_sync.needs_frame_callback(false));
        assert!(frame_sync.needs_frame_callback(true));
    }

    #[test]
    fn queued_relayout_only_releases_once_transitions_are_idle() {
        let mut runtime = FrameSyncState::default();
        let window = WindowFrameSyncState::default();

        assert!(!runtime.should_defer_relayout([&window]));

        runtime.queue_relayout();
        assert!(runtime.take_ready_relayout([&window]));
        assert!(!runtime.take_ready_relayout([&window]));
    }

}