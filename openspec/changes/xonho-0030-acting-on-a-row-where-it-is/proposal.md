# XONHO-0030 — Acting on a row where it is

## Why

The owner asked for this twice while using the application, and the second
time framed it exactly:

> the Delete button should be for a multi-selection; for a single file, either
> right-click like Windows Explorer, or icons that appear on the row

Both halves are right, and the reasons are not only taste.

**The toolbar has outgrown its row.** Seven verbs sit beside a breadcrumb that
can carry a sixty-character directory-bucket name. That row has been patched
twice — once when `Type a location` was clipped off the end, once when the
bucket's name drew *on top of* the buttons — and flex properties can only
decide who loses. Moving per-row actions off the toolbar shortens it for good.

**Nothing can be selected but one row.** `Delete…` acts on one object;
deleting a folder is refused entirely (`app.rs:1033`), and the reason is that
it means deleting every object under a prefix — an unbounded, destructive act
that `XONHO-0021` deliberately deferred to bulk, where the brief requires a
**confirmation stating the object count**.

**And bulk is now cheap where it was not.** `XONHO-0028` built the queue: many
operations, bounded concurrency, per-item outcome, a failure contained to its
own item. Deleting twenty objects is that machine wearing a different verb
rather than a second one.

`PROJECT_BRIEF.md` §4.5 carries it as **`[M]`**: *delete single/bulk/recursive
with a confirmation stating the object count.*

## What Changes

- **Right-click a row for the things you can do to it.** The table component
  already offers this — `TableDelegate::context_menu(row_ix, …)` — so this
  extends a seam rather than inventing one, and the sidebar's saved
  connections already use the same pattern.
- **Select more than one row**, and act on the selection.
- **Delete several objects at once**, through a confirmation that says **how
  many** — the count the brief asks for, which a single-object confirmation
  naming one key never needed.
- **Delete a folder**, which is the same act: every object under that prefix,
  counted before anything is sent.
- **The toolbar keeps only what acts on a place** — upload, new folder, and
  the path. What acts on a row moves to the row.

### Where the delete lives, and why not on hover

The owner's own decision of 2026-08-24 settles this: `Open` is a visible
button and double-click stays unbound, because *a stray double-click must not
be enough to write company bytes to disk*. A delete icon under the cursor is
easier to hit by accident than a double-click.

So **destructive actions live in the context menu only**. Hover may carry the
harmless ones. `XONHO-0021`'s confirmation would catch a misclick, but a guard
is not a reason to lay a trap.

### What is deliberately absent

- **Copy, move and rename.** Same section of the brief, and the brief itself
  puts them in v0.2. Rename is copy+delete with a UI that has to say it is not
  atomic.
- **Recursive delete across nested prefixes as a distinct promise.** A folder
  delete counts and removes what is under it; whether that walk should be
  streamed and cancellable at ten thousand objects is a question this change
  answers only as far as the count it shows.
- **Undo for a bulk delete.** `XONHO-0021` offers Undo when the service's
  response proves a marker exists. For many objects that is many markers and a
  partial restore is its own design; a bulk delete says plainly that it is not
  offered.

## Capabilities

### Modified Capabilities

- `object-deletion`: gains deleting more than one object, what the
  confirmation must state when it is more than one, what a folder delete means,
  and what is **not** offered afterwards.

## Impact

- **`caixonho-core`**: counting what is under a prefix before deleting it;
  many deletes through the queue rather than one through `spawn_delete`.
- **`caixonho-gui`**: a selection that can hold more than one row — the table
  component has `selected_row()` and no plural, so the set lives in the
  delegate beside `shown`; a context menu per row; the toolbar loses its
  row-acting verbs.
- **Dependencies**: none.
- **Docs**: `docs/requirements-status.md` §4.5's create/delete row;
  `docs/roadmap.md`.
- **`[M]` requirements this steps over**: sort honesty, server-side prefix
  search, multipart. The owner has met the wall this removes and has not met
  theirs.
