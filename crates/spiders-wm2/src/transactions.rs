use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

use crate::{
    model::{OutputId, WindowId, WmState, WorkspaceId},
    runtime::SpidersWm2,
};
use smithay::utils::Serial;
use spiders_shared::wm::StateSnapshot;

#[derive(Debug, Default)]
pub struct TransactionManager {
    next_transaction_id: u64,
    committed: Option<StateSnapshot>,
    history: VecDeque<TransactionHistoryEntry>,
    pending: Option<PendingTransaction>,
    deferred_removals: HashSet<WindowId>,
}

#[derive(Debug, Clone)]
pub struct PendingTransaction {
    pub id: u64,
    pub desired: StateSnapshot,
    pub coalescing_root_transaction_id: u64,
    pub coalescing_depth: usize,
    pub timeout_extensions: usize,
    pub timeout_extensions_budget: usize,
    pub affected_windows: HashSet<WindowId>,
    pub affected_workspaces: HashSet<WorkspaceId>,
    pub affected_outputs: HashSet<OutputId>,
    pub participants: std::collections::HashMap<WindowId, TransactionParticipant>,
    pub started_at: Instant,
    pub deadline: Instant,
    pub dirty_scopes: HashSet<DirtyScope>,
}

#[derive(Debug, Clone, Default)]
pub struct TransactionParticipant {
    pub configure_serial: Option<Serial>,
    pub acked: bool,
    pub committed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionParticipantStatus {
    Idle,
    WaitingForAck,
    WaitingForCommit,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionCommitReason {
    Ready,
    TimedOut,
    Superseded,
}

#[derive(Debug, Clone)]
pub struct TransactionHistoryEntry {
    pub id: u64,
    pub reason: TransactionCommitReason,
    pub duration_ms: u128,
    pub coalescing_root_transaction_id: u64,
    pub coalescing_depth: usize,
    pub replacement_transaction_id: Option<u64>,
    pub unresolved_window_ids: Vec<WindowId>,
    pub timeout_progress: Option<TimeoutProgress>,
    pub timeout_extensions: usize,
    pub timeout_extensions_budget: usize,
    pub ready_participant_count: usize,
    pub waiting_for_ack_count: usize,
    pub waiting_for_commit_count: usize,
    pub affected_window_count: usize,
    pub affected_workspace_count: usize,
    pub affected_output_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutProgress {
    Stalled,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParticipantProgressSummary {
    pub ready: usize,
    pub waiting_for_ack: usize,
    pub waiting_for_commit: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshPlan {
    pub transaction_id: Option<u64>,
    pub windows: HashSet<WindowId>,
    pub workspaces: HashSet<WorkspaceId>,
    pub outputs: HashSet<OutputId>,
    pub layout: LayoutRecomputePlan,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayoutRecomputePlan {
    pub workspace_roots: HashSet<WorkspaceId>,
    pub full_scene: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionCoalescedChain {
    pub root_transaction_id: u64,
    pub transaction_ids: Vec<u64>,
    pub active_transaction_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DirtyScope {
    Window(WindowId),
    Workspace(WorkspaceId),
    Output(OutputId),
    LayoutSubtree { workspace_id: WorkspaceId },
    FullScene,
}

impl TransactionManager {
    pub fn stage(&mut self, desired: StateSnapshot) {
        let transaction_id = self.allocate_transaction_id();
        let mut coalescing_root_transaction_id = transaction_id;
        let mut coalescing_depth = 0;
        let mut timeout_extensions_budget = partial_timeout_extension_budget();
        let previous_pending = self.pending.take();

        if let Some(pending) = previous_pending.as_ref() {
            coalescing_root_transaction_id = pending.coalescing_root_transaction_id;
            coalescing_depth = pending.coalescing_depth + 1;
            timeout_extensions_budget = pending.timeout_extensions_budget;
        }

        let Some(committed) = self.committed.as_ref() else {
            let mut pending = PendingTransaction::full(
                transaction_id,
                desired,
                coalescing_root_transaction_id,
                coalescing_depth,
                timeout_extensions_budget,
            );
            if let Some(previous_pending) = previous_pending.as_ref() {
                pending.preserve_progress_from(previous_pending);
                pending.preserve_dirty_scopes_from(previous_pending);
            }
            if let Some(previous_pending) = previous_pending {
                self.record_history_entry(
                    previous_pending,
                    TransactionCommitReason::Superseded,
                    Some(transaction_id),
                );
            }
            self.pending = Some(pending);
            return;
        };

        let mut pending = PendingTransaction::from_diff(
            transaction_id,
            committed,
            desired,
            coalescing_root_transaction_id,
            coalescing_depth,
            timeout_extensions_budget,
        );
        if let Some(previous_pending) = previous_pending.as_ref() {
            pending.preserve_progress_from(previous_pending);
            pending.preserve_dirty_scopes_from(previous_pending);
        }
        if let Some(previous_pending) = previous_pending {
            self.record_history_entry(
                previous_pending,
                TransactionCommitReason::Superseded,
                Some(transaction_id),
            );
        }
        self.pending = Some(pending);
    }

    pub fn pending(&self) -> Option<&PendingTransaction> {
        self.pending.as_ref()
    }

    pub fn committed(&self) -> Option<&StateSnapshot> {
        self.committed.as_ref()
    }

    pub fn history(&self) -> &VecDeque<TransactionHistoryEntry> {
        &self.history
    }

    pub fn coalesced_chains(&self) -> Vec<TransactionCoalescedChain> {
        let mut chains = BTreeMap::<u64, Vec<(usize, u64)>>::new();

        for entry in &self.history {
            chains
                .entry(entry.coalescing_root_transaction_id)
                .or_default()
                .push((entry.coalescing_depth, entry.id));
        }

        let mut result = chains
            .into_iter()
            .map(|(root_transaction_id, mut entries)| {
                entries.sort_by_key(|(depth, _)| *depth);
                let active_transaction_id = self
                    .pending
                    .as_ref()
                    .filter(|pending| pending.coalescing_root_transaction_id == root_transaction_id)
                    .map(|pending| pending.id);

                TransactionCoalescedChain {
                    root_transaction_id,
                    transaction_ids: entries.into_iter().map(|(_, id)| id).collect(),
                    active_transaction_id,
                }
            })
            .collect::<Vec<_>>();

        if let Some(pending) = self.pending.as_ref() {
            if !result
                .iter()
                .any(|chain| chain.root_transaction_id == pending.coalescing_root_transaction_id)
            {
                result.push(TransactionCoalescedChain {
                    root_transaction_id: pending.coalescing_root_transaction_id,
                    transaction_ids: Vec::new(),
                    active_transaction_id: Some(pending.id),
                });
            }
        }

        result.sort_by_key(|chain| chain.root_transaction_id);
        result
    }

    pub fn deferred_removals(&self) -> &HashSet<WindowId> {
        &self.deferred_removals
    }

    pub fn defer_window_removal(&mut self, window_id: WindowId) {
        self.deferred_removals.insert(window_id);
    }

    pub fn pending_refresh_plan(&self, wm: &WmState) -> Option<RefreshPlan> {
        self.pending
            .as_ref()
            .map(|pending| pending.refresh_plan(wm))
    }

    pub fn pending_debug_summary(&self, wm: &WmState) -> Option<String> {
        self.pending.as_ref().map(|pending| {
            let plan = pending.refresh_plan(wm);
            let progress = pending.participant_progress();
            format!(
                "tx={} windows={} workspaces={} outputs={} layout_roots={} full_scene={} participants={} ready={} waiting_ack={} waiting_commit={}",
                pending.id,
                plan.windows.len(),
                plan.workspaces.len(),
                plan.outputs.len(),
                plan.layout.workspace_roots.len(),
                plan.layout.full_scene,
                pending.participants.len(),
                progress.ready,
                progress.waiting_for_ack,
                progress.waiting_for_commit,
            )
        })
    }

    pub fn register_configure(&mut self, window_id: &WindowId, serial: Serial) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };

        let Some(participant) = pending.participants.get_mut(window_id) else {
            return;
        };

        participant.register_configure(serial);
    }

    pub fn mark_configure_acked(&mut self, window_id: &WindowId, serial: Serial) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };

        let Some(participant) = pending.participants.get_mut(window_id) else {
            return;
        };

        participant.mark_configure_acked(serial);
    }

    pub fn mark_window_committed(&mut self, window_id: &WindowId) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };

        let Some(participant) = pending.participants.get_mut(window_id) else {
            return;
        };

        participant.mark_committed();
    }

    #[cfg(test)]
    pub fn is_pending_ready(&self) -> bool {
        self.pending_resolution(Instant::now()).is_some()
    }

    pub fn pending_resolution(&self, now: Instant) -> Option<TransactionCommitReason> {
        self.pending
            .as_ref()
            .and_then(|pending| pending.resolution(now))
    }

    pub fn extend_partial_timeout(&mut self, now: Instant) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };

        if now < pending.deadline
            || pending.timeout_progress() != TimeoutProgress::Partial
            || pending.timeout_extensions >= pending.timeout_extensions_budget
        {
            return false;
        }

        pending.deadline = now + transaction_timeout_extension();
        pending.timeout_extensions += 1;
        true
    }

