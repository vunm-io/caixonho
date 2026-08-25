# Tasks — XONHO-0021 a delete you can take back

> Dependency order as before: port, then session, then window, then
> paperwork. Core is TDD (`AGENTS.md` §7). The red tests that matter most
> here are the *negative* ones — nothing deletes without the second act, and
> Undo is offered only on proof.

## 1. The port deletes, and can take one back

- [x] 1.1 `ObjectStore::delete_object` and `remove_marker`, with the double
      [dispatch: main]
      - Done in `main` (2026-08-25), red first (compile-red: the trait grew
        two methods and every implementor had to answer). The double records
        every version id `remove_marker` receives, so the undo test asserts
        the *right* marker was removed rather than merely that something
        was. `marker_removal_refused` scripts the asymmetric grant — allowed
        to delete, not to un-delete — which is a real IAM shape.
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

- [x] 1.2 The adapter maps the pair to `DeleteObject` [dispatch: main]
      - Done in `main` (2026-08-25). The marker is `Some` only on the
        **pair** — `delete_marker == true` *and* a version id — because some
        services set `version_id` on answers that are not markers, and half
        the proof is not proof. Failures classify through a shared
        `mutation_failure` helper (put's inline classify refactored into it)
        with each verb's own IAM action. Live test
        `this_machine_deleting_and_taking_it_back` seeds conditionally so it
        cannot clobber, and leaves the probe object behind on purpose —
        cleanup would use the verb under test to destroy the run's evidence.
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

- [x] 2.1 `Session::spawn_delete` and `spawn_undo_delete` [dispatch: main]
      - Done in `main` (2026-08-25). One deliberate narrowing surfaced here
        rather than absorbed: **no `Cancel` on a delete.** It is one
        request, and a cancel that raced it would leave the user unsure
        whether the object exists — worse than the moment of waiting. The
        spec never promised one (its cancel requirement is `object-transfer`'s,
        not this capability's).
      - The undo test asserts the exact marker (`markers_removed()`), and
        the refused-undo test pins that a failure does not claim
        restoration. The no-key log test covers delete **and** undo lines in
        one recording.
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

- [x] 3.1 Delete action and the named-key confirmation [dispatch: main]
      - Done in `main` (2026-08-25). The button wears the danger colour and
        sits after the three benign verbs; it only ever sets `Confirming`.
        Window tests pin the two-act rule from both ends: dismissing leaves
        nothing (and nothing was spawned — only `confirm_delete` spawns),
        and the action refuses folders and empty selections outright.
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

- [x] 3.2 The outcome strip, with Undo exactly on proof [dispatch: main]
      - Done in `main` (2026-08-25). Undo renders only on
        `Gone { marker: Some }` and `undo_delete` re-checks the proof before
        spawning. The re-read is the same `go_to` the navigation uses —
        observable in the test as the listing dropping to `Loading` — and
        `go_to` learned the one distinction this needed: navigating *away*
        drops the strip, re-reading the *same* location keeps it, because
        the strip's own outcome is what triggers that re-read.
      - The stale-connection guard drops the whole outcome (above all its
        Undo) when the event's deletion belongs to a connection no longer
        active; a failed undo speaks the undo's words (`during_undo`) and
        claims no restoration. Clippy caught a `marker` field on `Restoring`
        that nothing read — removed rather than kept: a field nobody reads
        is how retry-shaped ideas sneak in unreviewed.
      - Both decision states are in the screenshot harness: 18 images.
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

- [x] 4.1 README, roadmap, requirements-status [dispatch: main]
      - Done in `main` (2026-08-25). README's bullet leads with the two-act
        rule and the proofed Undo; roadmap opens an "M3 so far" table in the
        commit that makes it true; requirements-status opens §4.5 with four
        rows — the three `[M]` plus the marker-undo `[S]`, listed because it
        is built and a done `[S]` unlisted would make the section read
        emptier than it is. The single-object delete row is **partial** with
        the counted confirmation explicitly assigned to bulk, where a
        mistake multiplies. Counts by the script.
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

- [x] 5.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-25): fmt clean via `--check`, clippy zero at
        `-D warnings`, 319 core + 58 window green (7 + 1 ignored, the live
        and screenshot ones).
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [x] 5.2 CI green on both targets, run id recorded here [dispatch: main]
      - Run `32793898910` on `309b837` (this change's final code commit):
        `build (windows-latest)`, `build (macos-latest)`, `dependency audit`
        and `rustfmt` all success.
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

- [x] 5.4 Close-out review per `AGENTS.md` [dispatch: main]
      - Run 2026-08-25, before the live check as with the last two changes.
      - **Q4, three catches, all now tests:** `confirm_delete`'s transition
        was driven by nothing (the two-act rule's *second* act was asserted
        in prose only — and its from-any-other-phase no-op matters because
        the state machine must not rely on the render layer for the rule);
        `undo_delete`'s no-proof guard likewise; and the `go_to`
        conditional — strip survives a same-location re-read, departs on
        navigation — which was the subtlest line in the change and had no
        assertion. All three green after the state machine already worked,
        which is the honest note: these are guards against regression, not
        red-first design tests, and are recorded as such.
      - **Q3:** clippy's dead-field catch (`Restoring { marker }`) was
        already acted on in 3.2; nothing else unused — every new pub item
        has non-test callers or is the port double's scripting surface.
      - **Q1 departures:** one, surfaced in 2.1's note rather than
        absorbed — no `Cancel` on the delete spawn, because a cancel racing
        a one-request destructive call leaves the user unsure whether the
        object exists. The spec's cancel requirement belongs to
        `object-transfer`, not this capability, so no spec edit was needed.
      - **Q2 both directions:** README/roadmap/requirements written in the
        commits that made them true (4.1). Rows either side: `XONHO-0007`
        and `XONHO-0020` both still correctly *awaiting live acceptance*;
        M1/M2 tables untouched by this change.
      - **Q5:** what remains is written where it will be found — 5.3's live
        check in this file, and the marker semantics' only observable venue
        (`this_machine_deleting_and_taking_it_back`) documented on the test
        itself. Nothing lives only in conversation.
      - **Q4 residue for 5.3:** marker semantics against a real endpoint
        (the double's markers prove nothing about the service's), the
        listing re-read against real latency, and R2's versioning behaviour
        — R2 supports it, but "supports" is a docs claim until driven.
  - Paths: this change
  - Done criteria: the five questions answered and recorded here, question
    2 read the wide way.
  - Verification: the recorded findings
