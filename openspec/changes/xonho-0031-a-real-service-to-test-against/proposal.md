# Acting on a real service, not only on doubles

## Why

The owner said it plainly: *"các tính năng với s3 cơ bản bạn giúp tôi tự làm tự
test được không… miễn bạn test end-to-end flow giúp tôi, khi nào tôi rảnh thì
double check"*. Every change since `XONHO-0007` has ended the same way — built,
green, **awaiting live acceptance** — and the only person who can give that
acceptance is the one person whose time this project is meant to save.

`docs/roadmap.md` currently reads *"awaiting live acceptance"* on eight rows.
That is not a backlog of testing; it is a backlog on one person. The way out is
not to test less, it is to make most of it not need them.

## What is actually untested

The adapter — `S3ObjectStore`, the one file that turns this project's
intentions into HTTP — is exercised two ways today, and neither of them sends a
request:

- `StoreDouble` answers at the port, **above** the adapter. It proves what the
  session and the window do with an answer. It proves nothing about whether the
  request that would have produced that answer is the right one.
- `StaticReplayClient` replays canned bytes **below** the adapter. It proves
  the parsing and the error classification. It proves nothing about whether a
  real service would have sent those bytes.

Between them sits the thing neither covers: *does the request we build actually
mean, to a real S3 implementation, what we think it means?* Every listing,
every conditional write, every continuation token crosses that gap untested.

## What this changes

A real S3 service, in-process, that the tests can talk to over real HTTP.

- `s3s-fs` — a file-backed S3 server in pure Rust — started on a random port by
  the test that needs it. No Docker (the daemon on the owner's machine is not
  running), nothing to install, and it runs on both CI targets.
- Integration tests that drive the **real adapter** against it.
- The GUI's own headless harness pointed at the same server, so a flow runs
  window → session → adapter → HTTP → service and back.

Virtual-hosted addressing works against it without changing any production
code: `reports.localhost` resolves to `127.0.0.1` on macOS, and `s3s` parses
the bucket out of the `Host` header against a base domain.

## What this deliberately does not cover

Named here rather than discovered later, because a green CI that quietly means
less than it looks is worse than a red one.

- **Directory buckets and Local Zones.** Nothing emulates `s3express:
  CreateSession`, the `{base}--{zone}--x-s3` naming, or a directory that
  vanishes with its last object. This is the feature the application exists for
  and it stays the owner's to accept — *"phần s3 local zone tôi tự test"*.
- **Versioning, delete markers, and Undo.** `s3s-fs` has no versioning
  (`s3.rs` has no `put_bucket_versioning` and no delete markers), so
  `XONHO-0021`'s Undo cannot be proven here.
- **Denials.** The service has no IAM, so it refuses nothing. Classification is
  already covered below the adapter by the replay tests, which is the right
  place for it.

## `[M]` requirements

Delivers none directly. `PROJECT_BRIEF.md` §7–8 asks that the project be able
to show its own correctness; this is that, and it is the enabler for the rows
that are built and unaccepted rather than a row of its own.

Unbuilt `[M]` ahead of it, unchanged from `XONHO-0030`: signing in to IAM
Identity Center from the app (§4.1). This goes first anyway because it is what
makes every subsequent change cheaper to accept — including that one.
