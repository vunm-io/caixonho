## Context

The application already reads IAM Identity Center tokens from the AWS CLI's
cache and resolves credentials from them (`XONHO-0003`, recorded *done*). What
it cannot do is obtain one. The missing piece is therefore narrow: run the
device authorization flow and deposit the result where the existing path
already looks.

Three facts were read out of the dependency sources on 2026-08-20 rather than
recalled, and each one moves a decision below. All three are in
`~/.cargo/registry/src/*/aws-config-1.10.1/src/sso/cache.rs` and
`aws-sdk-ssooidc-1.108.0/src/operation/`.

1. **The client already exists in the build.** `aws-sdk-ssooidc` 1.108.0 is in
   `Cargo.lock`, pulled in by `aws-config`'s `sso` feature, and exposes exactly
   the three operations the flow needs: `RegisterClient`,
   `StartDeviceAuthorization`, `CreateToken`.
2. **The cache key is not what one would guess.** The file is
   `~/.aws/sso/cache/<lowercase hex SHA-1 of an identifier>.json`, and the
   identifier is **the `sso_session` name for token providers, but the
   `sso_start_url` for credentials providers**. Writing under the wrong one
   produces a file nothing reads and a sign-in that appears to do nothing.
3. **The SDK can refresh a token, but only if the registration is stored with
   it.** `aws-config` has its own writer, `save_cached_token`, which it calls
   after refreshing (`token.rs:134`), and `CachedSsoToken::refreshable()`
   returns true only when `client_id`, `client_secret`, `refresh_token` and
   `registration_expires_at` are all present. That writer is `pub(super)` — it
   cannot be called from outside the crate.

## Goals / Non-Goals

**Goals**

- A session obtainable from inside the application, on a machine with no AWS
  CLI installed.
- A token the existing credential path finds without being told about it.
- A sign-in that can be watched, abandoned, and understood when it fails.
- A polling loop testable without a network or a browser.

**Non-Goals**

- Refreshing tokens ourselves. Writing a *refreshable* entry hands that to the
  SDK, which already does it.
- Resolving `sso_session` for legacy inline profiles or `source_profile`
  chains. Named in the proposal as deliberately excluded.
- A second credential path. There is one, it works, and this feeds it.
- `CreateTokenWithIAM`, which serves a different, non-interactive flow.

## Decisions

### The flow lives in core, behind a port

`caixonho-core` gains a `SsoSignIn` port with the three steps as operations,
alongside `ObjectStore`, `SecretStore` and `ConnectionFile`. The real
implementation wraps `aws-sdk-ssooidc`; a double drives the state machine in
tests.

*Why:* the interesting part of this change is not any single call, it is the
loop — waiting on a human, honouring an interval, telling four failures apart,
and stopping when abandoned. Behind a port that loop is a unit test. Against
the live provider it is a manual ritual nobody repeats, which is how a rarely
taken branch ships broken. This is the same reason `ObjectStore` exists, and
`caixonho-gui` continues to see domain types only, never an `aws-sdk-*` type.

*Alternative rejected:* calling the SDK from the GUI where the progress is
shown. It would put a protocol state machine in the one crate this project has
already proved it cannot test.

### The token cache writer is ours, and mirrors the SDK's byte for byte

`save_cached_token` is `pub(super)`, so the format is replicated rather than
called: keys `accessToken`, `expiresAt`, `refreshToken`, `clientId`,
`clientSecret`, `registrationExpiresAt`, `region`, `startUrl`, with timestamps
in the same `DateTime`/RFC-3339 rendering, optional fields omitted rather than
written null.

*Why:* there is no supported way to ask the SDK to write this file, and the
alternative — an app-private cache — would mean building credential resolution
ourselves, discarding the half that already works.

*Consequence, stated plainly:* this is an undocumented compatibility surface
against a crate that is pinned but will be bumped. A test asserts that what we
write is what `aws-config` reads back, so a format drift fails in CI rather
than in someone's morning.

### The entry is written refreshable, or not at all

Registration output (`client_id`, `client_secret`, `registration_expires_at`)
and the refresh token are written alongside the access token.

*Why:* it is the difference between a browser trip per token expiry and one per
registration expiry. The SDK then refreshes silently on our behalf — the
capability is already built, and only withheld from us if we write a partial
entry.

*Trade-off:* a client secret at rest in a file, which the AWS CLI also does,
in the same file, for the same reason. It falls under the existing secret rules
and never reaches a log.

### Identity is the `sso_session` name, and the profile must declare it

The cache file is keyed on the `sso_session` name. A profile that does not
declare one is reported as *not saying where to sign in*, and no sign-in is
offered for it.

*Why:* it is the identifier the token-provider path reads. Guessing the other
identifier — the start URL — writes a file the token provider will not look at.
Reporting the gap is honest and cheap; guessing is a silent failure.

### The write is atomic

Write to a temporary file in the same directory, `fsync`, then rename over the
target.

*Why:* `~/.aws/sso/cache` is shared with the AWS CLI. A half-written file there
is not our bug to suffer — it is another tool's bug to suffer, caused by us.

### Polling is driven by the provider, and cancellable

The interval comes from `StartDeviceAuthorization`; `SlowDownException` widens
it; `AuthorizationPendingException` continues; `ExpiredTokenException` and
`AccessDeniedException` each end the attempt with their own cause. The loop
observes a cancellation signal between attempts.

*Why:* a fixed interval is the documented way to get rate-limited, and the four
exceptions are exactly the four outcomes the spec requires to be told apart —
the mapping already exists in the protocol and only needs to be preserved
rather than flattened into "sign-in failed".

## Risks / Trade-offs

- **The cache format drifts on a stack bump** → a round-trip test writes an
  entry and has `aws-config` read it back; ADR-0001 already makes bumps a
  deliberate PR, and this test is what that PR will trip over.
- **The wrong cache identifier produces a silent no-op** → the round-trip test
  asserts the path as well as the contents, so the mistake is caught once
  rather than debugged repeatedly.
- **A secret gains a new resting place** → the three-spelling redaction test
  from `XONHO-0012` is extended to the sign-in path, including the client
  secret and refresh token.
- **The browser cannot be opened** (headless, sandbox, no handler) → the code
  and URL stay on screen and remain completable elsewhere; opening the browser
  is convenience, never the mechanism.
- **A poll loop that outlives its window** → the attempt carries the expiry
  from the provider and ends itself; abandoning stops it; neither leaves a
  task running behind the window.
- **This is the first outbound call that is not S3** → it goes through the same
  shared `HttpStack`, so enterprise trust material keeps applying, and no
  second TLS configuration appears.

## Migration Plan

None. Nothing existing changes shape: the cache is already read, and an entry
this change writes is indistinguishable to every existing reader from one the
CLI wrote. A user who keeps using `aws sso login` is unaffected, and both tools
continue to share one session — which is the point of writing where they
already look.

## Open Questions

- ~~**Where the in-progress surface lives**~~ — **settled 2026-08-20 by the
  owner: a panel, in place of the connection body.** Not a modal. A sign-in is
  a state the connection is in, not an interruption to something else: the user
  asked for it, nothing else in the window is worth doing until it resolves,
  and the code they have to read belongs where they were already looking. A
  modal would also have to answer what happens to the window behind it, which
  is a question a panel never asks.
- **Whether a scope list is ever needed.** `sso_session` declares scopes; the
  common configuration leaves them implicit. Registration will pass through
  what the profile declares and nothing more until a real account shows a need.
