# Tasks — XONHO-0031 a real service to test against

> The point of this change is that the owner stops being the only person who
> can accept a generic-S3 flow. So the measure of it is not "tests exist" — it
> is **which roadmap rows can stop saying *awaiting live acceptance***. Task
> 4.2 is where that gets answered, and it is allowed to answer "fewer than we
> hoped".
>
> **Routing.** All `[dispatch: main]`. The work is one new test module plus a
> harness change in a file this session is holding, and the judgement — what a
> flow proves, and what it must not be read as proving — is the whole content.
> `agy` remains this workspace's second-priority executor and earns nothing
> here.

## 1. A service the tests can start

- [x] 1.1 `s3s-fs` as a dev-dependency, and the audit still clean
      [dispatch: main]
  - Paths: `Cargo.toml`, `crates/caixonho-core/Cargo.toml`
  - Done criteria: pinned in `[workspace.dependencies]` beside the others, with
    a comment saying why a test dependency is allowed here at all — `XONHO-0017`
    trimmed this graph deliberately and the next reader deserves the reason.
  - **Verified: `cargo deny check advisories` over the merged graph answers
    `advisories ok`.** 372 crates in `caixonho-core`'s tree, up from 224.

- [x] 1.2 Start one, on a port the OS chooses [dispatch: main]
  - Paths: `crates/caixonho-core/tests/` (new)
  - Done criteria: a helper that binds `127.0.0.1:0`, serves an empty
    filesystem root under a temporary directory, hands back the base URL, and
    stops when dropped. Base domain `localhost`, so virtual-hosted addressing
    resolves.
  - **Port 0, never a constant.** Two tests at once on a fixed port is a
    flake that reads as a product bug.
  - **What cost an hour, and is worth having written down:** the base domain
    must carry the port — `localhost:54321`, not `localhost`. `s3s` matches a
    virtual host with `strip_suffix(base_domain)`, which the port defeats, so
    every bucket request silently fell back to path-style and the service
    answered a *different operation* with HTTP 200. What reached the caller was
    `Unexpected { detail: "the service answered HTTP 200" }` — a message that
    reads like a product defect and is not one. The listener is therefore bound
    before the service is built, because the port is the OS's to choose.
  - Verification: a test that starts one, lists nothing, and stops

## 2. The adapter over real HTTP

- [x] 2.1 Reach it the way the application does [dispatch: main]
  - Paths: `crates/caixonho-core/tests/`
  - Done criteria: a temporary AWS config file naming `endpoint_url` and static
    keys, then `Session::open` — not a hand-built `SdkConfig`. The tier exists
    to test the wiring, and hand-building the configuration tests around it.
  - Verification: one listing that works

- [x] 2.2 Listing, both shapes [dispatch: main]
  - Done criteria: a location read with the delimiter groups into folders and
    objects; the flat walk (`list_keys_under`) returns every key at every
    depth. The same fixture through both, so the *difference* is what is
    asserted rather than each in isolation.
  - Verification: `cargo test -p caixonho-core --test <name>`

- [x] 2.3 Pagination, past one page [dispatch: main]
  - Done criteria: more objects than one page, and the walk reports the total.
    `XONHO-0030` proved this against a scripted double; this proves the
    continuation token the *real* service mints round-trips.
  - **First draft of this test was a lie.** It seeded 25 objects, and the
    adapter sets no `max_keys`, so the service's default of 1000 fetched
    everything in one page — the test passed without ever touching a
    continuation token, while its name reported the requirement as covered.
    1001 objects now, with `pages > 1` asserted; ablated back to 25 and it goes
    red. This is the same failure as `XONHO-0025`'s missing test, caught this
    time before the commit rather than by the owner afterwards.
  - Verification: the test, and the ablation

- [x] 2.4 The conditional write [dispatch: main]
  - Done criteria: a write to a free key lands; a conditional write to a taken
    key comes back as the question, refused **by the service**. This is the
    guarantee `XONHO-0020` claims and the first time anything has checked that
    a real service makes it.
  - Verification: the test

- [x] 2.5 Get, delete, and the round trip [dispatch: main]
  - Done criteria: bytes written come back byte-identical; a delete removes the
    key and the next listing does not show it.
  - Verification: the test

## 3. Flows from the window

