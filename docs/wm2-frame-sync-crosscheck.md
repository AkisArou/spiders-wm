# wm2 Frame-Sync Cross-Check vs niri

This note compares the current `spiders-wm2` frame-sync path against the local `niri` checkout at `/home/akisarou/projects/niri`.

The goal is not to copy niri wholesale. The goal is to identify the concrete places where `wm2` is thinner than niri in ways that can break frame-perfect relayouts and close handling.

## Scope

Files examined in `wm2`:

- `crates/spiders-wm2/src/frame_sync/runtime.rs`
- `crates/spiders-wm2/src/frame_sync/snapshots.rs`
- `crates/spiders-wm2/src/handlers/compositor.rs`
- `crates/spiders-wm2/src/handlers/xdg_shell.rs`
- `crates/spiders-wm2/src/compositor/shell.rs`
- `crates/spiders-wm2/src/state.rs`

Files examined in `niri`:

- `/home/akisarou/projects/niri/src/utils/transaction.rs`
- `/home/akisarou/projects/niri/src/handlers/compositor.rs`
- `/home/akisarou/projects/niri/src/handlers/xdg_shell.rs`
- `/home/akisarou/projects/niri/src/window/mapped.rs`
- `/home/akisarou/projects/niri/src/layout/mod.rs`
- `/home/akisarou/projects/niri/src/layout/scrolling.rs`
- `/home/akisarou/projects/niri/src/layout/closing_window.rs`

## High-Level Conclusion

`wm2` is not failing because its transaction primitive is fundamentally different from niri's. The transaction object itself is nearly the same.

`wm2` is failing because the surrounding state machine is much thinner:

1. close and unmap are triggered too early and too destructively,
2. configure emission is not throttled with the same precision,
3. commit-time transaction retention is weaker,
4. dmabuf readiness is not folded into transaction release,
5. the live window object is removed before the close transition is truly finished.

The result is that `wm2` can satisfy its own transaction bookkeeping while still producing a visible intermediate frame.

## What niri Actually Does

### 1. niri keeps transaction state on the mapped window object

In `niri`, per-window configure and transaction state lives on `Mapped` in `src/window/mapped.rs`:

- `transaction_for_next_configure: Option<Transaction>`
- `pending_transactions: Vec<(Serial, Transaction)>`

This sits next to the rest of the mapped-window state:

- configure intent,
- pending/fullscreen/maximized state,
- animation serials,
- interactive resize state,
- pending windowed-fullscreen tracking.

That means transaction lifecycle is not a separate mini-runtime; it is integrated into the full mapped-window state machine.

In `wm2`, `WindowFrameSyncState` tracks a smaller subset:

- pending location,
- pending transactions,
- resize overlay,
- cached snapshot.

This is enough for a prototype, but it means transaction release is not cross-checked against the richer window state that niri uses to decide whether a commit is really the right one.

### 2. niri throttles configure emission based on explicit configure intent

In `niri/src/layout/scrolling.rs`, layout refresh computes a combined `ConfigureIntent` across visible tiles and only sends configures when appropriate.

In `niri/src/window/mapped.rs`, `send_pending_configure()` only sends when:

- there are actual pending changes, or
- `needs_configure` is set,
- and it records side effects like `needs_frame_callback`, animation serials, and transaction serial association in one place.

This is important because niri explicitly acknowledges that multiple in-flight resize transactions are visually dangerous. It tries to prevent them via throttling rather than merely tolerating them.

In `wm2`, relayout decides `needs_configure` from a smaller condition set in `state.rs`:

- unmapped window, or
- size changed, or
- fullscreen flag changed.

This misses the more nuanced configure intent logic. The compositor can still end up with transaction timing that is technically valid but visually rough because it is willing to push configures more eagerly and with less coordination.

### 3. niri starts close animation before `window.on_commit()` and removes the window through layout

In `niri/src/handlers/compositor.rs`, when a mapped root commit loses its buffer:

1. it creates a new `Transaction`,
2. it starts the close animation before `window.on_commit()`,
3. it calls `layout.remove_window(&window, transaction.clone())`,
4. it keeps the `window` object alive as an `Unmapped` window afterwards.

This matters because the live object is not simply deleted. The compositor still has a coherent object model for the just-unmapped surface while the transition resolves.

In `wm2/src/compositor/shell.rs`, `handle_window_close()` does this instead:

