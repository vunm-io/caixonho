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

- [x] 2.1 The delegate holds a set, keyed by identity [dispatch: main]
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

- [x] 2.2 A selection belongs to its location [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: leaving the location clears it, beside the strips
    `end_location` already clears. Test it.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.3 A way for the user to actually tick a row [dispatch: main]
  - Paths: `crates/caixonho-gui/src/views/objects.rs`
  - **Not foreseen when this was planned, and it is not extra scope — it is a
    hole the plan left.** The design settled where a selection *lives* and
    said nothing about how a person *makes* one, on the assumption that
    cmd-click would come from the component. It does not: `TableEvent::
    SelectRow(usize)` reports no modifier keys, so there is nothing to read a
    cmd-click out of.
  - Done criteria: a tick column, first, with a tick-all in its header.
    Tested through the delegate.
  - Verification: `cargo test -p caixonho-gui`

## 3. The row's own actions

- [x] 3.1 A context menu per row [dispatch: main]
  - Paths: `crates/caixonho-gui/src/views/objects.rs`
  - Done criteria: `TableDelegate::context_menu` implemented — Preview, Open,
    Download, Delete for an object; for a folder, only what applies. **No
    delete on hover and no delete on double-click**: the owner's decision of
    2026-08-24 governs, and a guard is not a reason to build a trap.
  - Verification: `cargo test -p caixonho-gui`

- [x] 3.2 Right-click is discoverable [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - **Decided: one muted line under the table**, in the voice the "More to
    come." strip beside it already uses — "Right-click a row to preview, open,
    download or delete it. Tick rows to delete several."
  - Why that and not the alternatives: a tooltip needs the pointer to already
    be where the reader does not know to put it, and a toolbar hint would put
    back the row this change just emptied. The sidebar solved the same problem
    the same way, so the window now says it twice in one voice rather than
    once in two.
  - Verification: visible in every `bucket-09*` frame

- [x] 3.3 The toolbar keeps only what acts on a place [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: Preview, Open, Download and Delete leave the row of verbs;
    Upload, New folder and the path stay. The rule is *acts on a row* versus
    *acts on a place*, and it is what stops that row overflowing a third time.
  - Verification: `cargo test -p caixonho-gui`, and
    `bucket-09e-narrow-window-with-a-long-bucket-name` — 900px, a Local Zone
    bucket name, a tick in force. The harness gained a width parameter for it.
    **It found something**: the clipped trail landed flush against the red
    `Delete`, reading as one broken control. Fixed with a `pr` gap. What it
    also showed and this change did *not* fix is recorded below.

## 4. Deleting more than one

- [x] 4.1 A confirmation that states the number [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: more than one object confirms by **count**; exactly one
    still confirms by **key**, because a count of one is weaker than a name.
    Tests for both, and one asserting **nothing is sent before confirming**.
  - Verification: `cargo test -p caixonho-gui`

- [x] 4.2 A folder counts before it asks [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: three states — counting, counted, nothing-to-delete. The
    confirmation **cannot be confirmed while counting**: a dialog offering a
    number it may still change is asking for a yes to the wrong question.
    Tests: all three states; and confirming is refused mid-count.
  - **Ablate it**: allow confirmation while counting and confirm a test goes
    red.
  - Verification: `cargo test -p caixonho-gui`

- [x] 4.3 Many deletes go through the queue [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the deletes are queued, bounded, each with its own outcome,
    one failure contained. Test that a refusal in the middle leaves the rest
    deleted.
  - Verification: `cargo test -p caixonho-gui`

- [x] 4.4 A bulk delete says Undo is not offered [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: after more than one, no Undo — and the outcome **says so**.
    Silence would read as a bug: the owner has seen Undo appear after a single
    delete, and went looking for the button the one time it correctly did not
    appear.
  - Verification: `cargo test -p caixonho-gui`

- [x] 4.5 The screenshot harness covers the new states [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a multi-row selection, a counted confirmation, a folder
    still counting, and a bulk outcome with one failure — each a frame,
    pixel-distinct, driven through the controls.
  - Then **look at them**. Five times now an image has caught what no
    assertion did — `bucket-09e` makes it five.
  - Verification: `cargo test -p caixonho-gui`, and looking

## 5. Close-out

- [x] 5.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
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

- [x] 5.4 Reader-facing documents [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`, `README.md`
  - Done criteria: §4.5's create/delete row says bulk and folder are built and
    recursive-across-nested is what remains; a roadmap row; and **README** —
    which this session already found badly stale once, because it belongs to
    no change and so nobody re-reads it.
  - Verification: the script's totals match the tables

- [x] 5.5 Close-out review per `AGENTS.md` [dispatch: main]

**1. Did we build what was asked, or what was convenient?**

What was asked is in the owner's own words and all of it is here: the toolbar
Delete is for a multi-selection, and a single row is acted on where it is by
right-click. Two departures, both written down rather than done quietly.

- The owner offered a **second** option for a single row — icons appearing on
  hover, "nút preview cũng là 1 icon hình con mắt". Delete did not go there.
  Their own decision of 2026-08-24 governs, and a delete icon under the
  pointer is easier to hit than the double-click that decision already
  refused. Recorded in `design.md` and said to the owner, not decided in
  silence. **Preview and Open on hover remain open and unbuilt** — the ask was
  "either/or" and only one half was taken, which is a narrowing the owner
  should get to overrule.
- The **tick column** is not in the plan because the plan assumed cmd-click
  would come from the component. It does not. Added as task 2.3, with the
  reason, and the design amended.

**2. Do the reader-facing documents still tell the truth?**

`README.md` gained two bullets — bulk and counted-folder delete, and the rule
that a row is acted on where it is. This mattered: without them the README
still described a toolbar with Preview, Open, Download and Delete on it, which
is a screen that no longer exists. `docs/requirements-status.md` §4.5's
create/delete row moves from "bulk and recursive not started" to what is now
built, and names what is still left — the 5000 ceiling, and that copy/move do
not exist to move things out of an over-large prefix first. Totals re-run and
unchanged (`scripts/count-requirements.sh`), because the row stays *partial*.
`docs/roadmap.md` gained its row. `docs/design-language.md` was read and needs
nothing: it describes a strip's voice, and the strips added speak in it.

**3. Did we leave rubbish?**

Two things, both removed rather than noted:

- `Tally::keys()` was written as the type-level guard against a partial walk
  becoming a delete list — and then the window pattern-matched instead, so its
  only caller was one test assertion. A guard nothing is behind is decoration.
  Gone; the test asserts the shape directly.
- `DeletePhase::is_confirmable` — same story, one line, never called.

**4. What is asserted but not verified?** — and the defect this found

The question as this project asks it — *what did this change do to the
evidence?* — had the answer I guessed at planning time, and a worse one
underneath it.

The guessed one: the four verbs moved from `selected_row()` to a row index, so
`the_object_verbs_light_up_only_on_an_object` and
`preview_gates_on_an_object_selection` were asserting a gate that no longer
exists. Both rewritten to ask the new question rather than deleted.
`delete_gates_on_an_object_selection` had lost its subject entirely — a folder
*is* deletable now — so it became
`deleting_a_row_that_is_not_there_does_nothing`, which is what is still true.

The worse one, found by reading rather than by a failing test: **an abandoned
bulk delete left its keys in the queue.** `start_ready_deletes` reads the
bucket from `self.deletion`, and the queue was only ever pruned of *finished*
items — so six keys queued against `reports`, dismissed, then a delete
confirmed in `logs`, would have sent all seven to `logs`. Fixed by forgetting
the queue with the deletion, in one place that four paths now call; tested, and
ablated back to the prune-only version to confirm the test bites.

Still asserted and not verified, all of it live-only: that the context menu
opens at all (the harness cannot right-click), that a real directory bucket's
folder disappears with its last object, and every count against a real prefix.
Task 5.3.

**5. What is left, and where is it written?**

- **The path trail truncates from the wrong end** — keeps the bucket the
  sidebar is already showing, throws away the path that says where you are.
  Found by `bucket-09e`, recorded in `design.md` under "Found while building",
  not fixed: eliding a breadcrumb has its own decisions in it.
- **Preview and Open as hover icons** — the half of the owner's second option
  not taken. Above.
- **A prefix past 5000 objects** is refused rather than walked. That is this
  change's answer to its own open question, and it is a floor rather than a
  solution: the way out is copy/move, which is v0.2.
- Live acceptance: task 5.3, unrun.
