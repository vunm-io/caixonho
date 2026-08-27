# XONHO-0028 — A queue that holds more than one

## Why

The window holds one transfer. Not one at a time by policy — one by type:

```rust
/// The one download in flight or just settled, if any (`XONHO-0007`).
transfer: Option<Transfer>,
```

`Option`, not a collection (`app.rs:66`). Every "one file at a time" the owner
has met flows out of that single line, and so do three unbuilt `[M]`
requirements that are waiting for it rather than for themselves.

`PROJECT_BRIEF.md` §4.4 asks for it directly:

> **[M]** Transfer queue panel: per-item and aggregate progress, throughput,
> ETA, pause, resume, cancel, retry-failed, clear-completed.

And three more §4.4 rows cannot start until it exists. **Multipart upload**
splits one object into parts that run concurrently — each is a transfer
needing its own progress. **Retry with backoff and adaptive concurrency**
sheds parallelism when the service throttles, and the brief names the trigger:
*"many small files into one prefix"* — you cannot shed what you never had.
**Drag and drop** delivers ten files in one gesture and needs somewhere to put
them.

The owner reached the same edge from the other side while live-checking on
2026-08-27, asking for multi-selection so `Delete…` could act on more than one
row. Bulk delete is the same machine wearing a different verb: many operations,
bounded concurrency, per-item outcome, and an honest report when the fourth of
twenty fails.

## What Changes

- **The window holds a queue, not a transfer.** Many run at once, up to a
  bounded number; the rest wait their turn.
- **A panel lists them**, each with what it is, where it is going, how far it
  has got, and how it ended.
- **Aggregate progress**: how many are done of how many, and what the whole
  queue is moving.
- **Cancel one, cancel the rest, retry what failed, clear what finished.**
- **One failure does not stop the queue.** The others carry on and the failed
  one waits with a reason and a retry.

### What is deliberately absent, and why each

- **Pause and resume**, which the brief's line names. A `PutObject` in flight
  cannot be paused: the request is one stream, and stopping it is cancelling
  it. Honest pausing needs **multipart** — a separate `[M]` — where parts not
  yet sent can simply not be sent yet. Offering a Pause that quietly cancels
  and restarts from zero would be worse than not offering one. Deferred with
  its reason rather than faked.
- **Throughput and ETA**, also named on that line. Both are arithmetic over
  progress this change will finally have, and both are guesses presented as
  numbers — they deserve their own thinking about how wrong they are allowed
  to look. The queue is the prerequisite; they are not the point of it.
- **Multipart, adaptive concurrency, drag and drop.** The three rows waiting
  on this one. Each stays its own change.
- **Bulk delete.** Same machine, different verb, and it writes destructively —
  it needs the counted confirmation `XONHO-0021` deferred to it, which is a
  design question of its own.
- **Surviving a restart.** A queue that resumes after the application is
  quit needs somewhere durable to record intent, and an answer for a local
  file that moved in between. Named as absent so nobody assumes it.

## Capabilities

### Modified Capabilities

- `object-transfer`: gains that more than one transfer may be in flight, what
  a queue guarantees about the ones that are not, how a failure is contained,
  and what may be done to a queue as a whole.

## Impact

- **`caixonho-core`**: the session's spawn methods already return a `Cancel`
  and report progress through a channel — the shape survives. What is new is a
  bounded runner: something that holds waiting work, starts it as slots free,
  and tags every event with which item it belongs to.
- **`caixonho-gui`**: `transfer: Option<Transfer>` becomes a queue; the strip
  becomes a panel. The five transfer phases stay as they are — they describe
  *one* transfer and they still will.
- **Dependencies**: none.
- **Docs**: `docs/requirements-status.md` §4.4's queue row; `docs/roadmap.md`.
- **`[M]` requirements this steps over**: in-app sign-in (`XONHO-0011`, where
  the AWS CLI stays a hard dependency the brief forbids), sort honesty, and
  server-side prefix search. **`XONHO-0011` is the one with the stronger
  claim**, and it is being stepped over knowingly: it is a debt to users who
  do not exist yet, while this is the wall the one real user hits daily. Said
  plainly because the planning gate exists to make that choice visible rather
  than to prevent it.
