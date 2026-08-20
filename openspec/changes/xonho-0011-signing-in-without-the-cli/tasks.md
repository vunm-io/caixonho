## 1. The port and its double

- [ ] 1.1 Declare `aws-sdk-ssooidc` as a direct workspace dependency
      [dispatch: external-ok]
  - Paths: `Cargo.toml`, `crates/caixonho-core/Cargo.toml`
  - Done criteria: version `1.108.0` pinned in `[workspace.dependencies]` with
    a comment saying it was already in `Cargo.lock` by way of `aws-config`'s
    `sso` feature, and used by core only. `Cargo.lock` shows no new package.
  - Verification: `git diff Cargo.lock` is empty; `cargo tree -p caixonho-core
    -i aws-sdk-ssooidc` names caixonho-core as a direct dependent

- [ ] 1.2 Define the `SsoSignIn` port [dispatch: main]
  - Paths: `crates/caixonho-core/src/sso.rs`, `crates/caixonho-core/src/lib.rs`
  - Done criteria: an object-safe async trait with `register_client`,
    `start_device_authorization` and `create_token`, taking and returning
    domain types only — no `aws_sdk_ssooidc` type appears in any signature.
    Doc comments say what each step is for, in the style of
    `crates/caixonho-core/src/store.rs`.
  - Verification: `cargo build -p caixonho-core`; `grep -rn "aws_sdk_ssooidc"
    crates/caixonho-core/src/sso.rs` matches nothing

- [ ] 1.3 Add the double, driving the whole state machine [dispatch: main]
  - Paths: `crates/caixonho-core/src/sso.rs`
  - Done criteria: `pub(crate) mod double` with a `SsoSignInDouble` that can be
    scripted to answer: authorization pending N times then success, slow-down,
    expired token, access denied, and a transport failure. Same shape as
    `store::double::StoreDouble`.
  - Verification: `cargo test -p caixonho-core sso::`

## 2. The flow

- [ ] 2.1 Model the sign-in outcomes as causes [dispatch: main]
  - Paths: `crates/caixonho-core/src/error.rs`,
    `crates/caixonho-core/src/sso.rs`
  - Done criteria: declined, expired attempt, provider/network failure, and
    "profile does not declare where to sign in" are four distinct variants,
    none of which is reachable from, or reported as, a permission failure.
    `crates/caixonho-core/src/classify.rs` maps the OIDC exceptions onto them.
  - Done criteria (test): a test asserts that no sign-in cause classifies as
    denied-permission
  - Verification: `cargo test -p caixonho-core classify::`

- [ ] 2.2 Implement the polling loop against the port [dispatch: main]
  - Paths: `crates/caixonho-core/src/sso.rs`
  - Done criteria: honours the interval returned by
    `StartDeviceAuthorization`, widens it on slow-down, stops on success,
    decline, expiry or cancellation, and never polls after the attempt's own
    expiry. Cancellation is observed between attempts. Time is injected, not
    read from the clock, so tests do not sleep.
  - Done criteria (tests): pending-then-success, slow-down widens the wait,
    decline stops immediately, expiry stops with its own cause, cancel stops
    and writes nothing
  - Verification: `cargo test -p caixonho-core sso::` — every case above named
    in a test

- [ ] 2.3 Real adapter over `aws-sdk-ssooidc` [dispatch: main]
  - Paths: `crates/caixonho-core/src/adapter.rs` (or a sibling module beside
    it), `crates/caixonho-core/src/sso.rs`
  - Done criteria: the three calls go through the shared `HttpStack`, so
    enterprise trust material applies and no second TLS configuration is
    created. Region comes from the profile's `sso_session`.
  - Verification: `cargo build -p caixonho-core`; `grep -rn "HttpStack"` shows
    the adapter taking the shared stack rather than building a client

## 3. The token cache

- [ ] 3.1 Write the cache entry in the SDK's own format [dispatch: main]
  - Paths: `crates/caixonho-core/src/sso.rs` or
    `crates/caixonho-core/src/credentials.rs`
  - Done criteria: keys `accessToken`, `expiresAt`, `refreshToken`, `clientId`,
    `clientSecret`, `registrationExpiresAt`, `region`, `startUrl`; timestamps
    rendered the way `aws-config`'s `save_cached_token` renders them; optional
    fields omitted rather than written as null. Path is
    `~/.aws/sso/cache/<lowercase hex SHA-1 of the sso_session name>.json` —
    **the session name, not the start URL**; see design.md.
  - Verification: `cargo test -p caixonho-core sso::cache`

