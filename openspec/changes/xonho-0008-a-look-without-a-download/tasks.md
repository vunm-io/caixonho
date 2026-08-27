# Tasks — XONHO-0008 a look without a download

> Dependency order: the pure sniffs first, then the ranged port, then the
> session, then the surface, then the paperwork. Core is TDD (`AGENTS.md`
> §7); the sniff tests are the ones that earn their keep, because the edges
> (split UTF-8, BOM, empty, extensionless) are where a preview turns into
> noise.

## 1. Kind and truth, as pure functions

- [x] 1.1 The kind by extension, the truth by content [dispatch: main]
      - Done in `main` (2026-08-25), red first: five tests on `todo!()`
        bodies. `RasterKind` is core's own enum rather than
        `gpui::ImageFormat` — this crate names no UI type, and the GUI maps
        at its edge. The truncation tolerance leans on `from_utf8`'s own
        distinction: `error_len() == None` is "ended mid-character", and
        only that shape, only when the caller says a cut happened.
  - Paths: `crates/caixonho-core/src/preview.rs` (new module),
    `crates/caixonho-core/src/lib.rs`
  - Done criteria: `kind_of(key) -> PreviewKind` (`Text | Image { format } |
    None`) over the two extension sets, case-insensitive, extensionless →
    `None`; `text_of(bytes, truncated: bool) -> TextVerdict`
    (`Text(String) | Binary`) — strict UTF-8 with exactly one tolerated
    truncated tail character when `truncated`, NUL anywhere → `Binary`, BOM
    stripped. Red first; tests cover: a UTF-8 character split at the cut, a
    BOM'd file, an empty object, an extensionless key, a `.log` full of
    NULs.
  - Verification: `cargo test -p caixonho-core preview::`

## 2. The port reads a head

- [x] 2.1 `ObjectStore::get_object_head`, ranged, with the double
      [dispatch: main]
      - Done in `main` (2026-08-25), red first (compile-red plus three
        tests). The double's head is the first N of its scripted content
        with the content's size as the total — the relationship the real
        service maintains, so one scripted body keeps both reads consistent
        for free.
  - Paths: `crates/caixonho-core/src/store.rs`
  - Done criteria: `get_object_head(bucket, key, bytes) ->
    Result<ObjectHead>` where `ObjectHead { total: Option<u64>, body:
    Box<dyn ObjectRead> }` — `total` is the whole object's size, from the
    ranged response, distinct from the head's own length. The double serves
    a scripted head with a scripted total, plus the refusal shape. Red
    first.
  - Verification: `cargo test -p caixonho-core store::`

- [x] 2.2 The adapter maps it to a ranged `GetObject` [dispatch: main]
      - Done in `main` (2026-08-25). `total` from `content_range`'s
        after-slash half; `*` or absent → `None`. **One reading tightened
        from the plan:** the 416 arm answers `total: Some(0)` rather than
        parsing the 416's own header — a range starting at byte 0 is
        unsatisfiable in exactly one case, the empty object, so the total is
        known by implication and the error-path header never needs
        touching. `range_unsatisfiable()` sits beside the 412 and 301
        readers on `SdkFailure`. Live test `this_machine_reading_a_head`
        prints both numbers.
  - Paths: `crates/caixonho-core/src/adapter.rs`
  - Done criteria: `range: bytes=0-{n-1}`; `total` parsed from
    `content_range`'s `/total` half (absent or `*` → `None`); a 416
    (unsatisfiable range — the empty object) answered as an empty head with
    the total the response names, not an error; failures through the
    existing classifier as `s3:GetObject`; learned-region addressing shared.
    One `#[ignore]`d live test reading the head of a real object and
    printing both numbers.
  - Verification: `cargo clippy --workspace --all-targets -- -D warnings`,
    `cargo test -p caixonho-core`

## 3. The session previews

