# Spiders-WM2 Complete Architecture Analysis

## 1. TRANSACTION.RS - Frame-Perfect Relayout Engine

### Core Struct: Transaction

```rust
pub struct Transaction {
    inner: Arc<Inner>              // Shared reference counter for completion state
    deadline: Rc<RefCell<Deadline>> // Per-clone deadline registration handler
}

struct Inner {
    completed: AtomicBool                            // Completion flag
    notifications: Mutex<Option<(Sender<Client>, Vec<Client>)>> // Wayland client notifications
}
```

**Key Design Pattern:**

- Arc<Inner> uses strong_count to detect when all configure commits have been received
- Transaction is completed when:
  1. **All copies dropped** (is_last() returns true AND drop is called)
  2. **Deadline reached** (300ms timeout via calloop Timer)

### Methods and Use Cases:

| Method                                | Purpose                                   | Use Case                                          |
| ------------------------------------- | ----------------------------------------- | ------------------------------------------------- |
| `Transaction::new()`                  | Create frame-sync transaction             | Called before relayout/configure                  |
| `blocker()` → TransactionBlocker      | Get Blocker for add_blocker()             | Attach to surfaces to block commits               |
| `monitor()` → TransactionMonitor      | Get weak reference for checking release   | Used in window snapshots for animation lifecycle  |
| `is_completed()`                      | Check if transaction released             | Framebuffer state queries                         |
| `is_last()`                           | Check if this is last strong ref          | Optimization in drop()                            |
| `add_notification(sender, client)`    | Register client for blocker_cleared event | Notify compositor when blockers released          |
| `register_deadline_timer(event_loop)` | Register 300ms timeout                    | Idempotent - prevents timeout if already released |

### Deadline Mechanism:

```
Deadline::NotRegistered(Instant)  → First register_deadline_timer() call
  → Timer::from_deadline(instant)
  → Creates calloop timer source
  → On fire: inner.complete() → then Drop ping removes timer

Deadline::Registered { remove: Ping } → Already registered
```

**Critical Invariant:** Deadline only registered once via Rc<RefCell> per Transaction clone

### Blocker Implementation:

- `TransactionBlocker: Blocker` trait implementation
- state(): BlockerState::Pending (until is_released) → BlockerState::Released
- is_released: `inner.upgrade().is_none_or(inner.is_completed())`

### Notification Flow:

1. XdgShell pre-commit hook calls `add_notification(blocker_cleared_tx, client)`
2. When transaction completes: sends all clients on channel
3. State::notify_blocker_cleared() drains channel → calls blocker_cleared() on compositor state

---

## 2. CLOSING.RS - Animation State Management

### WindowSnapshot: Texture Capture

```rust
pub struct WindowSnapshot {
    buffer: TextureBuffer<GlesTexture>,    // Texture of rendered window
    bbox: Rectangle<i32, Logical>,         // Full include bbox with decorations
}
```

**Capture Process:**

1. Get window.bbox_with_popups() including all decorations
2. Render window surfaces to offscreen texture (Fourcc::Abgr8888)
3. Store texture + bounding box

**Conversion Methods:**

- `into_closing_window()` → Creates ClosingWindow for removal animation
- `into_resizing_window()` → Creates ResizingWindow for resize overlay during transaction

### ClosingWindow: Close Animation State

```rust
pub struct ClosingWindow {
    buffer: TextureBuffer<GlesTexture>,
    location: Point<i32, Logical>,
    size: Size<i32, Logical>,
    monitor: TransactionMonitor,  // Tied to close transaction
}
```

**Lifecycle:**

- Created when window closes (handle_window_close)
- location: element_location + snapshot.bbox.loc - geometry_location
- advance(now): Currently no-op (for future fade animations)
- is_finished(): returns monitor.is_released()
- render_element(): TextureRenderElement positioned/scaled

**Key Insight:** ClosingWindow buffer persists until its transaction completes, masking the removal

