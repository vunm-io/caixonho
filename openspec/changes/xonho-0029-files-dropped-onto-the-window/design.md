# Design — files dropped onto the window

## Context

The API exists and was read rather than assumed, which this project requires
before anyone says a framework does or does not do something:

- `on_drop::<T>(...)` on any `div` — `gpui/src/elements/div.rs:544`
- `ExternalPaths(SmallVec<[PathBuf; 2]>)`, with `paths()` —
  `gpui/src/interactive.rs:685`
- `can_drop(...)` — `div.rs:555` — to refuse before the drop happens
- `drag_over::<S>(...)` — `div.rs:1131` — to change the styling while a drag
  is over the element

`XONHO-0028` runs the queue and `XONHO-0026` asks where an upload goes. This
change only delivers work to the first and generalises the question the second
asks.

## Goals / Non-Goals

**Goals**

- A drop of many files becomes many queued uploads, in one act.
- A visible answer *before* the drop about whether it will land.
- One destination question that means the right thing for one file and for
  ten.

**Non-Goals**

- App → OS drag-out, folder upload, dropping onto a particular row. Each named
  in the proposal with its reason.

## Decisions

### The drop target is the location, not a row

The whole listing area accepts, and files go to the location on screen. A
per-row target — drop onto `reports/` to land inside it — is a different
feature and a much larger one: it needs a hit-tested destination, a preview of
which row will take it, and an answer for dropping onto a *file*.

The location is where `Upload…` already sends, so a drop is the same act
reached with the hand instead of the button. That equivalence is worth
preserving: two paths to one behaviour, not two behaviours.

### Refusal is `can_drop`, not silence

`can_drop` decides before the drop, so a place that will not take files can
refuse while the cursor is still moving. That is the difference between "this
application ignored me" and "not here".

Outside a bucket there is no destination — the account listing is a list of
buckets, and a bucket is not a folder to put a file in. Dropped there, the
window says a location is needed rather than guessing one.

### A dropped folder is refused, and this is the interesting one

`ExternalPaths` carries paths; a directory is a path. Three options were
weighed:

1. **Upload the files at its top level.** Rejected: it does *some* of what was
   asked, and the user cannot see which part without comparing by hand.
2. **Walk it and upload everything.** That is the `[M]` about *preserving
   prefix structure*, and it brings its own questions — symlinks, hidden
   files, a tree deep enough to need cancelling halfway.
3. **Refuse, saying so.** Chosen. It costs a sentence and leaves the user in
   no doubt.

The first is the tempting one and the worst: silently doing part of a job is
the failure this project's rules keep circling back to.

### The destination field means two things, and says which

One file: the whole key, editable — `XONHO-0026` unchanged.
Many files: the folder they share; each keeps its own name.

The field changing meaning with the number of files is a real hazard. The
mitigation is not cleverness but words: the strip says *"Upload 6 files to:"*
with a folder, against *"Upload to:"* with a key. If that turns out not to be
enough when the frames are looked at, the fix is more words, not a different
mechanism.

### `Upload…` gains `multiple: true` here rather than in its own change

One line at the picker — and it asks exactly the same destination question,
which is the substance of this change. Splitting them would put the question
in one change and half its callers in another.

## Risks / Trade-offs

- **[The drop target is the whole listing]** → dropping while looking at a
  folder you did not mean to be in sends files there. The destination strip
  shows where before anything is sent, which is the same protection a typed
  destination has.
- **[Ten files, ten rows, one bounded queue]** → that is `XONHO-0028`'s
  business and it is built. This change is the first thing that will really
  exercise it, which is the point.
- **[A drag-over style that lies]** → showing "will land" and then refusing
  would be worse than showing nothing. `can_drop` and the styling must agree,
  and the tasks call for them to be decided in one place rather than two.

## Open Questions

None. The one that mattered — whether gpui accepts external file drops at all
— was answered by reading its source before this was written, after a wrong
claim in the other direction had already been made in conversation.
