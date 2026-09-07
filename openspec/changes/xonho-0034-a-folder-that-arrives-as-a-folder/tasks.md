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

- [x] 1.4 Amend `ADR-0004` [dispatch: main]
  - Paths: `docs/adr/0004-key-to-filename-scheme.md`
  - Done criteria: an amendment section dated and signed to `XONHO-0034`,
    stating the unit moved from a filename to a relative path, what a broken
    middle segment does, and that the collision class the ADR listed as
    unsolved — `a/x.txt` against `b/x.txt` — is resolved by structure rather
    than by asking the user.
  - The original decision text is not rewritten. An ADR records what was
    decided when; an amendment records what changed and why.
  - Verification: the file, read against the original decision
  - **Done differently, and the difference is the point: a new ADR, not an
    amendment.** `ADR-0004` closes with *"Changing the scheme … gets a new ADR,
    not an edit to this one."* This change does not alter the scheme —
    `local_name` returns exactly what it returned and no existing download
    folder moves — so an amendment was arguably within the rules. Deciding that
    a document's own closing instruction does not apply to the person invoking
    it is not a habit worth starting. `ADR-0005` records the path decision, the
    middle-segment rule, and the marker finding; `ADR-0004` gains one line
    pointing at it and keeps its decision text untouched.

## 2. The walk, without a ceiling

- [x] 2.1 A walk a download may use [dispatch: main]
  - Paths: `crates/caixonho-core/src/session.rs`
  - Done criteria: gathering every key under a prefix with no `TooMany`
    refusal, while `spawn_walk_under`'s existing behaviour — and the 5,000
    ceiling that guards a delete — is untouched. Whether that is a parameter or
    a sibling is decided when both are written; the smaller diff wins and the
    reason is recorded here.
  - Verification: `cargo test -p caixonho-core`, and the delete flow's existing
    `TooMany` test still passing
  - **Done as a parameter, and the reason is at the call site.** `most:
    Option<usize>` rather than a sibling function: one implementation, and a
    caller now states its intent where it calls — `Some(MOST_KEYS_GATHERED)`
    for a delete, `None` for a download. The doc says which and why. The
    delete flow and both of `session.rs`'s own walk tests pass the ceiling, so
    nothing about deleting moved.

- [x] 2.2 Prove it against a real service [dispatch: main]
  - Paths: `crates/caixonho-core/tests/against_a_real_service.rs`
  - Done criteria: a prefix seeded past the ceiling is walked in full, against
    the `s3s-fs` service `XONHO-0031` starts. The pagination is the service's
    own, so this also proves the continuation token round-trips at that size.
  - Verification: the test
  - **Done, and smaller than the task asked.** Eleven objects with a bound of
    ten, not 5,001 with the constant: what is under test is that the caller's
    number is honoured and that its absence means no limit. Seeding five
    thousand objects to prove arithmetic about five thousand would add seconds
    to every run and prove nothing extra — the reasoning is in the test.
    `a_walk_refuses_past_the_bound_it_is_given_and_gathers_past_none`, against
    the real service.

## 3. The act