### ResizingWindow: Resize Overlay State

```rust
pub struct ResizingWindow {
    buffer: TextureBuffer<GlesTexture>,
    location: Point<i32, Logical>,
    size: Size<i32, Logical>,
    monitor: TransactionMonitor,  // Tied to resize transaction
}
```

**Usage:**

- Created during relayout when window needs resize
- Snapshot scaled to target_size (preserving decoration width/height ratios)
- Displayed during transaction until clients commit with new size
- is_finished(): checks monitor.is_released()

**Render Flow:**

- Both convert to Wm2RenderElements (TextureRenderElement)
- Positioned in world coordinates via location.to_f64().to_physical_precise_round()
- Composited on top of normal window stack

---

## 3. STATE.RS - Window and Transaction Coordination

### ManagedWindow Struct

```rust
pub(crate) struct ManagedWindow {
    pub(crate) window: Window,                              // Smithay window
    pub(crate) mapped: bool,                               // Visible in space?
    pub(crate) pending_location: Option<Point<i32, Logical>>, // Queued layout position
    pub(crate) matched_configure_commit: bool,            // Commit matched configure?
    pub(crate) snapshot: Option<WindowSnapshot>,          // Current texture snapshot
    pub(crate) resize_overlay: Option<ResizingWindow>,    // Active resize animation
    pub(crate) snapshot_dirty: bool,                      // Needs refresh?
    pub(crate) transaction_for_next_configure: Option<Transaction>, // Pending tx
    pub(crate) pending_transactions: Vec<(Serial, Transaction)>, // Queued txs by serial
}
```

### SpidersWm2 State Management

```rust
pub struct SpidersWm2 {
    managed_windows: Vec<ManagedWindow>,
    closing_windows: Vec<ClosingWindow>,

    // Notification channel for blocker_cleared callbacks
    blocker_cleared_tx: Sender<Client>,
    blocker_cleared_rx: Receiver<Client>,

    // Compositor infrastructure
    space: Space<Window>,
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    // ...
}
```

### Key State Machine Functions:

**add_window(window)**

- Creates ManagedWindow with initialized state
- Clears all optional fields (no snapshot, no overlay, no transaction)
- Sets snapshot_dirty=true for first capture

**handle_window_commit(surface)**

- Called on WlBuffer commit from renderer backend
- Checks mapped state and pending_location
- First map: schedule_relayout() + set_focus()
- Matched configure: clears resize_overlay, uses pending_location
- Calls space.map_element() to place window

**handle_window_close(surface)**

- Takes ManagedWindow from managed_windows
- Creates new Transaction for close animation
- If mapped: captures snapshot → creates ClosingWindow with monitor
- Unmaps from space
- Updates focus to last managed window
- **Calls schedule_relayout_with_transaction(transaction)**

**planned_layout_for_surface(surface)**

- Calculates tile layout: divides output_geometry.size.w equally
- Returns (location, size) for given surface index
- Used for initial configure size

**start_relayout(transaction?)**

1. Calculate tile widths (base_width, remainder handling)
2. For each managed_window:
   - Calculate target (location, size)
   - If already mapped + needs resize:
     - Create ResizingWindow overlay from snapshot
     - Unmap from space
   - Set pending_location
   - Create transaction if needed
   - Send toplevel.send_configure() with size
   - Store (serial, transaction) in pending_transactions
3. Drop transaction after loop (may trigger if all commits received)

**take_pending_transaction(commit_serial)**

- Called from xdg_shell pre-commit hook
- Drains pending_transactions while serial >= stored serial
- Returns latest transaction matching this commit serial

### Snapshot Management:

**refresh_window_snapshots(renderer)**

- Iterates managed_windows where mapped && snapshot_dirty
- Calls WindowSnapshot::capture() for each
- Stores in .snapshot field
- Clears snapshot_dirty flag

---

## 4. HANDLERS/COMPOSITOR.RS - Wayland Protocol Integration

### CompositorHandler Implementation

