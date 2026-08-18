## Context

See `proposal.md` — Why. The relevant existing state: `caithung-core` holds only
the capability seed and has no async surface; `caithung-gui` is the M0 spike,
whose tokio→channel→GPUI-executor bridge (`main.rs`) is the one part meant to
survive. Project constraints that bind this design are in
`docs/PROJECT_BRIEF.md` §5.2–5.3 and §6, and the UI stack is pinned by ADR-0001.

## Goals / Non-Goals

**Goals**

- One connection abstraction that later slices (prefix listing, capability
  probing, transfers) extend rather than replace.
- Listing logic testable with no AWS account and no network.
- An error type rich enough that the UI never has to parse a string.

**Non-Goals (design level)**

- No caching layer for bucket lists — a slice that needs it will add it.
- No connection pooling beyond what the SDK client already does internally.
- No persistence of the selected profile between runs; that arrives with the
  settings slice.

## Decisions

### The S3 port is a trait in core, with one AWS adapter

`trait ObjectStore` (async, object-safe) exposes only what caithung needs —
starting with `list_buckets`. `caithung-core` depends on the trait; the
`aws-sdk-s3` adapter implements it and is the only module that names an SDK
type.

*Why:* it is what makes the specs' scenarios testable at all — a hand-written
double returns canned successes and each error kind, so error classification is
covered by unit tests rather than by hoping. It also keeps the door open for
S3-compatible services and directory buckets, which need different clients
behind the same operations.

*Alternative rejected:* calling the SDK directly from core and testing against
MinIO only. Faster to write, but binds every future test to Docker and gives no
way to simulate a TLS-trust failure or an expired SSO token.

### Errors are classified once, at the adapter boundary

The adapter converts every SDK failure into `caithung_core::Error` immediately;
nothing above it sees `SdkError`. Classification order is fixed and tested:
TLS-trust → network → expired/invalid session → access denied → missing
configuration → unexpected. The order matters because the underlying strings
overlap (brief §5.3), and it is the one part of this change that a future
refactor could silently break, so its ordering is asserted by tests.

*Why here:* the boundary is the only place holding both the SDK's error detail
and the domain vocabulary. Classifying later means passing SDK types upward,
which the crate boundary forbids.

### A single tokio runtime owned by the app, handed to core

The GUI creates one multi-thread runtime at startup and passes a `Handle` into
core; core never creates a runtime of its own.

*Why:* the M0 spike spawns a runtime inline, which is fine for one feed and
wrong for an app — and a core crate that builds its own runtime cannot be reused
by the future CLI, which owns its own. Core stays runtime-agnostic in
construction while still being plainly tokio-based.

### Results reach the UI as messages, not shared state

Each request produces a typed outcome (`Loading` → `Loaded(Vec<Bucket>)` /
`Failed(Error)`) delivered over the existing channel bridge; the view renders
whatever the last outcome for the *current* connection was.

*Why:* it makes the "switching profiles" requirement structural rather than
disciplinary — an outcome carries the connection id it belongs to, so a late
response from the previous profile is dropped instead of being rendered as if it
belonged to the new one. Shared mutable state would make that a bug waiting to
happen.

### TLS is configured once, on the shared HTTP client

`rustls-platform-verifier` is installed on one HTTP client that is handed to
`aws_config`'s loader, so the same client serves SSO/credential calls and S3
calls.

*Why:* the credential path uses its own client by default, and configuring only
the service client is exactly the failure the brief calls out. One client, one
place, no way to configure half of it.

## Risks / Trade-offs

- **SDK credential chain is not unit-testable** → tested by hand against a real
  profile; the manual result is recorded in the change's validation notes so a
  future reader knows what was actually exercised.
- **Error classification depends on SDK-internal error shapes that can change
  across versions** → classification lives in one module with tests naming each
  case; an SDK bump that breaks it fails those tests rather than silently
  degrading messages into "unexpected".
- **`ListBuckets` does not return regions** → region shows as unknown until a
  later slice resolves it per bucket; the spec makes "unknown" a first-class
  display value so this is honest rather than a gap.
- **Deleting the synthetic feed removes the M0 performance demo** → the M0
  numbers are already recorded in ADR-0001; the real listing supersedes it.

## Migration Plan

Not applicable — no released artefact, no stored data, no compatibility surface.
The spike binary is replaced in place on `main`.

## Open Questions

- Whether the profile picker should also surface profiles that exist only in the
  environment (`AWS_PROFILE`, raw `AWS_ACCESS_KEY_ID`) — deferrable, affects
  neither the specs nor the task breakdown.
