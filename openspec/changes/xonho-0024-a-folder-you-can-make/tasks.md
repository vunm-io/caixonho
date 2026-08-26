# Tasks — XONHO-0024 a folder you can make

> Core is TDD (`AGENTS.md` §7). The interesting tests here are not "a put was
> made" but the two refusals: a name that cannot be one, and a bucket kind
> that cannot hold one.

## 1. Making one

- [ ] 1.1 A name that cannot be a folder is refused before any request
      [dispatch: main]
  - Paths: `crates/caixonho-core/src/store.rs` or a new small module
  - Done criteria: a pure function deciding whether a name may become a
    folder — non-empty, no `/`, and whatever else the key rules forbid. Red
    first. Tests: empty, `/` inside, leading and trailing whitespace, and a
    name that is fine. **Nothing reaches the service to find this out.**
  - Verification: `cargo test -p caixonho-core`

- [ ] 1.2 The marker, on a general purpose bucket [dispatch: main]
  - Paths: `crates/caixonho-core/src/store.rs`,
    `crates/caixonho-core/src/adapter.rs`,
    `crates/caixonho-core/src/session.rs`
  - Done criteria: `ObjectStore` gains create-folder; the adapter puts a
    zero-byte object at `<prefix><name>/`; `Session` gains its spawn. Red
    first, against `StoreDouble`. Tests: the key is the location's prefix
    plus the name plus exactly one `/`; a folder made at the bucket root has
    no leading `/`; a failure keeps its classified cause.
  - Verification: `cargo test -p caixonho-core`

- [ ] 1.3 A directory bucket is refused with what does work [dispatch: main]
  - Paths: `crates/caixonho-core/src/session.rs`
  - Done criteria: the decision is taken from the **bucket kind already in
    the listing**, before any request. Red first. Tests: a directory bucket
    yields the refusal and **no call reaches the store** (assert on the
    double's call count, not only on the returned value); a general purpose
    bucket puts. The refusal carries what to do instead, so the window has
    something to say rather than a bare "no".
  - Verification: `cargo test -p caixonho-core`

## 2. The window

- [ ] 2.1 `New folder…`, its prompt, and the two answers [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a button beside the five verbs, enabled only inside a
    bucket; a name prompt; the listing refreshed on success so the folder is
    there without a manual reload; the directory-bucket refusal shown as a
    sentence the user can act on, not as an error.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.2 The screenshot harness covers the new states [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the naming prompt, a refusal, and a made folder each get a
    frame, and each is **pixel-distinct** from every other — the assertion
    `XONHO-0009` added after twelve identical images got through.
  - Verification: `cargo test -p caixonho-gui`

## 3. Close-out

- [ ] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Verification: the commands themselves

- [ ] 3.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 3.3 Live: a folder on each kind of bucket [dispatch: main]
  - Done criteria: on the owner's machine — make a folder on a **general
    purpose** bucket, leave the location, come back, and confirm it is still
    there; then ask for one on a **directory** bucket and read the refusal.
    Both written here. The first is the one that can surprise: if the folder
    is gone on return, the marker is not doing what this design says it does.
  - Verification: what was seen, quoted

- [ ] 3.4 Reader-facing documents [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`
  - Done criteria: §4.5's create-folder row moves and says what it now does
    **and what it refuses**; the M3 roadmap table gains a row. Counts by
    `scripts/count-requirements.sh`.
  - Verification: the script's totals match the tables

- [ ] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Done criteria: the five questions answered here, question 2 read the wide
    way, and question 4 asked in the form `XONHO-0023` learned it the hard
    way: **what did this change do to the evidence?**
  - Verification: the recorded findings
