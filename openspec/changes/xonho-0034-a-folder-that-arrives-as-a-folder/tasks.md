# Tasks — XONHO-0034 a folder that arrives as a folder

> The measure of this change is a gesture that used to be impossible: one
> click that fetches a subtree, and the subtree being a subtree when it lands.
> It is finished when a folder downloaded from a real service can be compared
> to the bucket byte for byte and path for path.
>
> **Routing.** All `[dispatch: main]`. The core half is a pure function with
> property tests under `ADR-0004`'s injectivity rule, and the window half is
> the act — both are judgement about existing decisions rather than
> specification-following, which is what `agy` is good at. `agy` earns nothing
> here; GPUI and Rust are not the frontend the routing rule hands over.

## 1. A key becomes a path

- [x] 1.1 `local_path`, beside `local_name` and by its rules [dispatch: main]
  - Paths: `crates/caixonho-core/src/transfer.rs`
  - Done criteria: `local_path(key, under) -> MappedPath` returning the
    relative path and the worst `MappingOutcome` across segments. Every
    segment goes through the existing per-segment logic — extracted and shared,
    not copied, so the two can never drift.
  - `under` is the prefix the act began at; the path is what remains below it.
  - Verification: `cargo test -p caixonho-core local_path`
  - **Done.** `map_segment(segment, key)` extracted from `local_name`, which
    now calls it — shared, not copied. `local_path(key, under) -> MappedPath`
    walks the segments below `under` and reports the worst outcome;
    `MappingOutcome` gained `Ord` so "worse" is a comparison rather than a
    match arm that will be forgotten.

- [x] 1.2 A middle segment that cannot serve [dispatch: main]
  - Paths: `crates/caixonho-core/src/transfer.rs`
  - Done criteria: an empty segment, `.`, `..`, a Windows reserved device stem
    and an overlong segment each take the deterministic suffix — FNV-1a over
    the **whole key**, exactly as the last segment does. `a/../b` becomes two
    ordinary directory names and never a movement.
  - **This is the case `ADR-0004` never had**, and the reason the ADR is
    amended rather than cited.
  - Verification: a test per case, named for it
  - **Done.** `.` and `..` never reach the suffix path — their trailing dot is
    encoded first, so `a/../b/x.txt` keeps four components and carries no
    parent reference. Empty and reserved-device middle segments take the
    suffix, tested one case per test.

- [x] 1.3 The property `ADR-0004` exists for, widened [dispatch: main]
  - Paths: `crates/caixonho-core/src/transfer.rs`
  - Done criteria: a property test over generated keys asserting **two distinct
    keys never produce the same path**. The existing scheme's injectivity is
    what makes this true; the test is what keeps it true when someone edits the
    segment rules.
  - Verification: the property test, and an ablation — break the `%` encoding
    and watch it fail
  - **Done, and it found something.** The property was written as *no two
    distinct keys share a path* and failed on a pair the filesystem itself
    cannot separate: the folder marker `a/x.txt/` and the object `a/x.txt`.
    One wants to be a directory and one a file, of one name. No mapping can fix
    that, so the invariant was narrowed to **object keys** and the reason
    written where the test is — a marker colliding with an object is a
    destination conflict, which is where `ADR-0004` already routes what a pure
    function cannot see. A separate test pins what a marker maps to.
  - **Ablation run.** Removing `%` from the refused set makes the property fail
    with `12%3A30.log` and `12:30.log` colliding — the exact pair the ADR's
    encoding exists to keep apart. Restored, green.

- [ ] 1.4 Amend `ADR-0004` [dispatch: main]
  - Paths: `docs/adr/0004-key-to-filename-scheme.md`
  - Done criteria: an amendment section dated and signed to `XONHO-0034`,
    stating the unit moved from a filename to a relative path, what a broken
    middle segment does, and that the collision class the ADR listed as
    unsolved — `a/x.txt` against `b/x.txt` — is resolved by structure rather
    than by asking the user.
  - The original decision text is not rewritten. An ADR records what was
    decided when; an amendment records what changed and why.
  - Verification: the file, read against the original decision