**commit() Flow:**

1. on_commit_buffer_handler::<Self>(surface) - Standard buffer commit
2. If NOT sync subsurface:
   - Find root surface (walk up parent chain)
   - Check if has buffer (is_mapped)
   - If mapped: **handle_window_commit(root)**
   - If known but !mapped: **handle_window_close(root)**
3. xdg_shell::handle_commit(state, surface) - XDG-specific

### XdgShellHandler Implementation

**new_toplevel(surface)**

- add_transaction_pre_commit_hook(surface) → Registers pre-commit callback
- Creates Window::new_wayland_window(surface)
- add_window(window) → Adds to managed_windows

**toplevel_destroyed(surface)**

- Calls handle_window_close(surface) → Transaction created

**add_transaction_pre_commit_hook(surface)**
Core pre-commit hook logic:

```
On pre-commit of ToplevelSurface:
1. Extract commit_serial from XdgToplevelSurfaceData
2. Find matching ManagedWindow
3. Call take_pending_transaction(commit_serial)
4. If transaction found:
   - Set matched_configure_commit = true
   - If NOT yet completed:
     - register_deadline_timer(event_loop) [300ms]
     - If NOT is_last():
       - add_blocker(surface, transaction.blocker())
       - add_notification(blocker_cleared_tx, client)
```

**Matching Logic:** XdgShellHandler::handle_commit() sends initial configure before root creation

---

## 5. END-TO-END FRAME-PERFECT RELAYOUT FLOW

### Scenario A: Layout Change (Tile Resize)

```
INPUT EVENT (WinitEvent::Resized)
  ↓
schedule_relayout()
  ↓
start_relayout(None) creates Transaction [Tx1]
  For each window:
    - Calculate new size
    - If already mapped + size changed:
      - Snapshot old state
      - Create ResizingWindow overlay
      - Unmap from space
    - Set pending_location
    - Send toplevel.configure(new_size) → serial S1
    - Store (S1, Tx1) in pending_transactions
  - Drop Tx1 (strong_count now 1 = Smithay holds blocker)

CLIENT receives configure [S1]:
  Renders at new size
  Sends commit

ON COMMIT (compositor.rs:commit()):
  → handle_window_commit(surface)
  → xdg_shell pre-commit hook:
      - Extract S1
      - find_window_mut(surface).take_pending_transaction(S1) → Returns Tx1
      - matched_configure_commit = true
      - is_completed() = false, is_last() = false
      - register_deadline_timer(&event_loop) [300ms timer]
      - add_blocker(surface, Tx1.blocker())
      - add_notification(blocker_cleared_tx, client)
  → handle_window_commit continues:
      - matched_configure_commit was true
      - Clears resize_overlay = None
      - Takes pending_location
      - Calls space.map_element() at new location

COMPOSITOR RENDER LOOP (WinitEvent::Redraw):
  state.notify_blocker_cleared()
    - Drains blocker_cleared_rx
    - Calls client.blocker_cleared()
    - Smithay removes blocker from surface
  state.advance_resize_overlays()
    - For each ResizingWindow that is_finished():
      - Clears resize_overlay = None

CONDITION 1: All commits received before deadline
  - All ResizingWindows.is_finished() = true (Tx1 released on last blocker removed)
  - No timer fires
  - Next render: snapshots refreshed

CONDITION 2: Deadline reached [300ms]
  - Timer fires: Tx1.inner.complete()
  - Sets completed = AtomicBool::true
  - Sends blocked clients the releasing notification
  - ResizingWindows.is_finished() now returns true
  - Next render: overlays removed, frames continue
```

### Scenario B: Close Window

