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

### What the theme owns, and what the app owns

Read out of `gpui-component` rather than assumed, after one wrong guess in each
direction:

- **The theme owns colour, radius and typography.** A theme document names them
  and the toolkit applies them. Colours may be palette entries by name, so
  `violet-600` is written as itself, and every field is optional — a document
  overrides what it names and inherits the rest.
- **The app owns spacing and shadow.** `Theme::apply_semantic_config` resolves a
  semantic configuration and *returns* the token set; its own documentation
  calls the result a snapshot "for application-owned UI", and
  `apply_semantic_tokens` writes back only colour, radius, typography, and a
  flag for whether shadows exist at all. So the app resolves the configuration
  once at startup and holds the result.
- **Icon-tile sizes are app-only.** The schema has no notion of them.

The shadows are built per element rather than taken whole from a token, because
the rule is that a shadow is tinted with the colour of the thing casting it — a
single global shadow can only be one colour. What the scale fixes is the blur,
the offset and the opacity; the colour comes from the element.

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
