# ADR-0003: S3 behind a port, with one adapter

- **Status:** Accepted
- **Date:** 2026-08-19
- **Deciders:** Vu Nguyen

## Context

Every product decision this app makes — which credentials, what a failure
means, what may be claimed about permissions — has to be testable, and most of
it is reasoning *about* what S3 answered rather than about S3 itself.

Three forces pull on where the SDK sits:

- **The specs are written as scenarios.** "An account with no buckets is a
  truthful empty result, not an error"; "a probe that fails for any reason other
  a denial records nothing". Each names a specific answer from the service,
  including answers that are inconvenient to obtain on demand — a throttle, an
  expired token, a redirect from the wrong region.
- **`aws-sdk-s3` types must not reach the frontend** (crate invariant 1),
  because the UI is swappable and a CLI is planned over the same core.
- **S3-compatible services are a stated goal** (brief §5, M5): MinIO, R2,
  Backblaze. They speak the same operations through different endpoints and
  quirks.

## Decision

Object storage is reached through **one object-safe async trait — the port —
implemented by exactly one adapter**, which is the only module in the workspace
allowed to name an SDK type.

- `caixonho-core::store` declares the port and ships a hand-written double
  beside it, with one constructor per canned behaviour, so a test names the
  scenario it simulates rather than assembling state.
- `caixonho-core::adapter` implements the port over `aws-sdk-s3`, and maps
  every SDK failure through the classifier rather than letting an SDK error
  escape.
- The port speaks domain types only, so nothing above it can accidentally
  depend on the SDK's shape.

The port starts at exactly what the current slice needs and is extended by
later slices rather than designed ahead.

## Consequences

**What this buys.** The specs' scenarios are unit tests with no AWS account and
no network: a denial, a throttle, an expired session and a wrong region are all
one constructor away. That is what made it possible to assert, for instance,
that a throttled probe records nothing — a case that is genuinely hard to
produce against the real service on demand.

**What it costs.** Every new operation is written twice, once in the trait and
once in the double, before it is written in the adapter. That tax is the point:
it is what keeps the double honest.

**The gap it leaves, and this is the live one.** A double cannot tell you what
the service actually does. Today a real SSO session failure reached the app as
a dispatch failure carrying no error code, and 105 unit tests — all passing,
all green against the double — said nothing, because the double had never been
told that shape existed. The defect surfaced when a person opened the app.

A port makes the *reasoning* testable; it does not make the *assumptions*
testable. Those need a real endpoint, which is why the brief's M1 asks for a
MinIO rig and why that rig is not optional.

## Alternatives considered

**Call `aws-sdk-s3` directly from core, and test with recorded HTTP.** The SDK
ships `StaticReplayClient` for exactly this, and it tests more of the stack —
signing, retries, response parsing. Rejected as the primary seam because a
recorded exchange is written against one service's wire format, which forecloses
the S3-compatible goal, and because the fixtures are long enough that a test
stops naming its scenario. It remains the right tool for the *adapter's own*
tests, where the question is what the SDK does.

**Mock at the HTTP layer with a local server.** Rejected: slower, and it tests
the same thing as a MinIO rig with less fidelity. If a request is going over a
socket, it may as well go to a real implementation.

**No seam; test through the GUI.** Rejected: the frontend is exploratory by
invariant, the core is not.

## Where this lives

`caixonho-core::store` (port and double), `caixonho-core::adapter` (the one
implementation), `caixonho-core::classify` (SDK failure to structured cause).
