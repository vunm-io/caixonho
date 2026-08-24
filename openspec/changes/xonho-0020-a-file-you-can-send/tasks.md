# Tasks — XONHO-0020 a file you can send

> Same order-by-dependency as `XONHO-0007`: the key rule first, then the
> port, then the transfer, then the window, then the paperwork. Core is TDD
> (`AGENTS.md` §7) — red first, and for this change the red tests matter more
> than usual: the guarantee under test is "an object that existed still
> exists", which no amount of green elsewhere implies.
>
> **Ordering constraint outside this change:** the spec delta targets
> `object-transfer`, which reaches `openspec/specs/` only when `XONHO-0007`
> archives. Sync and archive of this change wait on that — see 6.5.

## 1. The key a taken key steps aside to

- [x] 1.1 Deriving a free object key, as its own rule [dispatch: main]
      - Done in `main` (2026-08-24), red first: four tests on a `todo!()`
        body. The dot is searched in the last segment only, and a segment
        ending in a dot is left alone — `name (2).` reads as broken and the
        dot is no extension boundary there. The design's claim that this is
        not `local_name` is now a test rather than prose: the same key goes
        through both and only the local side encodes the colon.
  - Paths: `crates/caixonho-core/src/transfer.rs`
  - Done criteria: `beside(key, n) -> String` inserting ` (n)` before the
    final dot of the key's last segment, leaving the prefix untouched;
    deterministic; a key with no dot, a key ending in a dot, and a key whose
    only dot is in a *prefix* segment all behave (the dot searched is in the
    last segment, not the whole key). **Not** `local_name`: object keys are
    bytes and the service refuses almost nothing, so percent-encoding here
    would rename what the user sent. A test asserts the two functions differ
    on a key containing `:` — the local side encodes it, this side does not.
  - Verification: `cargo test -p caixonho-core transfer::`

## 2. The port writes

- [x] 2.1 `ObjectStore::put_object`, conditional, with the double
      [dispatch: main]
      - Done in `main` (2026-08-24), red first. `PutOutcome` is
        `Created | KeyTaken | ConditionUnsupported` and none of them is an
        `Err`. The double reads the file before answering, so a test
        scripting a path that does not exist hears about it instead of
        getting a false success — and `Writes` is scripted independently of
        `Outcome`/`Content` for the reason those two already are.
  - Paths: `crates/caixonho-core/src/store.rs`
  - Done criteria: `put_object(bucket, key, body_path, IfAbsent) -> Result<PutOutcome>`
    where `IfAbsent` is a two-valued "refuse if the key exists" / "replace"
    and `PutOutcome` is `Created` | `KeyTaken` | `ConditionUnsupported`.
    `KeyTaken` is an **outcome, not an `Err`** — a precondition that did its
    job is not a failure and must not reach the failure vocabulary. Double
    gains: accepting, key-taken, condition-unsupported, refused
    (`s3:PutObject` denied), and a mid-request failure. Test red first.
  - Verification: `cargo test -p caixonho-core store::`

- [x] 2.2 The adapter maps it to `PutObject` with `if_none_match`
      [dispatch: main]
      - Done in `main` (2026-08-24). `412` is read through a new
        `SdkFailure::precondition_failed()`, sitting beside
        `redirect_region()` and reading the same status field — asked
        *before* classification, so a precondition that did its job never
        enters the failure vocabulary. `ConditionUnsupported` reuses the
        existing `Error::NotImplemented` classification rather than a new
        code path: `classify.rs` already maps that S3 code, and R2 is
        documented there as a service that returns it.
      - The live test `this_machine_writing_one_object_twice` writes the same
        key twice and asserts the second is `KeyTaken`. It is the **only**
        place this change's central guarantee can be checked: every unit test
        proves the double refuses a taken key, which says nothing about the
        service. A second `Created` is design.md's undetectable case, and
        this is where it stops being undetectable.
  - Paths: `crates/caixonho-core/src/adapter.rs`
  - Done criteria: body via `ByteStream::from_path` (streamed, never read
    whole); `if_none_match("*")` present for `IfAbsent::Refuse` and **absent**
    for `IfAbsent::Replace`; `412` read off `SdkFailure`'s status the way
    `301` already is (`classify.rs:223`) and returned as `KeyTaken`; `501` /
    not-implemented returned as `ConditionUnsupported`; every other cause
    through the existing classifier, and the redirect follow-once shape shared
    with the read paths. One `#[ignore]`d live test in the established
    env-var pattern, which is also where a real endpoint's conditional-write
    behaviour gets observed.
  - Verification: `cargo clippy --workspace --all-targets -- -D warnings`,
    `cargo test -p caixonho-core`

