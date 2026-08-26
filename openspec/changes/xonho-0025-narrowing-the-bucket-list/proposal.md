# XONHO-0025 — Narrowing the bucket list

## Why

The owner's work account lists eleven buckets and can open three. The other
eight are rendered in full, each carrying a red **No access** badge, and they
are in the way every single time that account is opened.

`PROJECT_BRIEF.md` §4.2 carries this as **`[M]`**: *Client-side filter of
loaded rows and server-side `prefix` search, with the UI stating which is
happening.* `docs/requirements-status.md` records the row as **partial** —
"Region narrowing exists. No name filter, no prefix search". The region
selector is the shape; this adds the narrowings that were missing.

## What Changes

- **Show only the buckets you can use.** A toggle that leaves the accessible
  ones and takes the refused ones away.
- **Filter by bucket kind** — all, directory buckets, general purpose. Today
  "All directory buckets" is a **badge** the window shows when every row
  happens to be one; it is not a control and nothing can be chosen with it.
- **Filter by name**, the plain client-side match §4.2 asks for.
- **The UI says what the filter covers.** The brief's filter-honesty rule:
  these narrow *loaded rows*, and the account listing is one page, so the
  count says how many of how many are shown.

### The thing this must not get wrong

"Show only accessible" is the goal, and it is reached by **taking the refused
ones away** rather than by keeping only the ones already known to be open.
The two sound identical and one of them is a trap.

The window probes what is on screen. `targets()` reads from the *filtered*
index list (`views/buckets.rs:150`), so a bucket the filter removes is never
reported as visible and therefore **never probed**. Keep only the buckets
observed open, and every bucket whose answer has not arrived is hidden — which
stops it being probed — which means its answer never arrives. A bucket the
user can perfectly well open would disappear on the first paint and never come
back. The filter would be starving the evidence it runs on.

So a bucket stays listed until an answer says otherwise, and the answer that
removes it is an observed authorization denial — not a wrong region, not an
expired session, not an unreachable network, each of which keeps its own cause
(`capability-awareness`: "absence of evidence is not a denial", "only a denial
may be presented as a denial").

The end state is exactly what was asked for: once every row has an answer, what
is left is the buckets you can use. What differs is only the seconds before
that, and in those seconds the list is complete rather than empty.

### What is deliberately absent

- **Server-side prefix search.** The other half of the same `[M]` row, and a
  different mechanism: it is a request, not a narrowing of loaded rows, and
  it belongs to the object listing rather than the bucket list.
- **A remembered selection of buckets** — the IntelliJ schema-picker shape
  the owner asked about. It is `[S]` (*Favorites / pinned buckets and
  prefixes*), it needs somewhere to persist that does not exist yet, and the
  reasoning is recorded in this change's design and in
  `docs/planned-changes.md` so the next proposal starts from it rather than
  rediscovering it.
- Sorting. §4.2's sort-honesty rule is a change of its own.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `bucket-listing`: gains what the user may narrow the account listing by,
  and what a narrowing must not claim — in particular that an unobserved
  bucket is never hidden as though it were denied.

## Impact

- **`caixonho-gui`**: the region selector's mechanism extended rather than
  duplicated — the delegate already filters row indices by a choice
  (`app.rs:1700`), and the new narrowings compose into the same place. The
  "All directory buckets" badge becomes a control.
- **`caixonho-core`**: nothing. Every input already crosses the port — kind
  comes with the listing, the denial comes from the capability store.
- **Dependencies**: none.
- **Docs**: `docs/requirements-status.md` §4.2's filter row;
  `docs/roadmap.md`'s M1 table.
- **`[M]` requirements this steps over**: the same three named in
  `XONHO-0024` — the virtualized-table claim, in-app sign-in, the transfer
  queue. This goes first because it is presentation-only, because it delivers
  half of an `[M]` row that has stood partial since `XONHO-0005`, and because
  the owner hits it on every single open of their work account.
