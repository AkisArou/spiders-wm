# CSS

`spiders-wm` uses a single CSS surface for layout selection, window styling,
and future compositor-rendered visuals. The selected layout currently provides
one stylesheet, typically `index.css`.

Today, CSS is parsed by `spiders-scene`, applied to the resolved
`workspace/group/window` tree, and used to compute layout geometry. `spiders-wm`
also consumes a subset of the resulting style data for compositor behavior such
as window borders.

Unsupported selectors and properties fail clearly during parsing. They are not
silently ignored.

## Pipeline

1. A selected layout provides a JSX tree and a CSS stylesheet.
2. The runtime resolves the current `workspace/group/window` tree.
3. `spiders-scene` matches selectors and computes styles for each node.
4. Layout properties determine geometry.
5. `spiders-wm` applies geometry and compositor-backed rendering details.

## Selectors

### Supported Now

- `workspace`
- `group`
- `window`
- `#id`
- `.class`
- `window[key="value"]` exact-match metadata selectors for `app_id`, `title`, `class`, `instance`, `role`, `shell`, and `window_type`
- `window:focused`
- `window:floating`
- `window:fullscreen`
- `window:urgent`
- runtime window state class aliases: `.focused`, `.floating`, `.fullscreen`, `.urgent`

### Planned

- `window:closing` `(TODO)`
- `workspace:enter-from-left` `(TODO)`
- `workspace:enter-from-right` `(TODO)`
- `workspace:exit-to-left` `(TODO)`
- `workspace:exit-to-right` `(TODO)`

### Notes

- `slot` is not a valid CSS selector target and currently produces a parse error.
- Selector matching is currently structural and metadata-based.
- The state class aliases remain available and match the same runtime state as the pseudo selectors above.

## Properties

### Layout And Sizing

Supported now:

- `display`
- `box-sizing`
- `aspect-ratio`
- `position`
- `inset`, `top`, `right`, `bottom`, `left`
- `overflow`, `overflow-x`, `overflow-y`
- `width`, `height`
- `min-width`, `min-height`
- `max-width`, `max-height`

### Flexbox

Supported now:

- `flex-direction`
- `flex-wrap`
- `flex-grow`
- `flex-shrink`
- `flex-basis`
- `align-items`, `align-self`
- `justify-items`, `justify-self`
- `align-content`, `justify-content`
- `gap`, `row-gap`, `column-gap`

### Grid

Supported now:

- `grid-template-rows`, `grid-template-columns`
- `grid-auto-rows`, `grid-auto-columns`, `grid-auto-flow`
- `grid-template-areas`
- `grid-row`, `grid-column`
- `grid-row-start`, `grid-row-end`, `grid-column-start`, `grid-column-end`
- named grid lines, named spans, and `repeat(...)`

### Box Model

Supported now:

- `border-width`
- `border-top-width`
- `border-right-width`
- `border-bottom-width`
- `border-left-width`
- `border-style`
- `border-top-style`
- `border-right-style`
- `border-bottom-style`
- `border-left-style`
- `padding`, `padding-top`, `padding-right`, `padding-bottom`, `padding-left`
- `margin`, `margin-top`, `margin-right`, `margin-bottom`, `margin-left`

Current behavior:

- `border-width` on `window` nodes is used by `spiders-wm` for compositor-drawn borders.
- `border-style` on `window` nodes suppresses compositor border edges whose style is `none`.
- `border-color` on `window` nodes is used by `spiders-wm` for compositor-drawn borders.
- `opacity` on `window` nodes currently scales compositor-drawn border alpha only. It does not change client content opacity.
- When CSS does not provide a border color, `spiders-wm` falls back to the existing focused and unfocused compositor border palette.

### Window Presentation

- `appearance`
- `opacity` best-effort for compositor-drawn borders only
- `border-radius` partial for titlebar top-corner fallback only
- `box-shadow` `(TODO)`
- `backdrop-filter` `(TODO)`
- `transform` parsed and typed in `spiders-scene`; `spiders-wm` currently consumes translated offsets for compositor window positioning and titlebar decoration offsets, but does not yet apply scale visually

### Titlebar

Supported now:

