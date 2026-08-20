## 1. Make room before building

- [ ] 1.1 [dispatch: main] Split `caixonho-gui/src/app.rs` (1113 lines) into
      modules, as a **pure move**: no behaviour change, no renames beyond what
      the move forces, reviewable by reading the line movements alone.
  - Paths: `crates/caixonho-gui/src/app.rs`, `crates/caixonho-gui/src/views/`
  - Done criteria: `app.rs` holds the application state and the bridge to the
    session; view rendering lives beside the existing `views/buckets.rs` and
    `views/credential_form.rs`. `cargo test --workspace` passes unchanged.
  - Verification: `cargo fmt --all --check && cargo clippy --workspace
    --all-targets -- -D warnings && cargo test --workspace`

## 2. The port, and the rules that make a listing correct

- [ ] 2.1 [dispatch: main] Add the domain types a listing needs.
  - Paths: `crates/caixonho-core/src/types.rs`
  - Done criteria: a location (bucket + prefix), a page (child prefixes,
    objects, whether more remains and how to ask for it), a folder, and an
    object carrying key, size, last-modified, storage class and ETag. Storage
    class and ETag are carried even though `XONHO-0006` renders neither —
    design.md, "The port gains one operation".
  - Verification: `cargo build -p caixonho-core`

- [ ] 2.2 [dispatch: main] Write the folder-inference rules as pure functions,
      test-first, before any adapter calls them.
  - Paths: `crates/caixonho-core/src/` (beside the adapter)
  - Done criteria: three rules hold, each with its own test — an entry whose
    key equals the current prefix is that folder and never an entry within
    itself; a prefix with no object behind it is still a folder; an object and
    a prefix sharing a name both survive. Fixtures mirror the real cases
    recorded in `docs/planned-changes.md`.
  - Verification: `cargo test -p caixonho-core`

- [ ] 2.3 [dispatch: main] Extend the `ObjectStore` port with the listing
      operation and give the test double a canned implementation.
  - Paths: `crates/caixonho-core/src/store.rs`
  - Done criteria: the trait gains one operation taking a location and an
    optional cursor and returning a page; the double in that module's tests can
    return a page, a truncated page, and each failure cause.
  - Verification: `cargo test -p caixonho-core`

- [ ] 2.4 [dispatch: main] Implement it in the adapter over `ListObjectsV2`.
  - Paths: `crates/caixonho-core/src/adapter.rs`
  - Done criteria: `delimiter=/`, the prefix, and the continuation token are
    sent; common prefixes become folders and contents become objects, with 2.2
    applied; the request shape is asserted in a test without a network, as
    `list_buckets_request` already is.
  - Verification: `cargo test -p caixonho-core`

- [ ] 2.5 [dispatch: main] Make a refused location its own outcome, and prove
      it is never emptiness.
  - Paths: `crates/caixonho-core/src/classify.rs`,
    `crates/caixonho-core/src/store.rs`
  - Done criteria: a listing refused on authorization grounds is an error
    carrying that cause, never an empty page — the same guarantee
    `a_denied_listing_is_an_error_never_an_empty_list` already makes for
    buckets, now for prefixes. Expired session, network and trust failures each
    stay themselves.
  - Verification: `cargo test -p caixonho-core`

- [ ] 2.6 [dispatch: main] Parse and render a location as text.
  - Paths: `crates/caixonho-core/src/types.rs`
  - Done criteria: pure functions both ways, with tests for a bucket alone, a
    bucket with a prefix, a trailing separator present and absent, and text
    that names nowhere. No network, no UI.
  - Verification: `cargo test -p caixonho-core`

## 3. Reading a location, without blocking the window

- [ ] 3.1 [dispatch: main] Carry the listing across the session bridge.
  - Paths: `crates/caixonho-core/src/session.rs`
  - Done criteria: a location is requested on the tokio runtime and its page
    returned over the channel, as bucket listing already is; nothing
    network-shaped touches the render thread (repo invariant 2). The outcome is
    recorded through `diagnostics`, alongside `listing_settled`.
  - Verification: `cargo test -p caixonho-core`

- [ ] 3.2 [dispatch: main] Fetch the next page as the end of the list is
      approached, not before and not all at once.
  - Paths: `crates/caixonho-core/src/session.rs`,
    `crates/caixonho-gui/src/`
  - Done criteria: the first page renders without waiting for the rest; a
    further page is requested only as the user reaches what is shown; a page
    already being fetched is not fetched twice.
  - Verification: `cargo test --workspace`