```
INPUT: Alt+W (close)
  ↓
close_focused_window()
  → Send xdg_toplevel.send_close()

CLIENT responds:
  Unreferences surface → triggers destruction

ON toplevel_destroyed(surface):
  handle_window_close(surface)
    - Remove from managed_windows
    - Create Transaction [Tx2]
    - Get monitor = Tx2.monitor()
    - Snapshot window
    - Create ClosingWindow(..monitor)
    - Unmap from space
    - Push ClosingWindow to closing_windows vec
    - Update focus
    - schedule_relayout_with_transaction(Tx2)
      ↓
    start_relayout(Tx2) creates relayout for remaining windows
      - For each managed window:
        - Calculate new positions (fewer windows = wider tiles)
        - Send configure - creates new relayout Transaction
        - Stores relayout Transaction with Tx2.monitor() reference

ON COMMIT (relayout configures):
  - Pre-commit hook matches serial
  - Takes pending_transaction (relayout transaction)
  - Registers blocker for relayout

RENDER LOOP:
  - ClosingWindow buffer rendered on top (is_finished checks Tx2.monitor)
  - Relayout overlay windows also rendered
  - When both Tx2 released AND relayout Tx released:
    - ClosingWindow.is_finished() = true → removed next frame
    - ResizingWindows.is_finished() = true → removed next frame
  - Frame shows smooth transition: old window persists while tiles relayout
```

### Critical Dependency: Strong Count Release

```
Release Sequence:
1. start_relayout() creates Transaction Tx
2. Tx stored in transaction_for_next_configure [strong_count = 2]
3. send_configure() returns serial S
4. (serial, Tx) pushed to pending_transactions [still strong_count = 2]
5. transaction_for_next_configure.take() [strong_count = 1]
6. Loop ends, Tx dropped from scope [strong_count = 1]

On client commit at serial S:
7. Pre-commit hook: take_pending_transaction(S) [strong_count = 2 again]
8. Check is_last() = false → add blocker
9. Block applied, Tx stored in blocker [strong_count = 3?]
10. Blocker removed → strong_count back to 1
11. Pre-commit hook returns, Tx dropped [strong_count = 0]
12. Inner dropped → complete() called → blocker_cleared sent

Note: Strong count incremented by blocker add_blocker() call in Smithay
```

---

## 6. DEPENDENCIES BETWEEN FILES

```
transaction.rs (Core)
  ↓ (uses)
state.rs
  ├─ Creates/drops Transaction
  ├─ Stores in ManagedWindow
  ├─ Registers deadline timers
  ├─ Manages completion notifications
  ↓
handlers/xdg_shell.rs
  ├─ add_transaction_pre_commit_hook()
  ├─ Extracts serial from configure
  ├─ Calls take_pending_transaction()
  ├─ Registers blockers
  ├─ Adds notifications
  ↓
handlers/compositor.rs
  ├─ Detects commits
  ├─ Routes to handle_window_commit()
  ↓
state.rs (cont'd)
  ├─ handle_window_commit calls pre-commit hook
  ├─ Updates pending_location/matched_configure_commit
  ↓
closing.rs (Rendering)
  ├─ ClosingWindow.is_finished() checks monitor
  ├─ ResizingWindow.is_finished() checks monitor
  ├─ Lifecycle tied to Transaction completion
  ↓
winit.rs (Render Loop)
  ├─ WinitEvent::Redraw
  ├─ Calls notify_blocker_cleared()
  ├─ Calls advance_closing_windows()
  ├─ Renders transition_render_elements()
  ├─ Calls send_frames_for_windows()
```

---

## 7. RENDER LOOP INTEGRATION (winit.rs)

