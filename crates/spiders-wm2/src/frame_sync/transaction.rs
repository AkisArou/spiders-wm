//! Transaction-based frame synchronization for frame-perfect relayouts.
//!
//! This module implements the core transaction mechanism ensuring that layout changes
//! become visible atomically on screen without intermediate frames.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::calloop::ping::{Ping, make_ping};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::wayland_server::Client;
use smithay::wayland::compositor::{Blocker, BlockerState};
use tracing::{error, trace, trace_span, warn};

/// Time limit for transaction completion before automatic timeout.
///
/// This prevents the compositor from hanging when a client fails to respond to
/// a configure event promptly. Set to a safe default for interactive use.
const TIME_LIMIT: Duration = Duration::from_millis(300);

/// Core transaction for frame-perfect relayouts.
///
/// A transaction represents a batch of relayout changes that should become visible
/// atomically. Each transaction can be cloned multiple times and distributed to
/// multiple windows. The transaction completes when either all clones are dropped
/// or the deadline timer fires.
#[derive(Debug, Clone)]
pub struct Transaction {
    inner: Arc<Inner>,
    deadline: Rc<RefCell<Deadline>>,
}

/// Monitor for tracking when a transaction completes.
///
/// Use this to poll whether a transaction is ready to be released in the render
/// loop without holding references to the transaction itself.
#[derive(Debug, Clone)]
pub struct TransactionMonitor(Weak<Inner>);

/// Blocker to add to Wayland surfaces during a transaction.
///
/// Implement Smithay's Blocker trait to prevent surface commits from becoming
/// visible until the transaction completes.
#[derive(Debug)]
pub struct TransactionBlocker(Weak<Inner>);

/// Internal deadline state machine for per-clone timer registration.
#[derive(Debug)]
enum Deadline {
    /// Timer not yet registered; stores the intended deadline instant
    NotRegistered(Instant),
    /// Timer registered; stores the Ping to cancel it on early completion
    Registered { remove: Ping },
}

/// Shared state for a transaction.
#[derive(Debug)]
struct Inner {
    /// Whether this transaction has completed
    completed: AtomicBool,
    /// Clients waiting for blocker_cleared events when transaction completes
    notifications: Mutex<Option<(Sender<Client>, Vec<Client>)>>,
}

impl Transaction {
    /// Creates a new transaction with a 300ms timeout deadline.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                completed: AtomicBool::new(false),
                notifications: Mutex::new(None),
            }),
            deadline: Rc::new(RefCell::new(Deadline::NotRegistered(
                Instant::now() + TIME_LIMIT,
            ))),
        }
    }

    /// Gets a blocker to add to window surfaces during this transaction.
    #[inline]
    pub fn blocker(&self) -> TransactionBlocker {
        TransactionBlocker(Arc::downgrade(&self.inner))
    }

    /// Gets a monitor to poll transaction completion in the render loop.
    #[inline]
    pub fn monitor(&self) -> TransactionMonitor {
        TransactionMonitor(Arc::downgrade(&self.inner))
    }

    /// Checks if this transaction has already completed.
    #[inline]
    pub fn is_completed(&self) -> bool {
        self.inner.is_completed()
    }

    /// Checks if this is the last handle to this transaction.
    #[inline]
    pub fn is_last(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }

    /// Registers a deadline timer with the event loop.
    pub fn register_deadline<T: 'static>(&self, event_loop: &LoopHandle<'static, T>) {
        let mut deadline = self.deadline.borrow_mut();
        if let Deadline::NotRegistered(instant) = *deadline {
            let timer = Timer::from_deadline(instant);
            let inner = Arc::downgrade(&self.inner);
            let token = event_loop
                .insert_source(timer, move |_, _, _| {
                    let _span =
                        trace_span!("frame_sync deadline", transaction = ?Weak::as_ptr(&inner))
                            .entered();
                    if let Some(inner) = inner.upgrade() {
                        trace!("frame_sync deadline reached");
                        inner.complete();
                    }

                    TimeoutAction::Drop
                })
                .expect("failed to register frame_sync deadline timer");

            let (ping, source) = make_ping().expect("failed to create deadline removal ping");
            let loop_handle = event_loop.clone();
            event_loop
                .insert_source(source, move |_, _, _| {
                    loop_handle.remove(token);
                })
                .expect("failed to register deadline removal ping source");

            *deadline = Deadline::Registered { remove: ping };
        }
    }

    /// Registers a callback to receive blocker_cleared events when transaction completes.
    pub fn add_notification(&self, sender: Sender<Client>, client: Client) {
        if self.is_completed() {
            error!("tried to add notification to a completed transaction");
            return;
        }

        let mut guard = self
            .inner
            .notifications
            .lock()
            .expect("frame_sync notifications poisoned");
        guard
            .get_or_insert((sender, Vec::new()))
            .1
            .push(client);
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        let _span =
            trace_span!("frame_sync drop", transaction = ?Arc::as_ptr(&self.inner)).entered();
        if self.is_last() {
            trace!("last frame_sync handle dropped; releasing blockers");
            self.inner.complete();

            if let Deadline::Registered { remove } = &*self.deadline.borrow() {
                remove.ping();
            }
        }
    }
}

impl TransactionMonitor {
    /// Checks if this transaction has been released.
    #[inline]
    pub fn is_released(&self) -> bool {
        self.0
            .upgrade()
            .is_none_or(|inner| inner.is_completed())
    }
}

impl Blocker for TransactionBlocker {
    fn state(&self) -> BlockerState {
        if self
            .0
            .upgrade()
            .is_none_or(|inner| inner.is_completed())
        {
            BlockerState::Released
        } else {
            BlockerState::Pending
        }
    }
}

impl Inner {
    /// Marks this transaction as complete and notifies all waiting clients.
    fn complete(&self) {
        self.completed.store(true, Ordering::Relaxed);

        let mut guard = self
            .notifications
            .lock()
            .expect("frame_sync notifications poisoned");
        if let Some((sender, clients)) = guard.take() {
            for client in clients {
                if let Err(err) = sender.send(client) {
                    warn!("error sending frame_sync blocker notification: {err:?}");
                }
            }
        }
    }

    /// Polls the completion status of this transaction.
    #[inline]
    fn is_completed(&self) -> bool {
        self.completed.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_creation() {
        let tx = Transaction::new();
        assert!(!tx.is_completed());
        assert!(tx.is_last());
    }

    #[test]
    fn transaction_clone_increments_count() {
        let tx1 = Transaction::new();
        assert!(tx1.is_last());

        let tx2 = tx1.clone();
        assert!(!tx1.is_last());
        assert!(!tx2.is_last());
    }

    #[test]
    fn monitor_tracks_completion() {
        let tx = Transaction::new();
        let monitor = tx.monitor();
        assert!(!monitor.is_released());

        drop(tx);
        assert!(monitor.is_released());
    }

    #[test]
    fn blocker_state_reflects_completion() {
        let tx = Transaction::new();
        let blocker = tx.blocker();
        assert_eq!(blocker.state(), BlockerState::Pending);

        drop(tx);
        assert_eq!(blocker.state(), BlockerState::Released);
    }
}
