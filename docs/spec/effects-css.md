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
- `window::titlebar`
- `window:focused::titlebar`
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

- `appearance` on `window`, where `appearance: none` disables compositor-drawn titlebars/decorations
- `opacity`
- `border-width`
- `border-color`
- `border-radius`
- `box-shadow`
- `backdrop-filter: blur(...)`
- constrained `transform`

## Window Decoration Parts

`window::titlebar` is a compositor-owned pseudo-element for server-drawn window
chrome.

It is not a DOM descendant and should not be modeled as `window .titlebar`.

V1 should allow `window::titlebar` to control titlebar presentation properties
such as:

- `background`
- `color`
- `height`
- `padding`
- `border-bottom-width`
- `border-bottom-style`
- `border-bottom-color`
- `font-family`
- `font-size`
- `font-weight`
- `letter-spacing`
- `text-transform`
- `text-align`
- `box-shadow`
- `border-radius`

These styles apply only to compositor-drawn decorations. They do not restyle
client-drawn application chrome.

When compositor-drawn titlebars exist, the compositor should materialize a
typed render-facing titlebar payload from the resolved window policy rather than
re-reading raw CSS at paint time.

For floating windows, authored layout geometry may be overridden by persistent
compositor-managed floating placement. In that case `window::titlebar` styling
still applies, but the compositor should render and interact against the
persisted floating window rect rather than the tiled layout rect.

That payload should contain at least:

- resolved visibility decision
- resolved `window::titlebar` style object
- display title text
- optional app identity for future icon or badge rendering

## Decoration Scope

`appearance: none` guarantees that `spiders-wm` does not draw compositor-managed
server-side decorations for the matched window.

It does not guarantee removal of client-side decorations drawn by the
application itself.

Implications:

- server-side decorations should be suppressed
- xdg-decoration aware clients may be nudged toward client-side mode
- client-drawn headerbars or custom chrome may still remain visible

V1 should document this as compositor decoration policy, not as a universal
"remove all titlebars" mechanism.

## Transform Scope

V1 `transform` support should stay intentionally small:

- `translate(x, y)`
- `scale(s)`

Full browser transform grammar is out of scope.

## Animation And Transition Support

The Rust implementation should use the `keyframe` crate for animation timelines
and interpolation.

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
- run `keyframe`-backed animation and transition state machines separately from
  layout geometry

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
