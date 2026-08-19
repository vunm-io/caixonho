# Design language

The visual direction is **[Rclone GUI for macOS](https://github.com/VitalysRDT/rclone-gui-ios)**:
a sidebar of connections, roomy content rows, semantic status told with an icon
and a coloured label rather than bare text, soft cards, and one accent colour
used sparingly. Owner's decision, taken 2026-08-19 after the M1 screens were
judged — accurately — to look unfinished.

This file is the reference the UI is held to. It exists because the first pass
had no reference at all: sizes were written inline where each element happened
to be added, and the result reads as a table bolted to a window.

## What the toolkit already provides

Most of what the reference implements by hand is already in `gpui-component`,
which this project depends on. The gap was never the toolkit:

| Reference (hand-written in Swift) | Already available here |
|---|---|
| `AppStatusBadge` | `badge.rs`, `tag.rs` |
| `SkeletonLoaderView` | `skeleton.rs`, plus a `skeleton` theme colour |
| Sidebar with sections and rows | `sidebar/` — header, footer, group, menu, items with icon, active state, collapsible children and a suffix slot |
| Status bar | `status_bar.rs` |
| `AppInlineMessage` | `alert.rs` |
| `AppToastBanner` | `notification.rs` |
| Semantic colours by hand | theme tokens `success` / `warning` / `danger` / `info`, each with foreground, hover and active |
| Sidebar and table colours by hand | `sidebar_*`, `table_head`, `table_hover`, `table_row_border`, `title_bar_*`, `status_bar_*` |

So the work is composition against the theme, not a design system built from
nothing. **Never hard-code a colour.** Every colour comes from `cx.theme()`;
anything the theme lacks is added to this document first and to the app's own
token module second.

## Tokens

Sizes come from the reference, which uses a 2pt-based scale. The toolkit's
theme carries a semantic token set — `spacing` (xxs…xxl), `typography`,
`radius` and `shadow`, where a shadow is a list of `BoxShadow` — so the scale
below is declared in the theme document rather than kept in code. Only what the
schema lacks, the icon-tile sizes, lives in the app.

**Spacing** — `4, 6, 8, 10, 12, 14, 16, 18, 24`. Inside a component: 8 between
an icon and its text, 12 in a row, 2–4 between a title and its subtitle. Around
a component: 14 for cards and inline messages, 24 for an empty state, 16 at the
window margin.

**Radius** — `theme.radius` for controls and chips, `theme.radius_lg` for cards
and dialogs. Where the reference is more specific: 11 for a metric pill, 14 for
a card or tile, 18 for an empty state or toast, 22 for a hero surface, fully
rounded for a status capsule.

**Icon tiles** — a rounded square holding a glyph, background tinted at 14% of
its own colour, or filled when it is the subject. Sizes 34 / 40 / 42 / 58, and
the radius is `min(14, size / 3)`. This is the reference's most recognisable
element and the cheapest way to make a list read as designed rather than dumped.

**Type** — roles, not sizes: a row title at body weight medium, its subtitle at
caption in the muted colour, a section header at 13pt uppercase with 0.5pt
tracking, numbers in the monospaced face so columns line up.

## Colour

The reference's accent is Tailwind's violet ramp, and `gpui-component` already
carries that ramp in its palette — so the colours are copied exactly rather than
matched by eye.

| Role | Value | Palette entry |
|---|---|---|
| Brand, light theme | `#7C3AED` | `violet-600` |
| Brand, dark theme | `#A78BFA` | `violet-400` |
| Brand deep — pressed states, gradient terminus | `#5B21B6` | `violet-800` |
| Brand soft — badge and pill backgrounds | the brand at 12–18% opacity | derived |

**Mind the toolkit's naming.** In `gpui-component`, following shadcn, `accent`
is the *subtle hover surface* — a menu row under the pointer, a pressed toggle —
and stays neutral. The brand colour is `primary`. Putting violet in `accent`
would tint every hover in the app.

The reference's light accent is `rgb(0.486, 0.227, 0.929)` and its deep accent
is `rgb(91, 33, 182)`; those are `violet-600` and `violet-800` to the digit.

The brand colour is loaded as a theme, not written into components:
`ThemeRegistry::load_themes_from_str` takes a theme document, and the app ships
one. A component asks the theme for `primary`, never for violet — which is what
keeps a second theme, or a light/dark pair, from becoming a search-and-replace.

**One brand colour, used sparingly.** In the reference it marks the active
sidebar row, the primary action, and the encrypted badge — nothing else.
Semantic meaning stays with the tokens the theme already has: `danger` for a
refused bucket, `warning`, `success`, `info`. A brand colour that appears on
everything stops meaning anything.

## Shadow

This is the detail that separates the reference from a flat mockup, and it is
copyable: GPUI's `BoxShadow` takes a colour, an offset, a blur radius, a spread
and an inset flag, so a tinted shadow is a first-class thing rather than
something to fake.

**Shadows are tinted with the element's own colour, never black.** That single
rule is most of the effect. A violet tile casts a violet shadow at low opacity;
a neutral card casts a neutral one.

| Use | Reference | Starting values here |
|---|---|---|
| Chip, small tile | own colour 18%, y 2 | blur 8, y 2, spread 0 |
| Raised control, primary tile | brand 25%, y 3 | blur 12, y 3, spread 0 |
| Hero surface | own colour 35%, y 14 | blur 36, y 14, spread 0 |

The blur figures are doubled from the reference's, because SwiftUI's shadow
radius and a CSS-style blur radius are not the same quantity — SwiftUI's is
roughly half. Treat them as a starting point to be trimmed by eye against a
screenshot, not as a conversion that is settled.

Two shadows already exist in the toolkit and should be reused rather than
reinvented: `popover_shadow` and `toast_shadow` in `gpui-component`.

## The shell

```
┌───────────────┬──────────────────────────────────────────┐
│  caixonho     │  breadcrumb / path                       │
│               ├──────────────────────────────────────────┤
│ CONNECTIONS   │                                          │
│  ● vunm       │  content — bucket list, later objects    │
│    work       │                                          │
│               │                                          │
│               ├──────────────────────────────────────────┤
│  + Add        │  status bar — counts, in-flight work      │
└───────────────┴──────────────────────────────────────────┘
```

Profiles move out of the title bar into a sidebar group, where the reference
puts its remotes: each row is an icon tile plus the profile name, with the
active one marked. The sidebar is where every future connection-shaped thing
belongs, so adding one does not mean redesigning the window.

The status bar carries what the title bar is currently misusing: how many
buckets are shown of how many, and whether anything is in flight.

## Status is a vocabulary, not a sentence

The reference's file list is the pattern to follow: a leading icon tile, name
and subtitle, and a trailing status of *icon + short label + colour* —
`✓ Downloaded`, `↓ 65 %`, `☁ On server`, `⟳ Syncing…`. Never a bare word in the
default text colour, which is what the first pass shipped.

For what a bucket's access is, which is where this project's headline feature
becomes visible:

| State | Rendering |
|---|---|
| Being probed | skeleton or a muted spinner in the badge's place — never text that says "checking" |
| Can be entered | no badge. The absence of a warning is the good news; a green tick on every row is noise |
| Cannot be entered | a lock glyph and **No access** in a capsule tinted from `danger`, the row's text muted, the reason and the IAM action on hover |
| Nothing observed yet | an em dash in the muted colour, and nothing else — the app does not guess |

`PROJECT_BRIEF.md` §4.3 asked for "dimmed + lock badge" from the start. The
first pass delivered the dimming and the words "cannot open", and no badge.

## Every screen has four states

Designed, not improvised: **loading** (skeleton rows in the shape of the content
that is coming, not a line of text), **empty** (icon tile, a headline, one
sentence saying why it is empty and not what to do about it when there is
nothing to do), **error** (an inline message with the cause and a single action,
sized to its content — never a button stretched across the window), and
**loaded**.

## Where the reference is not followed

One collision, and it is functional rather than aesthetic. The reference's rows
are roomy: a 40pt icon tile in a row of about 48pt. This project must scroll
100k objects at 60fps, and row height is the multiplier on everything.

So: the roomy treatment for lists that are short by nature — the sidebar, the
bucket list, transfers. A denser row for object listings, keeping the same
vocabulary at a smaller scale: a 20pt glyph in a row of about 32pt, the same
badges, the same colours. The language stays; the scale adapts to the count.
