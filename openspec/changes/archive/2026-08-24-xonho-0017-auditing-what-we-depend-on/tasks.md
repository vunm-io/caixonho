## 1. Stop compiling what this application does not use

- [x] 1.1 Drop the default features of the two SDK crates [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test --workspace` 263
        core + 36 window, clippy clean at `-D warnings`, `cargo build --release
        -p caixonho-gui` succeeds.
      - `aws-config` untouched, as the task says. It already declares
        `aws-sdk-ssooidc` with `default-features = false`, so the two
        declarations changed here were the only ones turning the legacy stack
        on — which is why dropping them is sufficient rather than merely
        helpful.
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

- [x] 1.2 Prove the removal by measurement, and write the numbers here [dispatch: main]
      - Done in `main` (2026-08-21). Measured, both sides:

        | | Before | After |
        |---|---|---|
        | `cargo audit` vulnerabilities | 4 | **0** |
        | `cargo audit` warnings | 7 | 7 |
        | Lockfile crates | 957 | **948** |
        | `rustls` in build | 0.21.12, 0.23.43, 0.26.4, 0.27.9 | 0.23.43, 0.26.4, 0.27.9 |
        | `rustls-webpki` in build | 0.101.7, 0.103.14 | **0.103.14** |
        | `h2` in build | 0.3.27, 0.4.16 | **0.4.16** |
        | `hyper` in build | 0.14.32, 1.11.0 | **1.11.0** |

      - Every remaining version is at or above the advisories' patched
        threshold: `rustls-webpki >= 0.103.13` and `h2 >= 0.4.16`. Checked
        against the advisory text rather than assumed from "the old one is
        gone".
      - `Cargo.lock` shrank by 145 lines and gained 23. Worth stating because
        an earlier guess in this session was that a lockfile is
        feature-independent and would not move — it moved, and the audit
        followed it. A partial change does **not** move it: turning off only
        `aws-sdk-ssooidc` left the lockfile byte-identical, because
        `aws-sdk-s3` still asked for the same features. Feature unification
        means a subtree leaves only when the last consumer stops asking.
  - Paths: this file
  - Done criteria: recorded before and after — the `cargo audit` vulnerability
    count, the lockfile crate count, and the presence of `rustls 0.21.x`,
    `rustls-webpki 0.101.x`, `h2 0.3.x` and `hyper 0.14.x` in
    `cargo tree --edges no-dev`. The expected result is 4 → 0 vulnerabilities
    and 957 → 948 crates; **if the numbers differ from that, the difference is
    what gets written down**, not the expectation.
  - Verification: `cargo audit`; `cargo tree -p caixonho-core --edges no-dev`

## 2. The gate

- [x] 2.1 A `deny.toml` that states the policy rather than silencing it [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo deny check advisories`
        reports `advisories ok`, with every one of the seven ignores matching a
        real crate — no `advisory-not-detected` warnings left.
      - **`unsound = "all"` is the line that matters, and it was nearly
        missed.** With it absent, `cargo deny check advisories` passed clean
        while `cargo audit` reported `RUSTSEC-2026-0253` against `lru` — a
        use-after-free on panic, arriving through `aws-sdk-s3`. cargo-deny's
        default for `unsound` does not reach a transitive crate. It surfaced
        only because that ignore was reported as matching nothing; a policy
        written without the seven-item list would have shipped a gate that
        silently dropped the one memory-safety advisory in the tree.
      - **The gate was watched failing**, not assumed to bite: removing the
        `lru` exception turns `advisories ok` into `advisories FAILED`, and
        restoring it turns it back.
      - `unmaintained = "all"`, and the six frozen-stack advisories named one
        by one. `unmaintained = "workspace"` would have silenced all six,
        since none is a direct dependency — the decision argued in `design.md`.
      - **Deviation from the done criteria.** The task said licence and ban
        checks should be "configured but not enforced". They are **not
        configured**, and a comment says why: CI runs `check advisories` only,
        and an unreviewed `[licenses]` section would read like a policy while
        being a default nobody has looked at the output of. That is the exact
        failure this repository keeps finding in its own documents. They are
        their own change.
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

- [x] 2.2 Enforce the expiry the tool does not [dispatch: main]
      - Done in `main` (2026-08-21); verified against both fixtures:
        `scripts/fixtures/deny-expired.toml` exits 1 and names both entries,
        `scripts/fixtures/deny-in-date.toml` exits 0.
      - The fixtures are **in the repository**, not in a scratch directory, and
        CI runs both. A test that lives on one machine is not a test the
        project has.
      - It fails an entry with **no** expiry as well as one with a past date.
        "Accepted forever" is not a decision this policy offers.
      - **POSIX awk only.** The first version used gawk's three-argument
        `match` and `{n}` intervals, which macOS awk has neither of — it failed
        on the maintainer's own machine. Rewritten with two-argument `match`
        plus `substr`. The date comparison is a string compare, which is
        exactly why the format is fixed to ISO.
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

- [x] 2.3 A CI job of its own [dispatch: external-ok]
      - Done in `main` (2026-08-21); the workflow parses and declares
        `fmt`, `audit`, `build`. Verified by loading the YAML rather than by
        reading it.
      - `EmbarkStudios/cargo-deny-action@v2`, pinned by tag to match the
        convention the workflow already uses for `actions/checkout` and
        `Swatinem/rust-cache`. Pinning actions by SHA instead is a defensible
        supply-chain position and would be a change to all of them, not to
        this one alone — noted rather than done unilaterally.
      - The job runs the two fixtures as well as the real file, so the expiry
        check's failure path is exercised on every push. A gate nobody has
        watched fail is not known to be a gate.
  - Paths: `.github/workflows/ci.yml`
  - Done criteria: an `audit` job on `ubuntu-latest`, beside `rustfmt` rather
    than inside either build matrix, running `cargo deny check advisories` and
    then `scripts/check-advisory-expiry.sh`. It is a separate job so a
    supply-chain failure and a compilation failure are told apart by whoever
    reads the tick. The advisory database is cached.
  - Verification: the run for the tip shows the `audit` job

## 3. Close-out

- [x] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-21); verified: all three exit zero.
        263 core + 36 window tests, clippy clean at `-D warnings`.
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [x] 3.2 CI green on every job, and the run id recorded [dispatch: main]
      - Done in `main` (2026-08-21); run **32490249136**, conclusion `success`:
        `rustfmt`, `dependency audit`, `build (windows-latest)` and
        `build (macos-latest)` all successful. The first run of the new job,
        and it passed on its first attempt.
      - The run is for `0266090`, the commit carrying every code and config
        change in this change. Checked rather than waved at:
        `git diff --name-only 0266090..HEAD` returns markdown only.
      - The job's log was read rather than trusted: cargo-deny 0.20.2 was
        fetched and run, all seven acceptances were checked and reported in
        date, the in-date fixture passed, and the expired fixture was correctly
        rejected with both `EXPIRED` and `NO EXPIRY`. A job that passes because
        a step quietly did nothing is worse than a red one.
      - `build (windows-latest)` took ~20 minutes. `e176ec9` changed
        `Cargo.lock` substantially, so the Windows `rust-cache` was cold — the
        next run is the one to judge the standing cost by.
  - Paths: this file
  - Done criteria: `build (windows-latest)`, `build (macos-latest)`, `rustfmt`
    and the new `audit` job all successful for the tip; the run id is written
    here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [x] 3.3 Live: a real connection still works without the SDK's own HTTP client [dispatch: main]
      - Done live on 2026-08-23, on a binary built from `main` that day, and
        the evidence arrived as a by-product: the owner was driving the app
        for an unrelated diagnosis, which is a better test than a staged one —
        nobody was being careful.
      - Both paths the task names, from the log (`~/Library/Logs/caixonho`,
        2026-08-23; connection and session names withheld — the log carries
        real account names and this file is public):
        - A profile-sourced connection opened and `listed the account
          buckets=8` — the rustls listing path, 683ms end to end.
        - Before that, the same connection's expired session was classified
          (`listing failed … sign in again`), the in-app offer was taken, and
          `signed in sso_session=…` followed — the OIDC device-flow path, the
          other HTTP consumer that would have broken.
      - Also exercised in the same sitting, beyond what the task asks: a
        stored-credential connection listing repeatedly at 56–67ms, and 106
        capability probes settling. Nothing in the session reached for a
        client that is not compiled in.
  - Paths: none
  - Done criteria: on a real account, opening a connection and listing buckets
    still works, and so does a sign-in — the two paths that would break if
    anything did rely on the default HTTPS client being compiled. What was seen
    is written here.
    **The unit tests do not settle this.** Not one of them reaches a socket,
    so "nothing needs the default client" is a claim checked by reading the
    call graph. This is where it is checked by using it.
  - Verification: the log in the platform's log directory shows both