- [x] 3.1 Ungate the driving from the photographing [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: opening and driving a headless window works on every
    platform; only `capture_screenshot` stays `#[cfg(target_os = "macos")]`.
  - **This is the task the whole change turns on.** A flow that runs only on
    the owner's platform has not removed the owner from the loop.
  - Verification: `cargo test -p caixonho-gui`, and Windows green in CI
  - **Done by a different route than the design named, and the route is
    the point.** The design said to ungate `shoot_at`. What `shoot_at` gates
    is the *renderer*, and driving needs none: gpui's test platform opens and
    drives a window on every target already, and the hundred-odd `#[gpui::test]`s
    prove it on Windows every push. So the flows run through `TestAppContext`
    over `World::against(&Service)` — a real config file, `Session::open`, a
    real adapter — and `shoot_at` keeps its gate for the one thing that needs
    it. Nothing platform-gated was added. This also kept the flows out of the
    trap the `XONHO-0032` font tests found the same week: two real headless
    platforms in one process abort, and `TestAppContext` builds none.
    **Windows green in CI: run 33960176049.**
  - Two things the flows did not work without, both found by running them:
    the harness moved into the library as `caixonho_core::test_service` behind
    a `test-service` feature (an integration-test module cannot be reached
    from another crate — release graph measured unchanged with
    `cargo tree --edges normal,no-dev`); and `cx.executor().allow_parking()`,
    because gpui's test scheduler records a wake from any other thread as
    non-determinism and every flow died on the first HTTP reply.

- [x] 3.2 Browse: open a bucket and see what is in it [dispatch: main]
  - Done criteria: through `select_profile` and `open_bucket`, against the real
    service — the rows on screen are the objects the service holds.
  - Verification: the test
  - **Done:** `opening_a_bucket_shows_the_objects_the_service_holds` — one
    folder inferred from the delimiter, then the objects, in table order.

- [x] 3.3 Upload, and the collision question [dispatch: main]
  - Done criteria: a local file uploaded through the window appears in the
    service; uploading it again raises the question rather than replacing.
  - Verification: the test
  - **Done:** `a_file_dropped_on_the_window_reaches_the_service_and_a_second_asks`
    — through the drop handler and the proposed destination; the second send
    settles as `KeyTaken` and the service still holds the first bytes, read
    off its filesystem rather than through a listing.

- [x] 3.4 Download, and the bytes [dispatch: main]
  - Done criteria: the file on disk afterwards is byte-identical to the object.
  - Verification: the test
  - **Done:** `a_download_writes_the_object_to_the_chosen_folder_byte_for_byte`
    — through the platform's folder dialog, answered by the test platform,
    rather than around it.

- [x] 3.5 Delete several, and delete a folder [dispatch: main]
  - Done criteria: `XONHO-0030`'s two flows, end to end — tick rows, confirm
    the counted confirmation, and the objects are gone **from the service**;
    then a folder, counted first, and its whole subtree gone.
  - This is the live check the owner was asked for in `XONHO-0030` task 5.3,
    minus the parts only a directory bucket can answer.
  - Verification: the test
  - **Done, as two tests:** `ticked_rows_are_deleted_from_the_service_after_the_counted_confirmation`
    (`Asked::Rows(2)`, then `Went { gone: 2 }`, then the two keys absent from
    the service and the untouched one present) and
    `a_folder_is_counted_first_and_then_its_whole_subtree_is_gone` (the
    confirmation names three keys where the grouped listing shows two rows —
    the defect `XONHO-0030` guards — then `Went { gone: 3 }`).

- [x] 3.6 Preview [dispatch: main]
  - Done criteria: a text object previews its first page from a ranged read
    against the real service.
  - Verification: the test
  - **Done:** `a_text_object_previews_its_first_page_from_the_service` —
    content, `shown` and `total` all from the service's own response.

## 4. What this does and does not buy

- [x] 4.1 A test per exclusion, that fails when the reason stops holding
      [dispatch: main]
  - Done criteria: named tests recording that this service has no versioning
    (so no Undo), no IAM (so no denials), and no directory buckets. Where the
    reason is observable — a delete answering with no version id — assert it.
  - A comment becomes folklore the day the dependency changes. A test does not.
  - **Three, and one of them was found rather than anticipated.** Versioning
    (a delete answers with no marker) and denials (a listing of a bucket
    nobody granted is not refused) were both in the plan. The third was not:
    `s3s-fs` stores a folder marker as a *directory* (`s3.rs:895`) and derives
    common prefixes only from *files* (`s3.rs:1712`), so an empty folder is
    invisible to it — which means `XONHO-0024`'s own scenario cannot be proven
    here. Found by a test failing, and kept as a test rather than deleted.
  - Verification: the tests, and the names reading as the exclusions

- [x] 4.2 Re-state the roadmap honestly [dispatch: main]
  - Paths: `docs/roadmap.md`, `docs/requirements-status.md`
  - Done criteria: go row by row through the eight *awaiting live acceptance*
    rows and say, per row, what is now proven and what is still owed. A row
    whose flow is covered here says so; a row that still needs a directory
    bucket or a versioned bucket **keeps its wording** and names why.
  - **This is where the change is judged.** Overstating here is the failure
    mode, not understating.
  - Verification: the table read against the tests that exist
  - **Done in `docs/roadmap.md`, row by row, and it answered "fewer than we
    hoped" for eight of fourteen.** Proven against a real service, from the
    window: browsing and pagination (`XONHO-0006`), single-object download
    (`XONHO-0007`), the service-made no-clobber refusal (`XONHO-0020`), the
    proposed destination (`XONHO-0026`), text preview with both numbers
    (`XONHO-0008`), bulk and folder delete (`XONHO-0030`). **Kept their
    wording, with the reason named:** `XONHO-0018` (nothing here redirects),
    `XONHO-0019`/`XONHO-0023`/`XONHO-0025`/`XONHO-0027` (one connection, no
    narrowing driven), `XONHO-0021`'s Undo (no versioning), `XONHO-0024` (an
    empty folder is invisible to `s3s-fs`), `XONHO-0028`'s many-at-once (each
    flow runs one item). Directory buckets and Local Zones stay the owner's on
    every row. Requirements-status notes updated to match; no State cell
    changed, so the counted summary is untouched.

