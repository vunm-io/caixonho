# XONHO-0027 — A bucket list you choose once

## Why

The owner asked for this directly, and drew the picture themselves: *"kiểu
khi connect db trong IntelliJ nó cho chọn schema để hiển thị"* — a data
source in IntelliJ asks once which schemas you work with, and thereafter
shows those.

`XONHO-0025` delivered the *narrowings* — kind, name, and whether a bucket
can be used — and deliberately persisted none of them, because a kind filter
and a name filter are things you change several times a minute. This is the
other half of the same request and it is a different kind of thing: a choice
made once, about a particular account, that should still be true tomorrow.

`PROJECT_BRIEF.md` §4.2 has it as **`[S]`**: *Favorites / pinned buckets and
prefixes; recent locations.*

The concrete case is the owner's work account: eleven buckets, three they can
open, and of those three usually one that matters. Narrowing gets them to
three every session. Choosing gets them to one, once.

## What Changes

- **Which buckets to show is a choice, remembered per connection.** Pick them
  from the account's own listing; the choice survives a restart.
- **A connection with no choice recorded shows everything**, which is what it
  does today. Nothing becomes hidden because a feature was added.
- **The choice is visible and reversible from the list itself** — a listing
  that is showing a chosen subset says so and offers to show everything,
  because a remembered filter nobody remembers setting is a bucket that has
  gone missing.
- **A bucket that has since disappeared from the account is not an error.**
  A choice is a wish about names, and the account is the authority on which
  of them exist.

### Where it is kept, and why not where it was first proposed

The owner asked whether this belongs in the connection's configuration, as
IntelliJ keeps a schema selection with the data source. It cannot, and the
reason is a fact rather than a preference: **half the connections have no
configuration to put it in.** A connection is either a profile *discovered*
in `~/.aws` or a credential *stored* by this application, and the list is the
two chained together (`app.rs:1429`). The profiles this owner uses daily are
all of the first kind — nothing is written about them anywhere.

So it goes in a **view-preferences store of its own**, keyed by connection
name, beside the connections file rather than inside it. That covers
discovered profiles and stored credentials alike, and it means a cosmetic
choice can never rewrite a file that holds credential configuration.

This was worked out and written down while `XONHO-0025` was being planned,
before there was a change to put it in.

### What is deliberately absent

- Pinned *prefixes* and recent locations, the rest of the `[S]` line. A
  location is not a bucket: it belongs to a bucket, it can cease to exist
  between sessions, and it needs its own thinking.
- Ordering, grouping or renaming the chosen buckets.
- Syncing the choice between machines. There is nowhere to sync it to, and
  inventing one for a display preference would be absurd.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `bucket-listing`: gains that which buckets are listed may be chosen and
  remembered per connection, what an unrecorded connection does, and what
  happens to a chosen bucket the account no longer has.

## Impact

- **`caixonho-core`**: a view-preferences port and its file implementation,
  in the shape `ConnectionFile` already establishes — a trait so a test can
  put a double where the platform's config directory goes.
- **`caixonho-gui`**: a way to choose, and the chosen set composing with
  `XONHO-0025`'s `Narrowing` as one more predicate rather than as a second
  mechanism beside it.
- **Dependencies**: none — the same serialisation the connections file uses.
- **Docs**: `docs/requirements-status.md` §4.2's filter row and the `[S]`
  favourites line; `docs/roadmap.md`; the parked section in
  `docs/planned-changes.md` gets its outcome.
- **`[M]` requirements this steps over**, and this one deserves saying
  plainly: this is **`[S]`, and `[M]` work is unbuilt** — server-side prefix
  search, sort honesty, the transfer queue, in-app sign-in. It is proposed
  because the owner asked for it twice and hits it on every open of their
  work account, not because it outranks them. **If it is deferred, that is
  the right call and this paragraph is why.**