    pub fn commit_pending(&mut self, reason: TransactionCommitReason) {
        if let Some(pending) = self.pending.take() {
            let desired = pending.desired.clone();
            self.record_history_entry(pending, reason, None);
            self.committed = Some(desired);
        }
    }

    pub fn drain_deferred_removals(&mut self) -> Vec<WindowId> {
        self.deferred_removals.drain().collect()
    }

    fn allocate_transaction_id(&mut self) -> u64 {
        self.next_transaction_id += 1;
        self.next_transaction_id
    }

    fn record_history_entry(
        &mut self,
        pending: PendingTransaction,
        reason: TransactionCommitReason,
        replacement_transaction_id: Option<u64>,
    ) {
        self.history.push_front(TransactionHistoryEntry {
            id: pending.id,
            reason,
            duration_ms: pending.started_at.elapsed().as_millis(),
            coalescing_root_transaction_id: pending.coalescing_root_transaction_id,
            coalescing_depth: pending.coalescing_depth,
            replacement_transaction_id,
            unresolved_window_ids: pending.unresolved_window_ids(),
            timeout_progress: (reason == TransactionCommitReason::TimedOut)
                .then(|| pending.timeout_progress()),
            timeout_extensions: pending.timeout_extensions,
            timeout_extensions_budget: pending.timeout_extensions_budget,
            ready_participant_count: pending.participant_progress().ready,
            waiting_for_ack_count: pending.participant_progress().waiting_for_ack,
            waiting_for_commit_count: pending.participant_progress().waiting_for_commit,
            affected_window_count: pending.affected_windows.len(),
            affected_workspace_count: pending.affected_workspaces.len(),
            affected_output_count: pending.affected_outputs.len(),
        });
        while self.history.len() > 16 {
            self.history.pop_back();
        }
    }
}

impl PendingTransaction {
    fn full(
        id: u64,
        desired: StateSnapshot,
        coalescing_root_transaction_id: u64,
        coalescing_depth: usize,
        timeout_extensions_budget: usize,
    ) -> Self {
        let affected_windows = desired
            .windows
            .iter()
            .map(|window| window.id.clone())
            .collect::<HashSet<_>>();

        Self {
            id,
            coalescing_root_transaction_id,
            coalescing_depth,
            timeout_extensions: 0,
            timeout_extensions_budget,
            participants: participants_for_windows(&affected_windows),
            started_at: Instant::now(),
            affected_windows,
            affected_workspaces: desired
                .workspaces
                .iter()
                .map(|workspace| workspace.id.clone())
                .collect(),
            affected_outputs: desired
                .outputs
                .iter()
                .map(|output| output.id.clone())
                .collect(),
            deadline: Instant::now() + transaction_timeout(),
            dirty_scopes: HashSet::from([DirtyScope::FullScene]),
            desired,
        }
    }

