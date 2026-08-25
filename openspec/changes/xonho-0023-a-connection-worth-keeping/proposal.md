# XONHO-0023 — A connection worth keeping

## Why

Clicking a connection costs **4 to 9 seconds** when its profile resolves
credentials through a `credential_process`, against **0.06 seconds** for one
whose secret this application already remembers. Measured on 2026-08-25
across two sittings: `vunm` 4.34–9.50s, `r2-caixonho` 4.07–5.09s, every
time, including the fourth click on the same connection.

The cause is not the network and not S3. `Session::open` builds a fresh
`SdkConfig` on every call, so every click re-runs the credential process —
a subprocess that shells out to a password manager, timed directly at
**3.99s warm and 16.36s cold**. The owner has been paying it per click for
days.

What makes this worth doing rather than enduring is what the SDK already
offers and this application throws away. `aws-config`'s default identity
cache is documented as *"load credentials upon first request, cache them,
and then reload them during another request when they are close to
expiring"* — it exists, it is on by default, and it lives on the config
object. Building a new config per open discards it before its first hit.
The fix is therefore mostly **not building the thing twice**, which is a
smaller change than the symptom suggests.

`XONHO-0022` fixed the neighbouring path — the keychain is now read once
per run — and its close-out review said plainly that it would not touch
this one. It did not. This is that one.

## What Changes

- **An opened connection is kept and reused.** Selecting a connection that
  is already open uses what is there instead of building it again: same
  client, same credential resolution, same learned regions.
- **Switching still ends everything the previous connection had on
  screen.** Nothing about `XONHO-0019`'s guarantee changes — the pane, the
  breadcrumb and the position still stop naming the account the user left,
  because that is a property of the *window's* position, not of whether a
  client object survived in memory.
- **Capability observations still clear on every switch, exactly as they
  do today.** This was nearly scoped the other way, and reading the spec at
  planning time stopped it: `capability-awareness` does not merely say
  observations are keyed by credentials, it carries a scenario that says
  *switching profile discards the previous profile's observations, and
  scopes return to unknown*. Keeping them across a round trip would
  contradict a requirement, not sharpen one — and it would trade a
  conservative answer about **permissions** for a few probes. So the
  expensive thing is reused and the cheap, safety-flavoured thing is still
  thrown away.
- **Re-opening on purpose stays possible.** A retry after a failure, and a
  sign-in that produced a new session, both need the connection built
  again — those keep working, and they are what tells kept-and-reused apart
  from stale.

### What is deliberately absent

- Any expiry or refresh policy of our own. The SDK's identity cache already
  reloads credentials near expiry; adding a second timer would be two
  mechanisms disagreeing about the same fact.
- Keeping more than one connection open at a time. The window shows one
  account; a pool is the shape a second window would need, and there is no
  second window.
- The `credential_process` call itself getting faster. It will still cost
  four seconds — **once**.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connections`: gains a requirement that selecting a connection this run
  has already built reuses what was built, and one that says what must
  still be true when it does — the switching guarantee unchanged, and a
  connection that failed or was re-authenticated rebuilt rather than
  reused.

`capability-awareness` is **not** modified. That is a deliberate narrowing
made during planning, recorded above: its switching scenario is explicit,
and this change stays inside it.

## Impact

- **`caixonho-core`**: `Session` holds what it has built beside the store
  and scheduler it already holds; `open` becomes "reuse or build"; a
  deliberate `reopen` for retry and post-sign-in. Everything `open` resets
  today — the scheduler slot, the store, and `credentials_changed` — keeps
  being reset, because the reuse is of the *client*, not of the session's
  idea of where the user is.
- **`caixonho-gui`**: nothing, except that clicks stop being slow. The
  switching behaviour it renders is unchanged, and `XONHO-0019`'s window
  tests should pass untouched — if they do not, that is the finding.
- **Dependencies**: none.
- **Docs**: `docs/requirements-status.md` §4.1 multiple-connections row;
  `docs/planned-changes.md`'s 2026-08-25 timing note gets its outcome.
- **Risk worth naming in the proposal**: this touches the seam
  `XONHO-0019` was written to fix. That change exists because a stale
  position outlived a connection switch, and this change makes a connection
  outlive a switch on purpose. They are compatible — one is about what the
  window points at, the other about what the session holds — but the
  design says so explicitly and the tests are expected to prove it.
