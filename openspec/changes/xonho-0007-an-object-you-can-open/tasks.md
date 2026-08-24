# Tasks — XONHO-0007 an object you can open

> Order is dependency order: the pure scheme first because everything names
> files through it, then the port, then the transfer's own rules, then the
> window, then the paperwork. Core tasks are TDD — the test red first, per
> `AGENTS.md` §7; the GUI tasks are exploratory with named selectors where a
> later test will want them.

## 1. The name a key gets on disk

- [ ] 1.1 The mapping, as one pure function with property tests
      [dispatch: main]
  - Paths: `crates/caixonho-core/src/transfer.rs` (new module),
    `crates/caixonho-core/src/lib.rs`
  - Done criteria: `local_name(key) -> Mapped` where `Mapped` carries the name
    and *what was done to produce it* (unchanged | substituted | suffixed),
    deterministic, identical on both platforms (no `cfg` in the mapping);
    covers the §4.4 shapes: Windows-reserved characters, control bytes,
    trailing `/`, `//`, keys differing only by case, reserved DOS names
    (`CON`, `NUL`, …), overlong names. Property tests assert determinism and
    that two distinct keys never map to one name without at least one of them
    reporting it.
  - Verification: `cargo test -p caixonho-core transfer::`

- [ ] 1.2 ADR for the scheme [dispatch: main]
  - Paths: `docs/adr/0002-key-to-filename-scheme.md` (number: next free under
    `docs/adr/`)
  - Done criteria: the encoding, the suffix rule, why percent-encoding over
    replacement characters (reversibility of *reading* a name back), and the
    §4.4 sentence about silent loss quoted as the constraint. Status
    `Accepted` on landing — this is decided by shipping, unlike ADR-0001.
  - Verification: file exists and `docs/adr/` index (if any) lists it

## 2. The port reads

- [ ] 2.1 `ObjectStore::get_object`, streaming, with the double
      [dispatch: main]
  - Paths: `crates/caixonho-core/src/store.rs`
  - Done criteria: one method returning size-when-stated plus a chunk stream
    (the existing async-trait, object-safe shape); `StoreDouble` grows canned
    constructors: chunked content, mid-stream failure, refusal
    (`s3:GetObject` denied). Test red first against the double.
  - Verification: `cargo test -p caixonho-core store::`

- [ ] 2.2 The adapter maps it to `GetObject` [dispatch: main]
  - Paths: `crates/caixonho-core/src/adapter.rs`
  - Done criteria: body streamed, never buffered whole; size from the
    response's content length, falling back to the caller's listed size;
    errors go through the same classifier the listing path uses (a denial is
    a denial, a redirect is `wrong_region`'s shape, throttle is not a
    denial). One `#[ignore]`d live test alongside the existing one, same
    pattern and same reason.
  - Verification: `cargo clippy --workspace --all-targets -- -D warnings`,
    `cargo test -p caixonho-core`

## 3. A download that cannot lie on disk

- [ ] 3.1 Working-path writer with a cleanup guard [dispatch: main]
  - Paths: `crates/caixonho-core/src/transfer.rs`
  - Done criteria: writes to `<final>.caixonho-partial` in the destination
    directory; promote-by-rename on completion disarms the guard; drop
    without promotion removes the working file. Tests: completion promotes
    and leaves no working file; failure mid-stream leaves neither working nor
    final file; an existing final file is untouched by a failed transfer;
    a pre-existing stale working file is replaced, not resumed.
  - Verification: `cargo test -p caixonho-core transfer::`

- [ ] 3.2 The transfer function: stream → writer, counting as it goes
      [dispatch: main]
  - Paths: `crates/caixonho-core/src/transfer.rs`,
    `crates/caixonho-core/src/session.rs`
  - Done criteria: `Session::spawn_download(location, key, destination, …)`
    following `spawn_objects`' delivery contract (deliver exactly once, on a
    runtime thread), emitting progress `{bytes, total: Option<u64>}` through
    a callback the GUI can drain; abort-safe per 3.1. Collision with an
    existing final name is returned as a question, not resolved: the caller
    answers replace / keep-both / abandon, and keep-both derives its name
    through 1.1's reporting path.
  - Done criteria (log): `diagnostics::transfer_settled` — bucket, bytes,
    outcome, cause on failure; **no key, no path**, asserted in all three
    spellings the `XONHO-0012` test uses.
  - Verification: `cargo test -p caixonho-core`, including a test that drops
    the handle mid-stream and asserts no file at the final path

