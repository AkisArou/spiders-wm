# Scene Incremental Recompute

## Purpose

This document describes a possible next-stage optimization for preview/runtime layout performance.

It is intentionally a design document only.

We should not implement this yet.

## Problem

After the completed runtime/performance work, the main remaining close-path bottleneck is still full scene/layout recomputation.

Current measured state:

- focus is much cheaper now
- CSS recompilation is no longer the issue
- app-owned invalidation logic is no longer the issue
- duplicate applies are no longer the issue
- close still pays roughly:
  - `~4-5ms` command work
  - `~55-60ms` immediate layout compute
  - plus reevaluation cost when authored layout depends on changed context

The dominant remaining runtime cost is inside:

- `spiders-scene`
- `spiders-wm-runtime`

Specifically the full rebuild of:

- styled layout tree
- Taffy tree and layout computation
- snapshot tree

## Goal

Reduce layout recompute cost for runtime interactions, especially close/spawn/layout-affecting changes, by reusing unchanged scene/layout subtrees when correctness allows it.

## Non-Goals

- Do not implement this immediately.
- Do not weaken correctness for authored layouts, CSS selectors, or geometry.
- Do not move layout invalidation policy back into `spiders-wm-www`.
- Do not assume all close/spawn operations can become constant-time.

## Current Architecture

Current steady-state path for a layout-affecting preview action:

1. runtime command mutates preview WM state
2. resolved layout tree is built
3. titlebar content is attached
4. stylesheet is already cached and reused
5. styled layout tree is rebuilt from scratch
6. Taffy tree is rebuilt from scratch
7. Taffy layout is computed from scratch
8. snapshot tree is rebuilt from scratch
9. snapshot geometries are collected back into preview state

Important completed optimizations already in place:

- explicit preview render actions from runtime
- no whole-session async reevaluation noise
- stylesheet compilation cache
- authored evaluation dependency tracking
- authored evaluation cache
- persistent lower-layer authoring service
- scene response cache for exact request reuse

Those optimizations removed avoidable work, but they did not eliminate the full scene/layout rebuild cost when the scene request genuinely changes.

## Why Exact Caching Is Not Enough

Exact caching helps only when the full request repeats.

For close on `master-stack`, the request changes because:

- visible window count changes
- stack subtree shape changes
- child sizes in the stack change

So exact-request cache hits are rare on the hot close transition.

That means the next meaningful optimization must be structural reuse, not exact-response caching.

## Design Direction

### High-Level Idea

Split the scene/layout pipeline into reusable layers and allow unchanged subtrees to be preserved across interactions.

Potential reuse layers:

1. resolved layout subtree reuse
2. styled subtree reuse
3. layout engine subtree reuse
4. snapshot subtree reuse

We should prefer starting at the highest layer that gives useful wins with manageable complexity.

## Candidate Reuse Layers

### Option A. Reuse Styled Subtrees

Reuse previously computed `StyledLayoutTree` branches when:

- resolved subtree identity is unchanged
- stylesheet identity is unchanged
- selector-relevant node metadata is unchanged

Benefits:

- avoids recomputing CSS matching and computed style for unchanged branches
- simpler than true incremental Taffy reuse

Limitations:

- still rebuilds Taffy tree and relayout from the styled tree
- may only give moderate wins if Taffy dominates

### Option B. Reuse Snapshot Subtrees

Reuse unchanged snapshot branches after layout if geometry and style are unchanged.

Benefits:

- could reduce snapshot rebuild/allocation cost
- good for rendering-side stability

Limitations:

- does not avoid style matching or Taffy compute
- likely not enough by itself

### Option C. Reuse Taffy/Layout Subtrees

Preserve node identity and avoid rebuilding or recomputing unaffected Taffy branches.

Benefits:

- potentially the biggest performance win

Limitations:

- highest complexity
- likely requires significant scene/layout engine redesign
- harder to prove correct

### Option D. Hybrid Approach

Start with styled subtree reuse and snapshot reuse.

If the remaining cost is still dominated by Taffy, then evaluate whether incremental layout-engine reuse is justified.

This is the recommended direction.

## Required Building Blocks

### 1. Stable Subtree Identity

We need a stable way to identify equivalent subtrees across recomputations.

Candidates:

- explicit node IDs where available
- structural fingerprints for group/content nodes
- window ID for window nodes
- titlebar/content child identity derived from parent window node

Questions:

- when does a subtree represent the “same node” with different geometry?
- when does a changed child list invalidate the parent subtree identity?

### 2. Invalidation Rules

For each layer we need to know what changes invalidate reuse.

Examples:

- stylesheet source changes
- selector-relevant metadata changes
- window focused/floating/fullscreen state changes
- layout subtree structure changes
- titlebar content changes
- window count changes
- canvas/monitor size changes

### 3. Layered Cache Ownership

The cache should live in lower layers, not UI consumers.

Likely ownership:

- `spiders-scene` owns scene/style/layout reuse caches
- `spiders-wm-runtime` decides when to request scene recompute
- apps remain dumb consumers

## Recommended First Implementation Slice

### Phase 1. Styled Subtree Reuse

Target:

- cache/reuse unchanged `NodeComputedStyle` branches

When to reuse:

- same resolved subtree identity
- same stylesheet identity
- same selector-relevant metadata on this subtree
- same titlebar fallback inputs

Why start here:

- smaller than full Taffy incremental reuse
- directly targets a real part of `compute_from_cached_sheet`
- gives us measurement data for whether style-tree rebuild is a meaningful share of total cost

### Phase 2. Snapshot Reuse For Unchanged Branches

Target:

- if layout geometry and styles for a subtree are unchanged, reuse the snapshot subtree

Why next:

- incremental and lower risk
- reduces tree allocation and post-processing

### Phase 3. Evaluate Taffy Reuse Need

Only if measurements still show most cost is inside layout compute.

At that point decide whether:

- partial Taffy reuse is feasible
- or the complexity is too high relative to the likely win

## Expected Benefit

Best-case:

- meaningful reduction in close/spawn latency for layouts where only part of the tree changes

Realistic-case:

- moderate improvement for mixed layouts
- smaller but still useful improvement for `master-stack`, because the stack subtree still genuinely changes on close

Worst-case:

- little improvement if Taffy/layout compute dominates and style-tree rebuild is a small fraction

This is why staged measurement is important.

## Risks

### Correctness Risks

- stale styles if selector-relevant metadata is not fully captured
- stale titlebar appearance
- stale snapshot subtrees after geometry-affecting changes
- broken authored-layout assumptions for runtime-dependent layouts

### Complexity Risks

- too much machinery for small gain
- cache invalidation becoming harder than full recompute
- adding hidden coupling between scene/runtime layers

## Measurement Plan For Future Work

Before implementing any slice:

- split `compute_from_cached_sheet` into:
  - style-tree build time
  - Taffy tree build time
  - Taffy compute time
  - snapshot build time

Then after each incremental step:

- compare close-path timings
- compare spawn-path timings
- verify scene correctness visually and via tests

## Recommendation

Do not implement full incremental layout recomputation immediately.

Recommended next project when we return to this:

1. instrument `spiders-scene` more finely
2. implement styled subtree reuse only
3. measure again
4. decide whether deeper Taffy reuse is justified

This keeps the work grounded and avoids a large speculative redesign.
