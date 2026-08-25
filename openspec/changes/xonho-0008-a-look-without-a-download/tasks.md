# Tasks — XONHO-0008 a look without a download

> Dependency order: the pure sniffs first, then the ranged port, then the
> session, then the surface, then the paperwork. Core is TDD (`AGENTS.md`
> §7); the sniff tests are the ones that earn their keep, because the edges
> (split UTF-8, BOM, empty, extensionless) are where a preview turns into
> noise.

## 1. Kind and truth, as pure functions

- [ ] 1.1 The kind by extension, the truth by content [dispatch: main]
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

- [ ] 2.1 `ObjectStore::get_object_head`, ranged, with the double
      [dispatch: main]
  - Paths: `crates/caixonho-core/src/store.rs`
  - Done criteria: `get_object_head(bucket, key, bytes) ->
    Result<ObjectHead>` where `ObjectHead { total: Option<u64>, body:
    Box<dyn ObjectRead> }` — `total` is the whole object's size, from the
    ranged response, distinct from the head's own length. The double serves
    a scripted head with a scripted total, plus the refusal shape. Red
    first.
  - Verification: `cargo test -p caixonho-core store::`

- [ ] 2.2 The adapter maps it to a ranged `GetObject` [dispatch: main]
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

- [ ] 3.1 `Session::spawn_preview` [dispatch: main]
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

- [ ] 4.1 The Preview action and the surface [dispatch: main]
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

- [ ] 4.2 Staleness, and the harness [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the preview drops on `end_location`, on `go_to` to a
    different location, and on a result arriving for a connection no longer
    active (the `XONHO-0021` guard shape, tested the same way); the text
    and image preview states join the screenshot harness.
  - Verification: `cargo test -p caixonho-gui`, and
    `cargo test -p caixonho-gui -- --ignored every_state`

## 5. Reader-facing documents, in this change

- [ ] 5.1 README, roadmap, requirements-status [dispatch: main]
  - Paths: `README.md`, `docs/roadmap.md`, `docs/requirements-status.md`
  - Done criteria: README says what previews and what honestly refuses;
    roadmap's M3 table gains this change's row; requirements-status §4.5
    preview row moves to **partial** — text and images built, markdown
    *rendering* named as the gap (shown as text today). **Counts by the
    script.**
  - Verification: the script's totals match the tables

## 6. Close-out

- [ ] 6.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [ ] 6.2 CI green on both targets, run id recorded here [dispatch: main]
  - Paths: none
  - Done criteria: all four jobs successful for the tip; run id here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 6.3 Live: a log's first page, a photo, and the refusals
      [dispatch: main]
  - Paths: none
  - Done criteria: on a real account — a large text log previewed with the
    truncation line's numbers checked against the object's listed size; a
    small image drawn; an oversized image refused without a fetch (watch
    the log: no bytes moved); a binary with a text extension called binary;
    and Back landing on a fresh listing. What was seen written here, names
    withheld.
  - Verification: the log shows preview outcomes with no keys

- [ ] 6.4 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: this change
  - Done criteria: the five questions answered and recorded here, question
    2 read the wide way.
  - Verification: the recorded findings
