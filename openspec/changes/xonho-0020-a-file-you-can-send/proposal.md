# XONHO-0020 — A file you can send

## Why

`XONHO-0007` made the arrow point one way: an object can come out of a bucket
and land on this machine. Everything a person does with an S3 client after
that is the return trip, and the owner named the order themselves —
download, upload, delete. This is the second.

It is also where this application first **writes to someone else's account**,
and that changes what the change is mostly about. Reading is forgiving: a
listing that goes wrong shows the wrong thing and is fixed by asking again.
A write that goes wrong destroys an object that existed. S3's `PutObject`
replaces the key it is given **silently and without asking** — no error, no
confirmation, and on an unversioned bucket no way back. That is the same
failure §4.4 calls the one unforgivable file-manager bug, pointed at the
service instead of the disk, and it is one API call away by default.

So the headline of this change is not "upload". It is that a file lands only
where nothing was, unless the user says otherwise — and that this is
enforced by the service rather than by a check this application performs and
hopes to win.

## What Changes

- **Upload one local file into the location on screen.** The user picks a
  file; it becomes an object under the current bucket and prefix, keyed by
  the file's own name.
- **An existing object is never replaced without being asked** — and the
  protection is **atomic**, not a check-then-write. `PutObject` is issued
  with `If-None-Match: *`, so the *service* refuses the write when the key
  exists (`412`) and the question comes back to the user: replace, keep both
  (a name derived beside it), or abandon. Measured, not assumed:
  `if_none_match` is on the SDK's `PutObject` builder (`aws-sdk-s3` 1.142.0,
  `put_object/builders.rs:727`).
  - Replacing is then a second, deliberate call **without** the condition —
    the only way this application ever overwrites an object is a user
    answering a question about that object.
- **Cancel** stops an upload in flight, on the same cooperative flag
  `XONHO-0007` established.
- **A file too large for one request is refused before it starts**, naming
  what lifts the limit rather than letting the service reject it after the
  upload has been attempted. `PutObject` caps at 5 GiB (brief §4.4);
  multipart is a change of its own.
- **The log records upload outcomes** in the shape it already records
  downloads: bucket, bytes, outcome, cause — and, per the requirement
  `XONHO-0007` wrote, never the key and never the local path.
- **No progress bar this time, and the reason is written down.** See below;
  it is a deliberate deferral, not an oversight.

### What is deliberately absent

- **Progress in bytes.** Measured during planning: `ByteStream::from_path`
  and its `FsBuilder` expose no counting hook, and the counting alternative
  (`ByteStream::from_body_1_x`, public, and the `http-body-1-x` feature is
  already enabled in this graph) means hand-writing an `http_body::Body` that
  the SDK may **rebuild on retry** — a counter that resets or double-counts
  under the retry policy is worse than none. Multipart makes progress fall
  out for free, one tick per part, and each part retries on its own. So
  progress and the size cap are **the same change**, and this one shows the
  file's total with an indeterminate state instead of lying about a fraction.
- Folders and recursive upload, the transfer queue, drag-and-drop, storage
  class or metadata on upload, and delete — each is its own row in §4.4–4.5
  and none of them is made easier by being bolted on here.

## Capabilities

### New Capabilities

None. Upload is the other direction of the capability `XONHO-0007`
introduced, and its proposal said so in as many words: "Upload and the queue
extend this capability in later changes rather than creating another."

### Modified Capabilities

- `object-transfer`: gains the upload direction — sending one local file to a
  location, the no-silent-replace guarantee and the question it produces,
  cancellation, and the pre-flight size refusal. **Ordering constraint:**
  this delta can only be synced or archived *after* `XONHO-0007` archives,
  because `openspec/specs/object-transfer/` does not exist until it does.
  `XONHO-0007` is 14/15, waiting on its own live check.

## Impact

- **`caixonho-core`**: `ObjectStore` grows `put_object` (a local path, a
  key, and whether to refuse an existing key); the adapter maps it to
  `PutObject` with `if_none_match`, and classifies `412` as its own outcome
  rather than an error — a precondition that did its job is not a failure.
  `transfer` grows the upload side and the keep-both naming for *object*
  keys, which is a different namespace from the local filenames of ADR-0004
  and must not reuse that scheme by accident. `diagnostics` gains the upload
  outcome.
- **`caixonho-gui`**: an **Upload…** action beside Download… and Open,
  enabled while a location is open; the existing transfer line grows the
  upload states, reusing the shape rather than a second widget.
- **Dependencies**: none.
- **Docs**: `README.md`, `docs/roadmap.md` M2 row, `docs/requirements-status.md`
  §4.4 rows (the upload half of row 1, and the collision row).
- **Risk worth naming here rather than only in design**: an S3-compatible
  endpoint that ignores `If-None-Match` would silently overwrite while this
  application believes it is protected. That is the one failure mode of the
  chosen mechanism, and the design says what is done about it.