    fn from_diff(
        id: u64,
        committed: &StateSnapshot,
        desired: StateSnapshot,
        coalescing_root_transaction_id: u64,
        coalescing_depth: usize,
        timeout_extensions_budget: usize,
    ) -> Self {
        let committed_windows = committed
            .windows
            .iter()
            .map(|window| (window.id.clone(), window))
            .collect::<std::collections::HashMap<_, _>>();
        let committed_workspaces = committed
            .workspaces
            .iter()
            .map(|workspace| (workspace.id.clone(), workspace))
            .collect::<std::collections::HashMap<_, _>>();
        let committed_outputs = committed
            .outputs
            .iter()
            .map(|output| (output.id.clone(), output))
            .collect::<std::collections::HashMap<_, _>>();

        let mut affected_windows = desired
            .windows
            .iter()
            .filter(|window| committed_windows.get(&window.id) != Some(window))
            .map(|window| window.id.clone())
            .chain(
                committed
                    .windows
                    .iter()
                    .filter(|window| !desired.windows.iter().any(|next| next.id == window.id))
                    .map(|window| window.id.clone()),
            )
            .collect::<HashSet<_>>();

        let affected_workspaces = desired
            .workspaces
            .iter()
            .filter(|workspace| committed_workspaces.get(&workspace.id) != Some(workspace))
            .map(|workspace| workspace.id.clone())
            .chain(
                committed
                    .workspaces
                    .iter()
                    .filter(|workspace| {
                        !desired
                            .workspaces
                            .iter()
                            .any(|next| next.id == workspace.id)
                    })
                    .map(|workspace| workspace.id.clone()),
            )
            .collect();

        let affected_outputs = desired
            .outputs
            .iter()
            .filter(|output| committed_outputs.get(&output.id) != Some(output))
            .map(|output| output.id.clone())
            .chain(
                committed
                    .outputs
                    .iter()
                    .filter(|output| !desired.outputs.iter().any(|next| next.id == output.id))
                    .map(|output| output.id.clone()),
            )
            .collect::<HashSet<_>>();

        affected_windows.extend(
            committed
                .visible_window_ids
                .iter()
                .filter(|window_id| !desired.visible_window_ids.contains(window_id))
                .cloned(),
        );
        affected_windows.extend(
            desired
                .visible_window_ids
                .iter()
                .filter(|window_id| !committed.visible_window_ids.contains(window_id))
                .cloned(),
        );

        if focused_fullscreen_window(committed) != focused_fullscreen_window(&desired) {
            affected_windows.extend(committed.visible_window_ids.iter().cloned());
            affected_windows.extend(desired.visible_window_ids.iter().cloned());
        }

        let dirty_scopes = dirty_scopes_for_diff(
            &affected_windows,
            &affected_workspaces,
            &affected_outputs,
            committed,
            &desired,
        );

        Self {
            id,
            coalescing_root_transaction_id,
            coalescing_depth,
            timeout_extensions: 0,
            timeout_extensions_budget,
            participants: participants_for_windows(&affected_windows),
            desired,
            affected_windows,
            affected_workspaces,
            affected_outputs,
            started_at: Instant::now(),
            deadline: Instant::now() + transaction_timeout(),
            dirty_scopes,
        }
    }

    fn resolution(&self, now: Instant) -> Option<TransactionCommitReason> {
        if now >= self.deadline {
            Some(TransactionCommitReason::TimedOut)
        } else if self
            .participants
            .values()
            .all(TransactionParticipant::is_ready)
        {
            Some(TransactionCommitReason::Ready)
        } else {
            None
        }
    }

    fn refresh_plan(&self, wm: &WmState) -> RefreshPlan {
        let mut plan = refresh_plan_for_scopes(&self.dirty_scopes, wm);
        plan.transaction_id = Some(self.id);
        plan
    }

    fn unresolved_window_ids(&self) -> Vec<WindowId> {
        self.participants
            .iter()
            .filter_map(|(window_id, participant)| {
                (!matches!(
                    participant.status(),
                    TransactionParticipantStatus::Idle | TransactionParticipantStatus::Ready
                ))
                .then(|| window_id.clone())
            })
            .collect()
    }

    pub(crate) fn participant_progress(&self) -> ParticipantProgressSummary {
        self.participants.values().fold(
            ParticipantProgressSummary {
                ready: 0,
                waiting_for_ack: 0,
                waiting_for_commit: 0,
            },
            |mut summary, participant| {
                match participant.status() {
                    TransactionParticipantStatus::Idle | TransactionParticipantStatus::Ready => {
                        summary.ready += 1;
                    }
                    TransactionParticipantStatus::WaitingForAck => {
                        summary.waiting_for_ack += 1;
                    }
                    TransactionParticipantStatus::WaitingForCommit => {
                        summary.waiting_for_commit += 1;
                    }
                }

                summary
            },
        )
    }

    pub(crate) fn timeout_progress(&self) -> TimeoutProgress {
        let progress = self.participant_progress();

        if progress.ready > 0 {
            TimeoutProgress::Partial
        } else {
            TimeoutProgress::Stalled
        }
    }

    fn preserve_progress_from(&mut self, previous: &PendingTransaction) {
        for (window_id, participant) in &mut self.participants {
            let Some(previous_participant) = previous.participants.get(window_id) else {
                continue;
            };

            if previous_participant.status() == TransactionParticipantStatus::Ready {
                *participant = previous_participant.clone();
            }
        }
    }

    fn preserve_dirty_scopes_from(&mut self, previous: &PendingTransaction) {
        self.dirty_scopes
            .extend(previous.dirty_scopes.iter().cloned());
    }
}

impl TransactionParticipant {
    fn register_configure(&mut self, serial: Serial) {
        self.configure_serial = Some(serial);
        self.acked = false;
        self.committed = false;
    }

    fn mark_configure_acked(&mut self, serial: Serial) {
        if self.configure_serial == Some(serial) {
            self.acked = true;
        }
    }

    fn mark_committed(&mut self) {
        if matches!(
            self.status(),
            TransactionParticipantStatus::WaitingForCommit
        ) {
            self.committed = true;
        }
    }

    fn is_ready(&self) -> bool {
        self.configure_serial.is_none() || (self.acked && self.committed)
    }

    pub(crate) fn status(&self) -> TransactionParticipantStatus {
        match (self.configure_serial.is_some(), self.acked, self.committed) {
            (false, _, _) => TransactionParticipantStatus::Idle,
            (true, false, _) => TransactionParticipantStatus::WaitingForAck,
            (true, true, false) => TransactionParticipantStatus::WaitingForCommit,
            (true, true, true) => TransactionParticipantStatus::Ready,
        }
    }
}

fn participants_for_windows(
    affected_windows: &HashSet<WindowId>,
) -> std::collections::HashMap<WindowId, TransactionParticipant> {
    affected_windows
        .iter()
        .cloned()
        .map(|window_id| (window_id, TransactionParticipant::default()))
        .collect()
}

