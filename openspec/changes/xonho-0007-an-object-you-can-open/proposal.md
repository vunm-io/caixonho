# XONHO-0007 — An object you can open

## Why

The application can tell the user everything about an object except what is
in it. Browsing landed with `XONHO-0006` and has now been exercised against
real accounts on both platforms; the first thing the owner asked for after
using it was the next verb: get the object out. A read-only browser that
cannot produce the file it is describing is a dead end one step later than
the bucket list was — and the brief has always priced this as the point of
the milestone: **M2 begins here** (§4.4, staged as `XONHO-0007` in
`planned-changes.md` since M1 was cut).

The second half of the verb arrived with the first. The owner's use for most
objects is *look at it*, not *file it* — and the objects in the real accounts
include kinds no in-app preview should attempt (PDF among them). The brief
already draws this line: preview covers text and images (§4.5, staged as
`XONHO-0008`), and **everything else gets an explicit "download to open"**.
That path is mechanically a download to a managed location handed to the
operating system's own opener — so it belongs to this change, not to the
preview it is often mistaken for. `XONHO-0008` stays what it is: ranged
first-N-KB viewing, later, on top of the plumbing built here.

Doing both halves here also keeps a promise the project made about scope:
rendering other people's file formats is a supply-chain and licensing
commitment (`XONHO-0017` is three days old; PDF renderers are C++ vendoring
or AGPL), and the operating system already owns that job.

## What Changes

- **Download to disk.** A selected object can be saved to a destination the
  user chooses. The write is atomic from the user's point of view: a partial
  download never sits at the final path looking like the real file.
- **Open with the default application.** Opening an object — the double-click
  verb — downloads it to a location this application manages and hands the
  file to the OS opener. No format knowledge, no viewer, no new dependency:
  the same mechanism that already opens the sign-in page opens the file.
- **Progress and cancel, sized to one object.** A download in flight is
  visible (bytes against total when the service stated one) and can be
  abandoned; abandoning leaves no partial file at the destination. The
  transfer *queue* — parallelism, retry policy, throughput, ETA — is the rest
  of M2 and is explicitly not here.
- **Key→filesystem safety, decided once.** S3 keys carry characters and
  shapes filesystems refuse (`: * ? " < > |`, trailing `/`, case collisions).
  The mapping is deterministic, and every collision or substitution is
  reported rather than silently absorbed — the brief calls silent loss the one
  unforgivable file-manager bug. The scheme lands with an ADR as §4.4 asks.
- **A local file that already exists is the user's decision**, never an
  overwrite: this slice ships the honest minimum (ask, offering keep-both),
  and the per-session remembered policy of §4.4 waits for the queue change
  where "remembered" has something to attach to.
- **The log says what transfers did, and still never inventories anyone's
  data.** Outcomes are recorded in the same shape listings already are —
  bucket, counts, sizes, cause on failure — extending the existing rule that
  an object key is the user's own data and does not belong in a file they are
  invited to send to a stranger.

## Capabilities

### New Capabilities

- `object-transfer`: moving object content between the service and this
  machine — for now, service→disk: explicit download to a chosen destination,
  download-to-open through a managed location, single-transfer progress and
  cancellation, the key→filesystem mapping, and the no-partial-file
  guarantees. Upload and the queue extend this capability in later changes
  rather than creating another.

### Modified Capabilities

- `diagnostics`: one requirement is added rather than one changed — the
  no-inventory practice the code already follows (a key is the user's own
  data) exists today as doc-comment law, not spec law. It becomes a
  requirement here, covering transfers before the first transfer ships: the
  log records transfer outcomes the way it records listings, and never an
  object key or a destination path.

## Impact

- **`caixonho-core`**: `ObjectStore` grows a read operation (streaming get
  with a size when known); the adapter implements it over `aws-sdk-s3`'s
  `GetObject`. A transfer module owns destination naming, the sanitization
  scheme, temp-file-then-rename, and cancellation; `diagnostics` gains
  transfer events. TDD applies as everywhere in core.
- **`caixonho-gui`**: row actions and the double-click verb; a minimal
  in-window representation of one transfer in flight (progress, cancel,
  failure with cause). No new panel framework — the queue panel is a later
  change's problem.
- **Dependencies**: none anticipated. Opening uses the platform mechanism the
  app already uses for URLs; hashing/temp naming come from std. If the URL
  opener turns out not to take file paths on either platform, the fallback is
  the platform's own command, and that decision gets measured in design
  rather than assumed.
- **Docs**: `README.md` status paragraph, `docs/roadmap.md` (M2 begins),
  `docs/requirements-status.md` §4.4 rows, ADR for the filename scheme.
- **Not touched**: upload, recursive/folder download, multipart, retry
  policy, bandwidth, presigned URLs, delete — all remain where the roadmap
  has them.