- `window::titlebar`
- `background`
- `background-color`
- `height`
- `border-bottom-width`
- `border-bottom-style` with `solid` and `none`
- `border-bottom-color`
- `padding`
- `color`
- `opacity`
- `text-align`
- `text-transform`
- `font-family` best-effort from common system family mappings
- `font-size` with `px` and `%`
- `font-weight` with `normal`, `bold`, `400`, and `700`
- `letter-spacing` with `px` values and `normal`
- `box-shadow` best-effort for multiple non-inset shadows
- `border-radius` best-effort for rounded top corners on compositor titlebars

Planned semantics:

- `appearance: auto` prefers compositor-rendered titlebars only when `window::titlebar` contributes style and river exposes the required decoration support. Otherwise it falls back to client-side decorations.
- `appearance: none` suppresses titlebars and requests a titlebar-free window.
- compositor titlebars currently render a solid background strip above the window using the computed `window::titlebar` `height` and `background` or `background-color`.
- when `window::titlebar` sets `border-bottom-width`, the compositor draws a bottom rule only if `border-bottom-style` is not `none`.
- that bottom rule uses `border-bottom-color` if present, otherwise `border-color`, otherwise the titlebar background color.
- compositor titlebars now draw a single-line text label from the window title, falling back to `app_id`, and `padding`, `color`, and `opacity` affect that rendered label.
- `text-align` supports `left`, `right`, `center`, `start`, and `end` for compositor titlebar labels.
- `text-transform` supports `none`, `uppercase`, `lowercase`, and `capitalize` for compositor titlebar labels.
- `font-family` currently resolves a small set of common family names and generics such as `sans-serif`, `serif`, and `monospace`; `options.titlebar_font` still takes precedence when configured.
- `font-size` supports `px` and `%` values for compositor titlebar labels.
- `font-weight` supports `normal`, `bold`, `400`, and `700` via regular and bold font selection when both are available.
- `letter-spacing` supports `normal` and `px` values for compositor titlebar labels.
- `box-shadow` currently renders a first-pass shadow approximation for all declared non-inset shadows.
- titlebar shadows are drawn on a separate layer beneath the clipped titlebar body.
- titlebar shadow geometry reuses the same rounded-top shape model as the compositor titlebar body.
- `border-radius` currently rounds the top corners of compositor titlebars only; it does not clip client window content.
- `window` `border-radius` currently acts as a fallback source for those titlebar top-corner radii when `window::titlebar` does not provide its own value.
- `appearance: none` is currently implemented as a best-effort `use_ssd()` request for SSD-capable clients while omitting compositor titlebar surfaces.
- clients that only support CSD can still show their own client-drawn decorations; river does not provide a stronger override for those windows.

### Motion

Supported now in `spiders-scene` parsing and computed styles:

- `transition-property`
- `transition-duration`
- `transition-timing-function`
- `transition-delay`
- `transition` shorthand via Stylo expansion to the typed longhands above
- `animation-name`
- `animation-duration`
- `animation-timing-function`
- `animation-delay`
- `animation-iteration-count`
- `animation-direction`
- `animation-fill-mode`
- `animation-play-state`
- `animation` shorthand via Stylo expansion to the typed longhands above
- `@keyframes` retained in the compiled scene stylesheet

Current behavior:

- motion declarations are parsed into typed scene values instead of being kept as raw CSS strings
- timing functions are available as structured easing values, including keyword easings, `cubic-bezier(...)`, `steps(...)`, and `linear(...)`
- `spiders-scene` now preserves compiled keyframe blocks for later runtime use
- `spiders-wm` now executes `opacity` transitions and keyframe animations over time for compositor-managed window borders and compositor titlebars
- motion advancement currently uses `manage_dirty()` to request follow-up manage/render sequences while an opacity transition or animation remains active
- `spiders-scene` now also resolves typed `transform` transitions and keyframe animations into sampled translation and scale values
- `spiders-wm` currently applies the translation portion of those sampled transforms to compositor window positions and titlebar offsets
- broader runtime transform support is still incomplete: scale is sampled but not yet visually applied by `spiders-wm`
- workspace transition selectors are still parsed but are not yet driven by dedicated wm-side transition state

## Example

```css
workspace {
  display: grid;
  grid-template-columns: 2fr 1fr;
  gap: 12px;
  padding: 12px;
}

#main {
  min-width: 0;
}

.stack {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

window {
  border-width: 2px;
}
```

See [CSS-PLAN.md](/home/akisarou/projects/spiders-wm/CSS-PLAN.md) for the implementation roadmap.
