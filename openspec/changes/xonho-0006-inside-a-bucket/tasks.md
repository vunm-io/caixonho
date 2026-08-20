## 1. Make room before building

- [x] 1.1 [dispatch: main] Take out of `caixonho-gui/src/app.rs` (1113 lines)
      everything that can leave it as a **pure move**, and keep new browsing
      code out of it by writing that code in `views/` from the start.
  - Paths: `crates/caixonho-gui/src/app.rs`, `crates/caixonho-gui/src/views/`
  - Done criteria: the parts of `app.rs` that do not touch application state
    live beside `views/buckets.rs` and `views/credential_form.rs`, with bodies
    byte-identical to what they were. `cargo test --workspace` passes
    unchanged.
  - Verification: `cargo fmt --all --check && cargo clippy --workspace
    --all-targets -- -D warnings && cargo test --workspace`
  - **Amended 2026-08-20, on discovering the original wording could not be
    satisfied.** It asked for a pure move *and* for view rendering to live in
    `views/`. In Rust those two cannot both hold here: everything still
    rendering in `app.rs` is a method reading private fields of
    `CaixonhoApp` — 72 uses of `self.` — and a sibling module cannot see them.
    Moving that code needs either `pub(crate)` on some eighteen fields, which
    makes every one of them writable from anywhere in the crate, or a
    signature for each function listing what it needs (`sidebar` would take
    six parameters). Both are changes of design, not movements of lines.
  - What was done instead: `guidance_for` and `unavailable_reason` — the two
    functions that never touch `self` — moved to `views/failure.rs` with their
    five tests (`10863b1`). `app.rs` is 922 lines.
  - Why stopping there is right rather than convenient: this task exists to
    make room, and the room is made. Browsing adds to `app.rs` one field, one
    branch in `body`, and calls into view modules that are new and therefore
    born in `views/`. Widening a struct's encapsulation across the crate to
    relocate rendering that already works would spend the very thing this
    split protects, to buy a line count.
  - Left open deliberately: turning the view methods into functions with
    explicit inputs is worth doing — it would make them testable — but as a
    change whose purpose that is. Recorded in `docs/planned-changes.md`.

## 2. The port, and the rules that make a listing correct

- [x] 2.1 [dispatch: main] Add the domain types a listing needs.
      - `Prefix` is a newtype rather than a `String`, and that is the load-
        bearing decision: S3 makes `photos` and `photos/` different requests
        with different answers, so a prefix normalised only at some call sites
        is a defect waiting for the one that forgets. Every value of the type
        has one shape — empty, or ending in `/` — enforced on the way in.
      - `Prefix::segments` is where a breadcrumb trail comes from, which is why
        no trail is stored: it is a reading of the location, not a second
        record that could disagree with it.
      - `Object::name_within` returns the empty string when a key *is* the
        prefix being listed — the marker case — so 2.2 has something exact to
        drop rather than a heuristic to guess with.
      - 8 tests, all failing shapes included; 182 pass in core.
  - Paths: `crates/caixonho-core/src/types.rs`
  - Done criteria: a location (bucket + prefix), a page (child prefixes,
    objects, whether more remains and how to ask for it), a folder, and an
    object carrying key, size, last-modified, storage class and ETag. Storage
    class and ETag are carried even though `XONHO-0006` renders neither —
    design.md, "The port gains one operation".
  - Verification: `cargo build -p caixonho-core`

- [x] 2.2 [dispatch: main] Write the folder-inference rules as pure functions,
      test-first, before any adapter calls them.
  - Paths: `crates/caixonho-core/src/` (beside the adapter)
  - Done criteria: three rules hold, each with its own test — an entry whose
    key equals the current prefix is that folder and never an entry within
    itself; a prefix with no object behind it is still a folder; an object and
    a prefix sharing a name both survive. Fixtures mirror the real cases
    recorded in `docs/planned-changes.md`.
  - Verification: `cargo test -p caixonho-core`
      - Proved the tests can fail before trusting them: removing the marker
        rule turns `a_folder_marker_is_the_folder_rather_than_an_entry_inside_
        itself` red with exactly the defect it guards — `photos/` listed among
        the children of `photos/`. Restored, and green.

- [x] 2.3 [dispatch: main] Extend the `ObjectStore` port with the listing
      operation and give the test double a canned implementation.
  - Paths: `crates/caixonho-core/src/store.rs`
  - Done criteria: the trait gains one operation taking a location and an
    optional cursor and returning a page; the double in that module's tests can
    return a page, a truncated page, and each failure cause.
  - Verification: `cargo test -p caixonho-core`

