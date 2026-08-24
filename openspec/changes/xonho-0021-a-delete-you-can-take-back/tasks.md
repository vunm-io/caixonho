# Tasks — XONHO-0021 a delete you can take back

> Dependency order as before: port, then session, then window, then
> paperwork. Core is TDD (`AGENTS.md` §7). The red tests that matter most
> here are the *negative* ones — nothing deletes without the second act, and
> Undo is offered only on proof.

## 1. The port deletes, and can take one back

- [ ] 1.1 `ObjectStore::delete_object` and `remove_marker`, with the double
      [dispatch: main]
  - Paths: `crates/caixonho-core/src/store.rs`
  - Done criteria: `delete_object(bucket, key) -> Result<Deleted>` where
    `Deleted { marker: Option<String> }` carries the marker's version id
    exactly when the service reported one; `remove_marker(bucket, key,
    version_id) -> Result<()>`. Double scripts: unversioned (no marker),
    versioned (a marker id), delete refused (`s3:DeleteObject`), marker
    removal refused (`s3:DeleteObjectVersion`), and it records the version
    id `remove_marker` was called with so a test can assert the *right*
    marker was removed. Red first.
  - Verification: `cargo test -p caixonho-core store::`

- [ ] 1.2 The adapter maps the pair to `DeleteObject` [dispatch: main]
  - Paths: `crates/caixonho-core/src/adapter.rs`
  - Done criteria: `delete_object` reads `delete_marker`/`version_id` off
    the response; `remove_marker` issues `DeleteObject` with `.version_id()`;
    both classify failures through the existing classifier with their own
    IAM actions (`s3:DeleteObject`, `s3:DeleteObjectVersion`); learned-region
    addressing shared with the other object paths. One `#[ignore]`d live
    test in the env-var pattern: delete a probe object on a **versioned**
    bucket, assert the marker is reported, undo, assert the object lists
    again — the only place marker semantics can be observed.
  - Verification: `cargo clippy --workspace --all-targets -- -D warnings`,
    `cargo test -p caixonho-core`

## 2. The session

- [ ] 2.1 `Session::spawn_delete` and `spawn_undo_delete` [dispatch: main]
  - Paths: `crates/caixonho-core/src/session.rs`,
    `crates/caixonho-core/src/diagnostics.rs`
  - Done criteria: both follow the established contract (deliver exactly
    once, on a runtime thread; through the installed store). Outcomes:
    `DeleteOutcome::Gone { marker: Option<String> } | Failed(Error)` and the
    undo's `Restored | Failed(Error)`. `diagnostics::delete_settled` /
    `undo_settled`: bucket, marker-involved, outcome, cause — **no key**,
    asserted with `assert_undisclosed` at the most detailed level like both
    transfer directions.
  - Verification: `cargo test -p caixonho-core`, including the no-key log
    test

## 3. The window

- [ ] 3.1 Delete action and the named-key confirmation [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a Delete action enabled only on an object selection (the
    Open/Download gating), rendered apart from the three benign verbs; it
    sets a `deleting: Option<…>` state (its own field — not `confirming`,
    whose `String` means a connection) and the confirmation strip names the
    key in strong wording with Delete/Cancel. Only the confirmation issues
    the delete; dismissing clears. Selectors: `delete-action`,
    `delete-confirm`, `delete-cancel`.
  - Verification: `cargo test -p caixonho-gui` — including the negative
    test: invoking Delete then dismissing leaves no spawn and no outcome
  - Note: nothing on any key shortcut, and nothing on double-click —
    consistent with the owner's 2026-08-24 decision that destructive-capable
    verbs cost a deliberate click

- [ ] 3.2 The outcome strip, with Undo exactly on proof [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: after success the location re-reads through the existing
    path and the strip says either "gone — permanent" (no marker) or
    "a marker was placed" with **Undo** (marker present); Undo spawns the
    marker removal and its own outcome replaces the strip; a refused undo
    names `s3:DeleteObjectVersion` and does not claim restoration. The
    outcome carries the connection it belongs to and is dropped on location
    change or switch (`XONHO-0019`'s discipline). Both decision states join
    the screenshot harness.
  - Verification: `cargo test -p caixonho-gui`, and
    `cargo test -p caixonho-gui -- --ignored every_state`

## 4. Reader-facing documents, in this change

- [ ] 4.1 README, roadmap, requirements-status [dispatch: main]
  - Paths: `README.md`, `docs/roadmap.md`, `docs/requirements-status.md`
  - Done criteria: README says an object can be deleted, that it takes two
    acts, and that Undo appears exactly when the bucket versions; roadmap
    marks M3's safe subset begun with this change's row; requirements-status
    opens §4.5 as a table the way `XONHO-0007` opened §4.4 — the delete row
    partial (single only, bulk/recursive absent), the delete-marker-undo
    `[S]` row done, the rest none. **Counts by
    `scripts/count-requirements.sh`.**
  - Verification: the script's totals match the tables

## 5. Close-out

- [ ] 5.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [ ] 5.2 CI green on both targets, run id recorded here [dispatch: main]
  - Paths: none
  - Done criteria: all four jobs successful for the tip; run id here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 5.3 Live: delete on both kinds of bucket, and undo one
      [dispatch: main]
  - Paths: none
  - Done criteria: on a real account — a probe object deleted on an
    unversioned bucket (outcome says permanent, no Undo shown); the same on
    a versioned bucket (marker reported, Undo pressed, object listed
    again); a delete refused where credentials cannot delete. What was seen
    written here, names withheld.
  - Verification: the log shows delete and undo outcomes with no keys

- [ ] 5.4 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: this change
  - Done criteria: the five questions answered and recorded here, question
    2 read the wide way.
  - Verification: the recorded findings
