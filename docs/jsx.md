# JSX Layouts

Layout modules return structural trees. They describe arrangement, not pixels.
Rust validates the tree, resolves window claims, applies layout CSS, and computes
final geometry.

## Elements

Supported JSX elements:

- `workspace`
- `group`
- `window`
- `slot`

## Element Rules

### `workspace`

- must be the root element
- represents the current workspace area
- can contain `group`, `window`, and `slot`

### `group`

- pure structural container
- can contain `group`, `window`, and `slot`

### `window`

- claims at most one matching unclaimed window
- useful for explicit placements

### `slot`

- claims zero or more matching unclaimed windows
- useful for stacks, sidebars, and catch-all regions

## Props

Common props:

- `id?: string`
- `class?: string`
- `children?: LayoutChildren`

`window` props:

- `match?: string`

`slot` props:

- `take?: number`

## Matching

`match` uses exact string clauses joined by spaces.

Format:

```text
key="value" key="value"
```

Supported keys:

- `app_id`
- `title`
- `class`
- `instance`
- `role`
- `shell`
- `window_type`

All clauses are ANDed.

## `take`

`slot` supports:

- omitted: claim remaining matches
- positive integer: claim up to that many matches

## Layout Context

Layout functions receive:

```ts
interface LayoutContext {
  monitor: {
    name: string;
    width: number;
    height: number;
    scale?: number;
  };
  workspace: {
    name: string;
    workspaces?: string[];
    windowCount: number;
  };
  windows: LayoutWindow[];
  state?: Record<string, unknown>;
}
```

## Minimal Example

```tsx
export default function Layout() {
  return <workspace />;
}
```

## Columns Example

```tsx
export default function Layout() {
  return (
    <workspace>
      <group id="main" class="stack">
        <window match='app_id="foot"' />
        <slot />
      </group>
    </workspace>
  );
}
```

## Sidebar Example

```tsx
export default function Layout() {
  return (
    <workspace>
      <group id="content">
        <slot />
      </group>
      <group id="sidebar">
        <window match='app_id="slack"' />
        <window match='app_id="discord"' />
      </group>
    </workspace>
  );
}
```

## Behavior Notes

- unclaimed `window` nodes remain in the resolved tree even if nothing matches
- claim order is document order
- later nodes only see windows that earlier nodes did not claim
- floating windows are compositor-managed and can override tiled placement
