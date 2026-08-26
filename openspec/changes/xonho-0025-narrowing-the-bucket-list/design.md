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

### The no-access narrowing hides observed denials, and nothing else

`capability-awareness` says a scope nobody has observed reads *unknown* and
"never as denied: absence of evidence is not a denial", and separately that
only an authorization denial may be presented as one — an expired session, a
wrong region or an unreachable network each keep their own cause.

So the predicate is *observed denied*, not *not observed allowed*. Written
that way round on purpose: the second is one keystroke away and would hide
every bucket on a slow account until its probes landed, which is the moment
the user most wants to see the list.

The visible consequence is that turning the narrowing on may hide nothing at
first and more later, as probes settle. That is honest — the list is
reflecting what is known — but it means rows can leave under the cursor, so
the count is what tells the user something moved.

### Named for what it does

**Hide no-access**, not *Only accessible*. The second names a set the
application cannot compute, and a control whose name overstates its
knowledge is the same defect as a badge that overstates it.

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
- **[Rows leave as probes settle]** → named above. The alternative — freezing
  the filter until it is re-toggled — trades a moving list for a stale one,
  and a stale list is the worse lie.
- **[Nothing is remembered, so the toggles are set again each session]** →
  deliberate, and cheap. The durable version is a change of its own with a
  home already chosen for it.

## Open Questions

None. The one question the owner raised — where the configuration lives —
is answered above, and answered from what the code actually does rather than
from the IntelliJ analogy that prompted it.
