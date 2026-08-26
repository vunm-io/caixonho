# Design — narrowing the bucket list

## Context

The window already narrows: a region `Select` whose choice filters row
indices in the table delegate (`app.rs:1700`). Everything here is a second,
third and fourth choice going through that same place — this is an extension
of a mechanism, not a new one.

Everything needed is already on screen. The bucket's kind comes with the
listing (`XONHO-0016`). The denial comes from the capability store, which is
what draws the **No access** badge today. Core is untouched.

## Goals / Non-Goals

**Goals**

- Kind, name and no-access narrowings that compose with region.
- A count that says what is hidden.
- A no-access narrowing that cannot hide a bucket the user could open.

**Non-Goals**

- Server-side prefix search — a request, not a narrowing.
- Persisting anything. See below; this is the owner's actual question and it
  gets a real answer rather than a deferral.

## Decisions

### Show accessible buckets by removing the refused, not by keeping the known-open

The control is *show only what I can access*, and it is written as
`access != Denied` rather than `access == Open`. Those two look like the same
sentence and one of them cannot work here.

Access is not a fact the listing carries; it is an observation, and it has
four states — `Open`, `Denied`, `Probing`, `Unobserved`
(`views/buckets.rs:189`). Observations arrive because the window reports which
rows are on screen and the scheduler probes them. **That report is built from
the filtered list**: `targets()` maps over `self.shown`, the index list the
filter produced (`views/buckets.rs:150`).

So `access == Open` closes a loop on itself. A bucket whose answer has not
arrived is not `Open`, so it is filtered out, so it is not in `shown`, so it
is never reported on screen, so it is never probed, so its answer never
arrives. A bucket the user can open would vanish on the first paint and never
return — and nothing on screen would say why. The filter would be starving the
evidence it runs on.

`access != Denied` has no such loop: an unanswered bucket stays listed, stays
probed, and leaves only when an answer says it should. Once every row has an
answer the two predicates agree exactly, which is why this is the same feature
and not a lesser one.

Which answers count is the second half. Only an observed **authorization
denial** removes a bucket. An expired session, a wrong region, an unreachable
network and a trust failure each keep their own cause and are not denials
(`capability-awareness`), so none of them may quietly delete a row.

### What the user sees while it settles

The list starts complete and shrinks as denials land, rather than starting
empty and filling. Rows can therefore leave under the cursor — which is why
the count ("showing N of M") is part of this change and not decoration: it is
the only thing that says something moved.

The alternative — freeze the set when the toggle is flipped — trades a moving
list for a stale one, and on an account still being probed the stale one would
be wrong for longer.

### Where the configuration lives — and the answer is "nowhere, yet"

The owner asked whether this belongs in the connection's configuration, as
IntelliJ keeps a data source's schema selection. Three things decide it, and
the first is a fact rather than a preference.

1. **Half the connections have no configuration to put it in.** A connection
   is either a profile *discovered* in `~/.aws` or a credential *stored* by
   this application, and `connections()` builds the list by chaining the two
   (`app.rs:1429`). The owner's `vietcap`, `vunm` and `r2-caixonho` are all
   profiles. There is no record of them for a setting to hang on, so
   "in the connection config" is not available for exactly the connections
   that prompted the question.
2. **View preferences do not belong in a credential record even where one
   exists.** That file is the non-secret half of a stored credential. Making
   a UI toggle rewrite it gives a cosmetic act the blast radius of a
   credential edit.
3. **These particular controls are not a saved selection anyway.** A kind
   filter and a name filter are things you change several times a minute. The
   IntelliJ analogy holds for *which buckets I work with* — a durable,
   per-connection choice — and that is a different feature.

So: **this change persists nothing.** The narrowings live in the window and
reset when the connection changes, which is also what keeps them honest —
a filter you set on one account silently applying to another is how a user
comes to believe a bucket is missing.

**And the durable one gets its own home when it is built**: a per-connection
*view preferences* store keyed by connection name, separate from the
connection file, so it covers discovered profiles and stored credentials
alike and so a preference can never corrupt a credential. Recorded here and
in `docs/planned-changes.md` so the proposal that builds it starts from this
rather than rediscovering point 1 late.

### Composition, and where it happens

One predicate per narrowing, combined with `and`, applied where the region
choice is applied today. Not four separate filtered lists: the count has to
be of the final set, and four passes with four counts is how the number comes
to disagree with the rows.

## Risks / Trade-offs

- **[A narrowing hides a bucket the user is looking for]** → the count is the
  mitigation, and the empty state names the narrowing rather than the
  account. An account that holds nothing and an account whose buckets are all
  hidden must not read the same.
- **[Rows leave as probes settle]** → named above; the count is what says so.
- **[The `!= Denied` predicate is one keystroke from the broken one]** → and
  the broken one fails *silently and permanently*, which is the worst
  combination available. It gets an ablation in the tasks rather than a
  comment, so the guard is a red test and not a hope.
- **[Nothing is remembered, so the toggles are set again each session]** →
  deliberate, and cheap. The durable version is a change of its own with a
  home already chosen for it.

## Open Questions

None. The one question the owner raised — where the configuration lives —
is answered above, and answered from what the code actually does rather than
from the IntelliJ analogy that prompted it.
