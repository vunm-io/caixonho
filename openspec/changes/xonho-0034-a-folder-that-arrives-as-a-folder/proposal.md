## Why

**A folder cannot be downloaded at all.** `Download…` acts on one object; on a
folder row it is not offered. Ticking twenty files, which `XONHO-0030` made
possible, offers no download either — the ticks feed delete and nothing else.
So the only way to fetch a prefix is to open it and click twenty times, and the
only way to fetch a subtree is to do that per level.

`PROJECT_BRIEF.md` §4.4 carries it as **`[M]`**: *upload/download files and
folders, preserving prefix structure.* `docs/requirements-status.md` has read
*"Folders and prefix-structure preservation are not started"* since
`XONHO-0007` opened the section on 2026-08-24.

**The pieces are all here.** `XONHO-0028` built the queue — bounded
concurrency, per-item outcome, cancel one or all, retry the failed.
`XONHO-0030` built the flat walk of a prefix and multi-row selection.
`XONHO-0007` built the download itself and the key→filename scheme. What is
missing is the act that composes them.

## What Changes

- **Download a folder.** Its whole subtree, walked flat, queued as one act.
- **Download a selection.** Ticked rows — files, folders, or both — in one act
  with one destination question.
- **The prefix structure is preserved on disk.** A folder arrives as a folder.
  `daily/monday.csv` fetched from a bucket's root lands at
  `<chosen>/daily/monday.csv`, not as a loose `monday.csv`.
- **The key→path mapping is extended from a name to a path.** Every segment
  goes through the scheme `ADR-0004` already fixes; a segment that cannot
  serve takes the same deterministic suffix. **This resolves a collision class
  the ADR explicitly punts**: `a/x.txt` and `b/x.txt` no longer contend for one
  local name, because they no longer share a directory.
- **A collision answer may cover the rest of the act**, when the user says so.
  Downloading two hundred files into a directory that already holds them is
  two hundred questions today, which is a feature nobody would finish using.
- **No ceiling on what a download may walk.** Bulk delete refuses above 5,000
  objects because an unbounded destructive act needs a bound. A download
  destroys nothing, and cancelling one costs the bytes already fetched.

**Not in this change:** uploading a folder. The `[M]` row names both
directions and this delivers one, so the row stays **partial** and says which
half. Multipart and ranged download stay where they are — a folder of large
objects is still capped by the single-request size, per object.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `object-transfer`: gains downloading a folder and downloading a selection as
  single acts, and the rule that prefix structure is preserved on disk. Its
  key→filename requirement is widened from a name to a path. Its collision
  requirement is amended: an answer may still not leak between unrelated
  transfers, but may cover the remainder of the act it belongs to when the
  user chooses that.

## Impact

- **`caixonho-core`**: one pure function beside `transfer::local_name` mapping
  a key to a relative path, and its property tests. No new spawn API — the
  existing per-object `spawn_download` is what the queue calls.
- **`caixonho-gui`**: a download that knows it is one act of many transfers —
  the keys it holds, the destination chosen once, and the collision answer
  chosen once. The row menu and the selection strip each gain the entry.
- **`docs/adr/0004-key-to-filename-scheme.md`**: an amendment. The decision
  does not change; its unit does, from a filename to a relative path, and the
  ADR must say what a broken **middle** segment does — a case it never had.
- **Dependencies**: none.
- **Docs**: `docs/requirements-status.md` §4.4, `docs/roadmap.md`.

### Planning gate

**`[M]` requirements this change delivers:** §4.4 *upload/download files and
folders, preserving prefix structure* — the download half. §4.4 *collision
policy … remembered per session* moves from partial toward done: the answer is
remembered for the act, which is the scope `requirements-status.md` said was
waiting for a queue to attach to.

**`[M]` requirements still unbuilt ahead of it:** in-app IAM Identity Center
sign-in (`XONHO-0011`, §4.1); sort honesty (§4.2); KMS denial told apart from
an S3 denial (§4.3); multipart upload and ranged download (§4.4); retry with
backoff and adaptive concurrency (§4.4); drag and drop app → OS (§4.4).

**Why this goes first anyway.** It is the mandatory row the owner met while
using the application, and it is composition rather than construction — the
queue, the walk and the download all exist and are exercised. `XONHO-0011`
remains the nearest mandatory gap and is unblocked by nothing here; it waits
on live checks only the owner can run.