- [x] 3.1 `Session::spawn_preview` [dispatch: main]
      - Done in `main` (2026-08-25), red-covered by seven tests. The double
        gained a get counter so the two no-fetch promises are assertions
        rather than prose: the oversized image and the unserved kind both
        show `gets_served() == 0`. The lying stream is cut at the gate with
        an honest cause naming the way out.
      - Logging follows the nothing-moved rule: `NoPreview` and
        `ImageTooLarge` write no line; `Binary` logs the page it moved to
        find that out; the no-key test covers the text path at the most
        detailed level.
  - Paths: `crates/caixonho-core/src/session.rs`,
    `crates/caixonho-core/src/preview.rs`,
    `crates/caixonho-core/src/diagnostics.rs`
  - Done criteria: one spawn that routes by `kind_of` — text: head fetch +
    `text_of`, delivering `PreviewOutcome::Text { content, shown: u64,
    total: Option<u64> } | Binary`; image: gate against the listed size
    first (`ImageTooLarge { size }` without fetching), then `get_object`
    gathered into memory with the gate enforced **during** the gather (the
    stream is not trusted to match the listing), delivering
    `Image { bytes, format }`; unsupported kind: `NoPreview` without any
    fetch. `IMAGE_PREVIEW_LIMIT` = 20 MiB, a constant.
    `diagnostics::preview_settled`: bucket, bytes fetched, outcome, cause —
    no key, asserted with `assert_undisclosed`.
  - Verification: `cargo test -p caixonho-core`, including: the oversized
    image fetches nothing (double records no get), the lying stream is cut
    at the gate, and the no-key log test

## 4. The window

- [x] 4.1 The Preview action and the surface [dispatch: main]
      - Done in `main` (2026-08-25). One correction made against the plan
        *during* implementation rather than after: the first cut of the
        surface replaced the whole contents pane, path bar included — the
        plan says "path bar stays", and it now does, so every verb remains
        reachable while a preview is open. The truncation line renders
        exactly when total exceeds shown, with both numbers through the
        shared `readable`.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a Preview action with the object-selection gating,
    placed with the benign verbs; `preview: Option<Preview>` carrying its
    connection; `contents()` renders the preview in place of the table when
    present — loading, text (monospace, with the truncation line "first
    {shown} of {total}" exactly when total exceeds shown), image
    (`gpui::Image` from the delivered bytes), and the three refusals
    (binary / too large / no preview for this kind), each offering Open.
    Back returns via the same `go_to` re-read the deletion strip uses.
    Selectors: `preview-action`, `preview-surface`, `preview-back`.
  - Verification: `cargo test -p caixonho-gui`

- [x] 4.2 Staleness, and the harness [dispatch: main]
      - Done in `main` (2026-08-25). The lifecycle mirrors the deletion
        strip's and is tested with the same shapes: a same-location re-read
        keeps the preview, departure and `leave_bucket` drop it, and an
        outcome for a left connection is dropped whole. The text and binary
        states are in the harness: 20 images.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the preview drops on `end_location`, on `go_to` to a
    different location, and on a result arriving for a connection no longer
    active (the `XONHO-0021` guard shape, tested the same way); the text
    and image preview states join the screenshot harness.
  - Verification: `cargo test -p caixonho-gui`, and
    `cargo test -p caixonho-gui -- --ignored every_state`

## 5. Reader-facing documents, in this change

- [x] 5.1 README, roadmap, requirements-status [dispatch: main]
      - Done in `main` (2026-08-25). The §4.5 preview `[S]` row enters as
        **partial** with markdown rendering named as the gap; the summary
        prose recounts §4.5 at 5 rows. The not-there-yet line swaps
        "previewing an object" for the narrower truth, "rendered markdown
        preview". Counts by the script.
  - Paths: `README.md`, `docs/roadmap.md`, `docs/requirements-status.md`
  - Done criteria: README says what previews and what honestly refuses;
    roadmap's M3 table gains this change's row; requirements-status §4.5
    preview row moves to **partial** — text and images built, markdown
    *rendering* named as the gap (shown as text today). **Counts by the
    script.**
  - Verification: the script's totals match the tables

## 6. Close-out

- [x] 6.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-25): fmt checked, clippy zero at
        `-D warnings`, 334 core + 62 window green (8 + 1 ignored).
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [x] 6.2 CI green on both targets, run id recorded here [dispatch: main]
      - Run `32818501688` on `dc7cc2e` (this change's final code commit):
        `build (windows-latest)`, `build (macos-latest)`, `dependency audit`
        and `rustfmt` all success.
  - Paths: none
  - Done criteria: all four jobs successful for the tip; run id here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [x] 6.3 Live: a log's first page, a photo, and the refusals
      - Done on the owner's machine, 2026-08-27, on a real account.
      - Includes the way *out* of a preview, which 6.4 fixed the same week:
        the breadcrumb had no effect while a preview was open, because
        `go_to` was serving two meanings at once. Confirmed working.
      [dispatch: main]
  - Paths: none
  - Done criteria: on a real account — a large text log previewed with the
    truncation line's numbers checked against the object's listed size; a
    small image drawn; an oversized image refused without a fetch (watch
    the log: no bytes moved); a binary with a text extension called binary;
    and Back landing on a fresh listing. What was seen written here, names
    withheld.
  - Verification: the log shows preview outcomes with no keys

- [x] 6.4 Close-out review per `AGENTS.md` [dispatch: main]
      - Run 2026-08-25, before the live check as with the last three.
      - **Q1, two departures caught while the change was warm, both now
        held to:** the first surface cut replaced the path bar the plan said
        stays (fixed in 4.1, noted there), and the too-large refusal
        hardcoded a sentence where the spec says *states the size* — clippy
        flagged the unread `size` field, and the flag was the requirement
        going unmet, not a field going spare. The refusal now says the size
        through the shared formatter.
      - **Q2 both directions:** README/roadmap/requirements in the commits
        that made them true; the not-there-yet line narrowed to the exact
        remaining gap (rendered markdown). Rows either side: `0007`, `0020`,
        `0021` all still *awaiting live acceptance*, correctly.
      - **Q3:** no dead API; the double's `gets_served` counter has three
        asserting callers; nothing commented out; the probe additions are
        refusals with reasons, not stubs.
      - **Q4, named residue for 6.3:** the truncation line's numbers against
        a real ranged response (the double's total is scripted, the
        service's is parsed from `content_range` — the parse itself has no
        unit test against a real header string, which is exactly what the
        `#[ignore]`d head test prints for eyeballing); gpui's decode of a
        real image's bytes (the unit path stops at handing bytes to
        `gpui::Image`, which defers decode to render); and the 416
        empty-object arm, which no canned test drives.
      - **Q5:** everything undone is a row in requirements-status or this
        file's 6.3; nothing lives only in conversation.
  - Paths: this change
  - Done criteria: the five questions answered and recorded here, question
    2 read the wide way.
  - Verification: the recorded findings

