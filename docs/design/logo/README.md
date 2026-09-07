# Logo — provisional

Drawn in Claude Code on 2026-09-07, because the application had none: the macOS
bundle carried no `CFBundleIconFile` and the Windows executable no resource, so
both platforms showed their generic placeholder.

**Provisional, and named so on purpose.** It follows Đất Nặn rather than
extending it, and the design system's owner has not passed on it.

## What it is, and what was rejected

A **small clay bucket**, seen slightly from above: an amber body tapering to a
rounded base, a rolled rim, an aqua opening, and a handle arcing over it.
Tilted −4°, and every part drawn thick enough to survive being 16 pixels wide.

A bucket because that is what the application is about. S3 calls its containers
buckets and `caixonho` is a small box; a mark that shows one says what the app
does before any word does.

Three rounds, each settled by rendering at 16, 32, 64 and 256 px rather than by
taste — size is the only honest judge of a mark this small.

**Round one — what shape survives.** A box with a separate lid became two
smudges at 16 px, which is the finding
`vunm-site/docs/design/logo/README.md` already recorded about three clay balls,
rediscovered here instead of remembered. A box with an inset slot kept one
silhouette but read as a card. A clay **c** won on legibility alone.

**Round two — the bucket, and its handle.** The owner asked for a bucket, and
the handle is the one part thin enough to vanish. A thick handle alone merged
into the body and read as a bag; a thin one nearly disappeared and broke a
language where everything else is chunky; a rim with no handle was safest and
read as a cup. Thick handle plus chunky rim kept both at every size.

**Round three — what the reference images showed.** The owner supplied two: a
flat bucket on a circular plate, and an outlined one. Set beside them, round
two's mark was missing the thing that makes a bucket a bucket — **the
opening**. It had a flat rim band, so it read as a box with a lid. Both
references also carried a taller handle and a deeper taper.

So the opening was added as an aqua ellipse inside a rolled rim, the handle
raised, and the base narrowed. The references' outlines were **not** taken:
Đất Nặn is clay — soft edges and gradient depth — and a hard outline would be a
different system wearing this one's colours.

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
