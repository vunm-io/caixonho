# Tasks — XONHO-0019

Commit scope for every task here: `<type>(XONHO-0019): ...`. Branch:
`fix/xonho-0019-a-pane-that-follows-the-connection`.

All paths are relative to the repository root
(`/Users/vunm/Workspaces/vunm-workspace/caixonho`).

## Routing (passdown-dispatch gate, 2026-08-22)

Every task runs on `main`. The three tasks tagged `[dispatch: external-ok]`
keep their tag — the tag records that the work *is* delegable, and the reason
for not delegating it is specific to this sitting, not to the task:

- **1.3 and 2.1** edit the same file and the same invariant chain as 1.2, which
  is `main` work. Handing them out costs more context transfer than the edit
  itself, and both refactors sit next to two traps this repo has already paid
  for (a `#[cfg]` pair that ANDs, and a test that stopped checking what it
  claimed while staying green).
- **4.2** is verification-only. The dispatch contract requires the orchestrator
  to re-run a task's done criteria before ticking it, and here the done
  criteria *are* the commands — so dispatching it buys nothing.

Unproven and worth a probe another day: `agy` is installed (1.1.16), but
whether its sandbox can run `cargo` here is untested. The workspace `AGENTS.md`
records the same failure mode for `flutter` (wrapper writes to an SDK cache
outside the repo); `cargo` wants `~/.cargo/registry`. Probe it on a task whose
verification is not the task, and record the result in `executor notes`.

## 1. A position that carries its connection

- [x] 1.1 Write the failing window test before the fix, and see it red
      [dispatch: main]
      - Dispatched: main (2026-08-22) — done; verified by running it against
        the unfixed code and reading the panic:

        ```
        assertion `left == right` failed: after switching connections the
        window still reports a position, so the trail, the path bar and the
        contents of the previous connection's bucket are all still on screen
          left: Some(Location { bucket: "reports", prefix: Prefix("") })
         right: None
        ```

        The test is `app::tests::switching_connections_ends_the_position`, and
        it drives `select_profile` and `go_to` rather than setting fields — the
        defect is precisely what one of those two forgets, so a test that poked
        the field would have assumed away what it was written to catch. Its
        *first* assertion, the one before the switch, passed: the window really
        was inside the first connection's bucket, so the failure below it is
        about the switch and not about a setup that never arrived.

        One deliberate seam: the test reads position through a `position()`
        helper in the test module rather than touching `app.location` inline.
        Task 1.2 replaces the field with an accessor, and that is a one-line
        change to the helper instead of an edit to the assertions — so the red
        recorded here and the green recorded there are the same test.
  - Paths: `crates/caixonho-gui/src/app.rs` (its `#[cfg(test)]` module)
  - Done criteria: a test builds a window over two connections using the
    `World` seam from `XONHO-0015` (`caixonho-core` feature `test-support`,
    `StoreDouble`), navigates into a bucket on the first connection, selects
    the second, and asserts that the trail, the path text and the object
    contents of the first connection's bucket are no longer shown. Run it
    against the current code and record in this file that it **failed**, with
    the assertion message — a test that has not been seen red has not been
    seen at all (`AGENTS.md` invariant on TDD).
  - Verification: `cargo test -p caixonho-gui` — the new test fails, and no
    other test changes result

- [x] 1.2 Hold the location together with the connection it was read on
      [dispatch: main]
      - Dispatched: main (2026-08-22) — done; verified by 1.1's test going
        green (40 window tests pass, 264 core) **and** by ablation: deleting
        the accessor's `.filter(...)` line puts the test back to the exact
        panic recorded under 1.1. The guard carries the test, which is the
        claim worth checking — this repo has already shipped one test that
        stopped checking what it claimed while staying green.
      - **Correction to design.md.** Its snippet reads
        `Some(p.connection) == self.outcome.active()`. `ActiveOutcome::active`
        returns `ConnectionId`, not `Option<ConnectionId>`
        (`caixonho-core/src/outcome.rs:66`), so the comparison is
        `position.connection == self.outcome.active()`. Recorded rather than
        quietly fixed: the design was written from the shape of
        `active_profile`, which *is* an `Option`, and the two are easy to
        conflate at exactly the place this change is about.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the window's `location: Option<Location>` field is replaced
    by a position that also carries the `ConnectionId` it was read on, and one
    private accessor returns the location only when that id equals
    `self.outcome.active()`. `go_to` records the active id when it sets the
    position; `leave_bucket` clears it. The field itself is not read anywhere
    outside the accessor. `Location` in `caixonho-core` is **not** modified —
    see design.md for why the connection stays out of the addressing form.
  - Verification: `cargo test -p caixonho-gui` — 1.1's test now passes

