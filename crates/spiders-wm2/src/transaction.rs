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

const TIME_LIMIT: Duration = Duration::from_millis(300);

#[derive(Debug, Clone)]
pub struct Transaction {
    inner: Arc<Inner>,
    deadline: Rc<RefCell<Deadline>>,
}

#[derive(Debug, Clone)]
pub struct TransactionMonitor(Weak<Inner>);

#[derive(Debug)]
pub struct TransactionBlocker(Weak<Inner>);

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

impl Transaction {
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

    pub fn blocker(&self) -> TransactionBlocker {
        TransactionBlocker(Arc::downgrade(&self.inner))
    }

    pub fn monitor(&self) -> TransactionMonitor {
        TransactionMonitor(Arc::downgrade(&self.inner))
    }

    pub fn is_completed(&self) -> bool {
        self.inner.is_completed()
    }

    pub fn is_last(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }

    pub fn add_notification(&self, sender: Sender<Client>, client: Client) {
        if self.is_completed() {
            error!("tried to add notification to a completed transaction");
            return;
        }

        let mut guard = self.inner.notifications.lock().expect("transaction notifications poisoned");
        guard.get_or_insert((sender, Vec::new())).1.push(client);
    }

    pub fn register_deadline_timer<T: 'static>(&self, event_loop: &LoopHandle<'static, T>) {
        let mut deadline = self.deadline.borrow_mut();
        if let Deadline::NotRegistered(instant) = *deadline {
            let timer = Timer::from_deadline(instant);
            let inner = Arc::downgrade(&self.inner);
            let token = event_loop
                .insert_source(timer, move |_, _, _| {
                    let _span =
                        trace_span!("transaction deadline", transaction = ?Weak::as_ptr(&inner))
                            .entered();
                    if let Some(inner) = inner.upgrade() {
                        trace!("transaction deadline reached");
                        inner.complete();
                    }

                    TimeoutAction::Drop
                })
                .expect("failed to register transaction deadline timer");

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
}

impl Drop for Transaction {
    fn drop(&mut self) {
        let _span =
            trace_span!("drop transaction", transaction = ?Arc::as_ptr(&self.inner)).entered();
        if self.is_last() {
            trace!("last transaction handle dropped; releasing blockers");
            self.inner.complete();

            if let Deadline::Registered { remove } = &*self.deadline.borrow() {
                remove.ping();
            }
        }
    }
}

impl TransactionMonitor {
    pub fn is_released(&self) -> bool {
        self.0.upgrade().is_none_or(|inner| inner.is_completed())
    }
}

impl Blocker for TransactionBlocker {
    fn state(&self) -> BlockerState {
        if self.0.upgrade().is_none_or(|inner| inner.is_completed()) {
            BlockerState::Released
        } else {
            BlockerState::Pending
        }
    }
}

impl Inner {
    fn complete(&self) {
        self.completed.store(true, Ordering::Relaxed);

        let mut guard = self.notifications.lock().expect("transaction notifications poisoned");
        if let Some((sender, clients)) = guard.take() {
            for client in clients {
                if let Err(err) = sender.send(client) {
                    warn!(?err, "error sending blocker notification");
                }
            }
        }
    }

    fn is_completed(&self) -> bool {
        self.completed.load(Ordering::Relaxed)
    }
}