## 2. The walk, without a ceiling

- [ ] 2.1 A walk a download may use [dispatch: main]
  - Paths: `crates/caixonho-core/src/session.rs`
  - Done criteria: gathering every key under a prefix with no `TooMany`
    refusal, while `spawn_walk_under`'s existing behaviour — and the 5,000
    ceiling that guards a delete — is untouched. Whether that is a parameter or
    a sibling is decided when both are written; the smaller diff wins and the
    reason is recorded here.
  - Verification: `cargo test -p caixonho-core`, and the delete flow's existing
    `TooMany` test still passing

- [ ] 2.2 Prove it against a real service [dispatch: main]
  - Paths: `crates/caixonho-core/tests/against_a_real_service.rs`
  - Done criteria: a prefix seeded past the ceiling is walked in full, against
    the `s3s-fs` service `XONHO-0031` starts. The pagination is the service's
    own, so this also proves the continuation token round-trips at that size.
  - Verification: the test

## 3. The act

- [ ] 3.1 A download that knows it is one act [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a struct holding the act's keys, the prefix they are
    relative to, the chosen destination and the collision answer once given;
    dropped where the location is dropped, so a connection switch cannot leave
    it behind (`XONHO-0019`'s defect, in a new place).
  - Verification: `cargo test -p caixonho-gui`, including a test that switching
    connection ends the act

- [ ] 3.2 Download a folder from its row [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`, `crates/caixonho-gui/src/views/objects.rs`
  - Done criteria: the row menu offers `Download…` on a folder; it asks once
    for a destination, walks, and queues every key. The existing single-object
    entry point is untouched.
  - Verification: the test, and `XONHO-0007`'s tests still green

- [ ] 3.3 Download the selection [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: ticked rows — objects, folders or both — become one act with
    one destination. Folders in the selection are walked; objects are taken as
    they are.
  - Verification: the test

- [ ] 3.4 One answer, for the rest of the act [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the collision question gains "and do this for the rest";
    choosing it settles the act's remaining transfers without asking, and
    settles **nothing** outside the act. A later act asks again.
  - Verification: tests for all three — within, outside, and afterwards

- [ ] 3.5 The destination is asserted [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs` or `crates/caixonho-core/src/transfer.rs`
  - Done criteria: a path that would land outside the chosen directory fails
    the transfer with a stated cause rather than being written. The mapping is
    believed to make this impossible; the assertion is there because "believed
    impossible" is how directory traversal ships.
  - Verification: a test that feeds a key engineered to escape

## 4. Flows from the window

- [ ] 4.1 A subtree fetched, and compared [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: through the window, against the real service — a folder with
    objects at three depths is downloaded, and every file on disk is compared
    to the object byte for byte **and path for path**.
  - This is the test that says the change did what it promised.
  - Verification: the test

- [ ] 4.2 The batch answer, end to end [dispatch: main]
  - Done criteria: a folder downloaded twice into the same destination; the
    second act is answered once and completes without further questions.
  - Verification: the test

## 5. Close-out

- [ ] 5.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Verification: the commands

- [ ] 5.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 5.3 Say what it still does not do [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`
  - Done criteria: §4.4's row stays **partial** and names the half that landed
    and the half that did not. The collision row moves and says what "remembered"
    now means — for an act, not for a session.
  - **Overstating here is the failure mode.** The row has read "not started"
    since 2026-08-24; "done" would be worse than that.
  - Verification: the rows, read against what exists

- [ ] 5.4 Close-out review per `AGENTS.md` [dispatch: main]
  - Question 4 has a known shape here: the property test covers the mapping,
    and what it cannot cover is a filesystem refusing a name the scheme thought
    acceptable — case-insensitive volumes and network mounts especially.
  - Verification: the recorded findings
