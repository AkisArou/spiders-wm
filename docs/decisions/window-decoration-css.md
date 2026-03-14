# Window Decoration CSS Decision

## Status

Accepted.

## Decision

`spiders-wm` does not keep a config-level `options.decorations` switch.

Window decoration visibility is controlled through effects CSS on the `window`
selector domain.

The accepted property for compositor-drawn titlebars and related server-side
window furniture is:

- `appearance`

The accepted value for disabling compositor decorations is:

- `appearance: none`

Example:

```css
window {
  appearance: none;
}
```

When decorations are compositor-drawn and visible, titlebar styling uses a
dedicated pseudo-element rather than a descendant selector:

```css
window::titlebar {
  height: 24px;
  background: #222222;
  color: #dddddd;
}
```

`window .titlebar` is intentionally not the model. The titlebar is compositor
chrome, not a DOM child node.

## Why

- titlebar visibility is presentation policy, so it belongs in effects CSS rather
  than top-level config
- `appearance` is a better semantic fit than layout properties like `display`
- `display: none` would imply removing a layout node from the structural tree,
  which is not what window decoration toggling means
- a dedicated custom property such as `--decoration` would be clear, but it adds
  project-specific syntax where a reasonable CSS-facing property already exists

## Interpretation Rule

In `spiders-wm`, `appearance` on `window` does not mean browser-native widget
styling.

Instead it maps to compositor decoration policy for server-managed window
furniture:

- `appearance: none` means do not draw the compositor titlebar/frame for the
  matched window
- omitted `appearance` leaves decoration policy at the compositor default
- `window::titlebar` styles the compositor-owned titlebar when that titlebar is
  present

This rule applies to compositor-managed server-side decorations only.

It does not guarantee removal of client-side decorations or custom application
chrome drawn inside the client buffer. On Wayland, clients may still choose to
draw their own headerbars or framelike UI when the compositor does not provide
server-side decorations.

V1 does not need to promise a full browser-compatible `appearance` value space.
It only needs the documented behavior for decoration toggling.

## Consequences

- remove `options.decorations` from the authored config contract
- document decoration behavior in effects CSS instead of config runtime docs
- keep structural layout CSS and effects CSS separate: decoration toggling is an
  effects concern, not a `taffy` layout concern
- be explicit that `appearance: none` is not a universal CSD suppression
  guarantee

## Non-Decision

- this does not yet define the full interaction with xdg-decoration negotiation
- this does not yet define KDE server-decoration handling; smithay and protocol
  support exist, but the protocol explicitly says concurrent use with
  `zxdg_decoration_manager_v1` is undefined, so V1 should not casually expose
  both without an explicit interoperability decision
- this does not yet define whether additional values like `auto` should map to
  explicit compositor policies in CSS later