- [x] 6.4 Leaving a preview: the breadcrumb had no effect [dispatch: main]
      - **Found live by the owner, 2026-08-26, and it shipped with this
        change.** Previewing an object at a bucket's root, clicking the bucket
        in the breadcrumb did *nothing at all* — no error, no movement, no
        sign anything had been clicked.
      - The cause was one function serving two meanings. `go_to` cleared the
        preview only when the location **changed**, a carve-out written for
        `XONHO-0021`'s deletion strip, which genuinely must survive the
        re-read it triggers. The bucket crumb walks to the location you are
        already at, so it took the re-read branch — and left the preview
        standing over the listing it had just refreshed.
      - **This change tested the wrong half of it.** A test here asserted "a
        re-read is not a departure", which is true and still is; nothing
        asserted what a *navigation* to the current location should do. The
        behaviour was deliberate and guarded, and the defect lived in the gap
        between the two meanings rather than in either of them.
      - Fixed by naming both: `go_to` is the door the user's clicks come
        through and always ends a preview — asking for a location, even the
        one you are standing in, is asking to see what is *in* it. The three
        internal refreshes (a deletion's outcome ×2, a made folder) now call
        `re_read_location`, which is the old behaviour unchanged. The existing
        test moved to that name, because it is what the assertion was always
        about; its wording is kept verbatim.
      - Ablated: dropping the clear from `go_to` turns the new test red and
        leaves the re-read test green.
      - **Still open, and it is the owner's other half of the report:** they
        also said there was "no back or close button". There *is* a `Back` at
        the left of the second row — but it is ghost text sitting directly
        under a breadcrumb of ghost text, so it reads as another crumb rather
        than as the way out. Not fixed here; recorded in
        `docs/planned-changes.md` because a discoverability fix is a design
        decision, not a bug fix.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the bucket crumb, and every other navigation, ends a
    preview; the deletion strip's re-read still does not.
  - Verification: `cargo test -p caixonho-gui`