## 3. The upload itself

- [x] 3.1 Size gate before anything is sent [dispatch: main]
      - Done in `main` (2026-08-24), red first. The boundary check is split
        out as `within_one_request(bytes)` so it can be tested at exactly
        the limit and one byte over **without writing five gibibytes** to a
        temp directory — the file check is then a `stat` plus that
        comparison. A `stat` that fails is a read failure, not a size
        verdict, and its detail carries no path.
      - `readable()` is duplicated from the GUI's formatter rather than
        shared: core must not depend on the GUI, and a refusal that says
        `5368709121` helps nobody. Small and deliberate.
  - Paths: `crates/caixonho-core/src/transfer.rs`
  - Done criteria: `SINGLE_REQUEST_LIMIT` (5 GiB, the service's documented
    figure, named and commented as such) and a pre-flight check returning a
    refusal that names the limit and multipart; a file that cannot be
    `stat`ed is a `Destination`-shaped read failure, not a size verdict.
    Tests at the boundary: exactly at the limit sends, one byte over refuses.
  - Verification: `cargo test -p caixonho-core transfer::`

- [x] 3.2 `Session::spawn_upload`, with the taken-key question
      [dispatch: main]
      - Done in `main` (2026-08-24). Contract identical to
        `spawn_download`'s. A taken key and an unsupported condition are
        delivered **without logging**: nothing moved, and the log records
        transfers, so writing a line for a question would put an event in it
        that never happened.
      - Keep-both is a bounded loop of conditional attempts, and its test is
        the one that carries this change: the double refuses every
        conditional write and accepts every unconditional one, so keep-both
        **exhausting its bound and reporting that** proves it never once
        fell back to an unconditional write to make progress. A green happy
        path could not have shown that.
      - `upload_settled` beside `transfer_settled` rather than a shared
        function with a direction flag — two call sites, two verbs in the
        log, and the assertion test reads better for it. The no-key test has
        a third subject here: the local source path names the user's own
        machine, and it is asserted absent too.
  - Paths: `crates/caixonho-core/src/transfer.rs`,
    `crates/caixonho-core/src/session.rs`
  - Done criteria: follows `spawn_download`'s contract exactly — deliver once
    on a runtime thread, cooperative `Cancel`, an `UploadOutcome` mirroring
    `DownloadOutcome` (`Finished { key }` | `KeyTaken { key }` |
    `ConditionUnsupported` | `Cancelled` | `Failed`). Keep-both is a bounded
    loop of conditional attempts (see design: the service is the only honest
    source of which keys are free); the bound is a constant and reaching it
    is reported, never silently abandoned.
  - Done criteria (log): `diagnostics::transfer_settled` extended or
    mirrored for uploads — bucket, bytes, outcome, cause; **no key, no local
    path**, asserted with `XONHO-0012`'s `assert_undisclosed` at the most
    detailed level, exactly as the download test does.
  - Verification: `cargo test -p caixonho-core`, including a cancel test and
    a keep-both test that asserts the first object was not touched

## 4. The window

