# TITLEBAR

Final titlebar architecture direction.

This file is the target design, not an intermediate note.

## Problems

1. Titlebar planning and titlebar rasterization are currently mixed together in deprecated native river code.
2. The old renderer in `crates/wm-river/src/backend/titlebar_renderer.rs` is native-specific:
   - `tiny_skia`
   - `ab_glyph`
   - raw pixel buffers
   - ad hoc filesystem font loading
3. Web and native do not want the same rendering backend, even if they share titlebar semantics.
4. `docs/css.md` already defines a CSS surface for `window::titlebar`, but the implementation boundary is still unclear.
5. We want custom titlebars to be reusable across Wayland and web, not trapped in one deprecated backend.

## What Exists Today

### CSS / style modeling

The CSS and scene stack already owns most of the typed titlebar-relevant style surface.

Implemented in `spiders-css` / `spiders-scene` today:
- `window::titlebar` pseudo-element selector support
- typed `ComputedStyle` values for:
  - `appearance`
  - `background`
  - `color`
  - `opacity`
  - `border_color`
  - `border_style`
  - `border_radius`
  - `box_shadow`
  - `text_align`
  - `text_transform`
  - `font_family`
  - `font_size`
  - `font_weight`
  - `letter_spacing`
  - `padding`
  - `height`
  - `transform`
  - motion-related fields

This means the style language is already mostly in the right place.

### Native river implementation

The deprecated river path already has:
- `AppearancePlan`
- `DecorationMode`
- `TitlebarPlan`
- logic that derives titlebar appearance from computed `window` and `window::titlebar` styles
- `tiny_skia` titlebar rasterization
- `ab_glyph` text rasterization
- best-effort support for:
  - background
  - bottom border
  - text color
  - text alignment
  - text transform
  - font family mapping
  - font size
  - font weight
  - letter spacing
  - box shadow
  - rounded top corners

### Web

Web now has a real titlebar subsystem.

The current browser preview renders titlebars through `crates/titlebar/web` from shared titlebar planning output, not through the old native renderer.

## CSS Reality Check

`docs/css.md` is broadly pointing in the right direction, but several titlebar semantics are still native-only or best-effort rather than truly shared.

### Already real in typed CSS/style modeling

These are genuinely modeled in shared style types today:
- `window::titlebar`
- `appearance`
- `background` / `background-color`
- `height`
- `border-bottom-width`
- `border-bottom-style`
- `border-bottom-color`
- `padding`
- `color`
- `opacity`
- `text-align`
- `text-transform`
- `font-family`
- `font-size`
- `font-weight`
- `letter-spacing`
- `box-shadow`
- `border-radius`
- `transform`

### Not actually shared yet

These semantics are still incomplete or not fully shared yet:
- scene-backed titlebar child layout from a declarative titlebar tree
- shared exact text measurement/truncation policy across native and web
- host-independent titlebar motion application
- clickable native button hit-regions on compositor backends

### Native-only or partial today

These behaviors currently exist only as native river behavior or best-effort fallback logic:
- compositor titlebar rasterization
- native font fallback and `options.titlebar_font` path loading
- box-shadow raster strategy
- rounded-corner clipping strategy
- `appearance: none` as river-specific decoration behavior

### Still effectively TODO

From the current docs and implementation, these are not complete shared behaviors yet:
- `backdrop-filter`
- full transform visual support on titlebar rendering across hosts
- a host-independent titlebar animation/runtime system
- shared exact parity between native and web titlebar rendering

## Answers

### Should we keep `tiny_skia` and `ab_glyph`?

Yes, but only as native rendering backend dependencies.

They are still a good fit for:
- Wayland/compositor-side titlebar pixel rendering
- deterministic raster output
- non-DOM rendering targets
- native snapshot tests for exact pixel behavior

They should not become the shared titlebar abstraction.

Reason:
- font loading and rendering constraints differ significantly between native and web
- web should not be forced through native font probing and custom wasm glyph rasterization by default

### Should we create a new titlebar crate?

Yes.

And because titlebars already have distinct cross-platform vs backend-specific concerns, the split should mirror the JS runtime family.

Recommended structure:
- `crates/titlebar/core`
- `crates/titlebar/native`
- `crates/titlebar/web`

Related reusable dependency:
- `crates/fonts/native`

This is the right direction.

### Should web render titlebars via canvas?

Not by default.

Default answer: use HTML/CSS for web titlebars.

Canvas should be optional and justified only if we later need:
- pixel-level parity with native raster output
- image export
- compositor-style offscreen rendering in browser

For the actual web app and editor preview, HTML/CSS is the better default because it gives:
- native browser text rendering
- easier interactivity
- easier debugging
- better accessibility
- easier integration with the rest of the web UI

### Should web and native share one renderer?

