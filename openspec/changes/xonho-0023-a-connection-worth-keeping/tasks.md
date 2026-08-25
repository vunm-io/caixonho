# Tasks — XONHO-0023 a connection worth keeping

> Core, so TDD (`AGENTS.md` §7). The tests that carry this one are counts
> again — "built once" is a number — plus one that is a *non*-change: the
> `XONHO-0019` window tests must pass untouched, and if they do not, the
> design is wrong rather than the tests.

## 1. Counting what gets built

- [ ] 1.1 Make building observable to a test [dispatch: main]
  - Paths: `crates/caixonho-core/src/session.rs`,
    `crates/caixonho-core/src/connection.rs`
  - Done criteria: something a test can read that says how many times a
    connection was *built* for a given source, distinct from how many times
    it was selected. The credential store's read counter (`XONHO-0022`) is
    the nearest existing instrument and may be enough on its own for
    stored-credential sources — if it is, use it and say so here rather
    than adding a second counter for the same fact.
  - Verification: `cargo test -p caixonho-core session::`

## 2. Reuse

- [ ] 2.1 `Session::open` reuses what it built [dispatch: main]
  - Paths: `crates/caixonho-core/src/session.rs`
  - Done criteria: a map from `ConnectionSource` to the built connection;
    `open` returns the kept one when there is one and builds otherwise.
    **Everything `open` resets today still resets** — `credentials_changed`,
    the scheduler slot, the store — and the store is re-installed over the
    reused connection exactly as over a fresh one. A failed open puts
    nothing in the map. Red first, and the red tests are:
    - selecting the same source twice builds once;
    - A → B → A builds each of A and B once, not A twice;
    - a source whose open failed is built again on the next selection;
    - a reused selection still mints a **new** `ConnectionId` and still
      clears observations, because those are what `XONHO-0019` and
      `capability-awareness` rest on.
  - Verification: `cargo test -p caixonho-core session::`

- [ ] 2.2 A sign-in drops what was kept for that source [dispatch: main]
  - Paths: `crates/caixonho-core/src/session.rs`
  - Done criteria: completing a sign-in for a source removes that source's
    entry, so the next selection builds with the session that now exists.
    Test: a source is opened (kept), a sign-in for it completes, the next
    open builds again.
  - Verification: `cargo test -p caixonho-core session::`

## 3. The guarantee that must not move

- [ ] 3.1 `XONHO-0019`'s tests pass unmodified [dispatch: main]
  - Paths: none — this task changes nothing on purpose
  - Done criteria: the window tests that came with `XONHO-0019` (the pane
    that cannot outlive its connection) run **untouched** and green. Record
    here that they were not edited. If any needed editing, stop: that is
    the design being wrong, and it is a finding rather than a fix.
  - Verification: `cargo test -p caixonho-gui`, and `git diff` showing no
    change to those tests

## 4. Close-out

- [ ] 4.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [ ] 4.2 CI green on both targets, run id recorded here [dispatch: main]
  - Paths: none
  - Done criteria: all four jobs successful for the tip; run id here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 4.3 Live: the second click is fast [dispatch: main]
  - Paths: none
  - Done criteria: on the owner's machine, select a `credential_process`
    profile (`vunm` or `r2-caixonho`), then another, then the first again,
    and read the open→listed durations out of the log. Expected: **first
    ~4s, later ones in the tenths.** If the second is still seconds, the
    identity cache is not where the time goes and the design is wrong about
    the cause — record that, because it is the more valuable outcome.
  - Verification: the log's own timings, before and after, quoted here

- [ ] 4.4 Reader-facing documents [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/planned-changes.md`
  - Done criteria: the §4.1 multiple-connections row notes that a
    connection is built once per run; the 2026-08-25 timing note in
    `planned-changes.md` gets its outcome written under it, with the
    after-numbers beside the before-numbers. **Counts by the script.**
  - Verification: the script's totals match the tables

- [ ] 4.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: this change
  - Done criteria: the five questions answered and recorded here, question
    2 read the wide way.
  - Verification: the recorded findings
