# Titlebar Burst-Open Performance Plan

## Goal

Make native custom-titlebar opening performance feel immediate during rapid multi-window bursts without regressing:

- initial configure sizing
- pre-map decoration-mode decisions
- close/unmap sequencing
- frame-sync correctness

## Current Measured State

Validated with `scripts/wm-smoke.sh` after adding a burst mode.

- Default mode still passes.
- Burst mode now opens and closes 10 windows quickly and reports timing summaries.
- Latest burst summary:
  - `opens=10`
  - `first_maps=10`
  - `close_starts=10`
  - `close_unmaps=10`
  - `relayouts=34`
  - `max_prepare_ms=32.619858`
  - `max_overlay_ms=24.072595`
  - `max_relayout_ms=48.525936`

## Confirmed Findings

1. The previous font resolver fix was real and important.
   Overlay rendering no longer explodes with window count the way it did before.

2. Pre-map duplicate work was reduced successfully.
   Initial configure now mostly prepares once and reuses the prepared titlebar snapshot.

3. The remaining burst-open problem is not solved.
   Even after those fixes, burst mode still produces many full relayouts and relayouts still reach roughly 50ms in debug smoke runs.

4. The current narrow first-map deferral is not structurally reliable.
   In the latest burst runs, first-map decisions logged `pending_unmapped_windows=0` for every first map, so the deferral branch never triggered.

5. The harness mattered.
   The original smoke script was too slow to reproduce the user complaint. The new burst mode is required for performance work here.

## Why The Current Branch Misses

The existing first-map optimization assumes multiple windows will still be unmapped when a given first map arrives.
That is not consistently true in practice.

In the validated burst run, windows still arrived quickly overall, but each first-map commit observed no remaining unmapped windows at the decision point. That means the WM still did immediate synchronous relayouts repeatedly through the burst.

## Working Hypothesis

The main remaining cost is repeated full-scene relayout work on the mapped path, not just titlebar rasterization.

Likely contributors:

- full `start_relayout()` on each burst step
- repeated scene snapshot recomputation for small incremental window-count changes
- repeated configure churn for already-mapped windows while the burst is still forming
- repeated titlebar overlay regeneration tied to those full relayouts

## Recommended Work Order

1. Make relayout coalescing burst-aware without breaking frame-sync.
2. Separate “window becomes visible immediately” from “full final tiled arrangement is recomputed immediately”.
3. Reduce repeated work for already-mapped windows during burst growth.
4. Only then revisit titlebar-specific micro-optimizations.

## Proposed Workstreams

### 1. Burst-safe relayout coalescing

Introduce a coalescing mechanism that does not replace correctness-critical configure tracking.

Target behavior:

- first mapped window still appears immediately using its planned layout
- subsequent burst arrivals do not each force a full synchronous global relayout inline
- one coalesced relayout finalizes the arrangement once the burst settles or at the next safe loop boundary

Important constraint:

- do not reuse the earlier failed idle-only experiment unchanged
- coalescing must preserve close-path behavior and transaction completion rules

Safer direction:

- queue a relayout request reason/state
- schedule exactly one deferred relayout when no relayout is already queued or active
- keep direct immediate relayout only for operations that are correctness-critical today
- explicitly distinguish `first-map-burst`, `close`, `workspace-switch`, `fullscreen`, and `floating-toggle` causes

### 2. Immediate provisional placement, deferred global reshaping

Use the precomputed first-map placement for immediate UX, but avoid immediately reconfiguring every existing mapped window during the burst.

That means:

- the new window maps right away from `ready_layout` / planned layout
- existing mapped windows keep their current geometry for a short coalescing window
- one later relayout performs the final global arrangement and configure set

This matches the user's “best UX is immediate” preference better than delaying the new titlebar/window entirely.

### 3. Reduce relayout work for already-mapped windows

Even with coalescing, relayout cost should come down.

Candidates:

- skip configure emission when the target geometry is unchanged
- avoid rebuilding titlebar overlays for windows whose titlebar subtree did not materially change
- consider caching scene-derived titlebar subtrees by window and invalidating only when layout-affecting inputs change
- avoid recomputing scene snapshot twice when one snapshot can serve both layout targeting and titlebar overlay refresh

### 4. Prepare titlebar work while waiting on frame-sync

This is still worth exploring, but only after the burst relayout shape is fixed.

Useful version of the idea:

- when a new window has an initial planned layout, prepare and retain its titlebar subtree/materialized overlay inputs ahead of final relayout
- on the eventual coalesced relayout, reuse those prepared artifacts if geometry/style identity still matches

Avoid:

- speculative work that must be thrown away most of the time
- anything that races against transaction/commit ordering assumptions

### 5. Instrumentation to keep during the next round

Keep or extend logging for:

- relayout cause
- queued-vs-executed relayout counts
- first-map decision state
- max/min relayout elapsed time
- configure count per relayout
- overlay count and overlay elapsed time

Add if needed:

- count of windows that actually changed target geometry in each relayout
- count of titlebar overlays regenerated vs reused

## Verification Plan

Use both:

- `./scripts/wm-smoke.sh`
- `WM_SMOKE_MODE=burst ./scripts/wm-smoke.sh`

Burst success criteria:

- materially fewer relayouts than current burst baseline
- lower worst-case relayout time
- lower total configure churn on already-mapped windows
- new windows still appear immediately
- no regressions in close/unmap behavior

## Near-Term Next Change

The most promising next implementation is:

1. add explicit relayout-cause tracking and a single safe relayout queue
2. use immediate provisional placement for first map
3. coalesce burst-open global relayouts
4. re-measure before doing deeper titlebar caching work

## Files Touched During This Investigation

- `scripts/wm-smoke.sh`
- `apps/spiders-wm/src/compositor/windows.rs`

Relevant existing hot-path files:

- `apps/spiders-wm/src/compositor/layout.rs`
- `apps/spiders-wm/src/handlers/xdg_shell.rs`
- `crates/titlebar/native/src/lib.rs`
