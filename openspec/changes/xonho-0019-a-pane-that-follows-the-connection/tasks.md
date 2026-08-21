# Tasks — XONHO-0019

Commit scope for every task here: `<type>(XONHO-0019): ...`. Branch:
`fix/xonho-0019-a-pane-that-follows-the-connection`.

All paths are relative to the repository root
(`/Users/vunm/Workspaces/vunm-workspace/caixonho`).

## 1. A position that carries its connection

- [ ] 1.1 Write the failing window test before the fix, and see it red
      [dispatch: main]
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

- [ ] 1.2 Hold the location together with the connection it was read on
      [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the window's `location: Option<Location>` field is replaced
    by a position that also carries the `ConnectionId` it was read on, and one
    private accessor returns the location only when that id equals
    `self.outcome.active()`. `go_to` records the active id when it sets the
    position; `leave_bucket` clears it. The field itself is not read anywhere
    outside the accessor. `Location` in `caixonho-core` is **not** modified —
    see design.md for why the connection stays out of the addressing form.
  - Verification: `cargo test -p caixonho-gui` — 1.1's test now passes

- [ ] 1.3 Move every reader of the position onto the accessor
      [dispatch: external-ok]
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

- [ ] 2.1 Extract the reset and call it from both the switch and the exit
      [dispatch: external-ok]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the reset that `leave_bucket` performs — position cleared,
    `listing` set to `Listing::Idle`, `more` set to `None`, `fetching` set to
    `false`, the objects table emptied — lives in one private method called by
    both `leave_bucket` and `select_profile`. `select_profile` calls it before
    it issues the new listing, so nothing of the previous connection is on
    screen while the new one loads.
  - Verification: `cargo test -p caixonho-gui`

## 3. The two consequences worth a test of their own

- [ ] 3.1 Nothing of the previous position survives the loading window
      [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs` (its `#[cfg(test)]` module)
  - Done criteria: a test that selects a second connection whose account
    listing has **not** yet answered, and asserts that what is shown is that
    connection's own pending or empty state — not the previous connection's
    bucket or prefix. This is the case the reproduction actually hit: the
    sidebar had already updated while the pane had not.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 3.2 Re-selecting the already-selected connection ends the location too
      [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs` (its `#[cfg(test)]` module)
  - Done criteria: a test asserting that clicking the connection that is
    already selected returns to the bucket table. design.md accepts this
    deliberately — that click re-lists the account, so landing on the fresh
    listing is the coherent answer. The test exists so the behaviour is a
    decision on record rather than a side effect someone later "fixes".
  - Verification: `cargo test -p caixonho-gui`

## 4. Close-out

- [ ] 4.1 Update the reader-facing documents in this change, not after it
      [dispatch: main]
  - Paths: `docs/requirements-status.md`
  - Done criteria: the §4.2 row *"Breadcrumbs plus an editable path bar"* keeps
    its **done** state and gains a note that the trail is now shown only for
    the selected connection, naming `XONHO-0019`. Do not flip the row to
    partial: the requirement was built: this change repairs a defect in it.
    Re-run the counter afterwards — this file has drifted twice in one day
    before, both times with the total right and the split wrong.
  - Verification: `./scripts/count-requirements.sh` agrees with the tables

- [ ] 4.2 `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
      [dispatch: external-ok]
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
