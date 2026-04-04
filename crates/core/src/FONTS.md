# FONTS

Final font intent and font resolution architecture direction.

This file is the target design, not an intermediate note.

## Problems

1. Native titlebar rendering currently uses ad hoc filesystem font probing in deprecated code.
2. Font selection is currently too close to one renderer/backend instead of being a shared style concern.
3. Web and native do not resolve fonts the same way.
4. Future hosts like `spiders-wm-xorg` should not duplicate native font resolution logic.
5. Font resolution must be performant and must not hit the filesystem for every render query.

## Goals

1. Make font intent a shared style concern, not a titlebar-specific concern.
2. Keep font loading and resolution out of `config.ts` as a separate explicit config subsystem.
3. Keep titlebar planning independent from concrete font loading.
4. Provide a reusable native font resolution crate for all native hosts.
5. Make native font resolution fast through caching or prebuilt font indexes.
6. Let web use browser-native font resolution by default.

## Non-Goals

1. Do not load fonts during CSS parsing.
2. Do not make shared core/style crates own native font file IO.
3. Do not force web through a custom wasm font raster/loading pipeline by default.
4. Do not keep hardcoded distro font path probing as the long-term model.

## Final Mental Model

There are three different concerns.

### 1. Font intent

This is shared and style-level.

Examples:
- `font-family`
- `font-size`
- `font-weight`
- `font-style` later if needed
- `letter-spacing`

This should live in shared typed style values.

### 2. Font resolution

This is backend/host-specific.

Examples:
- Linux system font lookup
- browser font selection by the browser engine
- future embedded or bundled font resolution

This should not live in CSS parsing and should not live in titlebar planning.

### 3. Text rendering

This is renderer-specific.

Examples:
- `ab_glyph` rasterization
- browser text rendering in DOM
- future canvas text rendering

This is separate from resolution.

## Shared Types

### `FontFamilyName`

Replace plain font-family strings with a typed shared value.

Recommended shape:

```rust
pub enum FontFamilyName {
    Named(String),
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUi,
}
```

This should live in shared style/types, not in titlebar.

Reason:
- font family is not titlebar-specific
- other text-rendered features will want the same type

### `FontQuery`

Use one shared semantic query for text rendering consumers.

Recommended shape:

```rust
pub struct FontQuery {
    pub families: Vec<FontFamilyName>,
    pub weight: FontWeightValue,
    pub size_px: i32,
}
```

This should also live in shared style/types from the start.

Reason:
- it is not titlebar-specific
- it should be reusable by titlebars, labels, overlays, and future text systems

## Ownership

### Shared style/types

Owns:
- `FontFamilyName`
- `FontQuery`
- typed `font-family` parsing output

Does not own:
- filesystem loading
- native platform font APIs
- browser font loading orchestration

### `crates/fonts/native`

Owns native font resolution for reusable native hosts.

This crate should be introduced.

Owns:
- native/system font discovery and lookup
- cached resolution from `FontQuery` to resolved font metadata/data
- reusable native resolver implementation for:
  - `apps/spiders-wm`
  - future `apps/spiders-wm-xorg`
  - any other native host

Does not own:
- titlebar planning
- CSS parsing
- DOM/browser rendering
- renderer-specific layout policy

### `crates/titlebar/native`

Consumes resolved native fonts.

Owns:
- titlebar rasterization
- text drawing using resolved fonts

Does not own:
- native font discovery policy
- filesystem scanning strategy

### Web

Web should use browser-native font resolution by default.

That means:
- shared code emits `font-family`, `font-size`, `font-weight`
- browser chooses and loads the actual font

Do not introduce `crates/fonts/web` initially.

That crate is only justified later if we need:
- explicit webfont asset orchestration
- preload/state tracking
- shared `@font-face` helpers

## Native Resolver Design

### Resolver boundary

The native resolver should be a reusable service, not filesystem logic embedded inside a renderer.

Example shape:

```rust
pub trait NativeFontResolver {
    type ResolvedFont;

    fn resolve(&self, query: &FontQuery) -> Option<Self::ResolvedFont>;
}
```

Important:
- the trait boundary belongs on the native side
- shared style/core crates should not depend on it

### What a resolved font should be

Do not make `fonts/native` return a renderer-specific type if possible.

Prefer a crate-owned resolved font result such as:
- font identity/metadata
- raw font bytes or shared buffer
- chosen family/style/weight info

Then `titlebar/native` can adapt that into `ab_glyph` or another raster backend.

## Performance Requirements

This is a hard requirement.

Native font resolution must not hit the filesystem on every font query.

Required direction:
- build a resolver cache
- use a persistent font index / database
- reuse resolved results for repeated `FontQuery` values

### Minimum expectations

At minimum:
- cache by `FontQuery`
- cache loaded font data/bytes
- avoid rescanning system font directories repeatedly

### Better implementation direction

Prefer one of these approaches:
- use a system service/index such as `fontconfig`
- or build one in-memory font database at startup/first use

Either way:
- repeated titlebar renders should hit memory, not the filesystem

### Result caching

The native resolver should likely keep:
- an indexed font catalog
- a query -> resolved font cache
- a resolved font -> parsed font object cache

This should make titlebar rendering effectively constant-time after warmup for common queries.

## Why not load fonts during CSS parsing?

Because CSS parsing should be pure style interpretation.

It should not:
- depend on platform font availability
- do filesystem IO
- allocate renderer-specific font objects
- differ semantically between native and web

Parsing should only produce font intent.

Resolution happens later at the host/backend boundary.

## Relationship to Titlebar

Titlebar should consume shared font intent, not own font loading.

So:
- `titlebar/core` uses `FontQuery`
- `titlebar/native` asks `fonts/native` to resolve it
- `titlebar/web` renders the font query back to CSS

That is the clean split.

## Migration Plan

1. Add this architecture document.
2. Introduce typed `FontFamilyName` in shared style/types.
3. Introduce shared `FontQuery` in shared style/types.
4. Update CSS parsing/output to use typed family values instead of raw strings where needed.
5. Create `crates/fonts/native`.
6. Move native font lookup logic out of deprecated titlebar renderer code.
7. Replace manual filesystem probing with a reusable cached native resolver.
8. Update `titlebar/native` to depend on injected or crate-provided native font resolution.
9. Keep web on browser-native font resolution.

## Decision Summary

Final decisions:
- font intent belongs in shared style/types
- `FontQuery` should live in shared style/types from the start
- font resolution should not happen during CSS parsing
- titlebar should not own native font discovery/loading
- create `crates/fonts/native` for reusable native font resolution
- do not create `crates/fonts/web` initially
- native font resolution must be cached and should avoid repeated filesystem work

## Open Questions

These should be answered during implementation without changing the direction above.

1. Should `fonts/native` use `fontconfig`, `fontdb`, or another backend first?
2. Should resolved fonts expose bytes, parsed faces, or only metadata + lazy loader handles?
3. Do we want `font-style` and fallback chain scoring in the first iteration, or start with family + weight + size?

Current recommendation:
- start with family + weight + size
- build a reusable cached native resolver
- keep browser resolution delegated to the browser