- [ ] 3.3 The open-cache: location and the startup sweep [dispatch: main]
  - Paths: `crates/caixonho-core/src/transfer.rs`
  - Done criteria: cache dir per platform (macOS
    `~/Library/Caches/caixonho/open`, Windows
    `%LOCALAPPDATA%\caixonho\cache\open`, Linux XDG cache), created on
    demand; a sweep function removing entries older than a fixed age, called
    once at startup — same pattern as the log's own rotation, tested with
    injected times, no wall clock in the test.
  - Verification: `cargo test -p caixonho-core transfer::`

## 4. The window

- [ ] 4.1 "Download…" on the object row, with a destination [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`,
    `crates/caixonho-gui/src/views/objects.rs`
  - Done criteria: the action exists on an object row (not on folders),
    fronts the platform save/choose-folder dialog, and hands off to 3.2. A
    collision question renders as a choice beside the transfer, not a modal
    over the whole window. Selectors: `download-action`,
    `transfer-progress`, `transfer-cancel`.
  - Verification: `cargo test -p caixonho-gui`, and the state is drawable by
    the `XONHO-0009` screenshot harness (add the in-flight state to it)

- [ ] 4.2 Progress and cancel for the one transfer [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: in-flight shows bytes, and bytes-of-total when stated;
    cancel aborts and the row says cancelled; failure shows the classified
    cause in the same vocabulary the listing failures use. Window tests
    drive it through the double's chunked constructor.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 4.3 Open — an explicit row action, and nothing on double-click
      [dispatch: main]
  - Owner decision 2026-08-24, revising this plan the day after it was
    written: Open is a visible button on the object's row, and double-click
    on an object is deliberately left unbound. The reasoning is safety of
    the accident: a double-click that lands on the wrong row must not be
    enough to write company data to disk and hand it to whatever
    application the OS pairs with it. Folders keep their existing
    double-click (enter) — that one navigates, it does not move bytes.
  - Paths: `crates/caixonho-gui/src/app.rs`,
    `crates/caixonho-gui/src/views/objects.rs`
  - Done criteria: an `open-action` selector on the object row downloads to
    the open-cache (3.3) and calls `open_with_system`; double-click on an
    object row does nothing new (selection only); while in flight the same
    progress surface as 4.2 shows; a failure to *open* after a successful
    download reports where the file is (with `reveal_path` as the action)
    and is not presented as a transfer failure.
  - Verification: `cargo test -p caixonho-gui` for the state transitions;
    the OS handoff itself is live-checked in 6.3

## 5. Reader-facing documents, in this change

- [ ] 5.1 README, roadmap, requirements-status [dispatch: main]
  - Paths: `README.md`, `docs/roadmap.md`, `docs/requirements-status.md`
  - Done criteria: README's status paragraph says objects download and open,
    and names the open-cache location; roadmap marks M2 in progress and this
    change's row; requirements-status moves the §4.4 rows this change
    actually ships (download, key-safety, collision-ask) with the queue rows
    left honest; **counts recomputed by `scripts/count-requirements.sh`, not
    by hand**.
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

- [ ] 6.3 Live: download a real object, cancel a big one, open a PDF
      [dispatch: main]
  - Paths: none
  - Done criteria: on a real account — a small object downloaded and byte-
    compared (`shasum`/`certutil`) against the same object fetched by any
    other tool; a large object cancelled mid-flight with nothing at the
    final path; an object with characters Windows refuses downloaded with
    the substitution reported; a PDF opened landing in the OS viewer. What
    was seen written here, names withheld.
  - Verification: the log shows the transfers' outcomes with no keys in any
    line

- [ ] 6.4 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: this change
  - Done criteria: the review is run and findings recorded here, including
    the second question read the wide way: rows either side of this change's
    own in every status table.
  - Verification: the recorded findings
