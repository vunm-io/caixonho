## 1. The log itself

- [ ] 1.1 [dispatch: claude-subagent] Test first: a secret handed to any logged
      failure does not appear in what is written, in any of the three spellings
      the credential store's test already documents.
- [ ] 1.2 [dispatch: claude-subagent] A logging module in core: a rolling file
      in the platform's log location, this crate at an informative level and
      everything else at warnings, one environment variable to raise it.
- [ ] 1.3 [dispatch: claude-subagent] A log that cannot be opened leaves the
      application running, and says so once rather than at every event.
- [ ] 1.4 [dispatch: claude-subagent] Expose where the file is, so the frontend
      can tell the user without knowing the platform's conventions.

## 2. What is worth recording

- [ ] 2.1 [dispatch: claude-subagent] Record the decisions, not the steps: a
      connection opened and from which source, a listing's outcome and its
      cause, a probe's result, credentials saved or forgotten by name.
- [ ] 2.2 [dispatch: claude-subagent] Assert that a failure's log line carries
      the same cause the user is shown — a log that disagrees with the screen is
      worse than none.

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