No.

They should share one titlebar plan/model and separate renderers.

Shared:
- titlebar inputs
- resolved titlebar semantics
- titlebar plan/model

Not shared:
- raster backend
- renderer backend
- DOM/canvas vs pixel-buffer rendering

## Final Mental Model

There are four titlebar concerns.

### 1. Titlebar style semantics

Owned by the CSS/style system.

Examples:
- `window::titlebar`
- background
- text styling
- shadows
- corner radii
- appearance hints

This already mostly lives in `spiders-css` and `spiders-scene`.

### 2. Titlebar structure

Owned by the config/runtime layer and compiled into a shared titlebar tree.

This layer should:
- let users define titlebar rules with `titlebars: [<titlebar ...> ...]`
- support `when` conditions and `disabled`
- support shared titlebar child nodes like `titlebar.group`, `titlebar.windowTitle`, `titlebar.button`, `titlebar.icon`
- keep layout/styling props out of JSX and in CSS

This is the declarative structure layer.

### 3. Titlebar planning

Owned by a new shared titlebar core crate.

This layer should:
- read resolved/computed style data
- read the resolved titlebar structure tree
- combine `window` and `window::titlebar` fallback rules
- apply host-independent semantics
- produce a canonical titlebar plan / layout-ready representation

It should consume shared font intent/types, not load fonts.

This is the cross-platform part.

### 4. Titlebar rendering

Owned by backend crates.

Examples:
- native raster renderer using `tiny_skia` and `ab_glyph`
- web DOM renderer using HTML/CSS
- optional future web canvas renderer

Native renderers should consume a reusable native font resolver instead of owning filesystem font lookup policy.

This is host/backend-specific.

## Recommended Crate Boundaries

### `crates/titlebar/core`

Owns shared titlebar planning and canonical titlebar data structures.

Owns:
- `TitlebarPlan`
- `DecorationMode`
- `TitlebarPlanInput`
- `TitlebarPlanPreset`
- titlebar rule selection inputs and resolved titlebar tree types
- titlebar fallback rules between `window` and `window::titlebar`
- titlebar text selection policy
- titlebar text transform policy
- host-independent alignment/padding/radius/shadow/titlebar metrics logic
- shared titlebar button plan and palette
- shared leading/trailing title text exclusion helpers
- generic host presets that reshape a plan without replacing shared planning

Consumes:
- shared font intent/types from the shared style layer

Does not own:
- `tiny_skia`
- `ab_glyph`
- `web_sys`
- DOM rendering
- native font resolution
- Wayland protocol calls

### `crates/titlebar/native`

Owns native titlebar raster rendering.

Owns:
- `tiny_skia`
- `ab_glyph`
- pixel-buffer rendering from `TitlebarPlan`

Consumes:
- resolved native fonts from `crates/fonts/native`

Does not own:
- CSS parsing
- titlebar planning rules
- native font discovery policy
- Wayland decoration orchestration

### `crates/titlebar/web`

Owns web-facing titlebar rendering helpers.

Initial target:
- produce a web render model or style map from shared titlebar planning output
- support DOM/HTML rendering first

Implemented now:
- `WebTitlebarViewModel`
- `WebTitlebarButtonState`
- `WebTitlebarTrailingContent`
- DOM-oriented button/trailing positioning derived from shared titlebar geometry

May later also own:
- optional canvas renderer if needed

Does not own:
- CSS parsing
- titlebar planning rules
- native rasterization

## Existing Crate Responsibilities

### `spiders-css` / `spiders-scene`

Keep ownership of:
- selector parsing
- pseudo-element matching
- typed computed style values
- shared font intent/types

Do not move CSS parsing into titlebar crates.

### `apps/spiders-wm`

Should eventually:
- ask shared titlebar core for `TitlebarPlan`
- use host-specific native/titlebar backend adapters

It should not own shared titlebar planning rules.

### `apps/spiders-wm-www`

Currently:
- builds titlebars through shared `titlebar/core` helpers
- uses a generic `TitlebarPlanPreset` to shape preview-specific defaults
- renders those plans via `titlebar/web`
- passes semantic trailing content to the web view model

It should continue to avoid inventing separate titlebar semantics.

### `crates/wm-river`

Current deprecated code is an extraction source.

What should move out:
- `TitlebarPlan`
- `DecorationMode`
- titlebar planning helpers
- titlebar raster renderer

What should stay native-app-specific:
- actual Wayland surface/buffer decoration orchestration
- `use_ssd()` / `use_csd()` / decoration protocol wiring

### `crates/fonts/native`

Owns reusable native font resolution.

Owns:
- native/system font discovery and lookup
- caching and indexing of native fonts
- resolving shared font queries into reusable native font artifacts

