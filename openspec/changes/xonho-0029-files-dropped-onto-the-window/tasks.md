# Tasks — XONHO-0029 files dropped onto the window

> Almost all window work, so the window tests carry it. The one that matters
> is not "a drop uploads" — it is that a drop which **cannot** be honoured
> refuses out loud, because the failure mode this change can ship is a drop
> that vanishes, and a vanished drop is indistinguishable from a broken app.
>
> **Routing, decided rather than defaulted.** All `[dispatch: main]`. The work
> is one file (`app.rs`) in sequence, and the destination-meaning decision in
> 2.2 is judgement. `agy` is this workspace's second-priority executor for
> external-ok work and earns nothing here for `XONHO-0028`'s reasons.

## 1. Taking several files

- [x] 1.1 `Upload…` accepts more than one [dispatch: main]
      - Done in `main` (2026-08-27). `multiple: true` — one word, and it was
        the word that left `XONHO-0028`'s queue with nothing to fill it.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: `multiple: true`, and every chosen path taken on rather
    than the last one popped. Test: three chosen files become three queued
    transfers.
  - Verification: `cargo test -p caixonho-gui`

- [x] 1.2 The destination means a folder when there is more than one
      [dispatch: main]
      - Done in `main` (2026-08-27). `core::folder::folder_prefix` beside
        `object_key`, and the difference between them is the point: the empty
        string is **refused** as a key and **accepted** as a folder, because
        an object must be named and "no folder" is a real answer meaning the
        bucket's root.
      - Ablated: making several files share one *key* turns
        `several_files_share_a_folder_and_keep_their_names` red. That version
        uploads three files over each other and leaves one — easy to write by
        accident, and the worst available outcome.
      - A refusal with several files must cost nothing *partly* sent, so the
        keys are built for all of them before any is started, and the test
        asserts the queue is untouched rather than checking a phase.
  - Paths: `crates/caixonho-gui/src/app.rs`,
    `crates/caixonho-core/src/folder.rs`
  - Done criteria: one file keeps `XONHO-0026`'s editable whole key; several
    share a folder and each keeps its own name. Tests: one file's default is
    unchanged from `XONHO-0026`; three files under a typed folder produce
    three keys with that prefix and their own names; a folder that cannot name
    one is refused **and nothing is sent** — asserted on the queue being
    untouched, not on the phase.
  - **Ablate it**: make many files share one *key* instead of one folder, and
    confirm a test goes red. That version silently uploads three files over
    each other and leaves one — the worst available outcome and an easy thing
    to write by accident.
  - Verification: `cargo test -p caixonho-core folder::`,
    `cargo test -p caixonho-gui`

## 2. The drop

- [x] 2.1 Files dropped on the listing are taken on [dispatch: main]
      - Done in `main` (2026-08-27). `on_drop::<ExternalPaths>` over the
        contents pane, and **`Upload…` and the drop converge on
        `offer_upload_of`** — two code paths would have become two
        behaviours, and a drop is meant to be the same act reached with the
        hand instead of the button.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: `on_drop::<ExternalPaths>` over the listing area; every
    path becomes a queued upload to the location on screen. The handler and
    `Upload…` **converge on one function** — a drop is the same act reached
    with the hand instead of the button, and two code paths would become two
    behaviours.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.2 A drop that cannot be honoured refuses, out loud [dispatch: main]
      - Done in `main` (2026-08-27). **The single place is
        `droppable_paths`**, asked by `can_drop`, by the drag-over styling and
        by the handler. Three predicates meant to agree is how a window comes
        to promise a landing it then refuses.
      - Ablated: letting a folder through turns two tests red, including the
        one that checks the three callers still answer alike.
      - A drop outside a bucket says a location is needed. Silence there is
        indistinguishable from a broken application: the user did something
        and nothing happened.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: outside a bucket, refused with "a location is needed"; a
    **folder** dropped, refused with its own reason. Neither uploads
    anything, and the assertion is on the queue being untouched.
  - `can_drop` and the drag-over styling are decided **in one place**: a
    window that shows "will land" and then refuses is worse than one that
    shows nothing. Record here where that single place is.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.3 The window says a drop will land, and where [dispatch: main]
      - Done in `main` (2026-08-27): `drag_over` tints the pane while files
        are over it, gated on the same predicate, and the destination strip
        names where before anything is sent.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: `drag_over` styling while files are over an accepting
    area, and the destination named before anything is sent.
  - Verification: `cargo test -p caixonho-gui`, and looking at the frames

- [x] 2.4 The screenshot harness covers the new states [dispatch: main]
      - Done in `main` (2026-08-27): `bucket-19-upload-many-into-a-folder`
        and `bucket-20-drop-refused`, pixel-distinct.
      - **Looked at, and it caught the thing the task said to look for —
        almost.** *"Upload 6 files into folder:"* is plainly distinguishable
        from *"Upload to:"*, so the words are enough. But the image showed the
        label asking for a folder over a box still hinting `bucket/path/name`
        — the *key* placeholder, unchanged.
      - Small, and exactly the kind of contradiction a reader resolves by
        guessing. The placeholder now changes with the meaning, set on the
        input's state where it belongs rather than at render.
      - Third time this session an image caught what no assertion did.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: several files awaiting one folder, and a refusal — each a
    frame, pixel-distinct, **driven through the controls**.
  - Then **look at them**. Twice this session an image has caught what no
    assertion did, and the specific thing to check here is whether *"Upload 6
    files to:"* and *"Upload to:"* are distinguishable at a glance. If they
    are not, the fix is more words, not a different mechanism.
  - Verification: `cargo test -p caixonho-gui`, and looking

## 3. Close-out

- [x] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-27): fmt and clippy exit 0, 392 core + 99
        window green (8 + 1 ignored).
  - Verification: the commands themselves

- [ ] 3.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 3.3 Live: a handful of files, dropped [dispatch: main]
  - Done criteria: on the owner's machine, drag **more files than
    `XONHO-0028`'s bound** onto the window in one gesture, with **one
    destination already taken** so a collision is asked mid-queue.
  - **This is `XONHO-0028`'s live check too**, and honestly this time: its
    task 3.3 says "at once", and until this change there was no way to mean
    it. Record the result in both.
  - Then: drop outside a bucket, and drop a folder. Both must refuse in words.
  - Verification: what was seen, quoted, with the log's own `took=` lines

- [ ] 3.4 Reader-facing documents [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`
  - Done criteria: §4.4's drag-and-drop row says the **OS → app** half is
    done and the **app → OS** half is not, with the Windows API question kept
    — the two halves have always been one row and only one of them moved. The
    upload row notes many files at once. Counts by the script.
  - Verification: the script's totals match the tables

- [ ] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Done criteria: the five questions answered here. Question 4 in this
    project's form — what did this change do to the evidence? — has a likely
    answer worth checking: a dropped path never went through the picker, so
    anything the picker validated is now unvalidated.
  - Verification: the recorded findings