- [x] 4.3 Say what it costs [dispatch: main]
  - **Measured.** The two new binaries cost **0.74s + 0.72s**; the whole
    workspace suite runs in **~13.6s** wall-clock including compilation, and
    the test phases total under 4s. Seventeen whole-stack flows for one and a
    half seconds, because the service is in-process and the tree is tiny.
  - The one to watch is `a_walk_past_one_page…`, which seeds 1001 objects on
    the filesystem. It is the slowest at ~0.8s and it is the price of that
    test being honest rather than passing on a single page.
  - Verification: `cargo test --workspace` timings

## 5. Close-out

- [x] 5.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - **And a gate check**: walk the file for macOS-gated items and confirm each
    reference sits inside one. Local clippy compiles one branch, so it is not
    evidence about the other target — `XONHO-0030` learned that from a red
    Windows build.
  - Verification: the commands, and the gate walk
  - **Done 2026-09-05.** fmt clean; `clippy --workspace --all-targets -D
    warnings` clean; 539 tests pass; `cargo deny check advisories` ok. Gate
    walk: every reference to `shoot`, `shoot_at`, `capture_screenshot` and
    `judgement_dir` sits inside `a_real_view_renders_to_an_image`, `shoot`,
    `shoot_at` or `every_state_is_written_for_judgement`, each gated; the
    flows reference none of them.

- [x] 5.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`
  - **Run 33960176049** — `build (windows-latest)` and `build (macos-latest)`
    both `success`, both artifacts uploaded. The core tier's own first CI
    (`33231193386`) was red on Windows for a reason unrelated to the tier's
    claims — `*.localhost` does not resolve there — fixed on `main` in
    `1ea55e7` before this section landed.

- [x] 5.3 Close-out review per `AGENTS.md` [dispatch: main]
  - Question 4 has a known shape here: this change's whole product *is*
    evidence, so the question becomes what the new evidence is worth — and in
    particular whether any existing test is now redundant, or whether any of
    them was only ever passing because a double was kinder than a service.
  - Verification: the recorded findings
  - **1. Asked or convenient?** Asked, with one recorded departure: the
    design named `shoot_at` as the harness to reuse and the flows use
    `TestAppContext` instead — written into 3.1 above with the reason, and it
    delivers the design's actual requirement (every platform) more directly.
  - **2. Reader-facing documents.** `docs/roadmap.md` re-stated (4.2);
    `docs/requirements-status.md` notes updated; `docs/planned-changes.md`
    re-audited on 2026-09-05 in its own commit (`c5a457d`) — the R2 section
    had described a fixed defect as live for four weeks. README unchanged: it
    describes the app, not its tests.
  - **3. Rubbish.** `tests/service.rs` is gone rather than left as a shim;
    its `#![allow(dead_code)]` went with it, since a library module has no
    per-binary dead code. `Service::holds`/`bytes_of` are used by two flows.
    Nothing commented out, no `TODO`s.
  - **4. Asserted but not verified.** The eight rows in 4.2 that kept their
    wording are exactly this list, each with its reason. Beyond them: the
    right-click menu is driven by calling `delete_row`/`preview_row`, not by
    a click — the menu's wiring is asserted by the existing double-backed
    tests only; `cx.open_with_system` is a no-op on the test platform, so
    "open" is a download proven and an open assumed; every flow runs against
    `s3s-fs`, and what a directory bucket, a versioned bucket or IAM would do
    is proven nowhere here. No existing test became redundant — the doubles
    still isolate a defect faster than an end-to-end run, and two of the
    exclusion tests were only found by this tier.
  - **5. Left, and written where.** The `wght`/Windows question is recorded
    in `XONHO-0032` task 1.2; the endpoint field `StoredCredential` lacks is
    in `planned-changes.md`; a versioned second service (MinIO, a daemon)
    stays this change's open question in `design.md`.