fn transaction_timeout() -> Duration {
    Duration::from_millis(150)
}

fn transaction_timeout_extension() -> Duration {
    Duration::from_millis(75)
}

fn partial_timeout_extension_budget() -> usize {
    1
}

fn dirty_scopes_for_diff(
    affected_windows: &HashSet<WindowId>,
    affected_workspaces: &HashSet<WorkspaceId>,
    affected_outputs: &HashSet<OutputId>,
    committed: &StateSnapshot,
    desired: &StateSnapshot,
) -> HashSet<DirtyScope> {
    let mut scopes = HashSet::new();

    scopes.extend(affected_windows.iter().cloned().map(DirtyScope::Window));
    scopes.extend(
        affected_workspaces
            .iter()
            .cloned()
            .map(DirtyScope::Workspace),
    );
    scopes.extend(affected_outputs.iter().cloned().map(DirtyScope::Output));

    let layout_workspaces = affected_workspaces
        .iter()
        .cloned()
        .chain(layout_workspaces_for_windows(committed, affected_windows))
        .chain(layout_workspaces_for_windows(desired, affected_windows))
        .collect::<HashSet<_>>();

    if focused_fullscreen_window(committed).is_some() || !layout_workspaces.is_empty() {
        scopes.extend(
            layout_workspaces
                .into_iter()
                .map(|workspace_id| DirtyScope::LayoutSubtree { workspace_id }),
        );
    }

    if !affected_outputs.is_empty() && affected_outputs.len() >= committed.outputs.len() {
        scopes.insert(DirtyScope::FullScene);
    }

    scopes
}

fn layout_workspaces_for_windows<'a>(
    snapshot: &'a StateSnapshot,
    affected_windows: &'a HashSet<WindowId>,
) -> impl Iterator<Item = WorkspaceId> + 'a {
    snapshot.windows.iter().filter_map(|window| {
        affected_windows
            .contains(&window.id)
            .then(|| window.workspace_id.clone())
            .flatten()
    })
}

fn refresh_plan_for_scopes(dirty_scopes: &HashSet<DirtyScope>, wm: &WmState) -> RefreshPlan {
    if dirty_scopes.contains(&DirtyScope::FullScene) {
        return RefreshPlan {
            transaction_id: None,
            windows: wm.windows.keys().cloned().collect(),
            workspaces: wm.workspaces.keys().cloned().collect(),
            outputs: wm
                .workspaces
                .values()
                .filter_map(|workspace| workspace.output.clone())
                .collect(),
            layout: LayoutRecomputePlan {
                workspace_roots: wm.workspaces.keys().cloned().collect(),
                full_scene: true,
            },
        };
    }

    let mut plan = RefreshPlan::default();

    for scope in dirty_scopes {
        match scope {
            DirtyScope::Window(window_id) => {
                plan.windows.insert(window_id.clone());
            }
            DirtyScope::Workspace(workspace_id) | DirtyScope::LayoutSubtree { workspace_id } => {
                plan.workspaces.insert(workspace_id.clone());
                plan.layout.workspace_roots.insert(workspace_id.clone());

                if let Some(workspace) = wm.workspaces.get(workspace_id) {
                    plan.windows.extend(workspace.windows.iter().cloned());

                    if let Some(output_id) = workspace.output.clone() {
                        plan.outputs.insert(output_id);
                    }
                }
            }
            DirtyScope::Output(output_id) => {
                plan.outputs.insert(output_id.clone());

                for workspace in wm.workspaces.values() {
                    if workspace.output.as_ref() == Some(output_id) {
                        plan.workspaces.insert(workspace.id.clone());
                        plan.layout.workspace_roots.insert(workspace.id.clone());
                        plan.windows.extend(workspace.windows.iter().cloned());
                    }
                }

                for (window_id, window) in &wm.windows {
                    if window.output.as_ref() == Some(output_id) {
                        plan.windows.insert(window_id.clone());
                    }
                }
            }
            DirtyScope::FullScene => unreachable!(),
        }
    }

    plan
}

fn focused_fullscreen_window(snapshot: &StateSnapshot) -> Option<&WindowId> {
    let focused_window_id = snapshot.focused_window_id.as_ref()?;
    let window = snapshot
        .windows
        .iter()
        .find(|window| &window.id == focused_window_id)?;

    window.mode.is_fullscreen().then_some(focused_window_id)
}

impl SpidersWm2 {
    pub fn sync_desired_transaction(&mut self) {
        let desired = self.app.wm.snapshot(
            &self.app.topology.outputs,
            self.app.config_runtime.current(),
        );
        self.runtime.transactions.stage(desired);
    }

