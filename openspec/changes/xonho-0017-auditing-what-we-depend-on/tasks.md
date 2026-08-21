## 1. Stop compiling what this application does not use

- [ ] 1.1 Drop the default features of the two SDK crates [dispatch: main]
  - Paths: `Cargo.toml`
  - Done criteria: `aws-sdk-s3` and `aws-sdk-ssooidc` are declared with
    `default-features = false` and the features actually used named
    explicitly — measured to be `["sigv4a", "http-1x", "rt-tokio"]` and
    `["rt-tokio"]` respectively. A comment says **why**: these crates' `rustls`
    and `default-https-client` features supply an HTTP client that `tls.rs`
    already replaces at every construction site, and they are the only thing
    pulling the legacy hyper-0.14 / rustls-0.21 stack into the build.
  - Done criteria: `aws-config` is **not** touched. It already declares
    `aws-sdk-ssooidc` with `default-features = false`, and whatever its own
    defaults provide is left alone rather than trimmed on a guess.
  - Verification: `cargo test --workspace`; `cargo build --release -p caixonho-gui`

- [ ] 1.2 Prove the removal by measurement, and write the numbers here [dispatch: main]
  - Paths: this file
  - Done criteria: recorded before and after — the `cargo audit` vulnerability
    count, the lockfile crate count, and the presence of `rustls 0.21.x`,
    `rustls-webpki 0.101.x`, `h2 0.3.x` and `hyper 0.14.x` in
    `cargo tree --edges no-dev`. The expected result is 4 → 0 vulnerabilities
    and 957 → 948 crates; **if the numbers differ from that, the difference is
    what gets written down**, not the expectation.
  - Verification: `cargo audit`; `cargo tree -p caixonho-core --edges no-dev`

## 2. The gate

- [ ] 2.1 A `deny.toml` that states the policy rather than silencing it [dispatch: main]
  - Paths: `deny.toml`
  - Done criteria: `[advisories]` denies vulnerabilities. `unmaintained` is set
    to the scope that still reports the six transitive warnings, and each is
    listed individually as `{ id = ..., reason = ... }` naming why it is
    accepted and the date the acceptance expires, in the fixed form 2.2 parses.
    No entry covers more than one advisory. `lru`'s unsound advisory is its own
    line with its own reason — it is not unmaintained and must not be filed as
    though it were.
  - Done criteria: licence and ban checks are **configured but not yet
    enforced**, with a comment saying they are deliberately deferred — turning
    them on unread is how a pipeline goes red for a reason nobody decided.
  - Verification: `cargo deny check advisories`

- [ ] 2.2 Enforce the expiry the tool does not [dispatch: main]
  - Paths: `scripts/check-advisory-expiry.sh`
  - Done criteria: reads `deny.toml`, finds every accepted advisory, and exits
    non-zero when a recorded date has passed, naming which. Measured, not
    assumed: `cargo-deny` 0.20.2 rejects an `expires` key with
    `error[unexpected-keys]`, accepting only `id` and `reason`, so the date
    lives in the reason and this script is what makes it mean anything.
  - Done criteria (test): running it against a fixture with a past date fails
    and names the advisory; against a future date it passes. An expiry checker
    that cannot fail is the same as no expiry checker.
  - Verification: the script against both fixtures

- [ ] 2.3 A CI job of its own [dispatch: external-ok]
  - Paths: `.github/workflows/ci.yml`
  - Done criteria: an `audit` job on `ubuntu-latest`, beside `rustfmt` rather
    than inside either build matrix, running `cargo deny check advisories` and
    then `scripts/check-advisory-expiry.sh`. It is a separate job so a
    supply-chain failure and a compilation failure are told apart by whoever
    reads the tick. The advisory database is cached.
  - Verification: the run for the tip shows the `audit` job

## 3. Close-out

- [ ] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` green [dispatch: main]
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [ ] 3.2 CI green on every job, and the run id recorded [dispatch: main]
  - Paths: this file
  - Done criteria: `build (windows-latest)`, `build (macos-latest)`, `rustfmt`
    and the new `audit` job all successful for the tip; the run id is written
    here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 3.3 Live: a real connection still works without the SDK's own HTTP client [dispatch: main]
  - Paths: none
  - Done criteria: on a real account, opening a connection and listing buckets
    still works, and so does a sign-in — the two paths that would break if
    anything did rely on the default HTTPS client being compiled. What was seen
    is written here.
    **The unit tests do not settle this.** Not one of them reaches a socket,
    so "nothing needs the default client" is a claim checked by reading the
    call graph. This is where it is checked by using it.
  - Verification: the log in the platform's log directory shows both

- [ ] 3.4 Correct the trace, and update the reader-facing documents [dispatch: main]
  - Paths: `docs/planned-changes.md`, `docs/requirements-status.md`,
    `README.md`, `docs/roadmap.md`
  - Done criteria: the §7–8 *Dependencies audited in CI* row moves off
    **none** — to **done** only if 3.3 has passed, and otherwise to **partial**
    with the gap named, the standard this file already holds itself to.
    **Recount the summary with `scripts/count-requirements.sh`.**
  - Done criteria: `docs/planned-changes.md`'s 2026-08-21 measurement is
    **corrected in place, with the correction visible** — it names `__rustls`
    and an `acceptor` feature as the cause, and that is not what the crate's
    feature table says. A note that reads as though it were right all along is
    worse than the error, because the next person will trust the next paragraph
    too.
  - Verification: the counted totals match the table rows

- [ ] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: this file
  - Done criteria: the five questions answered in writing, including what is
    asserted but not verified — at minimum, that the SDK feature reduction is
    proven only by reading the call graph until 3.3 says otherwise
  - Verification: the answers exist and name specifics, not reassurances