- [x] 2.4 [dispatch: main] Implement it in the adapter over `ListObjectsV2`.
  - Paths: `crates/caixonho-core/src/adapter.rs`
  - Done criteria: `delimiter=/`, the prefix, and the continuation token are
    sent; common prefixes become folders and contents become objects, with 2.2
    applied; the request shape is asserted in a test without a network, as
    `list_buckets_request` already is.
  - Verification: `cargo test -p caixonho-core`

- [x] 2.5 [dispatch: main] Make a refused location its own outcome, and prove
      it is never emptiness.
      - **`classify.rs` needed no change**, which is the finding rather than a
        shortcut: the adapter sends a listing failure through the same
        classifier and the same `s3:ListBucket` context the probe already
        used, so a refusal was already a typed cause before this task began.
        What was missing was the proof at this level, not the behaviour.
      - Established by `a_refused_location_is_an_error_and_never_an_empty_page`
        and `every_other_cause_stays_itself_across_the_port_too` in
        `store.rs` — the second matters as much as the first: an expired
        session, a network failure and a trust failure must each stay
        themselves, because a window that collapsed them would send the user
        to fix a sign-in that is fine.
  - Paths: `crates/caixonho-core/src/classify.rs`,
    `crates/caixonho-core/src/store.rs`
  - Done criteria: a listing refused on authorization grounds is an error
    carrying that cause, never an empty page — the same guarantee
    `a_denied_listing_is_an_error_never_an_empty_list` already makes for
    buckets, now for prefixes. Expired session, network and trust failures each
    stay themselves.
  - Verification: `cargo test -p caixonho-core`

- [x] 2.6 [dispatch: main] Parse and render a location as text.
      - Both the service's `s3://bucket/prefix/` and the same without the
        scheme, because someone reading a bucket name off a console types the
        short form and refusing it would be pedantry. `design.md` left this
        open; there is no ambiguity to fear, since a location always names a
        bucket first.
      - **Exactly one way to fail: naming no bucket.** A name this application
        dislikes is not a failure — what is a valid bucket name is the
        service's judgement, and pre-refusing one it would have accepted is
        declaring where `ADR-0002` says to observe. A name the service rejects
        comes back as a service failure with a cause of its own.
      - 6 tests, including that what is written reads back.
  - Paths: `crates/caixonho-core/src/types.rs`
  - Done criteria: pure functions both ways, with tests for a bucket alone, a
    bucket with a prefix, a trailing separator present and absent, and text
    that names nowhere. No network, no UI.
  - Verification: `cargo test -p caixonho-core`

## 3. Reading a location, without blocking the window

- [x] 3.1 [dispatch: main] Carry the listing across the session bridge.
      - **The session now keeps the open connection's store**, installed
        alongside the probe scheduler so the two can never come from different
        connections, and cleared together when a connection opens or fails to.
        Without it every folder entered would re-open the connection and
        re-resolve its credentials — seven seconds on this machine, twenty-six
        on the first run of a day, both measured. `XONHO-0004` took that wait
        out of startup; browsing must not put it back once per folder.
      - **Reading before choosing a connection is a mistake, not an empty
        folder.** It returns `MissingConfiguration`, and a test holds it there:
        this is the same distinction the whole project turns on, at the one
        place where returning an empty page would have been easiest.
      - `diagnostics::location_settled` names the bucket and prefix and counts
        what came back — **never a key**. A key is the user's own data, and a
        log they may send to a stranger has no business carrying an inventory
        of it.
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

- [x] 3.3 [dispatch: main] Probe prefixes through the machinery that already
      exists.
      - **No capability model changed, and no spec did either** — which is what
        the task predicted and what `XONHO-0005` had already paid for.
        `Scope::prefix` existed, with the reasoning already written down: a
        policy may grant a prefix where the bucket root is denied, and neither
        is evidence about the other.
      - What was added is one conversion, `Scope::at(&Location)`, and it exists
        because of a trap: a bucket's root is `Scope::bucket`, **not**
        `Scope::prefix` with an empty string. Those are different scopes and
        they hash differently, so a frontend building the second by hand would
        file an observation the bucket list could never find again — leaving
        the row at "checking…" over an answer that had already arrived. Doing
        it in one place with a test is what stops that ever being written.
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
