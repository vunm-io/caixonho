# Logo — provisional

Drawn in Claude Code on 2026-09-07, because the application had none: the macOS
bundle carried no `CFBundleIconFile` and the Windows executable no resource, so
both platforms showed their generic placeholder.

**Provisional, and named so on purpose.** It follows Đất Nặn rather than
extending it, and the design system's owner has not passed on it.

## What it is, and what was rejected

A **small clay bucket**: a tapered body in amber, a chunky aqua rim, and a
handle over it — one silhouette, tilted −5°, everything drawn thick enough to
survive being 16 pixels wide.

A bucket because that is what the application is about. S3 calls its containers
buckets, and `caixonho` is a small box; a mark that shows one says what the app
does before any word does.

Two rounds, both settled by rendering at 16, 32, 64 and 256 px rather than by
taste — size is the only honest judge of a mark this small.

**Round one** asked what shape survives. A box with a separate lid became two
smudges at 16 px, which is the finding
`vunm-site/docs/design/logo/README.md` already recorded about three clay balls
— rediscovered here instead of remembered. A box with an inset slot kept one
silhouette but read as a card, and the slot closed below 32 px. A clay **c**
won that round on legibility alone.

**Round two** replaced the c with the bucket, and the question moved to the
handle — the one part thin enough to disappear:

| Variant | Result at 16 px |
|---|---|
| Thick handle, no rim | Survives, but at full size the handle merges into the body and reads as a bag |
| **Thin** handle | Nearly gone, and a thin line breaks a language where everything else is chunky |
| Rim only, no handle | Safest, and reads as a cup rather than a bucket |
| **Thick handle + rim** | Both survive; the silhouette stays a bucket at every size — chosen |

The body was then given a deeper taper and the handle more height, because the
first bucket was square enough to read as a basket.

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
