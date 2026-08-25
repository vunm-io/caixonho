# Tasks — XONHO-0023 a connection worth keeping

> Core, so TDD (`AGENTS.md` §7). The tests that carry this one are counts
> again — "built once" is a number — plus one that is a *non*-change: the
> `XONHO-0019` window tests must pass untouched, and if they do not, the
> design is wrong rather than the tests.

## 1. Counting what gets built

- [x] 1.1 Make building observable to a test [dispatch: main]
      - Done in `main` (2026-08-26). **No second counter was needed**, and
        the reason is worth writing down: `XONHO-0022`'s read counter works
        for this, but only *bare* — wrapped in `Remembering`, a read counts
        runs rather than builds. Straight through, building a stored
        connection is the only thing that asks the credential store, so a
        read **is** a build. `bare_store_holding` + `builds()` are those two
        sentences in code.
      - The counting tests therefore all use `Stored` sources. That is not a
        gap: the reuse branch sits *above* the profile/stored split in
        `Session::open`, so both kinds go through the same line.
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

- [x] 2.1 `Session::open` reuses what it built [dispatch: main]
      - Done in `main` (2026-08-26), four tests, all four ablated.
      - `Connection::with_id` is the one addition beyond the plan, and it is
        the design's central claim made mechanical: reuse clones the resolved
        configuration and **replaces the id**, so a kept client can never
        make a stale outcome look current.
      - Keyed in a `Vec<(ConnectionSource, Connection)>` rather than a map.
        `ConnectionSource` carries a `StoredCredential` and is only `Eq`;
        deriving `Hash` down that chain to save a comparison over the handful
        of connections one person visits in a run would be paying in public
        API for nothing.
      - **Ablations, and what each one proved:**
        - never reuse → 3 red (`builds_it_once`, `does_not_rebuild_it`,
          `network_is_kept`);
        - forget on *any* failure, not just credential ones → 1 red
          (`network_is_kept`) — this is the half that would have quietly
          undone the change, and it now has a guard;
        - reuse the kept id instead of minting a new one → 1 red
          (`a_reused_connection_is_still_a_new_selection`), and **exactly
          one** across all 349 core tests. The id guarantee has a single
          guard; that is worth knowing before anyone edits this line.
      - `a_connection_that_failed_to_open_is_built_again` is the one test
        with **no one-line ablation** — there is no `Connection` to cache
        when open fails, so caching failure is not reachable by mutating a
        line. It guards an ordering that is easy to get wrong (pushing before
        the `?` rather than after) and a design direction, and it is recorded
        as a guard rather than claimed as load-bearing.
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

- [x] 2.2 A sign-in drops what was kept for that source [dispatch: main]
      - Done in `main` (2026-08-26), and **wider than the design said** —
        a departure, made deliberately. The design said "drops that source's
        entry". One Identity Center session serves *every* profile pointing
        at it, so a sign-in can revive several connections at once; dropping
        only the one the user was looking at would leave its siblings holding
        a client built when there was no session. So a successful sign-in
        clears everything kept.
      - Wired **before** `deliver`, because `deliver` is what makes the
        frontend retry (`app.rs:2031` calls `retry` on a session outcome).
        After it, the retry would race the forget.
      - Two mechanisms, not one, and they overlap on purpose: a credential
        failure at the listing already drops the entry, and the sign-in offer
        only appears *after* such a failure. The sign-in clear is the belt to
        that braces — it keeps the guarantee from depending on the frontend's
        flow order.
      - Ablated: `forget_opened_connections` made a no-op → 1 red.
      - The network half of `spawn_sign_in` is not unit-testable here, so the
        test drives the seam directly. Named rather than papered over.
  - Paths: `crates/caixonho-core/src/session.rs`
  - Done criteria: completing a sign-in for a source removes that source's
    entry, so the next selection builds with the session that now exists.
    Test: a source is opened (kept), a sign-in for it completes, the next
    open builds again.
  - Verification: `cargo test -p caixonho-core session::`

## 3. The guarantee that must not move