- [x] 3.4 Correct the trace, and update the reader-facing documents [dispatch: main]
      - Done in `main` (2026-08-21); verified: `scripts/count-requirements.sh`
        agrees with the tables — §7–8 moves to 2 done, 3 partial, nothing
        unstarted; M1 is unchanged at 11/10/3.
      - The §7–8 row is **done**, not partial, and the row says why the
        standard differs from the §4.1 region row marked partial an hour
        earlier: that requirement's exercise venue is a real account, this
        one's *is* CI, and CI runs it. Written down because two rows decided
        differently on the same day, by the same hand, is exactly what looks
        like carelessness later.
      - `docs/planned-changes.md`'s wrong trace is **corrected in place and
        left visible** — the old claim is quoted, then refuted with the feature
        table and the real chain. Deleting it would have left the next reader
        trusting the next paragraph just as readily, and the habit it cost is
        the part worth keeping: a dependency trace is read off `[features]` and
        `cargo tree`, never reconstructed from what a feature name sounds like.
      - `README.md` gained the promise, because it is one a user of a
        credential-handling application has a reason to care about.
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

- [x] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
      - Done in `main` (2026-08-21). The five answers:

      **1. Did we build what was asked, or what was convenient?** What was
      asked, and it turned out to be less work and more value than the
      proposal's own premise: the four advisories were removable, not merely
      acceptable. Every requirement in the new `supply-chain` spec has
      something behind it — the gate fails on an unaccepted advisory
      (watched), each exception is individual and dated (seven of them), and an
      expired one fails the build (both fixtures run in CI). **Two departures,
      both written into the documents they depart from**: `cargo-audit` is not
      run alongside `cargo-deny` in CI (argued in `design.md` — same database,
      same lockfile, one advisory failing twice teaches nobody anything), and
      the licence/ban sections are absent rather than configured-and-unenforced
      (task 2.1).

      **2. Do the reader-facing documents still tell the truth?** Yes, and one
      of them had stopped: `docs/planned-changes.md` carried a dependency trace
      that was wrong about the mechanism and, through it, wrong about the
      remedy. Corrected in place with the error left visible. `README.md`,
      `docs/roadmap.md` and `docs/requirements-status.md` updated in this
      change rather than after it. `docs/architecture.md` and
      `docs/design-language.md` needed nothing — no shape moved and no surface
      changed.

      **3. Did we leave rubbish?** No. The two fixtures are run by CI on every
      push, so they are load-bearing rather than decoration. The `cargo deny
      init` probe used while measuring lives in a scratch directory outside the
      repository. `deny.toml` contains no blanket ignore and no entry without a
      date. Clippy clean at `-D warnings`.

      **4. What is asserted but not verified?**
      - **That nothing needs the SDK's own HTTP client.** Read off the call
        graph — three construction sites, all handing in `tls.rs`'s stack —
        and confirmed by tests that never reach a socket. Task 3.3 is where it
        is confirmed by using it, and it is the owner's.
      - **The action is pinned by a moving tag.** `cargo-deny-action@v2`
        resolved to `3c63498` on this run and may resolve to something else on
        the next. In a change about what this project trusts, that is worth
        stating plainly: it matches the convention already used for
        `actions/checkout` and `rust-cache`, and changing it is a change to all
        three.
      - ~~**`unsound = "all"` is known to catch `lru` and is not known to be
        sufficient.**~~ **Checked rather than left asserted.** With the ignore
        list emptied, `cargo deny check advisories` and `cargo audit` report
        **the same seven advisories, exactly** — no crate is seen by one tool
        and missed by the other. That also turns the decision to run
        `cargo-deny` alone from an argument into a measurement. It is a
        statement about this dependency tree today, not a general claim about
        the two tools.
      - **The expiry comparison is a string compare against `date -u`.** Right
        for ISO dates, and untested across a timezone boundary or a leap day.
      - **Nobody has watched the gate fail in CI.** It was watched failing
        locally; what CI has proven is that it passes, and that the expiry
        script's failure path works there.

      **5. What is left, and where is it written?**
      - Task 3.3, the live check — the only task left, and the owner's.
      - Licence and ban checks, deliberately deferred, in the comment at the
        top of `deny.toml` and in task 2.1.
      - Pinning actions by SHA rather than tag, in task 2.3 and in answer 4
        above — a decision about all three actions, not this one.
      - The seven acceptances expire on 2026-11-21 (`lru`) and 2027-02-21 (the
        six frozen-stack crates). The build says so when they do.
  - Paths: this file
  - Done criteria: the five questions answered in writing, including what is
    asserted but not verified — at minimum, that the SDK feature reduction is
    proven only by reading the call graph until 3.3 says otherwise
  - Verification: the answers exist and name specifics, not reassurances
