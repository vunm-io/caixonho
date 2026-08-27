# XONHO-0029 — Files dropped onto the window

## Why

`XONHO-0028` built a queue and nothing can fill it. `Upload…` opens the
platform picker with `multiple: false` (`app.rs:1299`) and takes one path, so
the only way to put six transfers in a queue is to press the button six times.

That is not a small inconvenience — it makes `XONHO-0028`'s own live check
dishonest. Its task 3.3 says *"upload enough files **at once** to exceed the
bound"*, and six clicks is not at once. Testing it that way would be lowering
the bar until the change passes.

The owner asked for the obvious answer: drag files onto the window.

**`PROJECT_BRIEF.md` §4.4 asks for it as `[M]`**, and the direction matters:

> **[M]** Drag and drop **OS → app**. **App → OS** drag-out of
> *not-yet-downloaded* objects requires `IDataObject` +
> `CFSTR_FILEDESCRIPTOR` on Windows — assume unsupported by the framework
> until proven otherwise

The caveat is about dragging objects *out*. Dragging files *in* is listed
first and carries none.

**And gpui supports it today**, read out of its source rather than assumed —
this project has a rule against saying "unsupported" without citing the file:

- `on_drop::<T>(...)` on any `div` — `gpui/src/elements/div.rs:544`
- `ExternalPaths(SmallVec<[PathBuf; 2]>)` with `paths()` —
  `gpui/src/interactive.rs:685`
- `can_drop(...)` and `drag_over(...)` for refusing and for showing that a
  drop will land — `div.rs:555`, `div.rs:1131`

So one drop delivers every path in one event. This is the mechanism Zed uses
to accept files from Finder.

## What Changes

- **Files dropped on the window are uploaded** to the location on screen,
  each keeping its own name.
- **The window says a drop will land** before it does, and says where.
- **A drop where an upload cannot go is refused, with the reason** — outside a
  bucket there is no destination, and a silent no-op reads as a broken app.
- **`Upload…` accepts more than one file** as well. One line at the picker,
  and the same question about where they go, so it belongs here rather than
  in a change of its own.
- **A folder dropped is refused, saying so.** Uploading a directory tree is
  its own `[M]` — *"preserving prefix structure"* — and quietly uploading
  nothing, or only the files at the top, would both be worse than a sentence.

### The question this change exists to answer

With one file, `XONHO-0026` lets the user type the whole destination key —
a different name, a different path. With ten, typing ten keys is absurd.

So the destination means something different for many: **a folder they all go
into, each keeping its own name.** That is what every file manager does, and
it is a change to what the field *means* depending on how many files there
are — which the screen has to say plainly rather than leaving to be inferred.

### What is deliberately absent

- **App → OS**: dragging an object out to Finder. The brief's own caveat, the
  Windows API question from M0, and a separate direction entirely.
- **Folder upload**, per above: its own `[M]` about preserving prefix
  structure.
- **Dropping onto a specific row** to choose a destination folder without
  navigating. A nice idea and a different feature; the drop target here is the
  location on screen.

## Capabilities

### Modified Capabilities

- `object-transfer`: gains that files may arrive by being dropped on the
  window, where they go, and what is refused rather than half-done.

## Impact

- **`caixonho-core`**: nothing. Every dropped path becomes the same upload the
  queue already runs.
- **`caixonho-gui`**: a drop target over the listing; `upload_here` takes many
  paths; the destination strip means a folder when there is more than one.
- **Dependencies**: none — the API is in the gpui already pinned.
- **Docs**: `docs/requirements-status.md` §4.4's drag-and-drop row and the
  upload row; `docs/roadmap.md`.
- **`[M]` requirements this steps over**: in-app sign-in (`XONHO-0011`), sort
  honesty, server-side prefix search. Same list as `XONHO-0028` stepped over,
  and the same reason — with one addition that makes this stronger than that
  one: **without this, `XONHO-0028` cannot be honestly live-checked at all.**
