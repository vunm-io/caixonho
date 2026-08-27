# Design — acting on a row where it is

## Context

Two facts were read out of the table component before this was written, and
they point in opposite directions:

- **Row context menus exist.** `TableDelegate::context_menu(row_ix, …)`
  (`gpui-component .../table/delegate.rs:101`), called by the table with the
  row index. Nothing to build.
- **Multi-selection does not.** `selected_row()`, `selected_col()`,
  `selected_cell()` — every one singular
  (`.../table/state.rs:398`). There is no plural anywhere in that file.

So half of this change is wiring and half is a thing the component will not
give us.

`XONHO-0028`'s queue is the other half of the answer: bounded concurrency,
per-item outcome, one failure contained. Deleting twenty objects is that
machine with a different verb.

## Goals / Non-Goals

**Goals**

- A selection that can hold more than one row.
- Row actions reached deliberately, on the row.
- Bulk delete with a counted confirmation; folder delete as the same act.
- A shorter toolbar as a consequence rather than as a separate tidy-up.

**Non-Goals**

- Copy, move, rename — v0.2 in the brief's own plan.
- Undo after a bulk delete; said out loud rather than omitted.
- A streamed, cancellable count for a prefix holding tens of thousands. Named
  in Open Questions.

## Decisions

### The selection lives in the delegate, beside `shown`

The component offers one selected row and this change needs a set, so the set
is ours. It goes where `shown` already lives, for the same reason `shown` does:
a selection is about *which rows*, and the delegate is what knows which rows
there are.

Two things fall out and both are requirements rather than niceties:

- A selection **holds row identities, not indices**. `shown` is recomputed by
  every narrowing (`XONHO-0025`) and every re-read, and an index that survives
  a re-sort points at a different object. This is `XONHO-0028`'s lesson about
  looking an item up by position, and it is the same defect: silently acting
  on the wrong thing.
- A selection **clears when the location changes**. `end_location` already
  clears the strips; this joins them.

### A folder delete counts before it asks

The brief asks for a confirmation stating the object count, and for a folder
there is no count until something goes and looks. So the confirmation has
three states, not two: counting, counted, and nothing-to-delete.

**Counting must finish before the confirmation can be confirmed.** A dialog
that says "delete 47 objects?" while still counting is offering a number it
may be about to change — and the user's yes was to the number, not to the
prefix.

### Undo is refused for bulk, and refused *out loud*

`XONHO-0021` offers Undo exactly when the delete's own response reports a
marker. Twenty deletes report twenty markers, and restoring some of them is a
half-answer to a question nobody asked.

So a bulk delete says Undo is not offered. Saying it matters more than usual
here: the user has seen Undo appear after a single delete, and its silent
absence would read as a bug rather than a decision — which is exactly what
happened when Undo did not appear on an unversioned bucket and the owner went
looking for the button.

### Delete lives in the context menu, and nowhere a stray click reaches

The owner's decision of 2026-08-24 is the governing one: `Open` is a visible
button and double-click stays unbound, because a stray double-click must not
write company bytes to disk. A hover delete is easier to hit than that.

Hover may carry harmless verbs. This is the one place where "the confirmation
would catch it" is not a good enough argument — a guard is not a reason to
build a trap in front of it.

### The toolbar shrinks as a consequence

Preview, Open, Download and Delete act on a *row* and move to it. Upload, New
folder and the path act on a *place* and stay. That is a real rule rather than
a tidy-up, and it is what stops the row overflowing again — it has been patched
twice with flex properties, which only ever decide who loses.

## Risks / Trade-offs

- **[Right-click is invisible]** → nothing on screen announces a context menu.
  The sidebar has the same problem and answers it with a line of text
  underneath. The tasks make this a decision rather than an omission.
- **[A counted confirmation on a huge prefix]** → counting ten thousand
  objects takes several listings and the user waits at a dialog. Named in Open
  Questions; the honest minimum is that the dialog says it is counting.
- **[Bulk delete is the most destructive thing this application can do]** →
  and it is being added in the same change as a new selection model. The
  counted confirmation, the deliberate reach, and the refusal to offer Undo
  are all guards, and none of them is optional.

## Open Questions

- **A prefix with tens of thousands of objects.** Counting is listings, and
  the user is waiting. Whether that count should be streamed with a running
  total and a cancel, or whether a folder that large should be refused with an
  explanation, is genuinely open. The tasks require the dialog to say it is
  counting, which is the floor, not the answer.