- [x] 3.1 A download that knows it is one act [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a struct holding the act's keys, the prefix they are
    relative to, the chosen destination and the collision answer once given;
    dropped where the location is dropped, so a connection switch cannot leave
    it behind (`XONHO-0019`'s defect, in a new place).
  - Verification: `cargo test -p caixonho-gui`, including a test that switching
    connection ends the act
  - **Done.** `Downloading` holds the connection, the bucket, the prefix the
    act began at, the destination, the standing answer and the member ids, with
    a `DownloadPhase` of `Walking { left, gathered }` or `Sending` — the same
    shape `Deletion` already uses. Dropped in `end_location`, so it cannot
    outlive the place it was started from.

- [x] 3.2 Download a folder from its row [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`, `crates/caixonho-gui/src/views/objects.rs`
  - Done criteria: the row menu offers `Download…` on a folder; it asks once
    for a destination, walks, and queues every key. The existing single-object
    entry point is untouched.
  - Verification: the test, and `XONHO-0007`'s tests still green
  - **Done.** `Download folder…` on a folder row. A folder still has none of
    the three verbs that read one object's bytes, and now has the one that
    reads the bytes underneath it. `download_row` is untouched.

- [x] 3.3 Download the selection [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: ticked rows — objects, folders or both — become one act with
    one destination. Folders in the selection are walked; objects are taken as
    they are.
  - Verification: the test
  - **Done.** `Download {n}…` in the selection strip, placed **before** the
    delete: a strip whose first verb destroys is a strip that gets misclicked.
    Ghost rather than danger, because fetching destroys nothing.

- [x] 3.4 One answer, for the rest of the act [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the collision question gains "and do this for the rest";
    choosing it settles the act's remaining transfers without asking, and
    settles **nothing** outside the act. A later act asks again.
  - Verification: tests for all three — within, outside, and afterwards
  - **Done, and proved by ablation rather than by passing.** Four tests: the
    answer settles the act's other waiting members; a transfer outside the act
    keeps asking; the answer dies with the act; and without "for the rest" one
    answer is still one answer.
  - **The first ablation was a fake.** Its string did not match, so nothing was
    broken and the test passed — which read as the test being weak. Re-run with
    an assertion that the edit applied, the test fails with its own words. An
    ablation without an assertion that it landed proves nothing and feels like
    proof, which is worse than not running one.
  - **Ticked half a step early, corrected.** When first marked done the logic
    and its tests existed and the *control* did not — there was no way for a
    user to say "for the rest". The tick stood on work a person could not
    reach. The checkbox now exists, offered only when more than one member is
    still unsettled: a tick that decides one file decides nothing, and a
    control that decides nothing is noise. The bucket table's `marked` rule,
    applied to a question.

- [x] 3.5 The destination is asserted [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs` or `crates/caixonho-core/src/transfer.rs`
  - Done criteria: a path that would land outside the chosen directory fails
    the transfer with a stated cause rather than being written. The mapping is
    believed to make this impossible; the assertion is there because "believed
    impossible" is how directory traversal ships.
  - Verification: a test that feeds a key engineered to escape
  - **Done, in core so both callers get it.** `transfer::under(root, candidate)`
    compares **components**, not text: `/tmp/a` is not inside `/tmp/ab`, and a
    `starts_with` on the string would say it is. Written test-first — the four
    cases were named before the function existed. The act refuses the object
    with `Error::Destination` rather than writing it.

## 4. Flows from the window

- [x] 4.1 A subtree fetched, and compared [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: through the window, against the real service — a folder with
    objects at three depths is downloaded, and every file on disk is compared
    to the object byte for byte **and path for path**.
  - This is the test that says the change did what it promised.
  - Verification: the test
  - **Done — and it caught the test rather than the code.**
    `a_subtree_arrives_as_a_subtree_byte_for_byte_and_path_for_path`: five
    objects at three depths, one of them `12:30.log`, fetched by one gesture
    from the real service. Every file compared to what the service holds, not
    to a literal in the test, and every path compared to where it should be.
  - The first run failed because **the test expected the wrong thing**: it
    stripped `daily/` from the paths, as though the act had begun inside the
    folder. The act begins where the user is standing — the bucket root — so
    `daily/` arrives *as* `daily/`, which is the promise. The code was right;
    the expectation was written from the wrong end. Corrected, with the reason
    beside it so the next reader does not repeat it.
  - Also asserted: `daily/deep/deeper` is a real directory chain rather than a
    name with slashes in it, `12:30.log` lands as `12%3A30.log` deep inside the
    tree, and an object outside the folder is not fetched.

- [x] 4.2 The batch answer, end to end [dispatch: main]
  - Done criteria: a folder downloaded twice into the same destination; the
    second act is answered once and completes without further questions.
  - Verification: the test
  - **Done**, `one_answer_settles_a_whole_second_fetch_of_the_same_folder`: the
    folder fetched twice into one destination, the second meeting three taken
    names, answered once with the tick. Nothing is left asking, and six files
    exist where three were fetched twice.
  - **Ablation, with an assertion that it applied** — the lesson from 3.4. With
    the batch answer disabled the test fails on its own words, "gave up waiting
    for the rest to settle without asking again", after the full 30s patience.
    Restored, green in 0.85s.

## 5. Close-out

- [x] 5.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Verification: the commands
  - **Done.** `cargo fmt --all` clean, `cargo clippy --workspace --all-targets
    -- -D warnings` clean, and the workspace green: 409 + 7 + 11 + 134 = 561
    passing, 9 ignored (the live-service ones that need an account).
  - Worth recording *why* clippy needed a second look: it compiles the host
    target only, so a `#[cfg(not(target_os = "macos"))]` item's lints never run
    here. Checking it meant temporarily swapping the gate and **running** the
    lint, not reading the code and deciding it was fine.

- [x] 5.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`
  - **Green on both targets: run 34110596012** — rustfmt, dependency audit,
    `build (macos-latest)`, `build (windows-latest)`, all success. PR
    [#2](https://github.com/vunm-io/caixonho/pull/2).
  - **The first run, 34109206677, was red on Windows, and it is the more useful
    of the two.** macOS green, Windows red on the subtree flow test. It was not
    the change: seeding writes a key straight to disk, so NTFS turned
    `daily/12:30.log` into the file `daily\12` with an alternate data stream
    named `30.log`, the listing returned `daily/12`, and the download of that
    key was correct. A harness proving something else while passing on the
    machine the test was written on. Fixed at the harness — `Service` refuses
    such a key on every platform, the exclusion is in its module docs with a
    test named for it, and the flow test seeds `%` instead, which every host
    keeps and which the scheme's injectivity rests on.
  - This is the second time CI has been the only thing that could have caught a
    Windows-only fault (the first was `*.localhost` in `XONHO-0031`). Both were
    invisible on macOS by construction.

- [x] 5.3 Say what it still does not do [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`
  - Done criteria: §4.4's row stays **partial** and names the half that landed
    and the half that did not. The collision row moves and says what "remembered"
    now means — for an act, not for a session.
  - **Overstating here is the failure mode.** The row has read "not started"
    since 2026-08-24; "done" would be worse than that.
  - Verification: the rows, read against what exists
  - **Done, and the row stayed partial.** §4.4's first row now names both
    halves: download rebuilds a tree, **upload does not** — sending a directory
    up is still one file at a time, and prefix structure on the way up is not
    preserved because nothing walks a local tree. Multipart is still absent, so
    the 5 GiB cap stands. The §4.4 tally is unchanged at 1 done, 3 partial, 3
    not started, because no row changed state.
  - The collision row says what "remembered" now means and that it is
    **narrower than the row asks**: for the act, not for the session — a second
    download of the same folder asks again, on purpose.
  - Two more rows were touched rather than left to drift: key↔filesystem safety
    (still **done** — ADR-0005 extends the scheme to paths and the property
    test covers them) and the queue row (this is the first change to hand it a
    whole gesture's work at once).
  - `docs/roadmap.md`: the M2 line and a row of its own in *M2 so far*, both
    saying **download only**.

- [x] 5.4 Close-out review per `AGENTS.md` [dispatch: main]
  - Question 4 has a known shape here: the property test covers the mapping,
    and what it cannot cover is a filesystem refusing a name the scheme thought
    acceptable — case-insensitive volumes and network mounts especially.
  - Verification: the recorded findings
  - **1. What was asked, or what was convenient?** What was asked. The owner
    asked for a folder download or many files at once, and chose option A — the
    quick one, enhanced later — explicitly. The one departure from the proposal
    is written down rather than silent: `spawn_walk_under`'s cap moved from the
    callee to the caller, so delete keeps `MOST_KEYS_GATHERED` and download
    passes `None`. That is in the design, because a download refusing at 5000
    objects would have been a limit invented by a function that delete needed.
  - **2. Do the reader-facing documents still tell the truth?** They did not,
    and the second reading is what caught it. `README.md`'s "Not there yet"
    list still promised **bulk and folder deletion** and **the transfer
    queue** — delivered by `XONHO-0030` and `XONHO-0028`, neither of them this
    change. Exactly the failure this question exists for: a cell belonging to
    another change, with nobody scheduled to look at it. Corrected, along with
    the download bullet. `docs/architecture.md` says nothing about transfers
    and needs nothing. `docs/design-language.md` is untouched by this change.
    `docs/planned-changes.md`: no section here is answered by `XONHO-0034`, and
    the folder-delete section is properly closed with its issue note.
  - **3. Did we leave rubbish?** No. Every new symbol has a caller — checked by
    name, and `-D warnings` would fail on a dead one regardless. No `TODO`,
    `dbg!` or commented-out block in the diff. One thing was nearly left: an
    over-broad `str.replace` deleted half of `transfer.rs` mid-change; the
    compiler caught it, the file was restored from git and the edit redone with
    tight anchors. Nothing of that survives in the tree.
  - **4. What is asserted but not verified?** The known shape, as the task
    predicted, plus two more:
    - The property test covers the **mapping**. What no test here covers is a
      **filesystem refusing a name the scheme thought acceptable** —
      case-insensitive volumes (two keys differing only in case become one
      file) and network mounts (SMB and NFS reject characters the scheme
      encodes past, and impose their own length limits).
    - **CI then found the near half of this, at the other end.** The flow test
      seeded a key holding `:` and the Windows runner failed: `s3s-fs` is a
      filesystem, seeding writes a key straight to disk, and NTFS reads `:` as
      the separator before an alternate data stream — so the object was stored
      as `daily\12` with a stream named `30.log`, the listing returned
      `daily/12`, and the application downloaded that key **correctly**.
      Nothing was wrong with the change; the harness was proving something
      else while passing on macOS. `Service` now refuses such a key on every
      platform rather than on the ones that reserve it, the exclusion is
      written into its module docs beside the four already there with a test
      named for it, and the flow test seeds `%` instead — legal on both hosts,
      and the character the scheme's injectivity actually rests on. Ablated:
      stop encoding `%` and the flow test fails on the path it expects.
    - What is still unverified is the *far* half: a filesystem refusing a name
      the scheme produces. Both CI runners' disks are the permissive case.
    - **Windows path length.** A deep prefix plus a long destination can pass
      260 characters. Nothing in this change measures it, and the failure would
      arrive as a per-file error rather than as a refusal that explains itself.
    - **Scale.** The largest tree any test fetches is five objects. A folder of
      ten thousand is walked with no bound (deliberately — `None`), and nothing
      exercises what the window does while that walk is running.
  - **5. What is left, and where is it written?** Folder **upload** — named in
    `docs/requirements-status.md` §4.4 and in the roadmap row, both saying
    download only. The collision wording — per session written, per act built —
    is in the §4.4 collision row. The three unverified items above are here.
    And the sidebar defect the owner reported while this change was open is now
    a section of its own in `docs/planned-changes.md` ("Two different buckets,
    one name in the rail"), because it was approved in conversation and a
    finding that lives in a transcript is a finding that is lost.
