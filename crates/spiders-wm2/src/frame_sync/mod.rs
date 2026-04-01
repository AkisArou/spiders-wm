mod render_snapshot;
mod transaction;

use std::sync::mpsc::Sender;

use smithay::{
    backend::allocator::dmabuf::Dmabuf,
    backend::renderer::gles::GlesRenderer,
    reexports::{
        calloop::{Interest, LoopHandle},
        wayland_server::{Client, Resource, protocol::wl_surface::WlSurface},
    },
    utils::{Logical, Point, Scale, Serial, Size},
    wayland::compositor::{
        BufferAssignment, CompositorHandler, SurfaceAttributes, add_blocker, add_pre_commit_hook,
        with_states,
    },
    wayland::shell::xdg::{ToplevelCachedState, ToplevelSurface},
};

use self::{
    render_snapshot::WindowSnapshot,
    transaction::{PendingConfigureState, PendingLayout},
};

use crate::state::SpidersWm;

pub(crate) use render_snapshot::SnapshotRenderElement;

#[derive(Debug, Clone)]
pub(crate) struct SyncHandle(transaction::Transaction);

pub(crate) fn new_sync_handle<T: 'static>(loop_handle: &LoopHandle<'static, T>) -> SyncHandle {
    SyncHandle(transaction::Transaction::new(loop_handle))
}

pub(crate) fn install_window_pre_commit_hook(toplevel: &ToplevelSurface) {
    add_pre_commit_hook::<SpidersWm, _>(toplevel.wl_surface(), |state, _dh, surface| {
        let (commit_serial, dmabuf, got_unmapped) = with_states(surface, |states| {
            let dmabuf = {
                let mut guard = states.cached_state.get::<SurfaceAttributes>();
                let got_unmapped = matches!(
                    guard.pending().buffer.as_ref(),
                    Some(BufferAssignment::Removed)
                );
                let dmabuf = match guard.pending().buffer.as_ref() {
                    Some(BufferAssignment::NewBuffer(buffer)) => {
                        smithay::wayland::dmabuf::get_dmabuf(buffer).cloned().ok()
                    }
                    _ => None,
                };

                (dmabuf, got_unmapped)
            };

            let commit_serial = states
                .cached_state
                .get::<ToplevelCachedState>()
                .pending()
                .last_acked
                .as_ref()
                .map(|configure| configure.serial);

            (commit_serial, dmabuf.0, dmabuf.1)
        });

        if got_unmapped {
            state.capture_close_snapshot(surface);
        }

        let Some(commit_serial) = commit_serial else {
            return;
        };

        let blocker_cleared_tx = state.blocker_cleared_tx.clone();
        let event_loop = state.event_loop.clone();

        let Some(record) = state.find_window_mut(surface) else {
            return;
        };

        let matched = record.frame_sync.install_commit_blockers(
            surface,
            commit_serial,
            dmabuf,
            blocker_cleared_tx,
            &event_loop,
            |state, client| {
                let display_handle = state.display_handle.clone();
                state
                    .client_compositor_state(&client)
                    .blocker_cleared(state, &display_handle);
            },
        );
        tracing::debug!(
            window = %record.id.0,
            ?commit_serial,
            had_match = matched.had_match,
            pending_configures = matched.has_pending_configures,
            transaction = ?matched.transaction_debug_id,
            waited_on_dmabuf = matched.waited_on_dmabuf,
            "wm2 matched pending configure before commit"
        );
    });
}

#[derive(Debug)]
pub(crate) struct CapturedCloseSnapshot(WindowSnapshot);

#[derive(Debug, Default)]
pub(crate) struct WindowFrameSyncState {
    close_snapshot: Option<CapturedCloseSnapshot>,
    pending_configures: PendingConfigureState,
}

#[derive(Debug, Default)]
pub(crate) struct FrameSyncState {
    closing_overlays: Vec<ClosingWindowOverlay>,
}

#[derive(Debug)]
pub(crate) struct CommitSyncOutcome {
    pub(crate) had_match: bool,
    pub(crate) has_pending_configures: bool,
    pub(crate) transaction_debug_id: Option<usize>,
    pub(crate) waited_on_dmabuf: bool,
}

#[derive(Debug)]
pub(crate) struct OverlayPushResult {
    pub(crate) transaction_debug_id: usize,
    pub(crate) carried_overlays: usize,
}

#[derive(Debug)]
pub(crate) struct BeginUnmapResult {
    pub(crate) snapshot: Option<CapturedCloseSnapshot>,
}

#[derive(Debug)]
struct ClosingWindowOverlay {
    snapshot: WindowSnapshot,
    location: Point<i32, Logical>,
    transaction: transaction::Transaction,
    presented_once: bool,
}

pub(crate) fn capture_close_snapshot(
    renderer: &mut GlesRenderer,
    surface: &WlSurface,
    scale: Scale<f64>,
    alpha: f32,
) -> Option<CapturedCloseSnapshot> {
    WindowSnapshot::capture(renderer, surface, scale, alpha).map(CapturedCloseSnapshot)
}

