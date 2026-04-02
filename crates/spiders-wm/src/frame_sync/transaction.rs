use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::{Duration, Instant},
};

use smithay::{
    reexports::{
        calloop::{
            self, LoopHandle,
            ping::Ping,
            timer::{TimeoutAction, Timer},
        },
        wayland_server::Client,
    },
    utils::{Logical, Point, Serial, Size},
    wayland::compositor::{Blocker, BlockerState},
};
use tracing::{debug, error, trace, trace_span, warn};

const TIMEOUT: Duration = Duration::from_millis(75);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PendingLayout {
    pub(crate) location: Point<i32, Logical>,
    pub(crate) size: Size<i32, Logical>,
}

#[derive(Debug, Default)]
pub(crate) struct PendingConfigureState {
    pending: VecDeque<(Serial, PendingConfigure)>,
    ready: Option<PendingLayout>,
}

#[derive(Debug)]
struct PendingConfigure {
    layout: PendingLayout,
    transaction: Transaction,
}

#[derive(Debug)]
pub(crate) struct MatchedConfigure {
    pub(crate) layout: PendingLayout,
    pub(crate) transaction: Transaction,
}

#[derive(Debug, Clone)]
pub(crate) struct Transaction {
    inner: Arc<Inner>,
    deadline: Rc<RefCell<Deadline>>,
}

pub(crate) struct TransactionBlocker {
    inner: Weak<Inner>,
}

#[derive(Debug)]
enum Deadline {
    NotRegistered(Instant),
    Registered { remove: Ping },
}

#[derive(Debug)]
struct Inner {
    completed: AtomicBool,
    notifications: Mutex<Option<(Sender<Client>, Vec<Client>)>>,
}

impl PendingConfigureState {
    pub(crate) fn push(&mut self, serial: Serial, layout: PendingLayout, transaction: Transaction) {
        self.pending.push_back((
            serial,
            PendingConfigure {
                layout,
                transaction,
            },
        ));
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) fn latest_live_transaction(&self) -> Option<Transaction> {
        self.pending.iter().rev().find_map(|(_, pending)| {
            (!pending.transaction.is_completed()).then_some(pending.transaction.clone())
        })
    }

    pub(crate) fn mark_ready(&mut self, commit_serial: Serial) -> Option<MatchedConfigure> {
        let mut matched = None;

        while let Some((serial, _)) = self.pending.front() {
            if *serial <= commit_serial {
                let (_, pending) = self.pending.pop_front().expect("pending configure missing");
                matched = Some(MatchedConfigure {
                    layout: pending.layout,
                    transaction: pending.transaction,
                });
            } else {
                break;
            }
        }

        if let Some(matched) = matched.as_ref() {
            self.ready = Some(matched.layout);
        }

        matched
    }

    pub(crate) fn take_ready(&mut self) -> Option<PendingLayout> {
        self.ready.take()
    }

    pub(crate) fn clear(&mut self) {
        self.pending.clear();
        self.ready = None;
    }
}

impl Transaction {
    pub(super) fn new<T: 'static>(loop_handle: &LoopHandle<'static, T>) -> Self {
        let transaction = Self {
            inner: Arc::new(Inner {
                completed: AtomicBool::new(false),
                notifications: Mutex::new(None),
            }),
            deadline: Rc::new(RefCell::new(Deadline::NotRegistered(
                Instant::now() + TIMEOUT,
            ))),
        };
        transaction.register_deadline_timer(loop_handle);
        transaction
    }

    pub(crate) fn blocker(&self) -> TransactionBlocker {
        trace!(transaction = ?Arc::as_ptr(&self.inner), "generating blocker");
        TransactionBlocker {
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub(crate) fn add_notification(&self, sender: Sender<Client>, client: Client) {
        if self.is_completed() {
            error!("tried to add notification to a completed transaction");
            return;
        }

        let mut guard = self
            .inner
            .notifications
            .lock()
            .expect("transaction notifications poisoned");
        guard.get_or_insert((sender, Vec::new())).1.push(client);
    }

    pub(crate) fn is_completed(&self) -> bool {
        self.inner.is_completed()
    }

    pub(crate) fn is_last(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }

    pub(crate) fn debug_id(&self) -> usize {
        Arc::as_ptr(&self.inner) as usize
    }

    fn register_deadline_timer<T: 'static>(&self, loop_handle: &LoopHandle<'static, T>) {
        let mut cell = self.deadline.borrow_mut();
        if let Deadline::NotRegistered(deadline) = *cell {
            let timer = Timer::from_deadline(deadline);
            let inner = Arc::downgrade(&self.inner);
            let token = loop_handle
                .insert_source(timer, move |_, _, _| {
                    let _span = trace_span!("deadline timer", transaction = ?Weak::as_ptr(&inner))
                        .entered();

                    if let Some(inner) = inner.upgrade() {
                        warn!(
                            transaction = Arc::as_ptr(&inner) as usize,
                            release_reason = "timeout",
                            "wm transaction deadline expired"
                        );
                        inner.complete();
                    } else {
                        trace!("transaction completed without removing the timer");
                    }

                    TimeoutAction::Drop
                })
                .expect("failed to register transaction deadline timer");

            let (ping, source) =
                calloop::ping::make_ping().expect("failed to create deadline removal ping");
            loop_handle
                .insert_source(source, {
                    let loop_handle = loop_handle.clone();
                    move |_, _, _| {
                        loop_handle.remove(token);
                    }
                })
                .expect("failed to register deadline removal source");

            *cell = Deadline::Registered { remove: ping };
        }
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        let _span = trace_span!("drop", transaction = ?Arc::as_ptr(&self.inner)).entered();

        if self.is_last() {
            debug!(
                transaction = self.debug_id(),
                release_reason = "last-reference-drop",
                "wm transaction completed after retained handle drop"
            );
            self.inner.complete();

            if let Deadline::Registered { remove } = &*self.deadline.borrow() {
                remove.ping();
            }
        }
    }
}

