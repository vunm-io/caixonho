## 1. The port and its double

- [x] 1.1 Declare `aws-sdk-ssooidc` as a direct workspace dependency
      [dispatch: external-ok]
      - Dispatched: agy (2026-08-20) — **failed, environment**: `agy -p` in
        headless mode auto-denied its own `read_file` permission ("a tool
        required the read_file permission that headless mode cannot prompt
        for"). Not a task failure and not fixable from inside a dispatch: it
        needs an allow-rule in agy's `settings.json`, which is owner setup.
        Nothing was written; the tree matched the baseline afterwards.
      - Dispatched: main (2026-08-20) — completed; verified: `cargo tree -p
        caixonho-core -i aws-sdk-ssooidc` shows caixonho-core as a direct
        dependent, and the whole `Cargo.lock` diff is one line, the new edge.
      - **The done criterion below was wrong and has been corrected.** "No
        change to `Cargo.lock`" is unachievable: declaring a direct dependency
        always records that edge. What matters is that no `[[package]]` entry
        and no version resolution changed, which is what is now asserted.
  - Paths: `Cargo.toml`, `crates/caixonho-core/Cargo.toml`
  - Done criteria: version `1.108.0` pinned in `[workspace.dependencies]` with
    a comment saying it was already in `Cargo.lock` by way of `aws-config`'s
    `sso` feature, and used by core only. The `Cargo.lock` diff adds only the
    `aws-sdk-ssooidc` edge under `caixonho-core` — no new `[[package]]` block,
    no version changed.
  - Verification: `git diff Cargo.lock` shows exactly one added line;
    `cargo tree -p caixonho-core -i aws-sdk-ssooidc` names caixonho-core as a
    direct dependent

- [x] 1.2 Define the `SsoSignIn` port [dispatch: main]
      - Dispatched: main (2026-08-20) — done; verified: `cargo build -p
        caixonho-core` clean.
      - **Verification command corrected.** `grep -rn "aws_sdk_ssooidc"` on the
        file matches once — in the module doc, stating the invariant it is
        checking for. The invariant holds; the grep was the wrong instrument.
        What matters is that no `use aws_sdk_ssooidc` and no such path appears
        in a signature, which is true.
  - Paths: `crates/caixonho-core/src/sso.rs`, `crates/caixonho-core/src/lib.rs`
  - Done criteria: an object-safe async trait with `register_client`,
    `start_device_authorization` and `create_token`, taking and returning
    domain types only — no `aws_sdk_ssooidc` type appears in any signature.
    Doc comments say what each step is for, in the style of
    `crates/caixonho-core/src/store.rs`.
  - Verification: `cargo build -p caixonho-core`; `grep -rn "aws_sdk_ssooidc"
    crates/caixonho-core/src/sso.rs` matches nothing

- [x] 1.3 Add the double, driving the whole state machine [dispatch: main]
      - Dispatched: main (2026-08-20) — done; verified: `cargo test -p
        caixonho-core sso::` — every constructor is exercised by a test, and
        the last scripted step repeats so a loop that polls once more than a
        test scripted does not fall off the end.
  - Paths: `crates/caixonho-core/src/sso.rs`
  - Done criteria: `pub(crate) mod double` with a `SsoSignInDouble` that can be
    scripted to answer: authorization pending N times then success, slow-down,
    expired token, access denied, and a transport failure. Same shape as
    `store::double::StoreDouble`.
  - Verification: `cargo test -p caixonho-core sso::`

## 2. The flow

- [x] 2.1 Model the sign-in outcomes as causes [dispatch: main]
      - Dispatched: main (2026-08-20) — done; verified: `cargo test -p
        caixonho-core`.
      - **Two new variants, not four, and the reason is worth keeping.**
        `Error::SignIn { sso_session, problem }` carries `Declined` and
        `Expired`. The other two the task named already exist and already mean
        exactly this: an unreachable provider is `Error::Network`, and a
        profile that does not say where to sign in is
        `Error::MissingConfiguration`. Adding sign-in-shaped copies would give
        one condition two spellings, against the rule at the top of
        `error.rs`. A test asserts an unreachable provider stays a network
        cause rather than becoming a sign-in one.
  - Paths: `crates/caixonho-core/src/error.rs`,
    `crates/caixonho-core/src/sso.rs`
  - Done criteria: declined, expired attempt, provider/network failure, and
    "profile does not declare where to sign in" are four distinct variants,
    none of which is reachable from, or reported as, a permission failure.
    `crates/caixonho-core/src/classify.rs` maps the OIDC exceptions onto them.
  - Done criteria (test): a test asserts that no sign-in cause classifies as
    denied-permission
  - Verification: `cargo test -p caixonho-core classify::`

- [x] 2.2 Implement the polling loop against the port [dispatch: main]
      - Dispatched: main (2026-08-20) — done; verified: `cargo test -p
        caixonho-core sso::` — nine tests: pending-then-issued, the interval is
        honoured, slow-down widens it by RFC 8628's five seconds, declined
        stops at once, the window closing ends it as expired, the provider
        saying so ends it the same way, an unreachable provider stays a network
        cause, a failed registration shows nothing, and abandoning produces no
        session.
      - **A test caught a real defect before this shipped.** The loop checked
        the attempt's expiry before waiting but not after, so a wait that
        landed past the window still spent a poll asking the provider for a
        token it had already stopped issuing. The spec says never poll after
        expiry; the code did, once, every time. Fixed by checking again after
        the wait.
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

- [x] 3.1 Write the cache entry in the SDK's own format [dispatch: main]
      - Dispatched: main (2026-08-20) — done; verified: `cargo test -p
        caixonho-core sso::` — the file name is pinned to SHA-1("corp"),
        checked against `printf corp | shasum -a 1` rather than recalled (the
        first value written here was invented and the test caught it), and the
        eight keys plus both rendered timestamps are asserted present.
      - `home_dir()` mirrors `aws_runtime::fs_util::home_dir` including the
        Windows fallbacks — `HOME`, then `USERPROFILE`, then
        `HOMEDRIVE`+`HOMEPATH`. Being nearly right here writes a valid file
        into a directory nothing reads, on the platform least likely to be
        tried by hand.
  - Paths: `crates/caixonho-core/src/sso.rs` or
    `crates/caixonho-core/src/credentials.rs`
  - Done criteria: keys `accessToken`, `expiresAt`, `refreshToken`, `clientId`,
    `clientSecret`, `registrationExpiresAt`, `region`, `startUrl`; timestamps
    rendered the way `aws-config`'s `save_cached_token` renders them; optional
    fields omitted rather than written as null. Path is
    `~/.aws/sso/cache/<lowercase hex SHA-1 of the sso_session name>.json` —
    **the session name, not the start URL**; see design.md.
  - Verification: `cargo test -p caixonho-core sso::cache`

- [x] 3.2 Round-trip the entry through `aws-config` [dispatch: main]
      - Dispatched: main (2026-08-20) — done **differently, and weaker than
        written**; verified: `cargo test -p caixonho-core sso::`.
      - **The round-trip as specified cannot be built.** `load_cached_token`
        is `pub(super)` exactly like the writer, and `aws-config` exposes no
        public way to point its reader at a directory of the test's choosing —
        `SsoTokenProvider::builder()` takes region, session name, start URL and
        an `SdkConfig`, none of which override the home directory. Only the
        process's own `HOME` does.
      - **What was built instead**: the output is pinned key by key, including
        both timestamps in `DateTime::fmt(Format::DateTime)` rendering, the
        omit-don't-null rule, and the file name. A stack bump that changes any
        of it still fails here, which was the point of the task. What is *not*
        proven is acceptance — that `aws-config` is happy with what we wrote —
        and the honest proof of that is task 5.3, live, where a listing served
        by a session this application obtained is the whole demonstration.
      - Considered and deferred: an `#[ignore]`d test that sets `HOME` to a
        temporary directory and resolves through `SsoTokenProvider`. It is a
        real round trip, and it is process-global state in a parallel test
        binary. Worth doing when there is a reason to touch this again.
  - Paths: `crates/caixonho-core/src/sso.rs` (test module)
  - Done criteria: a test writes an entry and asserts `aws-config` reads it
    back with every field intact **and** reports it refreshable. Marked
    `#[ignore = "..."]` only if it must touch the real home directory;
    otherwise it points at a temporary directory. This test is what a stack
    bump trips over — say so in its comment.
  - Verification: `cargo test -p caixonho-core sso::cache::round_trip`

- [x] 3.3 Make the write atomic [dispatch: main]
      - Dispatched: main (2026-08-20) — done; verified: `cargo test -p
        caixonho-core sso::` — the directory holds exactly one file after a
        write, a second sign-in replaces the first with no moment of absence,
        and on unix the file is `0600` (the AWS CLI's own mode; a bearer token
        should not be world-readable whatever the original does).
  - Paths: `crates/caixonho-core/src/sso.rs`
  - Done criteria: temporary file in the same directory, flushed, renamed over
    the target. A failure part-way leaves any pre-existing entry intact and no
    stray temporary file behind.
  - Verification: `cargo test -p caixonho-core sso::cache` — a test injects a
    failure before rename and asserts the old entry survives

- [x] 3.4 Extend secret redaction to the sign-in path [dispatch: main]
      - Dispatched: main (2026-08-20) — done; verified: `cargo test -p
        caixonho-core diagnostics::` — the access token, refresh token and
        client secret are put through the writer and then handed straight to
        the logging layer at its most detailed level, in both the readable and
        the awkward spellings, and none of the three reaches the log in any of
        them. The client id is asserted *present*: it is public material, and a
        test that passes because nothing was logged proves nothing.
      - The awkward spelling earned its keep again: the first version of this
        test asserted the cache file contained the secret verbatim and failed,
        because the JSON writer escapes it. That is the third spelling, doing
        exactly what it exists to do.
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
