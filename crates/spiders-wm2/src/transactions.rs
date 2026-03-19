use std::{
    collections::{HashSet, VecDeque},
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
    pub replacement_transaction_id: Option<u64>,
    pub unresolved_window_ids: Vec<WindowId>,
    pub affected_window_count: usize,
    pub affected_workspace_count: usize,
    pub affected_output_count: usize,
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

        if let Some(pending) = self.pending.take() {
            self.record_history_entry(
                pending,
                TransactionCommitReason::Superseded,
                Some(transaction_id),
            );
        }

        let Some(committed) = self.committed.as_ref() else {
            self.pending = Some(PendingTransaction::full(transaction_id, desired));
            return;
        };

        let pending = PendingTransaction::from_diff(transaction_id, committed, desired);
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
            format!(
                "tx={} windows={} workspaces={} outputs={} layout_roots={} full_scene={} participants={}",
                pending.id,
                plan.windows.len(),
                plan.workspaces.len(),
                plan.outputs.len(),
                plan.layout.workspace_roots.len(),
                plan.layout.full_scene,
                pending.participants.len(),
            )
        })
    }

    pub fn register_configure(&mut self, window_id: &WindowId, serial: Serial) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };

        let participant = pending.participants.entry(window_id.clone()).or_default();
        participant.configure_serial = Some(serial);
        participant.acked = false;
        participant.committed = false;
    }

    pub fn mark_configure_acked(&mut self, window_id: &WindowId, serial: Serial) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };

        let Some(participant) = pending.participants.get_mut(window_id) else {
            return;
        };

        if participant.configure_serial == Some(serial) {
            participant.acked = true;
        }
    }

    pub fn mark_window_committed(&mut self, window_id: &WindowId) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };

        let Some(participant) = pending.participants.get_mut(window_id) else {
            return;
        };

        if participant.configure_serial.is_some() && participant.acked {
            participant.committed = true;
        }
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
            replacement_transaction_id,
            unresolved_window_ids: pending.unresolved_window_ids(),
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
    fn full(id: u64, desired: StateSnapshot) -> Self {
        let affected_windows = desired
            .windows
            .iter()
            .map(|window| window.id.clone())
            .collect::<HashSet<_>>();

        Self {
            id,
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

    fn from_diff(id: u64, committed: &StateSnapshot, desired: StateSnapshot) -> Self {
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

        if focused_fullscreen_window(committed) != focused_fullscreen_window(&desired)
            || !affected_outputs.is_empty()
        {
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
}

impl TransactionParticipant {
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

    use super::{DirtyScope, PendingTransaction, TransactionCommitReason, TransactionManager};
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

        let pending = PendingTransaction::from_diff(1, &committed, desired);

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

        let pending = PendingTransaction::from_diff(1, &committed, desired);

        assert!(pending.affected_windows.contains(&WindowId::from("w1")));
        assert!(pending.affected_windows.contains(&WindowId::from("w2")));
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