1. immediately removes the window record from `managed_windows`,
2. immediately removes it from the runtime/model,
3. optionally snapshots it,
4. unmaps the live element,
5. queues relayout under a transaction.

That is a much harsher state transition. Even if the closing snapshot remains visible, the live window model and mapped record are already gone before the rest of the system has finished settling.

This is one of the strongest candidates for the persistent visual glitch.

### 4. niri integrates dmabuf readiness into blocker release

In `niri/src/handlers/xdg_shell.rs`, matched transactions can be retained in `transaction_for_dmabuf` during the pre-commit hook.

If the commit depends on a dmabuf readiness blocker:

- niri registers the transaction deadline,
- adds a normal transaction blocker if needed,
- then delays dropping the transaction until the dmabuf readiness callback fires.

This is subtle but important: transaction completion is coupled not just to configure serial matching, but to the actual readiness of the new buffer path.

`wm2` does not have an equivalent step. In `wm2/src/handlers/xdg_shell.rs`, once a matching serial is found, the logic is essentially:

- register deadline,
- maybe add blocker,
- then let the transaction be dropped according to ordinary scope/lifetime.

If the new frame is not truly ready at that exact moment, `wm2` can release the old overlay too early.

### 5. niri's close overlay has an explicit waiting-to-animate phase

In `niri/src/layout/closing_window.rs`, `ClosingWindow` has:

- `AnimationState::Waiting { blocker, anim }`, then
- `AnimationState::Animating(anim)`.

The close animation does not just disappear when the blocker releases; it transitions from waiting into animation.

`wm2`'s `ClosingWindow` in `frame_sync/snapshots.rs` is a static texture plus `TransactionMonitor`.

It has no waiting/animating distinction:

- `is_finished()` is just `monitor.is_released()`,
- render is just the static snapshot.

That is fine for a static close path if the timing is perfect. But it leaves zero slack for transaction-release jitter. The first frame after release immediately drops the closing snapshot.

### 6. niri retains richer unmapped-window state after close

After unmap, niri reinserts the window into `unmapped_windows` so it can restart the initial configure sequence cleanly.

`wm2` instead removes the window record outright during close. That makes its lifecycle simpler, but it also makes the timing path more brittle because close, destroy, remap, and future commits have less state continuity.

## Cross-Check Against Current `wm2`

### Planner

The planner is separate from the frame-sync bug. It only determines target geometry.

Still, the requested temporary change is valid for reproductions: `planner.rs` should mimic the `master-stack` layout used in `test_config`, not equal-width columns.

### Likely Root Problems in `wm2`

Ordered by confidence:

1. `handle_window_close()` removes the live managed window too early.
   The close overlay is rendered, but the live record, runtime model entry, and mapped object are gone immediately.

2. `wm2` closes from two independent surfaces of truth.
   It can close from buffer disappearance in `handlers/compositor.rs` and also from `xdg_toplevel.destroyed` in `handlers/xdg_shell.rs`.
   Even if double-close is partially guarded by lookup failure, this is still a weaker lifecycle design than niri's single coherent mapped-to-unmapped path.

3. `wm2` does not fold buffer readiness into transaction lifetime.
   niri's pre-commit path retains the transaction until dmabuf readiness if necessary; `wm2` does not.

4. `wm2` has no configure-intent layer comparable to niri.
   It may be sending configures at moments that are legal but not visually stable.

5. `wm2`'s close overlay is all-or-nothing.
   It exists until release, then vanishes immediately, with no extra phase to absorb timing wobble.

## Practical Takeaways

If continuing `wm2`, the highest-value fixes are not more planner work.

The next serious fixes should be:

1. stop removing the managed window record immediately on close,
2. unify close/unmap lifecycle so there is one authoritative path,
3. retain matched transactions through the full commit-ready path,
4. add a buffer-readiness gate comparable to niri's dmabuf-aware pre-commit handling,
5. move toward a richer per-window configure state model instead of only `pending_location + pending_transactions`.

## Bottom Line

The current `wm2` transaction primitive is not the main problem.

The problem is that niri's frame-perfect behavior is produced by a larger, integrated state machine across:

- layout refresh,
- configure intent,
- mapped-window state,
- pre-commit blockers,
- dmabuf readiness,
- close animation lifecycle,
- and delayed window removal.

`wm2` currently implements only a narrower slice of that system. That narrower slice is enough to look plausible in tests, but not enough to guarantee the same on-screen frame perfection.
