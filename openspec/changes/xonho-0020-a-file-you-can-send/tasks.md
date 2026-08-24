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

- [ ] 1.1 Deriving a free object key, as its own rule [dispatch: main]
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

- [ ] 2.1 `ObjectStore::put_object`, conditional, with the double
      [dispatch: main]
  - Paths: `crates/caixonho-core/src/store.rs`
  - Done criteria: `put_object(bucket, key, body_path, IfAbsent) -> Result<PutOutcome>`
    where `IfAbsent` is a two-valued "refuse if the key exists" / "replace"
    and `PutOutcome` is `Created` | `KeyTaken` | `ConditionUnsupported`.
    `KeyTaken` is an **outcome, not an `Err`** — a precondition that did its
    job is not a failure and must not reach the failure vocabulary. Double
    gains: accepting, key-taken, condition-unsupported, refused
    (`s3:PutObject` denied), and a mid-request failure. Test red first.
  - Verification: `cargo test -p caixonho-core store::`

- [ ] 2.2 The adapter maps it to `PutObject` with `if_none_match`
      [dispatch: main]
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

- [ ] 3.1 Size gate before anything is sent [dispatch: main]
  - Paths: `crates/caixonho-core/src/transfer.rs`
  - Done criteria: `SINGLE_REQUEST_LIMIT` (5 GiB, the service's documented
    figure, named and commented as such) and a pre-flight check returning a
    refusal that names the limit and multipart; a file that cannot be
    `stat`ed is a `Destination`-shaped read failure, not a size verdict.
    Tests at the boundary: exactly at the limit sends, one byte over refuses.
  - Verification: `cargo test -p caixonho-core transfer::`

- [ ] 3.2 `Session::spawn_upload`, with the taken-key question
      [dispatch: main]
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

- [ ] 4.1 "Upload…" beside the other two verbs [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: enabled whenever a location is open (unlike Open and
    Download…, it needs no selection); `cx.prompt_for_paths` with
    `files: true, directories: false`; the key is the location's prefix plus
    the chosen file's name. Selector `upload-action`.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 4.2 The transfer line grows the upload states [dispatch: main]
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