```
WinitEvent::Redraw handler every vsync:

1. notify_blocker_cleared()
   - Drains blocker_cleared_rx channel
   - Calls blocker_cleared() on each client's compositor state
   - Smithay removes TransactionBlocker from surface

2. advance_closing_windows()
   - For each ClosingWindow:
     - window.advance(now) [no-op currently]
   - Retain only !window.is_finished(now)
   - This removes windows where monitor.is_released()

3. advance_resize_overlays()
   - For each ManagedWindow:
     - If resize_overlay.is_finished():
       - Clear resize_overlay = None
       - Map element at pending_location if set

4. Bind winit backend renderer

5. refresh_window_snapshots(renderer)
   - For mapped windows with snapshot_dirty:
     - Capture new WindowSnapshot
     - Store in managed_window.snapshot
     - Clear dirty flag

6. Get transition_render_elements()
   - Collects all ResizingWindow.render_element()
   - Collects all ClosingWindow.render_element(now)
   - Returns Vec<Wm2RenderElements>

7. smithay::space::render_output()
   - Normal window rendering
   - Plus transition_render_elements on top
   - Damage tracking

8. backend.submit(Some(&[damage]))

9. send_frames_for_windows(&output)
   - Frame callbacks for all mapped + pending windows

10. space.refresh()
    - Updates cached state

11. popups.cleanup()

12. display_handle.flush_clients()
    - Send pending events

13. backend.window().request_redraw()
    - Schedule next frame
```

---

## SUMMARY TABLE: File Responsibilities

| File                   | Responsibility                         | Key Data Structures                           |
| ---------------------- | -------------------------------------- | --------------------------------------------- |
| transaction.rs         | Frame-perfect synchronization          | Transaction, Inner, Deadline, Blocker         |
| closing.rs             | Animation texture capture & rendering  | WindowSnapshot, ClosingWindow, ResizingWindow |
| state.rs               | Window lifecycle & layout coordination | ManagedWindow, SpidersWm2                     |
| handlers/xdg_shell.rs  | XDG shell events & pre-commit hooks    | add_transaction_pre_commit_hook               |
| handlers/compositor.rs | Wayland compositor callbacks           | CompositorHandler::commit                     |
| handlers/mod.rs        | Handler aggregation                    |                                               |
| winit.rs               | Render loop & frame synchronization    | WinitEvent::Redraw handler                    |
| input.rs               | Keyboard/pointer events                | KeyAction, process_input_event                |
| main.rs                | Application entry point                |                                               |

---

## KEY INNOVATIONS

### 1. Per-Clone Deadline Registration

- Via `Rc<RefCell<Deadline>>` preventing duplicate timers
- Ping-based removal is frame-safe
- Allows Transaction clones to be passed around without duplicate registrations

### 2. Snapshot-Based Overlays

- ResizingWindow shows old content while relayout pending
- ClosingWindow persists until relayout completes
- Transaction monitor lifecycle manages cleanup
- Smooth visual transitions during layout changes

### 3. Serial-Based Transaction Matching

- Handles multiple sequential configures correctly
- Pre-commit hook extracts XdgToplevelSurfaceData serial
- Queue of (serial, transaction) ensures FIFO match across multiple configures

### 4. Dual-Level Blocking

- Per-surface blockers (transaction.blocker()) prevent client commits
- Per-client notifications (blocker_cleared) on release
- Serialized through mpsc channel in render loop
- Decouples pre-commit hook from compositor state updates

### 5. Monitor-Based Lifecycle Tracking

- TransactionMonitor (Weak<Inner>) for observer pattern
- Checked in animation is_finished() methods
- Avoids circular references between transactions and overlays

---

## CURRENT LIMITATIONS & FUTURE WORK

1. **No Fade Animations** - ClosingWindow.advance() is no-op
2. **No dmabuf Blocker** - GPU readiness not tracked
3. **Simple Close Path** - Doesn't queue closes during pending transactions
4. **No Damage Tracking Between Overlays** - Full redraw every frame
5. **Single Output** - Assumes one output exists
6. **Synchronous Snapshot Capture** - Blocks renderer thread briefly

---

## TESTING ENTRY POINTS

1. **ResizeTest**: Trigger window resize, verify overlay appears then disappears
2. **CloseTest**: Close window, verify snapshot persists during relayout
3. **TimeoutTest**: Unresponsive client, verify 300ms timeout triggers
4. **MultiClose**: Close multiple windows, check transaction queueing
5. **RapidResize**: Rapid window count changes, verify no overlay artifacts
