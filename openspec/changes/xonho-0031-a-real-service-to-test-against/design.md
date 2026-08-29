# Design — a real service to test against

## Context

Four facts were established before this was written, each by looking rather
than by recalling.

- **`s3s-fs` 0.15.0 implements what these tests need.** `list_objects_v2`,
  `list_objects_with_delimiter`, `list_objects_recursive`, `get_object`,
  `put_object`, `delete_object`, `delete_objects`, `head_object`,
  `create_bucket`, `copy_object`, multipart.
- **It honours `If-None-Match`** — `s3.rs:820`, answering `PreconditionFailed`
  with "Object already exists". `XONHO-0020`'s whole no-clobber guarantee is
  therefore provable here, which was the outcome most in doubt.
- **It paginates properly** — `continuation_token`, `max_keys`, `is_truncated`
  and `next_continuation_token` are all real (`s3.rs:663–768`). The folder walk
  and "Load more" have something to walk.
- **It has no versioning.** No `put_bucket_versioning`, no delete markers.
  `XONHO-0021`'s Undo cannot be proven here, and this is why.

And two about addressing, because they decide whether production code has to
change:

- `reports.localhost` resolves to `127.0.0.1` on macOS.
- `s3s` parses a bucket from the `Host` header against a configured base domain
  (`s3s/src/host.rs`).

Together those mean **virtual-hosted addressing works against a local server**,
so no `force_path_style` and no production change. That was the one thing that
could have made this expensive.

## Goals / Non-Goals

**Goals**

- The adapter exercised over real HTTP.
- One whole flow per built capability, from the window's controls.
- Nothing to install, no daemon; green on both CI targets.
- What is *not* covered stated where the tests are.

**Non-Goals**

- Directory buckets and Local Zones. The owner's own words: *"phần s3 local
  zone tôi tự test"*, and nothing could emulate it anyway.
- Versioning and Undo. Would mean MinIO, which means a daemon.
- Denials. No IAM here; classification is already tested below the adapter.
- Replacing any existing test. The doubles stay. This is a tier, not a
  substitution — a double still isolates a defect faster than an end-to-end run.

## Decisions

### The service runs in-process, not in Docker

Docker is installed on the owner's machine and its daemon is not running, which
is the ordinary state. A test tier whose first act is "start Docker Desktop" is
a tier that gets skipped, and a skipped check is worse than an absent one
because it still reads as coverage in CI configuration.

`s3s-fs` is a library. It binds a port and serves; the test starts it, uses it,
and drops it. Windows and macOS runners need nothing added.

**Cost, measured rather than assumed**: 148 crates, and `cargo audit` over its
graph reports zero vulnerabilities — `cargo deny check advisories` under this
repository's own `deny.toml` answers `advisories ok`. `XONHO-0017` spent real
effort getting this workspace's audit to zero and that must not be undone by a
test dependency; it is not.

### The tests go through the whole stack, including config loading

Not `S3ObjectStore::over(config, …)` with a hand-built configuration. A
temporary AWS config file naming an `endpoint_url` and static keys, then
`Session::open` — the same path the application takes.

This costs a few lines and buys the tier the thing it exists for. Building the
`SdkConfig` in the test would prove that *a* correctly-configured adapter works
while leaving untested whether the application configures one — and this
project has already shipped a defect of exactly that shape, where a store was
built and dropped so the connection lost what the listing had learned.

### The window's harness is reused, not rebuilt

`XONHO-0030` left `shoot_at` driving a real window headlessly with a
`StoreDouble` behind it. The same harness with a real service behind it *is*
the end-to-end test, and building a second one would leave two harnesses to
keep working.

That harness is `#[cfg(target_os = "macos")]` today, because capturing an image
needs a renderer. **These tests must not inherit that gate**: a flow that runs
only on the owner's own platform is the problem this change exists to solve.
So the driving is separated from the photographing — the window is opened and
driven everywhere, and only the screenshot stays macOS-only.

### What is not covered is a test that says so

Not a comment. A test named for the exclusion, asserting the reason still
holds where that is possible — for example that `s3s-fs` answers a delete with
no version id, which is exactly why Undo cannot be exercised here.

A comment saying "versioning is not supported" becomes folklore the day the
dependency gains it. A test that fails when the reason stops being true does
not.

## Risks / Trade-offs

- **[An end-to-end test that fails tells you less]** → and that is the standard
  objection to this tier. The mitigation is that it is a *tier*: the doubles
  stay, and they are what isolates. This one answers a different question —
  whether the pieces that each work still work together.
- **[A bound port is shared state]** → port 0, and the OS names it. Two tests
  running at once must not collide, and a hard-coded port is how they do.
- **[The suite gets slower]** → measured on the first run and stated. If a
  whole-stack flow costs more than a second, that is worth knowing before there
  are twenty of them.
- **[Green stops meaning what people think]** → the largest risk here, and the
  reason the third requirement exists. Eight roadmap rows say "awaiting live
  acceptance"; this change will let some of them say something better, and it
  must not let any of them say more than is true.

## Open Questions

- **Whether the versioned flows are worth a second service.** MinIO would cover
  Undo, at the cost of a daemon and a download in CI. Left open deliberately:
  the answer depends on how often Undo changes, and the honest position today
  is that it changed once and has been stable since.
