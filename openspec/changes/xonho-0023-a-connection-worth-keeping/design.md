# Design — a connection worth keeping

## Context

`Session::open` does five things: mints a new credentials epoch, clears the
scheduler slot, clears the store, builds a `Connection`, and installs a
scheduler over it. Four of those are cheap and correct. The fifth —
building — is where four to nine seconds go, because building resolves
credentials, and for two of the owner's profiles resolving means running a
subprocess that talks to a password manager.

This change keeps the four and stops repeating the fifth.

## Goals / Non-Goals

**Goals**

- Resolve a connection's credentials once per run.
- Leave every observable behaviour of a selection exactly as it is.
- Keep a deliberate rebuild for retry and for post-sign-in.

**Non-Goals**

- Retaining capability observations across a switch. The
  `capability-awareness` scenario forbids it, and this change stays inside
  that. Recorded in the proposal, repeated here because it is the thing
  most likely to be "improved" by someone later.
- A pool, an expiry, or a refresh policy of our own.
- Making `credential_process` itself fast. It stays four seconds; it stops
  being four seconds *per click*.

## Decisions

### Keep the built `Connection`, keyed by what identifies it

A map from the connection's identity — its `ConnectionSource` — to what was
built for it. Not a single slot: the owner's usage is A → B → A, and a
single slot would make the return trip rebuild, which is the case that hurts
most.

The `ConnectionId` is deliberately **not** the key. A new id is minted per
selection (`next_connection += 1`), which is what makes stale outcomes
droppable — that mechanism stays untouched and unshared. What is being
reused is the client for a *source*, and two selections of the same source
are the same source under two ids.

### Everything `open` resets today keeps being reset

`credentials_changed`, the scheduler slot and the store are all still
cleared and re-established on every selection. That is not conservatism for
its own sake: those three are what `XONHO-0019` and `capability-awareness`
lean on, and the change would be indefensible if reuse quietly moved them.

So the diff is narrow by construction — one branch around one `await`.

### `XONHO-0019` is untouched, and the tests should prove it

That change made a position carry the connection it was read on, so a stale
position cannot render under a new account's name. Nothing here touches
positions, and nothing here makes an *outcome* live longer: outcomes are
still tagged with a `ConnectionId` that is still fresh per selection.

The expected evidence is that `XONHO-0019`'s window tests pass **unmodified**
after this change. If any of them needs adjusting, that is not a test to
fix — it is this design being wrong, and the task says so.

### Failure is not cached, and neither is a signed-in-since connection

Two rebuild triggers, both narrow:

- A connection whose open **failed** never enters the map. Only success is
  worth keeping, and this also means a locked keychain or an expired
  session cannot be remembered as a working client.
- A **sign-in** for a source drops that source's entry, because the whole
  point of signing in is that credentials which did not work now do. This
  is the one place the change has to reach outside `open`, and it reaches
  exactly one line.

The alternative — an expiry so stale clients age out — was declined: the
SDK's own identity cache already reloads credentials near expiry, and a
second timer would be two mechanisms disagreeing about one fact.

### What this is expected to buy, so the live check can falsify it

First selection of a `credential_process` profile: unchanged, ~4s. Every
later selection of the same one: the cost of a listing alone, which the
same log measures at 0.06–0.3s for connections that do no credential work.
If the second selection is still slow, the identity cache is not where the
time goes and this design is wrong about the cause — which is worth knowing
and is what task 4.3 is for.

## Risks / Trade-offs

- **[A client outlives a credential change made outside the app]** → e.g.
  the user edits `~/.aws` mid-run. The SDK's identity cache has the same
  property today within a single connection's lifetime; this widens the
  window from one selection to one run. Named, and cheap to escape:
  restarting the app is already how a user re-reads profiles.
- **[Memory held per connection]** → one client per connection the user
  actually visited, for the length of a run. Small, bounded by how many
  connections exist.
- **[This touches XONHO-0019's seam]** → addressed above; the falsifiable
  form is "its tests pass unmodified", which is checkable rather than
  reassuring.

## Open Questions

None. The SDK's identity-cache behaviour was read before this was written
(`aws-config` 1.10.1: lazy cache on by default, documented as caching on
first request), and the capability scenario that bounds the scope was read
at planning time and changed it.