- [x] 4.1 "Upload…" beside the other two verbs [dispatch: main]
      - Done in `main` (2026-08-24). No `disabled` on this one, unlike Open
        and Download…: it acts on the location rather than on a row, and
        being in a location is what enables it. The key is
        `location.prefix + file name`.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: enabled whenever a location is open (unlike Open and
    Download…, it needs no selection); `cx.prompt_for_paths` with
    `files: true, directories: false`; the key is the location's prefix plus
    the chosen file's name. Selector `upload-action`.
  - Verification: `cargo test -p caixonho-gui`

- [x] 4.2 The transfer line grows the upload states [dispatch: main]
      - Done in `main` (2026-08-24). `Transfer` gained a `direction` and a
        `source`, not a twin — the running, cancelled and failed lines choose
        their words from the direction, and three phases are new:
        `KeyTaken`, `ConditionUnsupported`, `Sent`.
      - `KeyTaken` is deliberately a separate phase from `NameTaken` rather
        than a shared "something is in the way": what is in the way is
        someone else's data in a bucket, not a file on this machine, and the
        sentence has to read like it.
      - The running line shows an upload's **total with no fraction**. The
        alternative was a bar that does not move, which reads as a stall.
      - `Sent { stepped_aside }` is carried into the phase and said loudly,
        because a user not told that keep-both renamed the object will look
        for it under the name they sent. Window test asserts the flag
        survives the trip.
      - The exhaustive match on `TransferPhase` caught all three new states
        at compile time — the reason it has no wildcard arm.
      - Both decision-carrying states are in the screenshot harness: 16
        images now.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: `Transfer` gains a direction rather than acquiring a
    twin — running (total shown, indeterminate, cancel), key-taken (Replace /
    Keep both / Cancel against a **key**), finished (naming the key, and
    naming it loudly when keep-both changed it), cancelled, failed. The
    condition-unsupported state says the guarantee is unavailable here and
    makes proceeding an explicit second act. Window tests through
    `apply_transfer` as `XONHO-0007`'s do; the new states join the screenshot
    harness.
  - Verification: `cargo test -p caixonho-gui`, and
    `cargo test -p caixonho-gui -- --ignored every_state`

## 5. Reader-facing documents, in this change

- [ ] 5.1 README, roadmap, requirements-status [dispatch: main]
  - Paths: `README.md`, `docs/roadmap.md`, `docs/requirements-status.md`
  - Done criteria: README says a file can be sent and that an existing object
    is never replaced without asking; roadmap's M2 rows carry this change;
    requirements-status §4.4 row 1 moves to reflect the upload half, and the
    collision row names that the *remote* side is now covered while the
    per-session memory still is not. **Counts recomputed by
    `scripts/count-requirements.sh`, not by hand.**
  - Verification: the script's totals match the tables

## 6. Close-out

- [ ] 6.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [ ] 6.2 CI green on both targets, run id recorded here [dispatch: main]
  - Paths: none
  - Done criteria: all four jobs successful for the tip; run id written here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 6.3 Live: send a file, meet a taken key, and cancel one
      [dispatch: main]
  - Paths: none
  - Done criteria: on a real account — a file uploaded and its content
    verified by downloading it back and comparing checksums; the same upload
    repeated to hit the taken-key question, with **replace** and **keep both**
    each exercised and the untouched object confirmed still correct after
    keep-both; a cancel mid-upload; and a refusal on a bucket the credentials
    cannot write. What was seen written here, names withheld.
  - Verification: the log shows the outcomes with no keys and no local paths
    in any line

- [ ] 6.4 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: this change
  - Done criteria: the review is run and its findings recorded here,
    including question 2 read the wide way — the rows either side of this
    change's own.
  - Verification: the recorded findings

- [ ] 6.5 Sync the delta only after `XONHO-0007` archives [dispatch: main]
  - Paths: `openspec/specs/object-transfer/`
  - Done criteria: `XONHO-0007` has archived (its own 6.3 is the owner's
    live check), so `openspec/specs/object-transfer/spec.md` exists and this
    change's delta applies onto it. Attempting it earlier creates the
    capability from half its requirements.
  - Verification: `openspec validate --changes` clean, and
    `openspec/specs/object-transfer/spec.md` holds both directions
