## 1. The log itself

- [x] 1.1 [dispatch: claude-subagent] Test first: a secret handed to any logged
      failure does not appear in what is written, in any of the three spellings
      the credential store's test already documents.
      `no_secret_reaches_the_log_in_any_spelling`. Mutated three times to prove
      it can fail — readable, raw bytes and escaped-by-the-format — and once
      more against an empty capture, which trips the vacuity guard.
- [x] 1.2 [dispatch: claude-subagent] A logging module in core: a rolling file
      in the platform's log location, this crate at an informative level and
      everything else at warnings, one environment variable to raise it.
      `caixonho-core/src/diagnostics.rs`. Rolled by day *and* bounded by size,
      because a day of repeated failures is the spec's own scenario; the
      variable is `CAIXONHO_LOG` and is laid over the default rather than
      replacing it. No file-appender crate: `tracing-appender` is neither
      vendored here nor readable from crates.io from this session, so its
      version and licence could not be verified before use.
- [x] 1.3 [dispatch: claude-subagent] A log that cannot be opened leaves the
      application running, and says so once rather than at every event.
      `a_log_that_cannot_be_opened_leaves_the_application_running_and_says_so_once`
      — 101 failures, one announcement.
- [x] 1.4 [dispatch: claude-subagent] Expose where the file is, so the frontend
      can tell the user without knowing the platform's conventions.
      `Diagnostics::file` and `Diagnostics::directory` — the directory too,
      because the file's name changes under an interface that stays open.

## 2. What is worth recording

- [x] 2.1 [dispatch: claude-subagent] Record the decisions, not the steps: a
      connection opened and from which source, a listing's outcome and its
      cause, a probe's result, credentials saved or forgotten by name.
      Six recording functions in `diagnostics`, called from `connection::open`,
      `Session::spawn_listing`, `ProbeScheduler::finish` and the two credential
      spawns. Every signature takes a name, a count, a scope or an `Error` —
      there is no call site a secret could reach the logging layer through.
      `the_log_records_the_decisions_a_failure_is_explained_from`.
- [x] 2.2 [dispatch: claude-subagent] Assert that a failure's log line carries
      the same cause the user is shown — a log that disagrees with the screen is
      worse than none. `a_failure_the_log_records_is_the_failure_the_user_is_shown`
      drives a listing to failure and asserts the log holds the delivered
      error's own `Display`, which is the string the panel renders.

## 3. Finding it

- [ ] 3.1 [dispatch: main] The application can show where the log is, from
      somewhere a user will look.

## 4. Close-out

- [ ] 4.1 [dispatch: main] `cargo fmt --all`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo test --workspace` green.
- [ ] 4.2 [dispatch: main] CI green on both targets.
- [ ] 4.3 [dispatch: main] Live: run the app, make it fail, read the log, and
      confirm it explains the failure and holds no secret.
- [ ] 4.4 [dispatch: main] Update `docs/requirements-status.md` and run the
      close-out review in `AGENTS.md`.
