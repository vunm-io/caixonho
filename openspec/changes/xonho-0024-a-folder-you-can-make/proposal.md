# XONHO-0024 — A folder you can make

## Why

There is no way to make a folder. The owner went looking for the button
during a live check, and it is not there — `Preview`, `Open`, `Download…`,
`Upload…`, `Delete…` and nothing else. Every organising act therefore leaves
the app for the AWS console or the CLI.

`PROJECT_BRIEF.md` §4.5 carries it as **`[M]`**: *Create "folder" (zero-byte
marker)*. `docs/requirements-status.md` records that row as **partial** —
`XONHO-0021` delivered single-object delete, and says in as many words that
"bulk, recursive and create-folder are not started". This is the
create-folder part.

## What Changes

- **A folder can be made where the user is standing.** A name, a new empty
  location, and the listing showing it.
- **On a general purpose bucket that is a zero-byte object** whose key is the
  folder's name plus `/`. This is what the brief means by "marker", what the
  AWS console itself does, and the only thing that makes an empty folder
  visible in a store that has no folders.
- **On a directory bucket it is not, and cannot be.** This was checked
  against AWS's own documentation before the design was written, and it
  changed the design:

  > Directories are created during `PutObject` or `CreateMultiPartUpload`
  > operations and **automatically removed when they become empty** after
  > `DeleteObject` or `AbortMultiPartUpload` operations.

  An empty directory does not survive on a directory bucket — the service
  deletes it the moment it empties. So a "folder" made there would vanish
  between the making and the next listing, and an app that offered the button
  anyway would be lying twice: once when it said the folder was made, and
  again when the folder was gone with no explanation.

  **This is not a corner case for this project's owner**: the account they
  use daily is entirely directory buckets (`--x-s3`), so the naive
  implementation would be broken on the only account that matters to them.

- **What is offered there instead is the thing that actually works**: a
  folder becomes real when something is put in it, so the destination is
  chosen at upload time rather than beforehand. The design decides the exact
  shape; the proposal fixes only that a directory bucket must not be handed
  a button that cannot work.

### What is deliberately absent

- Rename, copy and move. Same section of the brief, each its own change, and
  rename in particular is copy+delete with a UI that has to say it is not
  atomic.
- Bulk and recursive delete. `XONHO-0021` named the counted confirmation as
  belonging to bulk, and it still does.
- Nested creation in one act (`a/b/c/`). One level is what a file manager
  gives you and what the listing can show.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `bucket-listing`: gains what it means to make a folder, what a folder *is*
  in each of the two bucket kinds, and what the app must do rather than offer
  a button that cannot work.

## Impact

- **`caixonho-core`**: `ObjectStore` gains a create-folder operation;
  `Session` gains its spawn; the bucket kind already crossing the port for
  the listing is what decides the two paths.
- **`caixonho-gui`**: a `New folder…` button beside the five verbs, a name
  prompt, and the refusal-with-an-alternative on a directory bucket.
- **Dependencies**: none — `PutObject` is already in use by `XONHO-0020`.
- **Docs**: `docs/requirements-status.md` §4.5's create-folder row;
  `docs/roadmap.md`'s M3 table.
- **`[M]` requirements this steps over**, per the planning gate: §4.2's
  virtualized-table claim is still unmeasured on a long listing, §4.1 still
  has no in-app sign-in (`XONHO-0011`), and §4.4's transfer queue does not
  exist. This goes first because it is small, because the owner asked for it
  while using the app, and because it is the last piece of §4.5's *safe*
  subset that writes nothing destructive.
