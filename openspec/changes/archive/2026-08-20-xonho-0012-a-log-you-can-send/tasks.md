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

- [x] 3.1 [dispatch: main] The application can show where the log is, from
      somewhere a user will look.
      - In the status bar, quietly and always, rather than behind a menu: the
        moment it is wanted is the moment something has already gone wrong, and
        hunting for it then is the worst time to start. The directory, not the
        file, because the file's name changes when the log rolls.

## 4. Close-out

- [x] 4.1 [dispatch: main] `cargo fmt --all`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo test --workspace` green.
- [x] 4.2 [dispatch: main] CI green on both targets.
      - Run `32333149778` on `e57015c`, the tip carrying this change:
        `build (windows-latest)` success, `build (macos-latest)` success,
        `rustfmt` success. The two commits of this change were each green on
        both targets when pushed (`32284158017`, `32317769510`).
- [x] 4.3 [dispatch: main] Live: run the app, make it fail, read the log, and
      confirm it explains the failure and holds no secret.
      - **Done 2026-08-20, on a current build, both paths.** The success path
        first: choosing a stored connection wrote `connection opened … source=
        "stored credential"`, then `listed the account … buckets=3`, then three
        `probe settled … observation=Allowed`. Then the failure, from the
        connection that had never been explicable:

            WARN connection refused connection=… id=2 source="stored credential"
                 cause=the credential store refused the request — allow caixonho
                 to use it and try again (connection `…`)
            WARN listing failed connection=… id=2 cause=the credential store
                 refused the request — …

        The log explains the failure in a sentence that names what to do about
        it, distinguishes a store that refused from a credential that was
        rejected, and carries no secret in any of the three spellings. This is
        the case the change was written for, and it now works.
      - It was **un-ticked earlier the same day** before being earned: it had
        been carrying "Partly" over a paragraph saying the failure had never
        been read, which reports done while the central check has not happened.
      - The file is created at startup and this machine's log holds a
        real event: `INFO connection opened connection=… source="profile"
        region="ap-southeast-1"`. Starting the app writes nothing further, which
        is correct now that startup contacts nothing — there are no decisions
        until a connection is chosen.
      - **Not yet done: a real failure read back out of the log.** That needs
        someone to use the app and then look, and it is the first thing worth
        doing next session, because it is the case the change exists for.
      - 2026-08-20, and this is why it had not happened: the application window
        open on this machine was **a build that predates logging entirely**. Its
        binary in `target/Caixonho.app` was built 2026-08-19 15:40 and the
        process had been running 20 hours; the logging commit `b9f00bb` is dated
        2026-08-20 00:50, and the whole of `XONHO-0004` (`cd50ed3`…`5906495`)
        also lands after it. Clicking in that window could never have produced a
        log line, because that build has no log writer in it — so the absence of
        failures in the log was evidence about the binary, not about the code.
        A current build was started from `target/debug/caixonho-gui` and did
        create today's file. **The check still needs a person to drive the
        current build into a failure and read it back.**
- [x] 4.4 [dispatch: main] Update `docs/requirements-status.md` and run the
      close-out review in `AGENTS.md`.
      - `docs/requirements-status.md`: both §7–8 rows this change names are
        rewritten from promise to record. Both stay **partial**, and each now
        says what the remainder is: no crash hook, so a panic still leaves
        nothing behind; and the AWS SDK's own output, quiet by default but
        raisable by `CAIXONHO_LOG` to levels carrying request and header
        material that nothing redacts.

      **1. What was asked, or what was convenient?** What was asked. The
      proposal's five bullets all landed: a file in the platform's log
      location, decisions rather than a trace, structural secret exclusion,
      SDK output quiet by default and raisable for one investigation, and a
      bounded file. One deliberate departure, already recorded at 3.1: the
      path is shown in the status bar rather than behind a menu, because the
      moment it is wanted is the moment something has already gone wrong.

      **2. Do the reader-facing documents still tell the truth?** They did
      not, and this is the finding of the review. The change shipped a
      user-visible feature — a log file, its location in the status bar, an
      environment variable — and `README.md`, `docs/architecture.md` and
      `docs/roadmap.md` said nothing about any of it. Fixed here: README's
      "Working today" gains the log; architecture gains `diagnostics` in the
      core map and a section on what is written and what may never be;
      roadmap gains the change in the M1 table. This is the second time this
      exact rule has caught this exact omission, which is an argument for the
      documentation task living in `tasks.md` from the start rather than
      being remembered at close-out.

      **3. Did we leave rubbish?** No. No `TODO`, `FIXME`, `dbg!` or
      `#[allow(dead_code)]` anywhere in `crates/`; `cargo clippy --workspace
      --all-targets` is clean on this project's own code. Its one warning is
      `block v0.1.6`, reached through `cocoa` → `gpui` at the pinned zed
      commit — upstream, macOS-only, and movable only by bumping the UI
      stack, which ADR-0001 makes a change of its own.

      **4. What is asserted but not verified?** The headline gap: **no real
      failure has ever been read back out of this log.** 4.3 explains why it
      had not happened — the window open on this machine was a build older
      than the logging code — but the check itself is still owed, and until
      it is paid the claim that the log explains a failure rests on unit
      tests and one success-path event. Also unverified: nothing exercises
      the roll at 4 MiB against a file that actually reaches 4 MiB; the
      Windows log location is asserted through `directories` rather than
      observed, since CI builds on Windows but no test reads a path there.

      **5. What is left, and where is it written?** The crash hook and the
      SDK-verbosity redaction gap are both in `docs/requirements-status.md`
      as the named remainder of two partial requirements. The live failure
      read-back stays open as 4.3 in this file. The `block` future-rejection
      warning goes to `docs/planned-changes.md` beside the UI-stack bump.
