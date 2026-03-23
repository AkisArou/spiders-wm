# CSS Plan

This file tracks the CSS roadmap for spiders-wm.

## Direction

- There is one CSS surface in spiders-wm.
- Documentation should describe the intended production model, with `(TODO)` markers on features that are not implemented yet.
- `docs/css.md` should stay close to the code and lose TODO markers as features land.

## Phases

### 1. CSS Doc Cleanup

Status: mostly complete

- Rewrite `docs/css.md` around a single CSS model.
- Remove the old `layout CSS` vs `effects CSS` distinction.
- Keep implemented features documented plainly.
- Mark planned features inline with `(TODO)`.
- Make clear that unsupported selectors/properties fail at parse time.

### 2. Scene Border Widths -> River Borders

Status: complete

- Read `border-width` values from `LayoutSnapshotNode` styles.
- Apply those widths through river `set_borders`.
- Keep current focused/unfocused color policy for now.
- Fall back to existing hardcoded border behavior when no scene border style exists.

### 3. Window State Selectors

Status: complete

- Support `window:focused`
- Support `window:floating`
- Support `window:fullscreen`
- Support `window:urgent`
- Decide whether to implement as real pseudo-classes or runtime-added classes/attributes.
- Runtime class aliases shipped: `.focused`, `.floating`, `.fullscreen`, `.urgent`

### 4. Appearance Semantics

Status: mostly complete

- Support `appearance: auto` in scene CSS parsing and computed styles.
- Support `appearance: none` in scene CSS parsing and computed styles.
- Prefer compositor titlebars for `appearance: auto` when river decoration support is available and `window::titlebar` contributes style.
- Fall back to river `use_csd` when compositor titlebars are unavailable.
- Make `appearance: none` explicitly mean no titlebar.
- Implement `appearance: none` as a best-effort `use_ssd` request for SSD-capable clients, with CSD-only clients remaining an unavoidable limitation.

### 5. Titlebar Rendering

Status: in progress

- Support `window::titlebar` selector.
- Add compositor-drawn titlebar surfaces through river decoration APIs.
- Start with minimal properties:
  - `height`
  - `background`
  - `background-color`
  - `border-bottom-width`
  - `border-bottom-style`
  - `border-bottom-color`
  - `padding`
  - `color`
  - `opacity`
  - `text-align`
  - `text-transform`
  - `font-family` partial
  - `font-size`
  - `font-weight`
  - `letter-spacing`
  - `box-shadow` partial
  - `border-radius` partial (top corners only)
- `border-color` and `border-bottom-color` drive the rendered titlebar bottom border.
- Keep `appearance: none` as the "no titlebar" contract once compositor titlebars exist.

### 6. Richer Visual Properties

Status: in progress

- `border-color`
- `border-radius` partial
- `opacity` partial for compositor-drawn borders; full window-content opacity remains blocked by current river rendering controls
- `box-shadow` partial, including window-level fallback into compositor titlebar shadows
- `transform` parsed and typed in `spiders-scene`; wm-side translation application landed, scale application still TODO
- `backdrop-filter` (TODO)

### 7. Motion

Status: in progress

- `transition-*` parsed and typed in `spiders-scene`
- `transition` shorthand expands through Stylo and lands in typed longhands
- `animation-*` parsed and typed in `spiders-scene`
- `animation` shorthand expands through Stylo and lands in typed longhands
- `@keyframes` compiled and retained in the scene stylesheet model
- runtime transition and animation scheduling in `spiders-wm` for `opacity`
- property interpolation and application in `spiders-wm` for `opacity` on compositor window borders and titlebars
- shared transform transition and animation sampling in `spiders-scene`
- wm-side translation consumption for window positions and titlebar offsets
- `window:closing` selector state for user-initiated close commands
- workspace transition selectors during workspace activation
- broader transform scale application in `spiders-wm` (TODO)
- broader property interpolation and application in `spiders-wm` (TODO)
- externally initiated `window:closing` remains blocked on protocol lifecycle hooks (TODO)

## Order

Recommended execution order:

1. Keep `docs/css.md` honest and readable.
2. Ship border width consumption from scene styles.
3. Add state selectors.
4. Lock down `appearance` semantics.
5. Implement minimal titlebar rendering.
6. Add richer visual properties.
7. Add transitions, then animations.