- [ ] 3.3 [dispatch: main] Probe prefixes through the machinery that already
      exists.
  - Paths: `crates/caixonho-core/src/probe.rs`
  - Done criteria: a prefix is a `Scope` with its prefix set — no new
    capability model, no change to `openspec/specs/capability-awareness/`. The
    existing budget still holds: viewport-only, debounced, bounded, never on
    write.
  - Verification: `cargo test -p caixonho-core`

## 4. The window

- [ ] 4.1 [dispatch: main] Hold one location as the single source of truth, and
      derive the trail from it.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: no second record of position exists anywhere; the breadcrumb
    trail is computed by splitting the location's prefix; selecting a step sets
    the location.
  - Verification: `cargo test --workspace`

- [ ] 4.2 [dispatch: main] Move buckets into the sidebar beneath their
      connection, flat.
  - Paths: `crates/caixonho-gui/src/app.rs`,
    `crates/caixonho-gui/src/views/`
  - Done criteria: the sidebar shows connections and, under the chosen one, its
    buckets. It does not expand into prefixes — design.md, "Navigation lives in
    the main panel". Use the toolkit's sidebar rather than rebuilding it with
    `div()`.
  - Verification: run the application and look

- [ ] 4.3 [dispatch: main] Give the main panel its second view: contents when a
      bucket is chosen, the bucket table when one is not.
  - Paths: `crates/caixonho-gui/src/views/`
  - Done criteria: the object list renders through the virtualized table with
    name, size and last-modified; folders are visibly openable and objects are
    not; `h_flex()` rows that own layout carry `.items_stretch()`
    (`docs/design-language.md`).
  - Verification: run the application and look

- [ ] 4.4 [dispatch: main] Breadcrumb trail and editable path bar above the
      contents.
  - Paths: `crates/caixonho-gui/src/views/`
  - Done criteria: the trail names every step from the bucket down and each is
    selectable; the path bar accepts text, goes there, and on text that names
    nowhere says so while leaving the open location unchanged.
  - Verification: run the application and look

- [ ] 4.5 [dispatch: main] Say when more is still coming, and tell an empty
      location from a refused one on sight.
  - Paths: `crates/caixonho-gui/src/views/`
  - Done criteria: a truncated listing states that more remains; an empty
    location reads as empty; a refused location reads as refused with its
    cause. The three are distinguishable without reading the log.
  - Verification: run the application against a location of each kind

## 5. Close-out

- [ ] 5.1 [dispatch: main] `cargo fmt --all`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo test --workspace` green.
  - Paths: whole workspace
  - Done criteria: all three commands exit zero
  - Verification: the commands themselves

- [ ] 5.2 [dispatch: main] CI green on both targets.
  - Paths: none
  - Done criteria: the run for the merge commit shows `build (windows-latest)`
    and `build (macos-latest)` both successful; the run id is recorded here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 5.3 [dispatch: main] Live: open a real bucket, walk into a prefix and
      back out, reach a bucket by typing its name, and read a refused prefix.
  - Paths: none
  - Done criteria: each is confirmed against a real account and what was seen
    is written down here — including the refused prefix, which is the case this
    change most easily gets wrong. The R2 fixture bucket carries the folder
    collisions; an account whose credentials cannot enumerate it carries the
    open-by-name case.
  - Verification: the log in the platform's log directory names each location
    and the cause of each failure

- [ ] 5.4 [dispatch: main] Update the reader-facing documents in this change,
      not after it.
  - Paths: `README.md`, `docs/architecture.md`, `docs/roadmap.md`,
    `docs/requirements-status.md`, `docs/design-language.md`
  - Done criteria: README's "Working today" says a bucket can be opened;
    architecture gains the listing operation and where the folder rules live;
    roadmap moves `XONHO-0006` to landed; requirements-status records prefix
    navigation and the path bar as done and the columns as **partial**, naming
    sorting, resizing and persistence as what is missing.
  - Verification: `grep -n "open" README.md` and read the four documents

- [ ] 5.5 [dispatch: main] Run the close-out review in `AGENTS.md` and write
      the five answers down here.
  - Paths: this file
  - Done criteria: all five questions answered in writing, including what is
    asserted but not verified
  - Verification: the answers are in this file
