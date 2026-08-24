# Design — an object you can open

## Context

Browsing is live against real accounts; the store port ends at
`list_objects`. This change adds the first operation that moves object
*content*, and the first that writes to the user's disk. Everything here
lands behind the existing seams: the port grows one method, the session grows
one spawn, the window grows one in-flight representation. The constraints
that shaped every earlier change apply unchanged — core is TDD'd against
doubles, the GUI stays exploratory, no new dependency without an argument,
and the log never inventories the user's data.

## Goals / Non-Goals

**Goals**

- One object, service → disk, with the file appearing only when whole.
- The open verb: download to a managed cache, hand to the OS opener.
- Visible progress and working cancellation for the one transfer.
- The key→filename scheme, decided once, recorded as an ADR.

**Non-Goals**

- The transfer queue, parallel transfers, retry/backoff, throughput, ETA —
  the rest of M2 attaches to a queue this change deliberately does not build.
- Folder/recursive download, upload, ranged preview (`XONHO-0008`), presigned
  URLs, delete.
- Collision policy *remembered per session* (§4.4): the remembered half needs
  the queue; this change ships ask-per-collision.

## Decisions

### The port grows `get_object`, streaming, not a byte-vector

`ObjectStore` gains one method that hands back a stream of chunks plus the
size when the service states one. A `Vec<u8>` return would be simpler and
wrong twice: a 10 GB object must not sit in memory, and progress reporting
needs the bytes to pass through somewhere countable. The double implements it
over canned chunks, which is also what lets cancellation and the atomicity
rule be unit-tested without a network.

The adapter maps it to `GetObject`; the body is already a stream there. The
size for progress is the response's own content length, falling back to the
listed size — the object may have changed since the listing.

### Written through a working path, promoted by rename

Content lands in `<final-name>.caixonho-partial` beside the destination (same
volume, so the promote is a rename, not a copy), and the final path appears
only on completion. Failure and cancel paths remove the working file; a crash
can leave one behind, which is why the name is unmistakably ours and why the
next download to the same destination replaces it rather than resuming it.
Resume is a queue-era feature.

### Cancellation is a flag between chunks, cleanup is a guard

*(Revised while implementing 3.2 — the original said "abort the task", and
the diagnostics delta is what overruled it: an aborted task is not alive to
log `download cancelled` or deliver the outcome, and a cancel that leaves
the log silent contradicts the spec written one file over.)*

The GUI holds a `Cancel` handle; the pump checks it between chunks, so a
cancel lands within one chunk's worth of bytes and the task itself cleans
up, logs and delivers. The working file's removal lives in a drop guard on
the writer, not in the happy path's tail — so the file goes away on cancel,
on error, and on panic alike, and the promote-by-rename disarms the guard.
No fuller token protocol: one transfer, one flag. The queue change can grow
it.

### Opening uses the platform's own verb, already in the toolkit

`gpui::App::open_with_system(&Path)` exists and is implemented on both
targets (`gpui/src/app.rs:1550`; macOS `platform.rs:916` via NSWorkspace,
Windows `platform.rs:612` via ShellExecuteW) — measured, not assumed, the
same way `open_url` was verified before `XONHO-0011` used it. So the open
verb is: download to cache dir, then `open_with_system`. `reveal_path`
(`app.rs:1545`) serves the no-opener report — "it is here" — without this
application learning any file format. No new dependency.

### The open-cache is a bounded directory the app owns

Opens land under the platform cache directory
(`~/Library/Caches/caixonho/open` · `%LOCALAPPDATA%\caixonho\cache\open`),
namespaced per bucket the way the log directory is namespaced per app. Bound:
on startup, entries older than a fixed age are removed — the same
sweep-on-open pattern the log's own rotation uses, no daemon, no setting.
The spec's rule is "bounded, not the user's job"; a startup sweep satisfies
it with one function.

### The filename scheme (ADR lands with the code)

Percent-encoding for the characters Windows refuses plus the separator and
control bytes; a trailing `/` or a key that folds to a case-collision gets a
deterministic suffix derived from the full key. Substitution and collision
are *reported* per the spec — the mapping never silently merges two keys.
The scheme is one pure function in core, property-tested against the shapes
§4.4 names, and the ADR records it because §4.4 asks for exactly that.

## Risks / Trade-offs

- **[One transfer at a time]** → honest for this slice; the UI keeps the
  door open by rendering "a transfer", not "the transfer panel". The queue
  change replaces the holder, not the port.
- **[Rename across volumes fails]** → the working file sits beside the
  destination, same directory, so promote is same-volume by construction.
- **[The open-cache holds company data on disk]** → it already does the
  moment any opener writes a temp copy; the cache is at least *ours*,
  bounded, and under the OS user's own profile. Documented in the README's
  status paragraph rather than hidden.
- **[Progress without a stated size]** → bytes-so-far alone; the spec's
  scenario only promises a fraction when the service stated a size.

## Open Questions

None held open on purpose. The opener existence, the cache location
convention, and the atomicity mechanism were all resolved by measurement or
by precedent above; what remains is executing them in order.