impl Blocker for TransactionBlocker {
    fn state(&self) -> BlockerState {
        if self
            .inner
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
    fn complete(&self) {
        self.completed.store(true, Ordering::Relaxed);

        let mut guard = self
            .notifications
            .lock()
            .expect("transaction notifications poisoned");
        if let Some((sender, clients)) = guard.take() {
            for client in clients {
                if let Err(error) = sender.send(client) {
                    warn!(?error, "error sending transaction blocker notification");
                }
            }
        }
    }

    fn is_completed(&self) -> bool {
        self.completed.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use smithay::reexports::calloop::EventLoop;
    use smithay::utils::{Point, Serial, Size};

    use super::{PendingConfigureState, PendingLayout, Transaction};

    fn layout(x: i32, y: i32, w: i32, h: i32) -> PendingLayout {
        PendingLayout {
            location: Point::from((x, y)),
            size: Size::from((w, h)),
        }
    }

    #[test]
    fn mark_ready_returns_latest_matching_layout() {
        let mut state = PendingConfigureState::default();
        let first = layout(0, 0, 100, 100);
        let second = layout(50, 0, 80, 100);
        let event_loop = EventLoop::<()>::try_new().expect("event loop");

        state.push(
            Serial::from(4_u32),
            first,
            Transaction::new(&event_loop.handle()),
        );
        state.push(
            Serial::from(8_u32),
            second,
            Transaction::new(&event_loop.handle()),
        );

        let matched = state.mark_ready(Serial::from(8_u32));

        assert_eq!(matched.map(|matched| matched.layout), Some(second));
        assert_eq!(state.take_ready(), Some(second));
        assert!(!state.has_pending());
    }

    #[test]
    fn mark_ready_ignores_newer_configures() {
        let mut state = PendingConfigureState::default();
        let first = layout(0, 0, 100, 100);
        let second = layout(100, 0, 100, 100);
        let event_loop = EventLoop::<()>::try_new().expect("event loop");

        state.push(
            Serial::from(4_u32),
            first,
            Transaction::new(&event_loop.handle()),
        );
        state.push(
            Serial::from(10_u32),
            second,
            Transaction::new(&event_loop.handle()),
        );

        let matched = state.mark_ready(Serial::from(5_u32));

        assert_eq!(matched.map(|matched| matched.layout), Some(first));
        assert_eq!(state.take_ready(), Some(first));
        assert!(state.has_pending());
    }

    #[test]
    fn take_ready_is_empty_without_match() {
        let mut state = PendingConfigureState::default();
        let event_loop = EventLoop::<()>::try_new().expect("event loop");
        state.push(
            Serial::from(4_u32),
            layout(0, 0, 100, 100),
            Transaction::new(&event_loop.handle()),
        );

        assert!(state.mark_ready(Serial::from(3_u32)).is_none());
        assert_eq!(state.take_ready(), None);
        assert!(state.has_pending());
    }

    #[test]
    fn latest_live_transaction_returns_latest_pending_transaction() {
        let mut state = PendingConfigureState::default();
        let event_loop = EventLoop::<()>::try_new().expect("event loop");
        let first = Transaction::new(&event_loop.handle());
        let second = Transaction::new(&event_loop.handle());

        state.push(Serial::from(4_u32), layout(0, 0, 100, 100), first);
        state.push(Serial::from(8_u32), layout(50, 0, 100, 100), second.clone());

        let live = state.latest_live_transaction().expect("live transaction");

        assert_eq!(live.debug_id(), second.debug_id());
    }
}