- [x] 1.3 Move every reader of the position onto the accessor
      [dispatch: external-ok]
      - Dispatched: main (2026-08-22) — done **inside 1.2, not after it**, and
        not by choice: replacing `location: Option<Location>` with
        `position: Option<Position>` stops the crate compiling until every
        reader has moved, so the two tasks are one edit. Splitting the commit
        would have meant inventing a broken intermediate state to commit.
      - Verified by grep rather than by reading: `self.position` appears at
        exactly four sites — the accessor (the only *read*), `go_to`,
        `leave_bucket`, and one write in the test helper `looking_at`. The
        eight former readers (`apply_page`'s guard, the load-more path, the
        prefix-entry path, two in `bucket_group`, the render branch, and the
        two test helpers) all call `location()`.
      - The done criteria named five expected sites and did not anticipate the
        sixth: `looking_at` in the test module *writes* the field to stand the
        window inside a bucket. Left as a write rather than routed through
        `go_to`, because `go_to` also issues a read — a helper that wanted to
        set up a position would have been setting up a network call too.
      - `cargo test --workspace` green: 264 core, 40 window.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: every site that derives the breadcrumb trail, the path bar
    text or the contents pane calls the accessor from 1.2 rather than reading
    the field. Confirm by grep that the field name appears only in its
    declaration, in `go_to`, in `leave_bucket`, in the reset from 2.1 and in
    the accessor.
  - Verification: `cargo test --workspace` stays green, and
    `grep -c '<field-name>' crates/caixonho-gui/src/app.rs` matches the count
    those five sites account for

## 2. One way to end a location

- [x] 2.1 Extract the reset and call it from both the switch and the exit
      [dispatch: external-ok]
      - Dispatched: main (2026-08-22) — done as `end_location`, called by
        `leave_bucket` and by `select_profile` before it issues the new
        listing.
      - **Written before its test, then corrected.** The extraction went in
        first and the suite stayed green — which proved nothing, because the
        read guard from 1.2 already covered the only assertion then existing.
        `AGENTS.md` invariant #7 is TDD, so the extraction was set aside
        (`git checkout` of the one file), 3.1's test written against the code
        *without* it, seen red — `left: 1, right: 0`, the previous
        connection's object still in the contents table — and only then
        restored. Recorded rather than tidied away: the failure mode here is
        writing a belt-and-braces change and letting an unrelated green suite
        pass for it.
      - **The exit path gains behaviour, it is not purely extracted.**
        `leave_bucket` never emptied the objects table; the reset does. No
        visible difference today, because `go_to` clears the table on the way
        in — so this closes a gap that was only ever invisible by luck.
      - `cargo test --workspace` green: 264 core, 42 window.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the reset that `leave_bucket` performs — position cleared,
    `listing` set to `Listing::Idle`, `more` set to `None`, `fetching` set to
    `false`, the objects table emptied — lives in one private method called by
    both `leave_bucket` and `select_profile`. `select_profile` calls it before
    it issues the new listing, so nothing of the previous connection is on
    screen while the new one loads.
  - Verification: `cargo test -p caixonho-gui`

## 3. The two consequences worth a test of their own

- [x] 3.1 Nothing of the previous position survives the loading window
      [dispatch: main]
      - Dispatched: main (2026-08-22) — done as
        `a_switch_leaves_no_contents_behind_while_the_next_account_loads`.
        Seen red before 2.1 was restored:

        ```
        assertion `left == right` failed: the previous connection's objects
        are still in the contents table while the new connection loads
          left: 1
         right: 0
        ```

      - It asserts on the gap on purpose: the second connection's listing is
        never answered in the test, so what it measures is the window the
        reproduction actually hit — sidebar already switched, listing not back
        yet. The page for the first connection is delivered through
        `apply_page` rather than poked into the delegate, so the row it counts
        got there the way a real one does.
      - This is the test that gives 2.1 its teeth. The 1.2 guard stops a stale
        position being *shown*; it leaves it *held*, and the contents table is
        where that shows up.
  - Paths: `crates/caixonho-gui/src/app.rs` (its `#[cfg(test)]` module)
  - Done criteria: a test that selects a second connection whose account
    listing has **not** yet answered, and asserts that what is shown is that
    connection's own pending or empty state — not the previous connection's
    bucket or prefix. This is the case the reproduction actually hit: the
    sidebar had already updated while the pane had not.
  - Verification: `cargo test -p caixonho-gui`