impl WindowFrameSyncState {
    pub(crate) fn store_close_snapshot(&mut self, snapshot: CapturedCloseSnapshot) {
        self.close_snapshot = Some(snapshot);
    }

    pub(crate) fn has_close_snapshot(&self) -> bool {
        self.close_snapshot.is_some()
    }

    pub(crate) fn begin_unmap(&mut self) -> BeginUnmapResult {
        self.pending_configures.clear();
        BeginUnmapResult {
            snapshot: self.close_snapshot.take(),
        }
    }

    pub(crate) fn has_pending_configures(&self) -> bool {
        self.pending_configures.has_pending()
    }

    pub(crate) fn live_transaction(&self) -> Option<SyncHandle> {
        self.pending_configures
            .latest_live_transaction()
            .map(SyncHandle)
    }

    pub(crate) fn track_pending_layout(
        &mut self,
        serial: Serial,
        location: Point<i32, Logical>,
        size: Size<i32, Logical>,
        sync_handle: SyncHandle,
    ) {
        self.pending_configures
            .push(serial, PendingLayout { location, size }, sync_handle.0);
    }

    pub(crate) fn take_ready_layout(
        &mut self,
    ) -> Option<(Point<i32, Logical>, Size<i32, Logical>)> {
        self.pending_configures
            .take_ready()
            .map(|layout| (layout.location, layout.size))
    }

    pub(crate) fn install_commit_blockers<T: 'static, F>(
        &mut self,
        surface: &WlSurface,
        commit_serial: Serial,
        dmabuf: Option<Dmabuf>,
        blocker_cleared_tx: Sender<Client>,
        loop_handle: &LoopHandle<'static, T>,
        notify_client_cleared: F,
    ) -> CommitSyncOutcome
    where
        F: Fn(&mut T, Client) + 'static,
    {
        let matched = self.pending_configures.mark_ready(commit_serial);
        let had_match = matched.is_some();
        let live_transaction = matched.and_then(|matched| {
            (!matched.transaction.is_completed()).then_some(matched.transaction)
        });
        let mut outcome = CommitSyncOutcome {
            had_match,
            has_pending_configures: self.pending_configures.has_pending(),
            transaction_debug_id: live_transaction
                .as_ref()
                .map(transaction::Transaction::debug_id),
            waited_on_dmabuf: false,
        };

        if let Some(transaction) = live_transaction.as_ref() {
            let is_last = transaction.is_last();
            if !is_last && let Some(client) = surface.client() {
                transaction.add_notification(blocker_cleared_tx.clone(), client.clone());
                add_blocker(surface, transaction.blocker());
            }
        }

        if let Some((blocker, source)) =
            dmabuf.and_then(|dmabuf| dmabuf.generate_blocker(Interest::READ).ok())
            && let Some(client) = surface.client()
        {
            outcome.waited_on_dmabuf = true;
            let mut transaction_for_dmabuf = live_transaction;
            let result = loop_handle.insert_source(source, move |_, _, state| {
                if let Some(transaction) = transaction_for_dmabuf.take() {
                    drop(transaction);
                }

                notify_client_cleared(state, client.clone());

                Ok(())
            });

            if result.is_ok() {
                add_blocker(surface, blocker);
            }
        }

        outcome
    }
}

impl FrameSyncState {
    pub(crate) fn overlay_count(&self) -> usize {
        self.closing_overlays.len()
    }

    pub(crate) fn prune_completed_closing_overlays(&mut self) {
        self.closing_overlays
            .retain(|overlay| !overlay.transaction.is_completed() || !overlay.presented_once);
    }

    pub(crate) fn push_closing_overlay(
        &mut self,
        snapshot: Option<CapturedCloseSnapshot>,
        location: Option<Point<i32, Logical>>,
        sync_handle: Option<SyncHandle>,
    ) -> Option<OverlayPushResult> {
        let (Some(CapturedCloseSnapshot(snapshot)), Some(location), Some(SyncHandle(transaction))) =
            (snapshot, location, sync_handle)
        else {
            return None;
        };

        let carried_overlays = self.closing_overlays.len();
        for overlay in &mut self.closing_overlays {
            overlay.transaction = transaction.clone();
        }

        let transaction_debug_id = transaction.debug_id();
        self.closing_overlays.push(ClosingWindowOverlay {
            snapshot,
            location,
            transaction,
            presented_once: false,
        });

        Some(OverlayPushResult {
            transaction_debug_id,
            carried_overlays,
        })
    }

    pub(crate) fn render_elements(
        &self,
        renderer: &mut GlesRenderer,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<SnapshotRenderElement> {
        self.closing_overlays
            .iter()
            .filter_map(|overlay| {
                let location = overlay.location.to_f64().to_physical_precise_round(scale);
                overlay
                    .snapshot
                    .render_element(renderer, location, scale, alpha)
            })
            .collect()
    }

    pub(crate) fn mark_closing_overlays_presented(&mut self) {
        for overlay in &mut self.closing_overlays {
            overlay.presented_once = true;
        }
    }
}
