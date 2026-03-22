# Supported CSS

`spiders-wm` uses two separate CSS domains:

- layout CSS controls structure and geometry
- effects CSS controls visual presentation and transitions

They are intentionally separate.

## Layout CSS

Layout CSS is applied to resolved layout nodes after JSX layout evaluation.

### Supported Selectors

- `workspace`
- `group`
- `window`
- `#id`
- `.class`
- `window[key="value"]` exact-match attributes on runtime window metadata

Selector matching is intentionally small and structural.

### Supported Properties

Current layout CSS support includes:

- `display`
- `box-sizing`
- `aspect-ratio`
- `flex-direction`
- `flex-wrap`
- `flex-grow`
- `flex-shrink`
- `flex-basis`
- `position`
- `inset`, `top`, `right`, `bottom`, `left`
- `overflow`, `overflow-x`, `overflow-y`
- `width`, `height`
- `min-width`, `min-height`
- `max-width`, `max-height`
- `gap`, `row-gap`, `column-gap`
- `align-items`, `align-self`, `justify-items`, `justify-self`
- `align-content`, `justify-content`
- `grid-template-rows`, `grid-template-columns`
- `grid-auto-rows`, `grid-auto-columns`, `grid-auto-flow`
- `grid-template-areas`
- `grid-row`, `grid-column`
- `grid-row-start`, `grid-row-end`, `grid-column-start`, `grid-column-end`
- named grid lines, named spans, and `repeat(...)`
- `border-width`, `border-top-width`, `border-right-width`, `border-bottom-width`, `border-left-width`
- `padding`, `padding-top`, `padding-right`, `padding-bottom`, `padding-left`
- `margin`, `margin-top`, `margin-right`, `margin-bottom`, `margin-left`

Unsupported properties should fail clearly instead of being ignored.

### Example

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
```

## Effects CSS

Effects CSS applies to real windows, compositor-drawn titlebars, and workspace
transition snapshots.

### Supported Selectors

- `window`
- `window[app_id="..."]`
- `window[title="..."]`
- `window::titlebar`
- `window:focused::titlebar`
- `workspace`

### Supported Pseudo States

- `:focused`
- `:floating`
- `:fullscreen`
- `:urgent`
- `:closing`
- `workspace:enter-from-left`
- `workspace:enter-from-right`
- `workspace:exit-to-left`
- `workspace:exit-to-right`

### Supported Static Properties

- `appearance`
- `opacity`
- `border-width`
- `border-color`
- `border-radius`
- `box-shadow`
- `backdrop-filter: blur(...)`
- constrained `transform`

### Titlebar Properties

`window::titlebar` supports:

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

`appearance: none` disables compositor-drawn decorations for matching windows.
It does not remove client-drawn chrome.

### Transform Support

Effects transforms are intentionally small:

- `translate(x, y)`
- `scale(s)`

### Animations And Transitions

Supported animation features:

- `@keyframes`
- `animation-*`
- `animation` shorthand
- `transition-*`
- `transition` shorthand

Interpolated properties currently include:

- `opacity`
- `border-radius`
- `box-shadow` blur and color
- `transform` translate and scale components

### Workspace Transitions

Directional workspace transitions are derived from workspace order.

- forward switches use `workspace:enter-from-right` and `workspace:exit-to-left`
- backward switches use `workspace:enter-from-left` and `workspace:exit-to-right`
- single-workspace switches are the main supported case

### Example

```css
window {
  opacity: 0.92;
  border-width: 2px;
  border-color: rgba(30, 36, 50, 0.35);
  border-radius: 14px;
}

window:focused {
  opacity: 1;
  border-color: rgba(196, 122, 48, 0.9);
}

window::titlebar {
  height: 30px;
  padding: 8px 12px;
  background: rgba(16, 20, 28, 0.92);
  color: #f3ead8;
}

workspace:enter-from-right,
workspace:exit-to-left {
  transition: opacity 180ms;
}
```
