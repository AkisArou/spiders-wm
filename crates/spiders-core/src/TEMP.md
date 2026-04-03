# TEMP

Non-compiling staging inventory for cross-crate type review.

`WmEnvironment` is intentionally excluded.

`wm_www::PreviewWindowProjection` and `wm_www::RuntimeWindowSnapshotInput` were present in the earlier staging pass, but they do not exist in the current source tree anymore, so they are called out as removed instead of reconstructed.

```rust
pub mod wm_runtime {
    // bindings.rs
    pub struct ParsedBindingsState {
        pub source: String,
        pub mod_key: String,
        pub entries: Vec<ParsedBindingEntry>,
    }

    pub struct ParsedBindingEntry {
        pub bind: Vec<String>,
        pub chord: String,
        pub command: Option<WmCommand>,
        pub command_label: String,
    }

    pub struct BindingKeyEvent {
        pub key: String,
        pub alt: bool,
        pub ctrl: bool,
        pub meta: bool,
        pub shift: bool,
    }

    struct ExpectedModifiers {
        alt: bool,
        ctrl: bool,
        meta: bool,
        shift: bool,
    }

    // snapshot.rs
    pub enum PreviewSnapshotClasses {
        One(String),
        Many(Vec<String>),
    }

    pub struct PreviewSnapshotNode {
        pub node_type: String,
        pub id: Option<String>,
        pub class_name: Option<PreviewSnapshotClasses>,
        pub rect: Option<LayoutRect>,
        pub window_id: Option<WindowId>,
        pub axis: Option<String>,
        pub reverse: bool,
        pub children: Vec<PreviewSnapshotNode>,
    }

    // layout.rs
    pub struct PreviewLayoutWindow {
        pub id: String,
        pub app_id: Option<String>,
        pub title: Option<String>,
        pub class: Option<String>,
        pub instance: Option<String>,
        pub role: Option<String>,
        pub shell: Option<String>,
        pub window_type: Option<String>,
        pub floating: bool,
        pub fullscreen: bool,
        pub focused: bool,
    }

    pub struct PreviewLayoutComputation {
        pub snapshot_root: Option<PreviewSnapshotNode>,
        pub diagnostics: Vec<PreviewDiagnostic>,
        pub unclaimed_window_ids: Vec<String>,
    }

    enum JsLayoutValue {
        Node(JsLayoutNode),
        Array(Vec<JsLayoutValue>),
        Null(Option<()>),
        Bool(bool),
        String(String),
        Number(f64),
    }

    struct JsLayoutNode {
        node_type: String,
        props: JsLayoutProps,
        children: Vec<JsLayoutValue>,
    }

    struct JsLayoutProps {
        id: Option<String>,
        class: Option<String>,
        match_expr: Option<String>,
        take: Option<u32>,
    }

    // host.rs
    pub enum WorkspaceTarget {
        Named(String),
        Next,
        Previous,
    }

    pub enum WorkspaceAssignment {
        Move(u8),
        Toggle(u8),
    }

    pub enum FocusTarget {
        Next,
        Previous,
        Direction(FocusDirection),
        Window(WindowId),
    }

    pub enum WindowToggle {
        Floating,
        Fullscreen,
    }

    // diagnostic.rs
    pub struct PreviewDiagnostic {
        pub source: String,
        pub level: String,
        pub message: String,
    }

    // session.rs
    pub struct PreviewSessionState {
        pub active_workspace_name: String,
        pub workspace_names: Vec<String>,
        pub windows: Vec<PreviewSessionWindow>,
        pub remembered_focus_by_scope: BTreeMap<String, WindowId>,
        pub layout_adjustments: LayoutAdjustmentState,
    }

    pub struct PreviewSessionWindow {
        pub id: String,
        pub app_id: Option<String>,
        pub title: Option<String>,
        pub class: Option<String>,
        pub instance: Option<String>,
        pub role: Option<String>,
        pub shell: Option<String>,
        pub window_type: Option<String>,
        pub floating: bool,
        pub fullscreen: bool,
        pub focused: bool,
        pub workspace_name: String,
    }

    struct SplitResizeTarget<'a> {
        node_id: String,
        node: &'a PreviewSnapshotNode,
        child_count: usize,
        grow_child_index: usize,
        shrink_child_index: usize,
    }

    // wm_runtime.rs
    pub enum RuntimeCommand {
        EnsureWorkspace { name: String },
        EnsureDefaultWorkspace { name: String },
        RequestSelectWorkspace { workspace_id: WorkspaceId, window_order: Vec<WindowId> },
        RequestSelectNextWorkspace { window_order: Vec<WindowId> },
        RequestSelectPreviousWorkspace { window_order: Vec<WindowId> },
        EnsureSeat { seat_id: SeatId },
        SyncOutput {
            output_id: OutputId,
            name: String,
            logical_width: u32,
            logical_height: u32,
        },
        PlaceNewWindow { window_id: WindowId },
        RequestFocusWindowSelection { seat_id: SeatId, window_id: Option<WindowId> },
        RequestFocusNextWindowSelection { seat_id: SeatId, window_order: Vec<WindowId> },
        RequestFocusPreviousWindowSelection { seat_id: SeatId, window_order: Vec<WindowId> },
        SyncHoveredWindow { seat_id: SeatId, hovered_window_id: Option<WindowId> },
        SyncInteractedWindow { seat_id: SeatId, interacted_window_id: Option<WindowId> },
        UnmapWindow { window_id: WindowId, window_order: Vec<WindowId> },
        RemoveWindow { window_id: WindowId, window_order: Vec<WindowId> },
        RequestCloseFocusedWindowSelection,
        AssignFocusedWindowToWorkspace { workspace_id: WorkspaceId, window_order: Vec<WindowId> },
        ToggleAssignFocusedWindowToWorkspace { workspace_id: WorkspaceId, window_order: Vec<WindowId> },
        ToggleFocusedWindowFloating,
        ToggleFocusedWindowFullscreen,
        SyncWindowIdentity {
            window_id: WindowId,
            title: Option<String>,
            app_id: Option<String>,
        },
        SyncWindowMapped { window_id: WindowId, mapped: bool },
    }

    pub enum RuntimeResult {
        Workspace(WorkspaceId),
        WorkspaceSelection(Option<WorkspaceSelection>),
        FocusSelection(FocusSelection),
        CloseSelection(CloseSelection),
        Seat(SeatId),
        Output(OutputId),
        Window(Option<WindowId>),
        FocusUpdate(FocusUpdate),
    }

    pub struct CloseSelection {
        pub closing_window_id: Option<WindowId>,
    }

    pub struct WmRuntime<'a> {
        model: &'a mut WmModel,
    }
}

pub mod wm_www {
    // apps/spiders-wm-www/src/session.rs
    pub struct PreviewSessionState {
        pub active_layout: LayoutId,
        pub snapshot_root: Option<PreviewSnapshotNode>,
        pub diagnostics: Vec<PreviewDiagnostic>,
        pub event_log: Vec<String>,
        pub last_action: String,
        runtime_state: RuntimePreviewSessionState,
        window_geometries: BTreeMap<WindowId, WindowGeometry>,
        unclaimed_window_ids: BTreeSet<WindowId>,
        stylesheets_by_layout: BTreeMap<LayoutId, String>,
    }

    // removed from current source after the earlier staging pass:
    // pub struct PreviewWindowProjection { ... }
    // pub struct RuntimeWindowSnapshotInput { ... }
}

pub mod wm_core {
    // ids.rs, expanded from id_type!
    pub struct WindowId(pub String);
    pub struct OutputId(pub String);
    pub struct LayoutId(pub String);
    pub struct WorkspaceId(pub String);
    pub struct SeatId(pub String);

    // types.rs
    pub enum ShellKind {
        XdgToplevel,
        X11,
        Unknown,
    }

    pub enum OutputTransform {
        Normal,
        Rotate90,
        Rotate180,
        Rotate270,
        Flipped,
        Flipped90,
        Flipped180,
        Flipped270,
    }

    pub struct LayoutRef {
        pub name: String,
    }

    pub enum WindowMode {
        Tiled,
        Floating { rect: Option<LayoutRect> },
        Fullscreen,
    }

    // resize.rs
    pub struct LayoutAdjustmentState {
        pub split_weights_by_node_id: BTreeMap<String, Vec<u16>>,
    }

    // api.rs
    pub enum QueryRequest {
        State,
        FocusedWindow,
        CurrentOutput,
        CurrentWorkspace,
        MonitorList,
        WorkspaceNames,
    }

    pub enum QueryResponse {
        State(StateSnapshot),
        FocusedWindow(Option<WindowSnapshot>),
        CurrentOutput(Option<OutputSnapshot>),
        CurrentWorkspace(Option<WorkspaceSnapshot>),
        MonitorList(Vec<OutputSnapshot>),
        WorkspaceNames(Vec<String>),
    }

    pub enum CompositorEvent {
        FocusChange {
            focused_window_id: Option<WindowId>,
            current_output_id: Option<OutputId>,
            current_workspace_id: Option<WorkspaceId>,
        },
        WindowCreated {
            window: WindowSnapshot,
        },
        WindowDestroyed {
            window_id: WindowId,
        },
        WindowWorkspaceChange {
            window_id: WindowId,
            workspaces: Vec<String>,
        },
        WindowFloatingChange {
            window_id: WindowId,
            floating: bool,
        },
        WindowGeometryChange {
            window_id: WindowId,
            floating_rect: Option<LayoutRect>,
            output_id: Option<OutputId>,
            workspace_id: Option<WorkspaceId>,
        },
        WindowFullscreenChange {
            window_id: WindowId,
            fullscreen: bool,
        },
        WorkspaceChange {
            workspace_id: Option<WorkspaceId>,
            active_workspaces: Vec<String>,
        },
        LayoutChange {
            workspace_id: Option<WorkspaceId>,
            layout: Option<LayoutRef>,
        },
        ConfigReloaded,
    }

    // focus.rs
    pub enum FocusScopeSegment {
        Workspace,
        Group { child_index: usize, label: String },
        Visual { child_index: usize },
    }

    pub struct FocusScopePath(Vec<FocusScopeSegment>);

    pub struct FocusScopePathParseError {
        input: String,
    }

    pub struct FocusTreeWindowGeometry {
        pub window_id: WindowId,
        pub geometry: WindowGeometry,
    }

    pub enum FocusAxis {
        Horizontal,
        Vertical,
    }

    pub enum FocusBranchKey {
        Scope(FocusScopePath),
        Window(WindowId),
    }

    pub struct FocusScopeNavigation {
        pub axis: FocusAxis,
        pub branches: Vec<FocusBranchKey>,
    }

    pub struct FocusTree {
        ordered_window_ids: Vec<WindowId>,
        scope_path_by_window: BTreeMap<WindowId, Vec<FocusScopePath>>,
        descendant_window_ids_by_scope: BTreeMap<FocusScopePath, Vec<WindowId>>,
        navigation_by_scope: BTreeMap<FocusScopePath, FocusScopeNavigation>,
    }

    pub enum FocusUpdate {
        Unchanged,
        Set(Option<WindowId>),
    }

    pub struct FocusSelection {
        pub focused_window_id: Option<WindowId>,
    }

    // command.rs
    pub enum FocusDirection {
        Left,
        Right,
        Up,
        Down,
    }

    pub enum LayoutCycleDirection {
        Next,
        Previous,
    }

    pub enum WmCommand {
        Spawn { command: String },
        Quit,
        ReloadConfig,
        SetLayout { name: String },
        CycleLayout { direction: Option<LayoutCycleDirection> },
        ViewWorkspace { workspace: u8 },
        ToggleViewWorkspace { workspace: u8 },
        ActivateWorkspace { workspace_id: WorkspaceId },
        AssignWorkspace { workspace_id: WorkspaceId, output_id: OutputId },
        FocusMonitorLeft,
        FocusMonitorRight,
        SendMonitorLeft,
        SendMonitorRight,
        ToggleFloating,
        ToggleFullscreen,
        AssignFocusedWindowToWorkspace { workspace: u8 },
        ToggleAssignFocusedWindowToWorkspace { workspace: u8 },
        FocusWindow { window_id: WindowId },
        SetFloatingWindowGeometry { window_id: WindowId, rect: LayoutRect },
        FocusDirection { direction: FocusDirection },
        SwapDirection { direction: FocusDirection },
        ResizeDirection { direction: FocusDirection },
        ResizeTiledDirection { direction: FocusDirection },
        MoveDirection { direction: FocusDirection },
        SpawnTerminal,
        FocusNextWindow,
        FocusPreviousWindow,
        SelectNextWorkspace,
        SelectPreviousWorkspace,
        SelectWorkspace { workspace_id: WorkspaceId },
        CloseFocusedWindow,
    }

    // snapshot.rs
    pub struct WindowSnapshot {
        pub id: WindowId,
        pub shell: ShellKind,
        pub app_id: Option<String>,
        pub title: Option<String>,
        pub class: Option<String>,
        pub instance: Option<String>,
        pub role: Option<String>,
        pub window_type: Option<String>,
        pub mapped: bool,
        pub mode: WindowMode,
        pub focused: bool,
        pub urgent: bool,
        pub closing: bool,
        pub output_id: Option<OutputId>,
        pub workspace_id: Option<WorkspaceId>,
        pub workspaces: Vec<String>,
    }

    pub struct WorkspaceSnapshot {
        pub id: WorkspaceId,
        pub name: String,
        pub output_id: Option<OutputId>,
        pub active_workspaces: Vec<String>,
        pub focused: bool,
        pub visible: bool,
        pub effective_layout: Option<LayoutRef>,
    }

    pub struct OutputSnapshot {
        pub id: OutputId,
        pub name: String,
        pub logical_x: i32,
        pub logical_y: i32,
        pub logical_width: u32,
        pub logical_height: u32,
        pub scale: u32,
        pub transform: OutputTransform,
        pub enabled: bool,
        pub current_workspace_id: Option<WorkspaceId>,
    }

    pub struct StateSnapshot {
        pub focused_window_id: Option<WindowId>,
        pub current_output_id: Option<OutputId>,
        pub current_workspace_id: Option<WorkspaceId>,
        pub outputs: Vec<OutputSnapshot>,
        pub workspaces: Vec<WorkspaceSnapshot>,
        pub windows: Vec<WindowSnapshot>,
        pub visible_window_ids: Vec<WindowId>,
        pub workspace_names: Vec<String>,
    }

    // workspace.rs
    pub struct WorkspaceSelection {
        pub workspace_id: WorkspaceId,
        pub focused_window_id: Option<WindowId>,
    }

    // wm.rs
    pub struct OutputModel {
        pub id: OutputId,
        pub name: String,
        pub logical_x: i32,
        pub logical_y: i32,
        pub logical_width: u32,
        pub logical_height: u32,
        pub enabled: bool,
        pub focused_workspace_id: Option<WorkspaceId>,
    }

    pub struct SeatModel {
        pub id: SeatId,
        pub focused_window_id: Option<WindowId>,
        pub hovered_window_id: Option<WindowId>,
        pub interacted_window_id: Option<WindowId>,
    }

    pub struct WindowGeometry {
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
    }

    pub struct WindowModel {
        pub id: WindowId,
        pub app_id: Option<String>,
        pub title: Option<String>,
        pub output_id: Option<OutputId>,
        pub workspace_id: Option<WorkspaceId>,
        pub mapped: bool,
        pub focused: bool,
        pub floating: bool,
        pub floating_geometry: Option<WindowGeometry>,
        pub fullscreen: bool,
        pub closing: bool,
    }

    pub struct WorkspaceModel {
        pub id: WorkspaceId,
        pub name: String,
        pub output_id: Option<OutputId>,
        pub focused: bool,
        pub visible: bool,
    }

    pub struct WmModel {
        pub windows: BTreeMap<WindowId, WindowModel>,
        pub workspaces: BTreeMap<WorkspaceId, WorkspaceModel>,
        pub outputs: BTreeMap<OutputId, OutputModel>,
        pub seats: BTreeMap<SeatId, SeatModel>,
        pub focused_window_id: Option<WindowId>,
        pub current_workspace_id: Option<WorkspaceId>,
        pub current_output_id: Option<OutputId>,
        pub focus_tree: Option<FocusTree>,
        pub last_focused_window_id_by_scope: BTreeMap<FocusScopePath, WindowId>,
    }

    // navigation.rs
    pub enum NavigationDirection {
        Left,
        Right,
        Up,
        Down,
    }

    pub struct WindowGeometryCandidate {
        pub window_id: WindowId,
        pub geometry: WindowGeometry,
        pub scope_path: Vec<FocusScopePath>,
    }

    enum SplitAxis {
        Horizontal,
        Vertical,
    }

    struct ScopeBranch<'a> {
        key: FocusBranchKey,
        geometry: WindowGeometry,
        descendants: Vec<&'a WindowGeometryCandidate>,
        scope_depth: Option<usize>,
    }

    // layout.rs
    pub struct LayoutSpace {
        pub width: f32,
        pub height: f32,
    }

    pub struct LayoutRect {
        pub x: f32,
        pub y: f32,
        pub width: f32,
        pub height: f32,
    }

    pub enum LayoutNodeType {
        Workspace,
        Group,
        Window,
        Slot,
    }

    pub enum RuntimeLayoutNodeType {
        Workspace,
        Group,
        Window,
    }

    pub struct LayoutNodeMeta {
        pub id: Option<String>,
        pub class: Vec<String>,
        pub name: Option<String>,
        pub data: BTreeMap<String, String>,
    }

    pub enum MatchKey {
        AppId,
        Title,
        Class,
        Instance,
        Role,
        Shell,
        WindowType,
    }

    pub struct MatchClause {
        pub key: MatchKey,
        pub value: String,
    }

    pub struct WindowMatch {
        pub clauses: Vec<MatchClause>,
    }

    pub enum RemainingTake {
        Remaining,
    }

    pub enum SlotTake {
        Count(u32),
        Remaining(RemainingTake),
    }

    pub enum SourceLayoutNode {
        Workspace { meta: LayoutNodeMeta, children: Vec<SourceLayoutNode> },
        Group { meta: LayoutNodeMeta, children: Vec<SourceLayoutNode> },
        Window { meta: LayoutNodeMeta, window_match: Option<WindowMatch> },
        Slot {
            meta: LayoutNodeMeta,
            window_match: Option<WindowMatch>,
            take: SlotTake,
        },
    }

    pub enum ResolvedLayoutNode {
        Workspace { meta: LayoutNodeMeta, children: Vec<ResolvedLayoutNode> },
        Group { meta: LayoutNodeMeta, children: Vec<ResolvedLayoutNode> },
        Window { meta: LayoutNodeMeta, window_id: Option<WindowId> },
    }

    // focus_visual.rs
    pub(crate) struct VisualEntry {
        pub(crate) window_id: WindowId,
        pub(crate) geometry: WindowGeometry,
        pub(crate) original_index: usize,
    }

    pub(crate) enum VisualChild {
        Scope(VisualScope),
        Window(VisualEntry),
    }

    pub(crate) struct VisualScope {
        pub(crate) axis: Option<FocusAxis>,
        pub(crate) children: Vec<VisualChild>,
    }

    // runtime/runtime_contract.rs
    pub struct LayoutModuleContract {
        pub export_name: String,
    }

    pub struct RuntimeInfo {
        pub name: String,
    }

    // runtime/prepared_layout.rs
    pub struct SelectedLayout {
        pub name: String,
        pub directory: String,
        pub module: String,
    }

    pub struct PreparedLayout {
        pub selected: SelectedLayout,
        pub runtime_payload: serde_json::Value,
        pub stylesheets: PreparedStylesheets,
    }

    pub struct PreparedStylesheet {
        pub path: String,
        pub source: String,
    }

    pub struct PreparedStylesheets {
        pub global: Option<PreparedStylesheet>,
        pub layout: Option<PreparedStylesheet>,
    }

    // runtime/layout_context.rs
    pub struct LayoutEvaluationContext {
        pub monitor: LayoutMonitorContext,
        pub workspace: LayoutWorkspaceContext,
        pub windows: Vec<LayoutWindowContext>,
        pub state: Option<LayoutStateContext>,
        pub workspace_id: WorkspaceId,
        pub output: Option<OutputSnapshot>,
        pub selected_layout_name: Option<String>,
        pub space: LayoutSpace,
    }

    pub struct LayoutMonitorContext {
        pub name: String,
        pub width: u32,
        pub height: u32,
        pub scale: Option<u32>,
    }

    pub struct LayoutWorkspaceContext {
        pub name: String,
        pub workspaces: Vec<String>,
        pub window_count: usize,
    }

    pub struct LayoutWindowContext {
        pub id: WindowId,
        pub app_id: Option<String>,
        pub title: Option<String>,
        pub class: Option<String>,
        pub instance: Option<String>,
        pub role: Option<String>,
        pub shell: Option<String>,
        pub window_type: Option<String>,
        pub floating: bool,
        pub fullscreen: bool,
        pub focused: bool,
    }

    pub struct LayoutStateContext {
        pub focused_window_id: Option<WindowId>,
        pub current_output_id: Option<OutputId>,
        pub current_workspace_id: Option<WorkspaceId>,
        pub visible_window_ids: Vec<WindowId>,
        pub workspace_names: Vec<String>,
        pub selected_layout_name: Option<String>,
        pub layout_adjustments: LayoutAdjustmentState,
    }

    // runtime/runtime_error.rs
    pub enum RuntimeError {
        NotImplemented(String),
        JavaScript { message: String },
        MissingExport { name: String, export: String },
        NonCallableExport { name: String, export: String },
        MissingRuntimeSource { name: String },
        ValueConversion { name: String, message: String },
        Validation { message: String },
        Config { message: String },
        Other { message: String },
    }

    pub struct RuntimeRefreshSummary {
        pub refreshed_files: usize,
        pub pruned_files: usize,
    }
}

pub mod wm_smithay {
    // state.rs
    pub struct SpidersWm {
        pub start_time: std::time::Instant,
        pub socket_name: OsString,
        pub display_handle: DisplayHandle,
        pub event_loop: LoopHandle<'static, Self>,
        pub loop_signal: LoopSignal,
        pub blocker_cleared_tx: Sender<Client>,
        pub blocker_cleared_rx: Receiver<Client>,
        pub space: Space<Window>,
        pub popups: PopupManager,
        pub compositor_state: CompositorState,
        pub xdg_shell_state: XdgShellState,
        pub shm_state: ShmState,
        pub dmabuf_state: DmabufState,
        pub dmabuf_global: Option<DmabufGlobal>,
        pub seat_state: SeatState<Self>,
        pub data_device_state: DataDeviceState,
        pub seat: Seat<Self>,
        pub backend: Option<WinitGraphicsBackend<GlesRenderer>>,
        pub focused_surface: Option<WlSurface>,
        pub(crate) config_paths: Option<ConfigPaths>,
        pub(crate) config: Config,
        pub(crate) managed_windows: Vec<ManagedWindow>,
        pub(crate) frame_sync: FrameSyncState,
        pub(crate) ipc_server: IpcServerState,
        pub(crate) ipc_clients: BTreeMap<IpcClientId, UnixStream>,
        pub(crate) ipc_socket_path: Option<PathBuf>,
        pub(crate) scene: SceneLayoutState,
        pub(crate) model: WmModel,
        pub(crate) next_window_id: u64,
    }

    pub(crate) struct ManagedWindow {
        pub(crate) id: WindowId,
        pub(crate) window: Window,
        pub(crate) mapped: bool,
        pub(crate) frame_sync: WindowFrameSyncState,
    }

    // actions/facade.rs
    pub struct WmActions<'a> {
        model: &'a mut WmModel,
    }

    // actions/window.rs
    pub struct CloseSelection {
        pub closing_window_id: Option<WindowId>,
    }

    // ipc.rs
    pub(crate) enum WmIpcStreamError {
        Transport(IpcTransportError),
        UnknownClient(UnknownClientError),
    }

    // handlers/compositor.rs
    pub(crate) struct ClientState {
        pub compositor_state: CompositorClientState,
    }

    // scene/adapter.rs
    pub(crate) struct LayoutTarget {
        pub(crate) window_id: WindowId,
        pub(crate) location: Point<i32, Logical>,
        pub(crate) size: Size<i32, Logical>,
        pub(crate) fullscreen: bool,
    }

    pub(crate) struct SceneLayoutState {
        config_paths: Option<ConfigPaths>,
        layout_service: Option<AuthoringLayoutService<DefaultLayoutRuntime>>,
        cache: SceneCache,
    }

    // compositor/layout.rs
    struct RelayoutSlot {
        location: Point<i32, Logical>,
        size: Size<i32, Logical>,
    }

    // frame_sync/transaction.rs
    pub(crate) struct PendingLayout {
        pub(crate) location: Point<i32, Logical>,
        pub(crate) size: Size<i32, Logical>,
    }

    pub(crate) struct PendingConfigureState {
        pending: VecDeque<(Serial, PendingConfigure)>,
        ready: Option<PendingLayout>,
    }

    struct PendingConfigure {
        layout: PendingLayout,
        transaction: Transaction,
    }

    pub(crate) struct MatchedConfigure {
        pub(crate) layout: PendingLayout,
        pub(crate) transaction: Transaction,
    }

    pub(crate) struct Transaction {
        inner: Arc<Inner>,
        deadline: Rc<RefCell<Deadline>>,
    }

    pub(crate) struct TransactionBlocker {
        inner: Weak<Inner>,
    }

    enum Deadline {
        NotRegistered(Instant),
        Registered { remove: Ping },
    }

    struct Inner {
        completed: AtomicBool,
        notifications: Mutex<Option<(Sender<Client>, Vec<Client>)>>,
    }

    // frame_sync/mod.rs
    pub(crate) struct SyncHandle(transaction::Transaction);

    pub(crate) struct CapturedCloseSnapshot(WindowSnapshot);

    pub(crate) struct WindowFrameSyncState {
        close_snapshot: Option<CapturedCloseSnapshot>,
        pending_configures: PendingConfigureState,
    }

    pub(crate) struct FrameSyncState {
        closing_overlays: Vec<ClosingWindowOverlay>,
    }

    pub(crate) struct CommitSyncOutcome {
        pub(crate) had_match: bool,
        pub(crate) has_pending_configures: bool,
        pub(crate) transaction_debug_id: Option<usize>,
        pub(crate) waited_on_dmabuf: bool,
    }

    pub(crate) struct OverlayPushResult {
        pub(crate) transaction_debug_id: usize,
        pub(crate) carried_overlays: usize,
    }

    pub(crate) struct BeginUnmapResult {
        pub(crate) snapshot: Option<CapturedCloseSnapshot>,
    }

    struct ClosingWindowOverlay {
        snapshot: WindowSnapshot,
        location: Point<i32, Logical>,
        transaction: transaction::Transaction,
        presented_once: bool,
    }

    // frame_sync/render_snapshot.rs
    pub(crate) type SnapshotRenderElement =
        RescaleRenderElement<SurfaceTextureRenderElement<GlesRenderer>>;

    struct EncompassingTexture {
        texture: GlesTexture,
        _sync_point: SyncPoint,
        loc: Point<i32, Physical>,
    }

    pub(crate) struct WindowSnapshot {
        elements: Rc<Vec<SurfaceTextureRenderElement<GlesRenderer>>>,
        texture: OnceCell<(GlesTexture, Point<i32, Physical>)>,
    }
}
```