Does not own:
- titlebar planning
- CSS parsing
- DOM rendering
- Wayland-specific titlebar buffer orchestration

## Rendering Decision

### Native / Wayland

Use:
- `titlebar/core` for planning
- `fonts/native` for font resolution
- `titlebar/native` for raster rendering

Output:
- raw pixels or pixmap-backed render artifacts suitable for Wayland buffers

Current river limitation:
- native titlebar buttons may be rendered from shared button plans, but should not be assumed clickable
- `river_decoration_v1` does not expose decoration-surface button events
- `river_seat_v1` only exposes high-level window interaction and pointer state
- global pointer bindings are not a clean replacement because matching bindings steal button presses from the focused surface
- native titlebar hit-testing should only be added when the backend has a dedicated, race-free input path for decoration surfaces

### Web

Use:
- `titlebar/core` for planning
- `titlebar/web` for rendering

Default output:
- HTML/CSS render model

Not default:
- canvas raster output

### Why HTML/CSS first on web

Because titlebars are not only images.

They are interactive UI with:
- text
- hover/active states
- buttons
- hit regions
- host-level affordances

DOM is a better default substrate for that than canvas.

## Current Status

Implemented today:
- shared titlebar planning in `crates/titlebar/core`
- shared button plan, palette, and left/right text exclusion helpers
- generic plan presets in `titlebar/core`
- native pixel rendering in `crates/titlebar/native`
- web DOM view-model generation in `crates/titlebar/web`
- semantic trailing content support for web hosts

Still intentionally not shared or not finished:
- declarative `titlebars: [<titlebar ...>]` config rules
- scene-backed titlebar child layout
- exact native/web text truncation parity
- native clickable titlebar buttons on river
- host-independent animation/runtime behavior

## Final Config Shape

The final user-facing configuration should be rule-based and titlebar-specific.

Use:

```tsx
export default () => ({
  titlebars: [
    <titlebar class="default-titlebar">
      <titlebar.group class="left">
        <titlebar.icon asset="app-icon" class="app-icon" />
        <titlebar.workspaceName class="workspace-name" />
      </titlebar.group>

      <titlebar.group class="center">
        <titlebar.windowTitle class="window-title" />
      </titlebar.group>

      <titlebar.group class="right">
        <titlebar.badge class="env-badge">DEV</titlebar.badge>

        <titlebar.button class="floating-button" onClick={actions.toggleFloating}>
          <titlebar.icon asset="pin" />
        </titlebar.button>

        <titlebar.button class="close-button" onClick={actions.close}>
          <titlebar.icon asset="close" />
        </titlebar.button>
      </titlebar.group>
    </titlebar>,

    <titlebar when={{ workspace: "code" }} class="code-titlebar">
      <titlebar.group class="left">
        <titlebar.text class="workspace-label">CODE</titlebar.text>
      </titlebar.group>

      <titlebar.group class="center">
        <titlebar.windowTitle class="window-title" />
      </titlebar.group>
    </titlebar>,

    <titlebar when={{ appId: "foot" }} disabled />,
  ],
})
```

Rules:
- `titlebars` is an ordered list
- the rule without `when` is the default rule
- later matching rules override earlier rules
- last match wins
- `disabled` means no custom titlebar for that match

`when` should support:
- `workspace`
- `slot`
- `appId`
- `title`
- `floating`
- `fullscreen`

## Final Element Set

Use a constrained shared titlebar DSL, not arbitrary web DOM/components.

Supported elements:
- `<titlebar>`
- `<titlebar.group>`
- `<titlebar.windowTitle>`
- `<titlebar.workspaceName>`
- `<titlebar.text>`
- `<titlebar.badge>`
- `<titlebar.button>`
- `<titlebar.icon>`

Rules:
- `class` is allowed on titlebar elements as a styling hook
- layout/styling props like `height`, `padding`, `transform`, `font-size`, etc. should stay in CSS
- `titlebar.icon` should render user-owned icon content or assets, not titlebar-owned built-in icons
- `titlebar.button` currently supports shared serializable action descriptors, not arbitrary JS callbacks
- arbitrary JS callbacks may be supported in the future, but are not implemented today
- shared action helpers remain a future-facing authoring goal and should normalize to the serializable action shape

## CSS Ownership

CSS should own titlebar layout and appearance.

That includes:
- `display`
- flex/grid/block layout
- `gap`
- `align-items`
- `justify-content`
- `height`
- `padding`
- `font-*`
- colors
- borders
- shadows
- transforms

JSX should define structure and behavior, not box-model props.

Example:

