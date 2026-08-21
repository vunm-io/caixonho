## Why

Switching connections leaves the content pane showing the previous
connection's bucket. The sidebar updates correctly — the new connection is
selected, its bucket list is drawn, the status bar counts its buckets — while
the pane and the breadcrumb keep naming a bucket that belongs to an account
the user is no longer looking at. Reproduced in both directions on 2026-08-21
against a build made after the last code commit; the log shows no
`listed a location` after the switch, so this is a stale record rather than a
slow redraw.

The first reading is that `select_profile` forgets a reset that
`leave_bucket` performs. That is the symptom. The cause is a divergence
between the spec and the type:

- `object-browsing` requires the system to hold one location — **a connection,
  a bucket and a prefix** — as the single answer to where the user is.
- `Location` carries a bucket and a prefix. There is no connection in it.

So the connection half of the location is kept beside the location, as the
window's selected profile — and the doc comment sitting directly above
`Location` is the argument against exactly that: *"a second record of where you
are is a second thing that can be wrong."* It is the second record that drifts.
Adding a reset to `select_profile` would silence today's path while leaving
every future caller free to make the same mistake, because nothing in the types
requires position and connection to move together.

Today the application only reads, so the consequence is a misleading label
rather than a misplaced write. That is a property of the current feature set,
not of this code: transfers are the milestone after this one. Two accounts
holding buckets of the same name is ordinary — a `-dev` and a `-prod` profile
of the same project — and this is the screen that would tell the user they are
in the wrong one.

## What Changes

- The window holds the location **together with the connection it belongs to**,
  so a location from a connection that is no longer selected cannot be
  represented as the current position. Switching connections ends the location
  by construction rather than by remembering to clear it.
- `Location` itself is unchanged: it stays the service's addressing form
  (bucket and prefix), which is what the port, the path bar text and the
  diagnostics fields all mean by it. The pairing is added at the layer where
  position is decided, not pushed through core.
- `leave_bucket` and the connection switch end the location through one path
  instead of two that can drift.
- A window test covers the switch: with two connections and a location open,
  selecting the other connection leaves nothing of the first one's position on
  screen. This is newly possible — `XONHO-0015` built the seam that lets a test
  drive a real window over doubles.
- No **BREAKING** change: the port, the on-disk formats and the existing specs'
  other requirements are untouched.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `object-browsing`: the requirement *"The application states where it is"*
  already names the connection as part of the location. It gains the
  consequence that was left implicit — that changing the connection ends the
  location, and that nothing about a previous connection's position survives
  the switch — so the requirement can be tested rather than merely asserted.

  Sequencing note: `object-browsing` has no file in `openspec/specs/` yet — it
  is introduced by `XONHO-0006`, which is still open (19/20, its last task a
  live check). This change's delta lands on a capability whose main spec
  arrives when `XONHO-0006` archives, and it must be synced in that order.

## Impact

- `crates/caixonho-gui/src/app.rs` — the location field and its two writers
  (`go_to`, `leave_bucket`) and the switch that skips them (`select_profile`);
  the readers that derive the trail, the path bar and the contents from it.
- `crates/caixonho-core` — unchanged. The core was asked for the right listing
  all along and answered it; the defect is entirely in what the window kept.
- `docs/requirements-status.md` — the §4.2 breadcrumb row keeps its **done**
  state and gains a note, per the close-out rule that reader-facing documents
  move with the change.
- No dependency, no configuration, no schema, no migration.

## Planning gate

**`[M]` requirements delivered: none.** This change repairs a requirement
already claimed done rather than delivering a new one, and says so plainly
instead of borrowing credit from the row it touches.

**`[M]` requirements still unbuilt ahead of it**, from
`docs/requirements-status.md`:

| Requirement | Section | Standing |
|---|---|---|
| In-app login via the OIDC device flow | §4.1 | `XONHO-0011`, in flight at 12/19 |
| Sort honesty — say when a sort covers only loaded rows | §4.2 | nothing sorts yet |
| KMS denial distinguished from an S3 denial | §4.3 | no object reads yet |

**Why this goes first anyway.** It is not chosen over any of the three. The
first is already in flight and is blocked on a live check only the owner can
run, not on engineering time; the other two are gated on features that do not
exist yet — there is no sort to be honest about, and no object read to
attribute a denial to. Against that, this is a bounded correctness fix in a
path that ships today, and the test seam that makes it regressible landed the
day before in `XONHO-0015`. Deferring it would mean carrying a screen that can
confidently name the wrong account into the milestone that adds writing.
