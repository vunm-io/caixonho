# Logo — provisional

Drawn in Claude Code on 2026-09-07, because the application had none: the macOS
bundle carried no `CFBundleIconFile` and the Windows executable no resource, so
both platforms showed their generic placeholder.

**Provisional, and named so on purpose.** It follows Đất Nặn rather than
extending it, and the design system's owner has not passed on it.

## What it is, and what was rejected

A lowercase **c** in clay: one stroke, round caps, tilted −5°, amber holding
most of the arc and turning aqua at the lower terminal.

Three directions were drawn and measured at 16, 32, 64 and 256 px before
choosing, because size is the only honest judge of a mark this small:

| Direction | Why not |
|---|---|
| A box with a separate lid | Two pieces. At 16 px they stop being a box and become two smudges — which is the finding `vunm-site/docs/design/logo/README.md` already recorded about three clay balls, repeated here before it was remembered |
| A box with an inset slot | One piece, but the silhouette is a rounded rectangle, which reads as a card or a button; the slot closes up below 32 px |
| A clay **c** | Legible at 16 px, one connected shape, and the same reasoning that chose a **V** over clay balls for the personal mark |

A single-colour version was drawn first. The two-tone stroke was kept because it
gives the mark identity without breaking it into pieces — the two-lumps-of-clay
idea of the `VuNM` mark, expressed inside one shape rather than as two. The
first gradient passed through a dull olive where amber met aqua; the stops now
hold amber to 64% and turn over between 90% and 100%, which removes it.

## Colours

Both are the application's own, from `crates/caixonho-gui/assets/theme.json` —
nothing invented for the logo:

| Token | Value | In the mark |
|---|---|---|
| `primary.background` | `#F5A81C` | the arc |
| `ring` / aqua accent | `#3AAFC9` | the lower terminal |
| `background` | `#F2F4F0` | the avatar's plate |

The clay overlay is the same two-stop structure the `VuNM` mark uses: a warm
highlight from the top left, a cool shadow into the bottom right.

## The files

| File | Use |
|---|---|
| `caixonho-mark.svg` | transparent, standard proportions — documents, a nav bar |
| `caixonho-avatar.svg` | 512 square on a paper plate — GitHub, and the source every app icon is rendered from |
| `caixonho-favicon.svg` | the mark scaled into the box, no plate — 16–32 px |

## Not done yet

Nothing consumes these. The macOS bundle does not name an icon and the Windows
executable carries no `VERSIONINFO` resource, so wiring them in is its own
change — and worth doing after someone who owns the design system has looked at
this.