```css
window::titlebar.default-titlebar {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 24px;
  padding: 0 8px;
  background: rgba(22, 31, 45, 0.96);
  color: rgba(230, 233, 239, 1);
}

window::titlebar-group.left,
window::titlebar-group.right {
  display: flex;
  align-items: center;
  gap: 6px;
}

window::titlebar-group.center {
  display: flex;
  flex: 1;
  min-width: 0;
  justify-content: center;
}

window::titlebar-window-title.window-title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
```

This implies titlebar child nodes must become real scene/layout nodes, not just manual button/text offsets.

## Final Internal Direction

The final internal pipeline should be:

1. Parse `titlebars: [<titlebar ...>]` into ordered titlebar rules.
2. Resolve the active rule via `when` and `disabled`.
3. Compile the selected rule into a shared titlebar AST.
4. Convert that AST into scene/layout nodes with shared CSS selector support.
5. Let the shared style/layout pipeline compute layout.
6. Render from that layout in:
   - `titlebar/web` via DOM
   - `titlebar/native` via raster drawing

## Canonical Shared Types

Shared titlebar planning still needs canonical host-neutral types, but they now sit behind a declarative titlebar rule and node model.

Important shared planning types include:

```rust
pub enum DecorationMode {
    ClientSide,
    CompositorTitlebar,
    NoTitlebar,
}

pub struct TitlebarPlan {
    pub window_id: WindowId,
    pub mode: DecorationMode,
    pub title: String,
    pub font: FontQuery,
    pub height: i32,
    pub background: ColorValue,
    pub text_color: ColorValue,
    pub text_align: TextAlignValue,
    pub text_transform: TextTransformValue,
    pub letter_spacing: i32,
    pub padding_top: i32,
    pub padding_right: i32,
    pub padding_bottom: i32,
    pub padding_left: i32,
    pub border_bottom_width: i32,
    pub border_bottom_color: ColorValue,
    pub box_shadow: Option<Vec<BoxShadowValue>>,
    pub corner_radius_top_left: i32,
    pub corner_radius_top_right: i32,
}
```

This remains intentionally host-neutral resolved data, not raw CSS.

`FontQuery` should come from shared style/types, not from titlebar-local types.

## Important Separation

Do not mix these layers again:

- CSS parsing
- titlebar planning
- titlebar rasterization
- host decoration protocol calls

They should be separate.

That is the main architectural correction.

## Migration Status

Completed:
1. Added this architecture document.
2. Created `crates/titlebar/core`.
3. Moved `TitlebarPlan` and `DecorationMode` there.
4. Moved shared titlebar planning helpers out of deprecated `wm-river` planner code into `titlebar/core`.
5. Kept CSS parsing and computed style ownership in `spiders-css` / `spiders-scene`.
6. Introduced shared typed font intent in the style layer.
7. Created `crates/fonts/native`.
8. Moved native font lookup logic there with caching/indexing.
9. Created `crates/titlebar/native`.
10. Moved `tiny_skia` / `ab_glyph` raster rendering there.
11. Updated native host code to depend on `titlebar/core` + `fonts/native` + `titlebar/native`.
12. Created `crates/titlebar/web`.
13. Implemented a DOM/HTML render model from `TitlebarPlan` there.
14. Updated `apps/spiders-wm-www` preview surfaces to use `titlebar/web` instead of ad hoc HTML blocks.

Still optional/future:
15. Only consider a web canvas backend later if actual parity requirements justify it.
16. Replace manual titlebar child positioning with scene-backed titlebar node layout.
17. Introduce `titlebars: [<titlebar ...>]` rule parsing and selection.
18. Introduce the shared titlebar AST and map it into scene/layout nodes.

## Decision Summary

Final decisions:
- keep `tiny_skia` and `ab_glyph`, but only for native rendering
- keep font intent in shared style/types, not in titlebar-local types
- create `crates/fonts/native` for reusable native font resolution
- create a new titlebar family split as:
  - `crates/titlebar/core`
  - `crates/titlebar/native`
  - `crates/titlebar/web`
- keep CSS parsing and typed style ownership in `spiders-css` / `spiders-scene`
- move shared titlebar planning out of deprecated native code
- use declarative `titlebars: [<titlebar ...>]` rules for structure and rule selection
- keep titlebar layout/styling in CSS rather than JSX props
- use user-owned icon assets/content rather than built-in titlebar icon names
- use HTML/CSS for web titlebar rendering by default
- do not make canvas the default web titlebar renderer

## Open Questions

Remaining questions without changing the direction above:

1. Should exact text truncation behavior be shared across native and web, or is semantic alignment enough?
2. What backend/protocol path should native hosts use for clickable titlebar buttons?
3. How much arbitrary JS callback behavior should be allowed in titlebar button handlers across hosts?

Current recommendation:
- keep web rendering DOM-first
- keep native rendering pixel-based
- keep native buttons render-only until the backend exposes a clean decoration input path
- add exact button/hit-region parity only after backend input support is confirmed
