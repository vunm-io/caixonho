# XONHO-0026 — A destination you choose

## Why

`Upload…` decides where the file lands and does not ask. The key is the
prefix you are standing in plus the local file's own name
(`app.rs:1150`), and nothing on screen can change either half.

The owner found this the way it is meant to be found — by trying to use the
application. Asked to make a folder on their work account, `XONHO-0024` tells
them a directory bucket keeps a folder only while something is in it and to
upload into the path they want instead. **Then `Upload…` gives them no way to
say a path.** The advice is correct and the application does not implement
it.

That is a hole `XONHO-0024` opened and should have closed. The observation
was written down while planning it — *"on a directory bucket the useful
feature is choosing the destination key at upload time"* — and then left as a
note instead of a number, which made `XONHO-0024` half a feature on the only
account its owner uses daily.

The macOS file dialog's own **New Folder** button is worth naming, because it
looks like the answer and is not: it makes a folder on the local disk, in
`~/Downloads`. Nothing in that dialog concerns S3.

## What Changes

- **The destination is offered, filled in, and editable.** Choosing a file
  proposes `<where you are><the file's name>` — today's behaviour, now
  visible — and the user may change it before anything is sent.
- **Typing a path is how a folder is made on a directory bucket.** Sending to
  `uploads/2026/report.csv` creates `uploads/` and `2026/` as part of the
  write, which is what AWS's own documentation says to do and the only thing
  that works there.
- **A destination that cannot be a key is refused before anything is sent**,
  in the same voice `XONHO-0024` refuses a folder name.
- **Everything `XONHO-0020` guarantees still holds.** The no-clobber
  guarantee is made by the service (`If-None-Match`) against whatever key is
  sent, so replace / keep-both / abandon keep working on a typed destination
  exactly as on a derived one.

### What is deliberately absent

- Uploading more than one file, and uploading folders. Both are `[M]` in
  §4.4 and both need the transfer queue that does not exist yet.
- A picker that browses the bucket to choose the destination. The path bar
  already navigates; a second navigator inside an upload is a second answer
  to a question the window has answered.
- Remembering the last destination. It would be wrong more often than right:
  the place you are standing is the better default, and it is already the
  default.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `object-transfer`: gains that the destination of an upload is chosen rather
  than derived, what it defaults to, and what a destination that cannot be a
  key does.

## Impact

- **`caixonho-core`**: validation for a destination key, beside
  `folder::key_for` — the same question about a different shape, and one
  module should own both.
- **`caixonho-gui`**: the upload strip gains a destination field; the key is
  read from it rather than composed at `app.rs:1150`.
- **Dependencies**: none.
- **Docs**: `docs/requirements-status.md` §4.4's upload row;
  `docs/roadmap.md`'s M2 table; the parked note in `docs/planned-changes.md`
  gets its outcome.
- **`[M]` requirements this steps over**: the transfer queue, multipart, and
  retry with backoff — all §4.4, all larger. This goes first because it is
  small, because it is the only way to organise objects on a directory
  bucket, and because `XONHO-0024` currently gives advice the application
  cannot take.
