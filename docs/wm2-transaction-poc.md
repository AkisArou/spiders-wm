# WM2 Transaction POC

This document describes the minimal `spiders-wm2` proof of concept added in
`crates/spiders-wm2`.

## Goal

Prove that a Smithay compositor can achieve frame-perfect tiled relayouts without
bringing back the full spiders runtime stack yet.

The prototype intentionally does **not** include:

- config loading
- JS runtime
- JSX
- `spiders-scene`
- custom animation system

It only includes the minimum machinery needed to validate the hard part:

1. hardcoded horizontal tiling
2. a nested Smithay compositor using the local `smallvil`/winit pattern
3. `Alt+Enter` spawning `foot`
4. `Alt+q` sending `xdg_toplevel.close`
5. a transaction model adapted from niri

## Hardcoded Behavior

- Every mapped toplevel participates in a single horizontal row.
- The compositor divides the output width evenly across all managed windows.
- A relayout sends new configure sizes to all windows in the target layout.
- The already-presented positions remain visible until the transaction releases.
- Once the transaction releases, the compositor swaps the new positions into the
  `Space` atomically.

## Transaction Model

The transaction code in `crates/spiders-wm2/src/transaction.rs` copies the core
idea from niri:

- every relayout creates one `Transaction`
- each window gets a cloned handle tied to the configure serial sent to it
- in the surface pre-commit hook, the compositor matches `last_acked.serial`
  against that window's pending transactions
- if a matching pending transaction exists, a Smithay commit blocker is added to
  the surface
- the transaction completes when the last strong handle drops, or after a
  300 ms timeout

This gives two key properties:

1. new client commits do not become visible independently
2. all blocked commits become releasable together once the transaction completes

## Presented vs Pending Layout

The prototype keeps one in-flight pending layout:

- `managed_windows`: all toplevels known to the compositor
- `pending_layout`: the target positions that should be presented next

Relayout flow:

1. compute target rectangles for all managed windows
2. send configures and attach transaction handles to the configure serials
3. keep rendering the old `Space` mapping
4. once the transaction monitor reports released, call `blocker_cleared()` for
   clients and then remap all windows to the new positions

That is the minimal version of the desired-vs-presented split needed for
frame-perfect tiled rearrangement.

## Why This Is Minimal

This prototype deliberately allows only one relayout transaction in flight at a
time. If another relayout is requested while one is pending, it is queued and
started after the current one applies.

That keeps the proof focused on correctness rather than throughput.

## Limitations

This is still a proof of concept, not the final compositor architecture.

Known limitations:

- no resize throttling yet
- no dmabuf-readiness blocker layer yet
- no window snapshots for close/unmap transitions
- no animation system yet
- close-path relayout is simpler than the final intended behavior
- no shell layers, decorations, or advanced focus policy beyond the minimum

## Next Steps

If this prototype behaves correctly under nested testing, the next steps should
be:

1. add resize throttling so only one meaningful size transaction is in flight
2. add dmabuf/GPU-readiness blockers where needed
3. harden destroy/unmap behavior during pending transactions
4. add snapshot-backed close transitions
5. only then reintroduce authored layout/runtime layers on top of this
   transaction core
