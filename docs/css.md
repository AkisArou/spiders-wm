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
- `padding`, `padding-top`, `padding-right`, `padding-bottom`, `padding-left`
- `margin`, `margin-top`, `margin-right`, `margin-bottom`, `margin-left`

Current behavior:

- `border-width` on `window` nodes is used by `spiders-wm` for compositor-drawn borders.
- Border colors are still controlled by compositor policy, not CSS.

### Window Presentation

- `appearance`
- `opacity` `(TODO)`
- `border-color` `(TODO)`
- `border-radius` `(TODO)`
- `box-shadow` `(TODO)`
- `backdrop-filter` `(TODO)`
- `transform` `(TODO)`

### Titlebar

Supported now:

- `window::titlebar`
- `background`
- `background-color`
- `height`

Still TODO:

- `color` `(TODO)`
- `padding` `(TODO)`
- `border-bottom-width` `(TODO)`
- `border-bottom-style` `(TODO)`
- `border-bottom-color` `(TODO)`
- `font-family` `(TODO)`
- `font-size` `(TODO)`
- `font-weight` `(TODO)`
- `letter-spacing` `(TODO)`
- `text-transform` `(TODO)`
- `text-align` `(TODO)`
- `box-shadow` `(TODO)`
- `border-radius` `(TODO)`

Planned semantics:

- `appearance: auto` prefers compositor-rendered titlebars only when `window::titlebar` contributes style and river exposes the required decoration support. Otherwise it falls back to client-side decorations.
- `appearance: none` suppresses titlebars and requests a titlebar-free window.
- compositor titlebars currently render a solid background strip above the window using the computed `window::titlebar` `height` and `background` or `background-color`.
- `appearance: none` is currently implemented as a best-effort `use_ssd()` request for SSD-capable clients while omitting compositor titlebar surfaces.
- clients that only support CSD can still show their own client-drawn decorations; river does not provide a stronger override for those windows.

### Motion

- `transition-*` `(TODO)`
- `transition` shorthand `(TODO)`
- `animation-*` `(TODO)`
- `animation` shorthand `(TODO)`
- `@keyframes` `(TODO)`

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