- [ ] 3.2 Round-trip the entry through `aws-config` [dispatch: main]
  - Paths: `crates/caixonho-core/src/sso.rs` (test module)
  - Done criteria: a test writes an entry and asserts `aws-config` reads it
    back with every field intact **and** reports it refreshable. Marked
    `#[ignore = "..."]` only if it must touch the real home directory;
    otherwise it points at a temporary directory. This test is what a stack
    bump trips over — say so in its comment.
  - Verification: `cargo test -p caixonho-core sso::cache::round_trip`

- [ ] 3.3 Make the write atomic [dispatch: main]
  - Paths: `crates/caixonho-core/src/sso.rs`
  - Done criteria: temporary file in the same directory, flushed, renamed over
    the target. A failure part-way leaves any pre-existing entry intact and no
    stray temporary file behind.
  - Verification: `cargo test -p caixonho-core sso::cache` — a test injects a
    failure before rename and asserts the old entry survives

- [ ] 3.4 Extend secret redaction to the sign-in path [dispatch: main]
  - Paths: `crates/caixonho-core/src/diagnostics.rs`,
    `crates/caixonho-core/src/sso.rs`
  - Done criteria: access token, refresh token and client secret cannot reach
    the log — asserted in all three spellings the `XONHO-0012` test uses
    (readable, byte array, escaped by the destination format), and no logging
    signature accepts them.
  - Verification: `cargo test -p caixonho-core diagnostics::`

## 4. The window

- [ ] 4.1 Offer signing in where the cause is stated [dispatch: main]
  - Paths: `crates/caixonho-gui/src/views/failure.rs`,
    `crates/caixonho-gui/src/app.rs`
  - Done criteria: a connection unavailable for an expired or absent session,
    whose profile declares an `sso_session`, shows a sign-in action beside the
    cause. One with no `sso_session` states that as the cause and shows no
    action.
  - Verification: run the application against a profile of each kind; the log
    names the cause in both

- [ ] 4.2 Show the attempt while it runs [dispatch: main]
  - Paths: `crates/caixonho-gui/src/views/`, `crates/caixonho-gui/src/app.rs`
  - Done criteria: user code and verification address readable and selectable,
    a statement that the browser is being waited on, and a way to abandon.
    Nothing about the attempt is only in the browser. New code lands in
    `views/`, not in `app.rs`.
  - Verification: run a sign-in and abandon it mid-way; the connection returns
    to its prior state and the log shows polling stopped

- [ ] 4.3 Open the browser as convenience, never as mechanism
      [dispatch: external-ok]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the verification page opens on the user's act; a failure to
    open is not an error, because the code and address are already on screen.
  - Verification: the sign-in completes with the browser opened by hand from
    the shown address

- [ ] 4.4 Name the states so a test can find them [dispatch: external-ok]
  - Paths: `crates/caixonho-gui/src/views/`
  - Done criteria: the in-progress surface and the sign-in action carry
    `debug_selector` names, per `XONHO-0015`. No-ops in release; the only way
    a later test can assert either exists.
  - Verification: `cargo test -p caixonho-gui`

## 5. Close-out

- [ ] 5.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [ ] 5.2 CI green on both targets [dispatch: main]
  - Paths: none
  - Done criteria: the run for the tip shows `build (windows-latest)` and
    `build (macos-latest)` successful; the run id is recorded here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 5.3 Live: sign in from the application on a machine with the AWS CLI
      unavailable [dispatch: main]
  - Paths: none
  - Done criteria: a real Identity Center session obtained from the app with
    the CLI off `PATH`, a listing served by it, and what was seen written here.
    Then: the same connection after the token is expired or deleted, showing
    the offer where the cause is stated. Both confirmed against a real account.
  - Verification: the log in the platform's log directory names the connection
    and the outcome of each attempt

- [ ] 5.4 Update the reader-facing documents in this change, not after it
      [dispatch: main]
  - Paths: `README.md`, `docs/roadmap.md`, `docs/requirements-status.md`
  - Done criteria: README says the AWS CLI is no longer required and what that
    means; roadmap moves `XONHO-0011` to landed; `requirements-status.md` marks
    in-app device-flow login **done**, inline re-login offer **done**, and
    leaves IAM Identity Center **partial** with the legacy-inline and
    `source_profile` gap named as what remains. **Recount the summary line with
    a script, not by hand** — it drifted twice in one day during `XONHO-0006`,
    both times with the total right and the split wrong.
  - Verification: the counted totals match the table rows

- [ ] 5.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: this change
  - Done criteria: the review in `AGENTS.md` §Close-out is run and what it
    found is recorded here, including anything it found wrong
  - Verification: the recorded findings