    pub fn maybe_commit_pending_transaction(&mut self) {
        if let Some(reason) = self.runtime.transactions.pending_resolution(Instant::now()) {
            self.runtime.transactions.commit_pending(reason);
            self.app.layout.commit_desired();
            self.runtime.render_plan.promote_staged();
            self.finalize_deferred_window_removals();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        time::{Duration, Instant},
    };

    use super::{
        DirtyScope, PendingTransaction, TimeoutProgress, TransactionCoalescedChain,
        TransactionCommitReason, TransactionManager, TransactionParticipantStatus,
    };
    use crate::model::{ManagedWindowState, WmState, WorkspaceState};
    use smithay::utils::SERIAL_COUNTER;
    use spiders_shared::{
        ids::{OutputId, WindowId, WorkspaceId},
        wm::{
            OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowMode, WindowSnapshot,
            WorkspaceSnapshot,
        },
    };

    fn window(id: &str, workspace_id: &str, focused: bool, mode: WindowMode) -> WindowSnapshot {
        WindowSnapshot {
            id: WindowId::from(id),
            shell: ShellKind::XdgToplevel,
            app_id: None,
            title: None,
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            mode,
            focused,
            urgent: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(WorkspaceId::from(workspace_id)),
            workspaces: vec![],
        }
    }

    fn workspace(id: &str, visible: bool, focused: bool) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::from(id),
            name: id.into(),
            output_id: Some(OutputId::from("out-1")),
            active_workspaces: vec![id.into()],
            focused,
            visible,
            effective_layout: None,
        }
    }

    fn snapshot(
        focused_window_id: Option<&str>,
        visible_window_ids: &[&str],
        workspaces: Vec<WorkspaceSnapshot>,
        windows: Vec<WindowSnapshot>,
    ) -> StateSnapshot {
        StateSnapshot {
            focused_window_id: focused_window_id.map(WindowId::from),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "winit".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 1280,
                logical_height: 720,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces,
            windows,
            visible_window_ids: visible_window_ids
                .iter()
                .map(|id| WindowId::from(*id))
                .collect(),
            workspace_names: vec!["ws-1".into(), "ws-2".into()],
        }
    }

    fn wm_for_refresh_plan() -> WmState {
        let mut wm = WmState::default();

        wm.focused_output = Some(OutputId::from("out-1"));
        wm.workspaces.insert(
            WorkspaceId::from("ws-1"),
            WorkspaceState {
                id: WorkspaceId::from("ws-1"),
                name: "ws-1".into(),
                output: Some(OutputId::from("out-1")),
                windows: vec![WindowId::from("w1"), WindowId::from("w2")],
            },
        );
        wm.workspaces.insert(
            WorkspaceId::from("ws-2"),
            WorkspaceState {
                id: WorkspaceId::from("ws-2"),
                name: "ws-2".into(),
                output: Some(OutputId::from("out-2")),
                windows: vec![WindowId::from("w3")],
            },
        );

        wm.windows.insert(
            WindowId::from("w1"),
            ManagedWindowState::tiled(
                WindowId::from("w1"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );
        wm.windows.insert(
            WindowId::from("w2"),
            ManagedWindowState::tiled(
                WindowId::from("w2"),
                WorkspaceId::from("ws-1"),
                Some(OutputId::from("out-1")),
            ),
        );
        wm.windows.insert(
            WindowId::from("w3"),
            ManagedWindowState::tiled(
                WindowId::from("w3"),
                WorkspaceId::from("ws-2"),
                Some(OutputId::from("out-2")),
            ),
        );

        wm
    }

    #[test]
    fn diff_marks_old_and_new_visible_workspace_windows_affected() {
        let committed = snapshot(
            Some("w1"),
            &["w1"],
            vec![
                workspace("ws-1", true, true),
                workspace("ws-2", false, false),
            ],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-2", false, WindowMode::Tiled),
            ],
        );
        let desired = snapshot(
            Some("w2"),
            &["w2"],
            vec![
                workspace("ws-1", false, false),
                workspace("ws-2", true, true),
            ],
            vec![
                window("w1", "ws-1", false, WindowMode::Tiled),
                window("w2", "ws-2", true, WindowMode::Tiled),
            ],
        );

        let pending = PendingTransaction::from_diff(1, &committed, desired, 1, 0, 1);

        assert!(pending.affected_windows.contains(&WindowId::from("w1")));
        assert!(pending.affected_windows.contains(&WindowId::from("w2")));
    }

    #[test]
    fn diff_marks_siblings_affected_when_fullscreen_focus_changes() {
        let committed = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![workspace("ws-1", true, true)],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
            ],
        );
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![workspace("ws-1", true, true)],
            vec![
                window("w1", "ws-1", true, WindowMode::Fullscreen),
                window("w2", "ws-1", false, WindowMode::Tiled),
            ],
        );

        let pending = PendingTransaction::from_diff(1, &committed, desired, 1, 0, 1);

        assert!(pending.affected_windows.contains(&WindowId::from("w1")));
        assert!(pending.affected_windows.contains(&WindowId::from("w2")));
    }

    #[test]
    fn diff_does_not_expand_all_visible_windows_for_output_only_change() {
        let committed = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![
                workspace("ws-1", true, true),
                workspace("ws-2", false, false),
            ],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
                window("w3", "ws-2", false, WindowMode::Tiled),
            ],
        );
        let mut desired = committed.clone();
        desired.outputs[0].logical_width = 1920;

        let pending = PendingTransaction::from_diff(1, &committed, desired, 1, 0, 1);

        assert!(pending.affected_outputs.contains(&OutputId::from("out-1")));
        assert!(!pending.affected_windows.contains(&WindowId::from("w1")));
        assert!(!pending.affected_windows.contains(&WindowId::from("w2")));
        assert!(!pending.affected_windows.contains(&WindowId::from("w3")));
    }

    #[test]
    fn transaction_waits_for_matching_ack_and_commit_before_ready() {
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);

        let serial = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial);

        assert!(!transactions.is_pending_ready());

        transactions.mark_window_committed(&WindowId::from("w1"));
        assert!(!transactions.is_pending_ready());

        transactions.mark_configure_acked(&WindowId::from("w1"), serial);
        assert!(!transactions.is_pending_ready());

        transactions.mark_window_committed(&WindowId::from("w1"));
        assert!(transactions.is_pending_ready());
    }

    #[test]
    fn transaction_ignores_non_matching_ack_serial() {
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);

        let serial = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial);
        transactions.mark_configure_acked(&WindowId::from("w1"), SERIAL_COUNTER.next_serial());
        transactions.mark_window_committed(&WindowId::from("w1"));

        assert!(!transactions.is_pending_ready());
    }

    #[test]
    fn transaction_ignores_ack_for_untracked_window() {
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);
        transactions.register_configure(&WindowId::from("w1"), SERIAL_COUNTER.next_serial());
        transactions.mark_configure_acked(&WindowId::from("ghost"), SERIAL_COUNTER.next_serial());
        transactions.mark_window_committed(&WindowId::from("ghost"));

        assert!(!transactions.is_pending_ready());
        assert!(transactions
            .pending()
            .unwrap()
            .participants
            .contains_key(&WindowId::from("w1")));
        assert!(!transactions
            .pending()
            .unwrap()
            .participants
            .contains_key(&WindowId::from("ghost")));
    }

    #[test]
    fn transaction_reconfigure_resets_participant_progress() {
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);

        let first_serial = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), first_serial);
        transactions.mark_configure_acked(&WindowId::from("w1"), first_serial);
        transactions.mark_window_committed(&WindowId::from("w1"));
        assert!(transactions.is_pending_ready());

        let second_serial = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), second_serial);

        assert!(!transactions.is_pending_ready());

        let participant = transactions
            .pending()
            .unwrap()
            .participants
            .get(&WindowId::from("w1"))
            .unwrap();
        assert_eq!(participant.configure_serial, Some(second_serial));
        assert!(!participant.acked);
        assert!(!participant.committed);
    }

    #[test]
    fn transaction_waits_for_all_participants_before_ready() {
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![workspace("ws-1", true, true)],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);

        let serial_1 = SERIAL_COUNTER.next_serial();
        let serial_2 = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial_1);
        transactions.register_configure(&WindowId::from("w2"), serial_2);

        transactions.mark_configure_acked(&WindowId::from("w1"), serial_1);
        transactions.mark_window_committed(&WindowId::from("w1"));
        assert!(!transactions.is_pending_ready());

        transactions.mark_configure_acked(&WindowId::from("w2"), serial_2);
        assert!(!transactions.is_pending_ready());

        transactions.mark_window_committed(&WindowId::from("w2"));
        assert!(transactions.is_pending_ready());
    }

    #[test]
    fn timeout_progress_reports_partial_when_some_participants_are_ready() {
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![workspace("ws-1", true, true)],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);
        let serial_1 = SERIAL_COUNTER.next_serial();
        let serial_2 = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial_1);
        transactions.register_configure(&WindowId::from("w2"), serial_2);
        transactions.mark_configure_acked(&WindowId::from("w1"), serial_1);
        transactions.mark_window_committed(&WindowId::from("w1"));

        assert_eq!(
            transactions.pending().unwrap().timeout_progress(),
            TimeoutProgress::Partial
        );
    }

    #[test]
    fn timeout_progress_reports_stalled_when_nothing_is_ready() {
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);
        transactions.register_configure(&WindowId::from("w1"), SERIAL_COUNTER.next_serial());

        assert_eq!(
            transactions.pending().unwrap().timeout_progress(),
            TimeoutProgress::Stalled
        );
    }

    #[test]
    fn transaction_timeout_allows_pending_commit() {
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);

        let serial = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial);

        if let Some(pending) = transactions.pending.as_mut() {
            pending.deadline = Instant::now() - Duration::from_millis(1);
        }

        assert!(transactions.is_pending_ready());
    }

    #[test]
    fn partial_timeout_can_be_extended_before_commit() {
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![workspace("ws-1", true, true)],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);
        let serial_1 = SERIAL_COUNTER.next_serial();
        let serial_2 = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial_1);
        transactions.register_configure(&WindowId::from("w2"), serial_2);
        transactions.mark_configure_acked(&WindowId::from("w1"), serial_1);
        transactions.mark_window_committed(&WindowId::from("w1"));

        let now = Instant::now();
        if let Some(pending) = transactions.pending.as_mut() {
            pending.deadline = now - Duration::from_millis(1);
        }

        assert!(transactions.extend_partial_timeout(now));
        assert_eq!(transactions.pending().unwrap().timeout_extensions, 1);
        assert_eq!(transactions.pending_resolution(now), None);
    }

    #[test]
    fn partial_timeout_extension_budget_is_capped() {
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![workspace("ws-1", true, true)],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);
        let serial_1 = SERIAL_COUNTER.next_serial();
        let serial_2 = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial_1);
        transactions.register_configure(&WindowId::from("w2"), serial_2);
        transactions.mark_configure_acked(&WindowId::from("w1"), serial_1);
        transactions.mark_window_committed(&WindowId::from("w1"));

        let now = Instant::now();
        if let Some(pending) = transactions.pending.as_mut() {
            pending.deadline = now - Duration::from_millis(1);
        }
        assert!(transactions.extend_partial_timeout(now));

        let later = Instant::now();
        if let Some(pending) = transactions.pending.as_mut() {
            pending.deadline = later - Duration::from_millis(1);
        }
        assert!(!transactions.extend_partial_timeout(later));
        assert_eq!(
            transactions.pending_resolution(later),
            Some(TransactionCommitReason::TimedOut)
        );
    }

    #[test]
    fn stalled_timeout_does_not_extend() {
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);
        transactions.register_configure(&WindowId::from("w1"), SERIAL_COUNTER.next_serial());

        let now = Instant::now();
        if let Some(pending) = transactions.pending.as_mut() {
            pending.deadline = now - Duration::from_millis(1);
        }

        assert!(!transactions.extend_partial_timeout(now));
        assert_eq!(
            transactions.pending_resolution(now),
            Some(TransactionCommitReason::TimedOut)
        );
    }

    #[test]
    fn commit_pending_records_ready_history_entry() {
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);
        transactions.commit_pending(TransactionCommitReason::Ready);

        assert_eq!(
            transactions.history().front().unwrap().reason,
            TransactionCommitReason::Ready
        );
        assert_eq!(transactions.history().front().unwrap().duration_ms, 0);
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .timeout_extensions_budget,
            1
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .ready_participant_count,
            1
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .waiting_for_ack_count,
            0
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .waiting_for_commit_count,
            0
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .coalescing_root_transaction_id,
            1
        );
        assert_eq!(transactions.history().front().unwrap().coalescing_depth, 0);
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .replacement_transaction_id,
            None
        );
    }

    #[test]
    fn pending_resolution_reports_timeout_reason() {
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);
        let serial = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial);

        if let Some(pending) = transactions.pending.as_mut() {
            pending.deadline = Instant::now() - Duration::from_millis(1);
        }

        assert_eq!(
            transactions.pending_resolution(Instant::now()),
            Some(TransactionCommitReason::TimedOut)
        );
    }

    #[test]
    fn timeout_commit_records_unresolved_window_ids() {
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);
        let serial = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial);
        transactions.commit_pending(TransactionCommitReason::TimedOut);

        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .unresolved_window_ids,
            vec![WindowId::from("w1")]
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .ready_participant_count,
            0
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .waiting_for_ack_count,
            1
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .waiting_for_commit_count,
            0
        );
        assert_eq!(
            transactions.history().front().unwrap().timeout_progress,
            Some(TimeoutProgress::Stalled)
        );
        assert_eq!(
            transactions.history().front().unwrap().timeout_extensions,
            0
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .timeout_extensions_budget,
            1
        );
    }

    #[test]
    fn partial_timeout_history_records_progress_and_extensions() {
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![workspace("ws-1", true, true)],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();

        transactions.stage(desired);
        let serial_1 = SERIAL_COUNTER.next_serial();
        let serial_2 = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial_1);
        transactions.register_configure(&WindowId::from("w2"), serial_2);
        transactions.mark_configure_acked(&WindowId::from("w1"), serial_1);
        transactions.mark_window_committed(&WindowId::from("w1"));

        let now = Instant::now();
        if let Some(pending) = transactions.pending.as_mut() {
            pending.deadline = now - Duration::from_millis(1);
        }
        assert!(transactions.extend_partial_timeout(now));

        let later = Instant::now();
        if let Some(pending) = transactions.pending.as_mut() {
            pending.deadline = later - Duration::from_millis(1);
        }
        transactions.commit_pending(TransactionCommitReason::TimedOut);

        assert_eq!(
            transactions.history().front().unwrap().timeout_progress,
            Some(TimeoutProgress::Partial)
        );
        assert_eq!(
            transactions.history().front().unwrap().timeout_extensions,
            1
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .timeout_extensions_budget,
            1
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .ready_participant_count,
            1
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .waiting_for_ack_count,
            1
        );
    }

    #[test]
    fn superseded_chain_preserves_timeout_extension_budget() {
        let committed = snapshot(
            Some("w0"),
            &["w0"],
            vec![workspace("ws-0", true, true)],
            vec![window("w0", "ws-0", true, WindowMode::Tiled)],
        );
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![workspace("ws-1", true, true)],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
            ],
        );
        let replacement = snapshot(
            Some("w3"),
            &["w3"],
            vec![workspace("ws-2", true, true)],
            vec![window("w3", "ws-2", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();
        transactions.committed = Some(committed);

        transactions.stage(desired);
        let serial_1 = SERIAL_COUNTER.next_serial();
        let serial_2 = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial_1);
        transactions.register_configure(&WindowId::from("w2"), serial_2);
        transactions.mark_configure_acked(&WindowId::from("w1"), serial_1);
        transactions.mark_window_committed(&WindowId::from("w1"));

        let now = Instant::now();
        if let Some(pending) = transactions.pending.as_mut() {
            pending.deadline = now - Duration::from_millis(1);
        }
        assert!(transactions.extend_partial_timeout(now));

        transactions.stage(replacement);

        assert_eq!(transactions.pending().unwrap().timeout_extensions_budget, 1);
        assert_eq!(transactions.pending().unwrap().timeout_extensions, 0);
        assert_eq!(
            transactions.history().front().unwrap().timeout_extensions,
            1
        );
    }

    #[test]
    fn superseded_chain_preserves_ready_participant_progress_for_same_window() {
        let committed = snapshot(
            Some("w0"),
            &["w0"],
            vec![workspace("ws-0", true, true)],
            vec![window("w0", "ws-0", true, WindowMode::Tiled)],
        );
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![workspace("ws-1", true, true)],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
            ],
        );
        let replacement = snapshot(
            Some("w1"),
            &["w1", "w3"],
            vec![workspace("ws-1", true, true)],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w3", "ws-1", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();
        transactions.committed = Some(committed);

        transactions.stage(desired);
        let serial_1 = SERIAL_COUNTER.next_serial();
        let serial_2 = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial_1);
        transactions.register_configure(&WindowId::from("w2"), serial_2);
        transactions.mark_configure_acked(&WindowId::from("w1"), serial_1);
        transactions.mark_window_committed(&WindowId::from("w1"));

        transactions.stage(replacement);

        let participant = transactions
            .pending()
            .unwrap()
            .participants
            .get(&WindowId::from("w1"))
            .unwrap();
        assert_eq!(participant.status(), TransactionParticipantStatus::Ready);

        let newcomer = transactions
            .pending()
            .unwrap()
            .participants
            .get(&WindowId::from("w3"))
            .unwrap();
        assert_eq!(newcomer.status(), TransactionParticipantStatus::Idle);
    }

    #[test]
    fn superseded_chain_does_not_preserve_incomplete_participant_progress() {
        let committed = snapshot(
            Some("w0"),
            &["w0"],
            vec![workspace("ws-0", true, true)],
            vec![window("w0", "ws-0", true, WindowMode::Tiled)],
        );
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let replacement = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let mut transactions = TransactionManager::default();
        transactions.committed = Some(committed);

        transactions.stage(desired);
        let serial = SERIAL_COUNTER.next_serial();
        transactions.register_configure(&WindowId::from("w1"), serial);
        transactions.mark_configure_acked(&WindowId::from("w1"), serial);

        transactions.stage(replacement);

        let participant = transactions
            .pending()
            .unwrap()
            .participants
            .get(&WindowId::from("w1"))
            .unwrap();
        assert_eq!(participant.status(), TransactionParticipantStatus::Idle);
    }

    #[test]
    fn superseded_chain_preserves_prior_layout_dirty_scope() {
        let committed = snapshot(
            Some("w0"),
            &["w0"],
            vec![workspace("ws-0", true, true)],
            vec![window("w0", "ws-0", true, WindowMode::Tiled)],
        );
        let desired = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let replacement = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Fullscreen)],
        );
        let mut transactions = TransactionManager::default();
        transactions.committed = Some(committed);

        transactions.stage(desired);
        transactions
            .pending
            .as_mut()
            .unwrap()
            .dirty_scopes
            .insert(DirtyScope::LayoutSubtree {
                workspace_id: WorkspaceId::from("ws-legacy"),
            });

        transactions.stage(replacement);

        assert!(transactions.pending().unwrap().dirty_scopes.contains(
            &DirtyScope::LayoutSubtree {
                workspace_id: WorkspaceId::from("ws-legacy"),
            }
        ));
    }

    #[test]
    fn stage_replacement_records_superseded_history_entry() {
        let committed = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let desired = snapshot(
            Some("w2"),
            &["w2"],
            vec![workspace("ws-2", true, true)],
            vec![window("w2", "ws-2", true, WindowMode::Tiled)],
        );
        let replacement = snapshot(
            Some("w3"),
            &["w3"],
            vec![workspace("ws-3", true, true)],
            vec![window("w3", "ws-3", true, WindowMode::Tiled)],
        );

        let mut transactions = TransactionManager::default();
        transactions.committed = Some(committed);
        transactions.stage(desired);
        transactions.stage(replacement);

        assert_eq!(
            transactions.history().front().unwrap().reason,
            TransactionCommitReason::Superseded
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .replacement_transaction_id,
            Some(2)
        );
        assert_eq!(
            transactions
                .history()
                .front()
                .unwrap()
                .coalescing_root_transaction_id,
            1
        );
        assert_eq!(transactions.history().front().unwrap().coalescing_depth, 0);
        assert_eq!(
            transactions.history().front().unwrap().timeout_extensions,
            0
        );
        assert_eq!(
            transactions
                .pending()
                .unwrap()
                .coalescing_root_transaction_id,
            1
        );
        assert_eq!(transactions.pending().unwrap().coalescing_depth, 1);
    }

    #[test]
    fn coalesced_chains_track_superseded_history_and_active_pending() {
        let committed = snapshot(
            Some("w1"),
            &["w1"],
            vec![workspace("ws-1", true, true)],
            vec![window("w1", "ws-1", true, WindowMode::Tiled)],
        );
        let desired = snapshot(
            Some("w2"),
            &["w2"],
            vec![workspace("ws-2", true, true)],
            vec![window("w2", "ws-2", true, WindowMode::Tiled)],
        );
        let replacement = snapshot(
            Some("w3"),
            &["w3"],
            vec![workspace("ws-3", true, true)],
            vec![window("w3", "ws-3", true, WindowMode::Tiled)],
        );

        let mut transactions = TransactionManager::default();
        transactions.committed = Some(committed);
        transactions.stage(desired);
        transactions.stage(replacement);

        assert_eq!(
            transactions.coalesced_chains(),
            vec![TransactionCoalescedChain {
                root_transaction_id: 1,
                transaction_ids: vec![1],
                active_transaction_id: Some(2),
            }]
        );
    }

    #[test]
    fn refresh_plan_expands_workspace_scope_to_all_workspace_windows() {
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![
                workspace("ws-1", true, true),
                workspace("ws-2", false, false),
            ],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
                window("w3", "ws-2", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();
        transactions.stage(desired);

        if let Some(pending) = transactions.pending.as_mut() {
            pending.affected_windows = HashSet::from([WindowId::from("w1")]);
            pending.affected_workspaces = HashSet::from([WorkspaceId::from("ws-1")]);
            pending.affected_outputs.clear();
            pending.dirty_scopes = HashSet::from([
                DirtyScope::Window(WindowId::from("w1")),
                DirtyScope::Workspace(WorkspaceId::from("ws-1")),
                DirtyScope::LayoutSubtree {
                    workspace_id: WorkspaceId::from("ws-1"),
                },
            ]);
        }

        let plan = transactions
            .pending_refresh_plan(&wm_for_refresh_plan())
            .unwrap();

        assert!(plan.windows.contains(&WindowId::from("w1")));
        assert!(plan.windows.contains(&WindowId::from("w2")));
        assert!(!plan.windows.contains(&WindowId::from("w3")));
    }

    #[test]
    fn refresh_plan_expands_output_scope_to_all_windows_on_output() {
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![
                workspace("ws-1", true, true),
                workspace("ws-2", false, false),
            ],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
                window("w3", "ws-2", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();
        transactions.stage(desired);

        if let Some(pending) = transactions.pending.as_mut() {
            pending.affected_windows.clear();
            pending.affected_workspaces.clear();
            pending.affected_outputs = HashSet::from([OutputId::from("out-1")]);
            pending.dirty_scopes = HashSet::from([DirtyScope::Output(OutputId::from("out-1"))]);
        }

        let plan = transactions
            .pending_refresh_plan(&wm_for_refresh_plan())
            .unwrap();

        assert!(plan.windows.contains(&WindowId::from("w1")));
        assert!(plan.windows.contains(&WindowId::from("w2")));
        assert!(!plan.windows.contains(&WindowId::from("w3")));
        assert!(plan
            .layout
            .workspace_roots
            .contains(&WorkspaceId::from("ws-1")));
    }

    #[test]
    fn refresh_plan_layout_subtree_scope_expands_to_workspace_windows() {
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![
                workspace("ws-1", true, true),
                workspace("ws-2", false, false),
            ],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
                window("w3", "ws-2", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();
        transactions.stage(desired);

        if let Some(pending) = transactions.pending.as_mut() {
            pending.dirty_scopes = HashSet::from([DirtyScope::LayoutSubtree {
                workspace_id: WorkspaceId::from("ws-1"),
            }]);
        }

        let plan = transactions
            .pending_refresh_plan(&wm_for_refresh_plan())
            .unwrap();

        assert!(plan.windows.contains(&WindowId::from("w1")));
        assert!(plan.windows.contains(&WindowId::from("w2")));
        assert!(!plan.windows.contains(&WindowId::from("w3")));
        assert!(plan
            .layout
            .workspace_roots
            .contains(&WorkspaceId::from("ws-1")));
    }

    #[test]
    fn refresh_plan_full_scene_scope_expands_to_all_windows() {
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![
                workspace("ws-1", true, true),
                workspace("ws-2", false, false),
            ],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
                window("w3", "ws-2", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();
        transactions.stage(desired);

        if let Some(pending) = transactions.pending.as_mut() {
            pending.dirty_scopes = HashSet::from([DirtyScope::FullScene]);
        }

        let plan = transactions
            .pending_refresh_plan(&wm_for_refresh_plan())
            .unwrap();

        assert!(plan.windows.contains(&WindowId::from("w1")));
        assert!(plan.windows.contains(&WindowId::from("w2")));
        assert!(plan.windows.contains(&WindowId::from("w3")));
        assert!(plan.layout.full_scene);
    }

    #[test]
    fn pending_debug_summary_reports_layout_scope_counts() {
        let desired = snapshot(
            Some("w1"),
            &["w1", "w2"],
            vec![
                workspace("ws-1", true, true),
                workspace("ws-2", false, false),
            ],
            vec![
                window("w1", "ws-1", true, WindowMode::Tiled),
                window("w2", "ws-1", false, WindowMode::Tiled),
                window("w3", "ws-2", false, WindowMode::Tiled),
            ],
        );
        let mut transactions = TransactionManager::default();
        transactions.stage(desired);

        if let Some(pending) = transactions.pending.as_mut() {
            pending.dirty_scopes = HashSet::from([DirtyScope::LayoutSubtree {
                workspace_id: WorkspaceId::from("ws-1"),
            }]);
        }

        let summary = transactions
            .pending_debug_summary(&wm_for_refresh_plan())
            .unwrap();

        assert!(summary.contains("tx=1"));
        assert!(summary.contains("layout_roots=1"));
        assert!(summary.contains("full_scene=false"));
    }
}
