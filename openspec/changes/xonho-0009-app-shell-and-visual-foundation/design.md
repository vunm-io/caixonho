## Context

See `proposal.md` for motivation and [`docs/design-language.md`](../../../docs/design-language.md)
for what the result must look like. What shapes the approach:

- `gpui-component` already ships `sidebar/`, `status_bar.rs`, `badge.rs`,
  `tag.rs`, `alert.rs`, `skeleton.rs` and `notification.rs`, and its theme
  carries `sidebar_*`, `status_bar_*`, `table_*`, `skeleton`, and `success` /
  `warning` / `danger` / `info` with every variant. Nothing here needs building
  from nothing.
- Its palette is shadcn's, which is Tailwind's, so the reference's violet is
  present by name.
- Themes are documents: `ThemeRegistry::load_themes_from_str` takes one.
- `crates/caixonho-gui/src/main.rs` is 723 lines holding five unrelated jobs.

## Goals / Non-Goals

**Goals:**

- A shell every later screen lands in without redesigning the window.
- Colour and elevation as tokens, so a change is one file and not a sweep.
- The four states designed once, reused everywhere.

**Non-Goals:**

- No behaviour change. Any observable promise that shifts means this change
  overstepped.
- No object browsing, no breadcrumb — `XONHO-0006` brings the content that
  needs them.
- No per-element glass. GPUI has no backdrop blur below the window; the
  reference's material surfaces become a surface colour, a hairline border and
  a tinted shadow.

## Decisions

### The accent arrives as a theme document, not as constants

The app loads its own theme through `ThemeRegistry::load_themes_from_str` and
components ask for `accent`. Naming violet inside a component would put the
palette in a hundred places and make a second theme a search-and-replace.

Alternative considered: a `caixonho-gui::theme` module exposing colour
constants. It reads simpler for one theme and blocks every later one, including
the light/dark pair the toolkit gives away.

### Spacing and shadow are theme tokens too, not a local module

This section originally planned a `tokens.rs` holding the spacing scale and the
shadows, on the assumption that the toolkit's theme carried only colour and
radius. It carries more than that: `SemanticThemeConfig` has `spacing`
(xxs…xxl), `typography`, `radius` and `shadow`, where each shadow is a
`Vec<BoxShadow>` — so a tinted shadow is declared in the theme document rather
than constructed in code, and `Theme::apply_semantic_config` installs the set.

So the spacing scale and the three shadows go in the theme document with the
colours, and components read them through `cx.theme().semantic_tokens()`. A
local module survives only for what the schema genuinely lacks — the icon-tile
sizes — and shadows nothing the theme already provides.

### The status vocabulary is a function of the state, in one place

One function maps `Access` to a glyph, a label, a tint and whether the row dims.
Rendering asks it. The alternative — a match arm per column — is how the first
pass ended up dimming a row while its own cell said "cannot open" in the default
text colour.

Its states follow `docs/design-language.md`: probed rows show a skeleton, an
enterable bucket shows no badge at all, a refused one shows the lock badge in
`danger` with its row muted, and an unobserved one shows an em dash.

### Modules follow the reference's shape, not the toolkit's

`views/` per feature, `components/` for what more than one view uses, `theme.rs`
and `tokens.rs` beside them, `app.rs` for state, `main.rs` reduced to the window
and `main()`. This is the layout the reference arrived at, and the one a reader
can navigate without reading anything.

### The window stays opaque for now

`WindowBackgroundAppearance::Blurred` exists and is one line, but a blurred
window background is a per-platform cost and Windows is a first-class target
here. It goes in behind a screenshot comparison, once the shell is standing, not
as part of the foundation.

## Risks / Trade-offs

- **Nothing about appearance can be asserted by a test** → the tokens are data
  and the vocabulary is one function, so a correction after review is an edit in
  one place rather than a sweep. `docs/design-language.md` is the reviewable
  artefact; the screenshots are the verification.
- **A large diff in one crate, with no test coverage over rendering** → core is
  untouched, so the existing suite still guards everything it guarded before,
  and the module split lands as its own commit before any restyling so the two
  are reviewable apart.
- **The reference is a phone-and-Mac consumer app; this is a file explorer** →
  the language is copied, the density is not: roomy rows for short lists,
  a denser scale for object listings, both stated in the design language.
- **The blur conversion from SwiftUI to GPUI is an approximation** → the shadow
  scale is three named values in one module, tuned once against a screenshot.
