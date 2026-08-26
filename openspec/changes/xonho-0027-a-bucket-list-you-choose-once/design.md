# Design — a bucket list you choose once

## Context

`XONHO-0025` gave the account listing four narrowings and persisted none of
them, deliberately: a kind filter and a name filter are changed several times
a minute, and a filter set on one account and silently applied to another is
how someone comes to believe a bucket has gone missing.

This is the opposite kind of thing — a choice made once, about one account —
and it therefore has to answer the question `XONHO-0025` was allowed to duck:
**where does it live, and how does the user know it is in force?**

## Goals / Non-Goals

**Goals**

- A per-connection choice that survives a restart.
- A store that works for discovered profiles and stored credentials alike.
- A listing that says when it is reduced by a choice made in another session.

**Non-Goals**

- Pinned prefixes, recent locations, ordering, syncing. See the proposal.
- Persisting `XONHO-0025`'s narrowings. Still view state, still reset on a
  connection change.

## Decisions

### Its own store, keyed by connection name

Not in the connections file. Two reasons, and the first is decisive:

1. **Half the connections are not in that file.** `connections()` chains
   profiles *discovered* in `~/.aws` with credentials *stored* by this
   application (`app.rs:1429`). A profile has no record anywhere, so there is
   nothing to add a field to.
2. That file holds the non-secret half of a **credential**. A display choice
   that rewrites it gives a cosmetic act the blast radius of a credential
   edit, and `lib.rs`'s invariant about where secrets may go deserves a file
   nothing cosmetic touches.

So: a view-preferences file beside the connections file, keyed by connection
name, in the shape `ConnectionFile` already establishes — a trait, so a test
puts a double where the platform's config directory goes and no test ever
writes into the developer's own configuration.

The key is the connection **name**, which is what the user chose in the list
and what both kinds of connection have. It is not unique across the two kinds
in principle; in practice a profile and a stored credential of the same name
are the same account under two doors, and one choice for both is the
behaviour a person would predict.

### A choice is a set of names, and the account is the authority

Stored as names, not as anything richer. A name is what the user picked and
what survives a bucket being deleted and recreated.

Which means a recorded name that no longer exists is **not an error and not
something to clean up**. Listing passes over it. The alternative — pruning
the choice to what the account currently lists — quietly loses a bucket that
was absent for one session because of a region hiccup or a denied listing,
and the user never asked for it to be forgotten.

### It composes into `Narrowing`, as one more predicate

`XONHO-0025` put four predicates in one pass so the count is of the final
set. This is the fifth. Anything else — a second filtering stage, a separate
"chosen rows" list — reintroduces exactly the disagreement between the count
and the rows that the one-pass design exists to prevent.

It differs from the other four in **when it is cleared**: they reset on a
connection change, and this one is *loaded* on a connection change.

### Saying it is in force is the requirement, not the polish

A narrowing you set thirty seconds ago needs no announcement. A choice made
three weeks ago is indistinguishable from a bug: the account has eleven
buckets and the screen shows one, and nothing on it explains why.

So the listing says it is a chosen subset and how many the account holds, and
offers "show all" — which does **not** discard the choice, because a user
checking whether a bucket still exists has not decided to stop using their
choice.

This is the same rule as `XONHO-0025`'s hidden-by-narrowing empty state, one
step further out: an account that holds nothing, an account narrowed to
nothing, and an account reduced by a remembered choice must read as three
different things.

## Risks / Trade-offs

- **[A choice becomes a bucket that has gone missing]** → the whole of the
  "says it is in force" requirement exists for this. It is the single most
  likely way this feature does harm.
- **[Another config file to write, migrate and corrupt]** → real, and the
  mitigation is that it holds nothing valuable: an unreadable or malformed
  preferences file means every bucket is shown, which is the behaviour before
  this change. It must never fail a listing.
- **[Keying by name conflates a profile and a stored credential of the same
  name]** → named above and accepted; the alternative is a compound key the
  user cannot see and would not predict.
- **[`[S]` work ahead of `[M]` work]** → named in the proposal rather than
  hidden here. It is the owner's call, and the proposal says so.

## Open Questions

None blocking. The one that mattered — where a per-connection preference can
live at all — was answered while planning `XONHO-0025`, from what
`connections()` actually does rather than from the IntelliJ analogy that
prompted it.
