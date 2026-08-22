## Context

The window keeps `location: Option<Location>` and, separately, which
connection is active. `Location` is `{ bucket, prefix }` — the service's own
addressing form. The connection is not in it, so the pair is held by
convention, and `select_profile` breaks the convention: it mints a new
`ConnectionId`, clears the bucket rows and the error, issues the new listing,
and leaves `location` pointing into the connection the user just left.

Both records are already documented as one. `Location`'s type comment says a
second record of where you are is a second thing that can be wrong; the
`location` field repeats it and defines `None` as *"a connection is chosen and
no bucket is — the bucket table"*, which is exactly the state a switch should
land in. Neither comment is wrong. Nothing enforces them.

Constraint: `Location` is what the port's `list_objects` takes, what the path
bar renders through `to_string()`, and what `diagnostics` logs as `bucket` and
`prefix`. It is the addressing form, and a connection is not part of S3
addressing.

## Goals / Non-Goals

**Goals**

- A position belonging to a connection that is not selected cannot be
  displayed, whether or not any particular caller remembers to clear it.
- One path ends a location, used by both the connection switch and
  `leave_bucket`.
- The behaviour is covered by a test that drives a real window, not by review.

**Non-Goals**

- Preserving position across a switch — remembering where you were in each
  connection and restoring it. That is a feature, and a different change.
- Changing `Location`, the port, the log vocabulary, or the path bar's text
  form.
- Anything about writes. The application has no write path; this change does
  not add the first one.

## Decisions

### Pair the connection with the location in the window, and guard on read

The window holds the position as the location **and** the connection it was
read on, and exposes it through one accessor that yields nothing when that
connection is not the active one:

```rust
struct Position { connection: ConnectionId, at: Location }

position: Option<Position>,

fn location(&self) -> Option<&Location> {
    self.position
        .as_ref()
        .filter(|p| p.connection == self.outcome.active())
        .map(|p| &p.at)
}
```

**Amended during implementation (2026-08-22).** This snippet first read
`Some(p.connection) == self.outcome.active()`, which does not typecheck:
`ActiveOutcome::active` returns a `ConnectionId`, not an `Option<ConnectionId>`
(`caixonho-core/src/outcome.rs:66`). Corrected in place rather than only in
`tasks.md`, because a design document that stays wrong is the version someone
reads next. The slip is worth naming: `active_profile`, the field immediately
beside this one in the window, *is* an `Option` — and conflating the two is
the shape of the very defect this change removes.

Every reader that derives the trail, the path text or the contents goes through
`location()`. A stale value is then *harmless* rather than *avoided*: the
guard holds even if a future caller forgets the reset, which is the failure this
change exists to prevent recurring.

**Alternatives considered.**

- *Add `connection` to `Location` in core.* Closest to the spec's wording, and
  rejected on cost: it would push a window concern through the port signature,
  the path bar's rendered text and two diagnostics fields, to fix something no
  core caller can get wrong — core is handed a location and answers it.
- *Only clear `location` in `select_profile`.* One line, fixes the reproduction,
  and leaves the invariant unenforced for the next caller. The bug is that
  position and connection are coupled by convention; a second convention is not
  a fix.
- *Derive position from the connection's own state, one location per
  connection.* This is the non-goal above wearing a disguise: it changes what
  the product does on a switch, and should be proposed on its own merits.

### The switch and `leave_bucket` end a location the same way

`select_profile` calls the same reset `leave_bucket` uses — position cleared,
`listing` back to `Idle`, `more` dropped, `fetching` false, the objects table
emptied — extracted so the two cannot drift. The read guard makes this
belt-and-braces rather than load-bearing, which is deliberate: the guard keeps
the display correct, the reset keeps the state honest.

### Re-selecting the already-selected connection also ends the location

`select_profile` mints a new `ConnectionId` on every call, including when the
user clicks the connection already selected. Under the guard, that ends the
location and returns to the bucket table.

This is accepted rather than worked around. That click re-lists the account —
it is a reconnect, and landing on the fresh account listing is a coherent
answer to it. Written down because it is a behaviour change a reader would
otherwise meet as a surprise, and because the alternative — comparing profile
index instead of connection id — would make the guard depend on a second
notion of sameness, which is the shape of the original defect.

## Risks / Trade-offs

- **A reader reaches `position` directly and bypasses the guard** → the field
  stays private to the window and every existing reader is moved to
  `location()` in this change; the window test asserts the displayed result,
  so a bypass fails the test rather than passing review.
- **The guard hides a genuine bug by silently showing the bucket table** →
  accepted, and bounded: the states it can silently produce are the ones a
  freshly selected connection legitimately shows. It cannot invent a position,
  only decline to show a foreign one.
- **`object-browsing` has no main spec yet** → its spec is introduced by
  `XONHO-0006`, still open at 19/20. This delta must be synced after that one
  archives, not before; the tasks carry that ordering.
- **Test needs the window seam** → `XONHO-0015` landed `test-support` and
  `World` the day before this change. If the seam turns out not to reach the
  sidebar's connection selection, the fallback is a test at the accessor level
  plus a manual check, and the task says so rather than quietly dropping the
  coverage.

## Migration Plan

None. No stored data, no configuration and no format changes; the only visible
difference is that a switch lands on the bucket table instead of a stale
bucket. Rollback is reverting the change.

## Open Questions

- Should the position instead be **remembered per connection** and restored on
  return? It is a better product answer and a larger one, and it is a non-goal
  here. Recording it so the decision is visible: this change makes the switch
  correct, and leaves the switch forgetful. If the owner wants restoration, it
  is its own change on top of this one, not a widening of this one.
