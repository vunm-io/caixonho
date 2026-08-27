# Tasks — XONHO-0030 acting on a row where it is

> This change adds **the most destructive thing this application can do** in
> the same breath as a new selection model. The tests that carry it are the
> guards: a selection that cannot drift onto the wrong object, a count that
> cannot be confirmed before it is finished, and a delete that cannot be
> reached by one stray click.
>
> **Routing, decided rather than defaulted.** All `[dispatch: main]`. Counting
> a prefix is core and is judgement-shaped; everything else is one file
> (`app.rs`) in sequence, where an external executor cannot own the file this
> session is holding. `agy` remains this workspace's second-priority executor
> and earns nothing here.

## 1. Counting what a folder holds

- [x] 1.1 Count the objects under a prefix [dispatch: main]
  - Paths: `crates/caixonho-core/src/store.rs`,
    `crates/caixonho-core/src/session.rs`
  - Done criteria: something that walks a prefix and reports how many objects
    are under it, page by page, cancellable. Red first against `StoreDouble`.
    Tests: a prefix with no objects reports zero; one spanning several pages
    reports the total, not the first page; cancelling stops the walk.
  - **A count that stops early must not be reported as a total.** That is the
    failure this task exists to prevent — a confirmation saying "47 objects"
    when the walk gave up at 47 of 3000.
  - Verification: `cargo test -p caixonho-core`

## 2. A selection that holds more than one

- [ ] 2.1 The delegate holds a set, keyed by identity [dispatch: main]
  - Paths: `crates/caixonho-gui/src/views/objects.rs`
  - Done criteria: a selection beside `shown`. It holds **keys, not row
    indices**. Tests, and the second is the one that matters:
    - several rows can be selected and read back;
    - **re-narrowing or re-reading the listing does not move the selection
      onto different objects** — select two, change what is shown, and the
      same two objects are still the ones selected.
  - **Ablate it**: store indices instead of keys and confirm the second test
    goes red. `XONHO-0028` shipped exactly this defect in its first draft, in
    a different file, and it is silent when it happens.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.2 A selection belongs to its location [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: leaving the location clears it, beside the strips
    `end_location` already clears. Test it.
  - Verification: `cargo test -p caixonho-gui`

## 3. The row's own actions

- [ ] 3.1 A context menu per row [dispatch: main]
  - Paths: `crates/caixonho-gui/src/views/objects.rs`
  - Done criteria: `TableDelegate::context_menu` implemented — Preview, Open,
    Download, Delete for an object; for a folder, only what applies. **No
    delete on hover and no delete on double-click**: the owner's decision of
    2026-08-24 governs, and a guard is not a reason to build a trap.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 3.2 Right-click is discoverable [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: nothing on screen announces a context menu. The sidebar
    answers this with a line of text underneath. **Decide what this pane does
    and write down why here** — an invisible feature is one nobody uses, and
    "it is a convention" is how a convention nobody knows stays unknown.
  - Verification: the recorded decision, and the frames

- [ ] 3.3 The toolbar keeps only what acts on a place [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: Preview, Open, Download and Delete leave the row of verbs;
    Upload, New folder and the path stay. The rule is *acts on a row* versus
    *acts on a place*, and it is what stops that row overflowing a third time.
  - Verification: `cargo test -p caixonho-gui`, and looking at the frames at a
    **narrow window** — the two overflow defects this fixes were both only
    visible below the width the harness renders at.

## 4. Deleting more than one

- [ ] 4.1 A confirmation that states the number [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: more than one object confirms by **count**; exactly one
    still confirms by **key**, because a count of one is weaker than a name.
    Tests for both, and one asserting **nothing is sent before confirming**.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 4.2 A folder counts before it asks [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: three states — counting, counted, nothing-to-delete. The
    confirmation **cannot be confirmed while counting**: a dialog offering a
    number it may still change is asking for a yes to the wrong question.
    Tests: all three states; and confirming is refused mid-count.
  - **Ablate it**: allow confirmation while counting and confirm a test goes
    red.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 4.3 Many deletes go through the queue [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the deletes are queued, bounded, each with its own outcome,
    one failure contained. Test that a refusal in the middle leaves the rest
    deleted.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 4.4 A bulk delete says Undo is not offered [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: after more than one, no Undo — and the outcome **says so**.
    Silence would read as a bug: the owner has seen Undo appear after a single
    delete, and went looking for the button the one time it correctly did not
    appear.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 4.5 The screenshot harness covers the new states [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a multi-row selection, a counted confirmation, a folder
    still counting, and a bulk outcome with one failure — each a frame,
    pixel-distinct, driven through the controls.
  - Then **look at them**. Four times this session an image has caught what no
    assertion did.
  - Verification: `cargo test -p caixonho-gui`, and looking

## 5. Close-out

- [ ] 5.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Verification: the commands themselves

- [ ] 5.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 5.3 Live: delete several, and delete a folder [dispatch: main]
  - Done criteria: on the owner's machine, on a **directory bucket**: select
    several throwaway objects and delete them; then delete a folder holding
    more than one. Expected — the confirmation states the count, the deletes
    run through the queue, and on a directory bucket **the folder disappears
    with its last object** (documented, and watched once on 2026-08-27).
  - Then the case that must not be silent: confirm that no Undo is offered and
    that the screen says why.
  - **Use throwaway objects.** This is the first live check in this project
    that destroys more than one thing at a time.
  - Verification: what was seen, quoted, with the log's own lines

- [ ] 5.4 Reader-facing documents [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`, `README.md`
  - Done criteria: §4.5's create/delete row says bulk and folder are built and
    recursive-across-nested is what remains; a roadmap row; and **README** —
    which this session already found badly stale once, because it belongs to
    no change and so nobody re-reads it.
  - Verification: the script's totals match the tables

- [ ] 5.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Done criteria: the five questions here. Question 4 in this project's form
    — what did this change do to the evidence? — has a likely answer worth
    checking: a selection is new state that every existing window test was
    written without, so tests that assumed one selected row may now be
    asserting less than they read as.
  - Verification: the recorded findings