- [x] 3.1 `XONHO-0019`'s tests pass unmodified [dispatch: main]
      - Done in `main` (2026-08-26). **`git diff --stat` shows zero lines
        changed under `crates/caixonho-gui/`** — not one window test edited,
        which is the falsifiable form the plan asked for. 63 window tests
        green.
      - The three that speak directly to this seam and passed untouched:
        `switching_connections_ends_the_position`,
        `re_selecting_the_same_connection_also_ends_the_location`, and
        `an_outcome_from_a_left_connection_is_dropped`. The second is the
        interesting one — re-selecting the *same* connection is now the
        reuse path, and it still ends the location.
  - Paths: none — this task changes nothing on purpose
  - Done criteria: the window tests that came with `XONHO-0019` (the pane
    that cannot outlive its connection) run **untouched** and green. Record
    here that they were not edited. If any needed editing, stop: that is
    the design being wrong, and it is a finding rather than a fix.
  - Verification: `cargo test -p caixonho-gui`, and `git diff` showing no
    change to those tests

## 4. Close-out

- [x] 4.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-26): fmt exit 0, clippy exit 0 at
        `-D warnings`, 349 core + 63 window green (8 + 1 ignored).
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [x] 4.2 CI green on both targets, run id recorded here [dispatch: main]
      - Run `32876575105` on `0dcca25`: `build (windows-latest)`,
        `build (macos-latest)`, `dependency audit` and `rustfmt` all
        success. (A second run follows for the observability fix in 4.6.)
  - Paths: none
  - Done criteria: all four jobs successful for the tip; run id here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 4.3 Live: the second click is fast [dispatch: main]
      - **First sitting, 2026-08-26: inconclusive, and the fault was the
        instrument's.** The owner reported no improvement. Two things came
        out of the log, and only one of them is about speed.
      - The binary was checked first (`ps -o lstart` 00:16:10, bundle mtime
        00:16:10, commit 00:12:59) — the running app **did** carry the
        change, so this was not a stale build.
      - **The measurement this task asks for had been made impossible by the
        change itself.** See 4.6. Reused selections wrote no log line at all,
        so "read the open→listed durations out of the log" had no durations
        to read for exactly the selections under test.
      - What could still be read, from the 17:15:38Z run: seven selections
        listed with **no `connection opened` line before them** — those were
        reuses, and they are the first direct evidence the branch fires. The
        first `vunm` click took 8.75s to list; consecutive later listings
        landed 0.6–1.7s apart, human clicking included. Suggestive, not a
        measurement.
      - **And the sitting did not exercise the claim.** The owner's most
        recent run clicked two connections, both for the first time — both
        necessarily built, both necessarily slow. The claim is about the
        *second* click on the *same* connection. Worth saying plainly rather
        than reading "no improvement" as a refutation of something that was
        not tested.
      - Re-run with the instrumented build (binary 00:20:36) before drawing
        any conclusion.
  - Paths: none
  - Done criteria: on the owner's machine, select a `credential_process`
    profile (`vunm` or `r2-caixonho`), then another, then the first again,
    and read the open→listed durations out of the log. Expected: **first
    ~4s, later ones in the tenths.** If the second is still seconds, the
    identity cache is not where the time goes and the design is wrong about
    the cause — record that, because it is the more valuable outcome.
  - Verification: the log's own timings, before and after, quoted here

