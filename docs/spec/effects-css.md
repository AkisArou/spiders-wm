# Effects CSS Spec

## Overview

Effects CSS is separate from structural layout CSS.

- layout CSS styles structural layout nodes and drives geometry
- effects CSS styles real windows and workspace snapshots

This separation is intentional and should remain in the Rust rewrite.

## Selector Domain

V1 should support selectors equivalent to:

- `window`
- `window[app_id="..."]`
- `window[title="..."]`
- `workspace`

Supported pseudo states should include:

- `:focused`
- `:floating`
- `:fullscreen`
- `:urgent`
- `:closing`
- `workspace:enter-from-left`
- `workspace:enter-from-right`
- `workspace:exit-to-left`
- `workspace:exit-to-right`

## Static Properties

V1 should support at least:

- `opacity`
- `border-width`
- `border-color`
- `border-radius`
- `box-shadow`
- `backdrop-filter: blur(...)`
- constrained `transform`

## Transform Scope

V1 `transform` support should stay intentionally small:

- `translate(x, y)`
- `scale(s)`

Full browser transform grammar is out of scope.

## Animation And Transition Support

V1 should support:

- `@keyframes`
- `animation-*`
- `animation` shorthand
- `transition-*`
- `transition` shorthand

Interpolated animated properties should include at least:

- `opacity`
- `border-radius`
- `box-shadow` blur and color
- constrained `transform` translate/scale components

## Close Animation Model

The compositor should support delayed-close animation through `window:closing`.

When a `window:closing` rule defines a transition:

- the live surface is replaced by a frozen snapshot
- the snapshot animates toward the `:closing` style
- the actual destroy completes after the transition duration

This behavior is part of the product and should be preserved.

## Workspace Transition Model

Directional workspace transitions should be preserved for single-tag switches.

Rules:

- direction is inferred from tag order
- forward switches use `:enter-from-right` and `:exit-to-left`
- backward switches use `:enter-from-left` and `:exit-to-right`
- multi-tag switches may fall back to a simpler fade behavior in V1

These transitions operate on snapshots, not by re-laying out live windows during
the animation.

## Recommended Implementation Split

- parse stylesheet into typed effect rules
- match rules against live window/workspace snapshot state
- compute static visual state
- run animation and transition state machines separately from layout geometry

## Current Limitations Intentionally Preserved In V1

- no true inset shadow rendering requirement
- no full elliptical radius requirement
- no full browser CSS grammar

## Acceptance Criteria

V1 is acceptable when:

- focused and unfocused window styling is visible
- floating and fullscreen pseudo states affect styling
- close animations work through delayed destruction and snapshots
- directional workspace transitions work for single-tag changes
- unsupported CSS fails clearly instead of behaving silently and unpredictably