- [x] 3.2 Re-selecting the already-selected connection ends the location too
      [dispatch: main]
      - Dispatched: main (2026-08-22) — done as
        `re_selecting_the_same_connection_also_ends_the_location`.
      - **Green the moment it was written, and that is the correct result.**
        `select_profile` mints a new `ConnectionId` on every call, so the 1.2
        guard already ends the location on a re-select; no code was needed.
        This is a characterisation test, not a red→green one — its job is to
        put an accepted behaviour on the record so a later reader does not
        "fix" it by comparing profile index instead of connection id, which
        would reintroduce the second notion of sameness this change removed.
        Said plainly here because a test that never failed is exactly the kind
        that quietly stops meaning anything.
  - Paths: `crates/caixonho-gui/src/app.rs` (its `#[cfg(test)]` module)
  - Done criteria: a test asserting that clicking the connection that is
    already selected returns to the bucket table. design.md accepts this
    deliberately — that click re-lists the account, so landing on the fresh
    listing is the coherent answer. The test exists so the behaviour is a
    decision on record rather than a side effect someone later "fixes".
  - Verification: `cargo test -p caixonho-gui`

## 4. Close-out

- [x] 4.1 Update the reader-facing documents in this change, not after it
      [dispatch: main]
      - Dispatched: main (2026-08-22) — done; the §4.2 breadcrumb row keeps
        **done** and gains the note, naming `XONHO-0019` and saying in the row
        itself why it did not move to partial.
      - `./scripts/count-requirements.sh` re-run and compared against the
        prose totals on lines 72–73: 11 done, 10 partial, 3 not started, and
        §4.1/§4.2/§4.3 split 2-5-1 / 3-3-1 / 6-2-1. Identical before and after,
        which is the expected result of a note-only edit — checked rather than
        assumed, because this file has drifted twice in one day with the total
        right and the split wrong.
  - Paths: `docs/requirements-status.md`
  - Done criteria: the §4.2 row *"Breadcrumbs plus an editable path bar"* keeps
    its **done** state and gains a note that the trail is now shown only for
    the selected connection, naming `XONHO-0019`. Do not flip the row to
    partial: the requirement was built: this change repairs a defect in it.
    Re-run the counter afterwards — this file has drifted twice in one day
    before, both times with the total right and the split wrong.
  - Verification: `./scripts/count-requirements.sh` agrees with the tables

- [x] 4.2 `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
      [dispatch: external-ok]
      - Dispatched: main (2026-08-22) — both exit zero. `fmt` touched two
        places, both mine: a `let ... else` I had wrapped by hand, and an
        import out of order. `clippy` had nothing to say; its only output is
        the pre-existing future-incompat notice for `block v0.1.6`, which is a
        dependency notice and not a lint.
      - Suite re-run *after* formatting rather than before: 264 core, 42
        window, green.
  - Paths: whatever the tools change
  - Done criteria: both commands exit zero with no diff left unstaged
  - Verification: the two commands

- [ ] 4.3 CI green on both targets [dispatch: main]
  - Paths: none
  - Done criteria: the workflow's `cargo test --workspace` passes on macOS and
    on Windows. Record the run id here, and the test counts on both, so the
    next change can tell a real gap from an expected one.
  - Verification: the CI run, cited by id

- [ ] 4.4 Sync this delta only after `XONHO-0006` archives [dispatch: main]
  - Paths: `openspec/changes/xonho-0019-a-pane-that-follows-the-connection/specs/object-browsing/spec.md`
  - Done criteria: `object-browsing` has no file in `openspec/specs/` yet — it
    is introduced by `XONHO-0006`, open at 19/20 with a live check as its last
    task. This change's delta modifies a requirement that only exists inside
    that change, so `/opsx:sync` here before `XONHO-0006` archives would write
    a modification of nothing. Confirm `openspec/specs/object-browsing/spec.md`
    exists before syncing; if `XONHO-0006` is still open when this change is
    otherwise finished, leave this task unticked and say so in the handoff
    rather than syncing early.
  - Verification: `ls openspec/specs/object-browsing/spec.md` succeeds, then
    `openspec validate` passes

- [ ] 4.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: none
  - Done criteria: the review in `AGENTS.md` is run and its findings recorded
    here — including, explicitly, whether any claim in proposal.md or design.md
    turned out to be wrong once the code was written. Two of the last three
    changes found a repo document asserting something false; the review is
    where that gets caught rather than inherited.
  - Verification: the findings are written in this file