- [ ] 4.4 Reader-facing documents [dispatch: main]
      - **Prose done 2026-08-26; the numbers wait on 4.3.** The §4.1 row,
        the roadmap rows (including `XONHO-0022`'s, which was missing) and
        the outcome section under the 2026-08-25 timing note are all written.
        The after-column in that table says *awaiting the live sitting* and
        stays that way until 4.3 produces real numbers — which is why this
        box is not ticked.
  - Paths: `docs/requirements-status.md`, `docs/planned-changes.md`
  - Done criteria: the §4.1 multiple-connections row notes that a
    connection is built once per run; the 2026-08-25 timing note in
    `planned-changes.md` gets its outcome written under it, with the
    after-numbers beside the before-numbers. **Counts by the script.**
  - Verification: the script's totals match the tables

- [x] 4.6 The log can still see a selection [dispatch: main]
      - Done in `main` (2026-08-26), and it is a **defect this change
        introduced**, found by the owner's live check and by no test.
      - `connection opened` is written inside `connection::open`, which reuse
        skips. So half the selections went silent: the log showed a listing
        for a connection it never showed being chosen. A reader — and anyone
        trying to measure whether this change worked — could no longer tell a
        fast connection from one that was never clicked.
      - Two fixes, and the second matters more:
        - `connection reused` is now its own line, mirroring
          `connection opened`;
        - **`listed the account` and `listing failed` now carry `took`**,
          measured from before the open. Subtracting two log lines was never
          right even before this change: credentials resolve *lazily*, inside
          the listing, so `connection opened` was already being written
          before the expensive part happened. The old "open → listed" numbers
          in `docs/planned-changes.md` were measuring the listing *including*
          credential resolution — the right quantity, reached by accident.
      - **Q4 should have caught this and did not.** The close-out review
        asked what was asserted but not verified and named three gaps; none
        was "this change removes the only signal the live check reads from".
        Recorded because the review's blind spot is worth more than the fix.
      - Paths: `crates/caixonho-core/src/diagnostics.rs`,
        `crates/caixonho-core/src/session.rs`
      - Verification: `cargo test -p caixonho-core diagnostics -- --ignored`
        (the test that writes to this machine's real log directory), green,
        asserting on `connection reused` and `took=1.5`

- [x] 4.5 Close-out review per `AGENTS.md` [dispatch: main]
      - Run 2026-08-26, before the live check, as with the last five — and
        **the live check then found something the review had missed**, written
        up in 4.6 and amending Q4 below.
      - **Q1: one departure, written into the document it departs from.**
        `design.md` said a sign-in drops *that source's* entry; the code
        drops everything kept. One Identity Center session serves every
        profile pointing at it, so a sign-in can revive several connections
        at once and the narrow version would leave siblings holding a client
        built when there was no session. Recorded in task 2.2 and in the
        method's own doc comment rather than left as a silent widening.
      - **Q1, the other direction — what was *not* built:** the proposal's
        original claim that observations should survive a round trip. Reading
        `capability-awareness` at planning time killed it, and nothing here
        quietly reinstated it. `a_reused_connection_is_still_a_new_selection`
        is the guard.
      - **Q2, and it caught something belonging to a different change.**
        `XONHO-0022` **had no roadmap row at all** — it landed 2026-08-25 and
        the table went straight from `XONHO-0019` to nothing. Exactly the
        failure mode `AGENTS.md` describes for this question: a cell nobody
        is scheduled to look at. Added, along with this change's own row.
        `requirements-status.md` §4.1's multiple-connections row now says a
        client is built once per run and **stays partial** — one connection
        on screen at a time is unchanged, and moving the row would have been
        the drift. Counts by the script: unmoved, as expected.
        `README.md`, `docs/architecture.md` and `docs/design-language.md`
        checked and say nothing this contradicts — nothing a user can see is
        different except that a click stops being slow.
      - **Q3: nothing left behind.** `Connection::with_id` has exactly one
        caller and is the design's claim in code, not API kept for later.
        `forget_if_credentials_failed` and `forget_opened_connections` are
        each called from production, not only from tests — checked by grep,
        not assumed. No new constants, no `TODO`s, no scripts.
      - **Q4, the honest gap, and it is the same shape as last time.** Every
        assertion here counts reads of a *double*. That a real
        `credential_process` therefore runs once per run is a claim no unit
        test can make — 4.3 exists for it, and it is written so that "the
        second click is still slow" is a recorded finding rather than a
        silent disappointment. Two more, named:
        - the `spawn_sign_in` wiring is verified by reading one line, because
          its other half is a network round trip;
        - `a_connection_that_failed_to_open_is_built_again` has no one-line
          ablation (there is no `Connection` to cache when open fails), so it
          is a guard against a design direction rather than a load-bearing
          assertion. Both said out loud in tasks 2.1 and 2.2.
      - **Q4, the assumption underneath everything:** that `aws-config`'s
        lazy identity cache is what makes reuse pay. That was read before the
        design was written rather than after (`lib.rs:1048`), but reading a
        default is not measuring one. 4.3 is the measurement.
      - **Q4, amended after the fact:** the review named three unverified
        things and missed the one that bit. It never asked what the change
        did to the *instrument* the live check depends on. It removed it:
        reuse skipped the line that announced a selection. A review that asks
        "what is asserted but not verified" should also ask "what did this
        change do to the evidence".
      - **Q5:** the `planned-changes.md` timing note now carries its own
        outcome, with an after-column marked *awaiting the live sitting* and
        the falsifiable expectation beside it. Nothing else was discovered
        and left in a transcript.
  - Paths: this change
  - Done criteria: the five questions answered and recorded here, question
    2 read the wide way.
  - Verification: the recorded findings
