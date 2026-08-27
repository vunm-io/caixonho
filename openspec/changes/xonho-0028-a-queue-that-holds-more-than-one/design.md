# Design — a queue that holds more than one

## Context

The pieces for one transfer are already right and this change should not
disturb them. `spawn_upload` and `spawn_download` return a `Cancel`, report
progress through a channel, and deliver a settled outcome; `TransferPhase`
describes the five ends a single transfer can come to. All of that survives.

What does not exist is anything that holds more than one, decides which run,
and says which event belongs to which.

## Goals / Non-Goals

**Goals**

- Many transfers accepted, a bounded number running, the rest waiting.
- Every event attributable to its own item.
- A failure contained to one item.
- Cancel, retry-failed, clear-finished, over one item or the queue.

**Non-Goals**

- Pause and resume. See below — this is the decision, not an omission.
- Throughput, ETA, multipart, adaptive concurrency, drag and drop, bulk
  delete, surviving a restart. Each named in the proposal with its reason.

## Decisions

### Pause is not offered, because it cannot be honest yet

The brief's queue line names pause and resume, and this change declines them.

A `PutObject` is one request carrying one stream. There is no protocol move
that suspends it: stopping the stream *is* cancelling the upload, and starting
again starts from zero bytes. A Pause button that cancelled and re-uploaded
would be a lie about what happened to the eight hundred megabytes already
sent.

Honest pausing arrives with **multipart**, a separate `[M]`: parts already
sent stay sent, and a paused upload is simply one that stops offering the next
part. So pause is not deferred for effort — it is deferred because the
mechanism that makes it truthful is a different requirement, and building it
before that means building the lie.

A waiting transfer, on the other hand, can be held back trivially, and that
covers most of what someone reaches for Pause to do: stop *more* from starting.
Cancelling the queue does that today.

### Bounded concurrency, fixed, and small

A fixed small bound rather than a configurable one or an adaptive one.
Adaptive is its own `[M]` and needs the throttling signals to steer by;
configurable is a setting nobody can choose well without those signals either.

The bound has to exist. The brief names the failure it prevents — *"many small
files into one prefix is the classic trigger"* for `503 SlowDown` — and a
queue that starts everything at once would reach that on its first real use.

### Every event carries the item it belongs to

The single-transfer window can assume any progress event is *the* transfer's.
With a queue that assumption becomes a bug that shows as one file's progress
bar moving for another file's bytes.

So events are tagged. This is the same discipline `XONHO-0019` applied to
connections — an outcome carries the id it was read on, and one that does not
match is dropped rather than rendered — and the reason is identical: a late
answer about something the user has moved on from.

A cancelled or cleared item's late events are dropped, not resurrected.

### A collision holds no slot

A transfer waiting for the user to answer replace / keep-both / abandon is
waiting on a *human*, which may be minutes. Holding a concurrency slot while
it waits would let two unanswered questions stall a queue of twenty.

So the answer is a state that sits outside the running set. This falls out of
modelling waiting-for-a-slot and waiting-for-an-answer as different things
rather than as one "not running".

### `TransferPhase` stays as it is

Five phases describing one transfer's end. They were right for one and they
are right for each of many; what changes is that there are several, not what
each says. A change that rewrote them would be a change to what a transfer
*is*, which this is not.

### What quitting means, said rather than left

Quitting with transfers in flight loses them, and objects already fully
uploaded stay uploaded. That is what happens today with one transfer and it
does not get worse; naming it is the point, because a queue makes it *feel*
like something more durable than it is. Surviving a restart is named absent in
the proposal.

## Risks / Trade-offs

- **[Declining a named `[M]` sub-feature]** → pause and resume are on the
  brief's line and this change does not deliver them. Argued above rather
  than quietly dropped; `docs/requirements-status.md` must say the row is
  partial *and which part*.
- **[A panel is more screen than a strip]** → the queue has to be visible
  without taking the listing hostage. The strips this window uses are one row;
  a list of twenty is not. Where it lives is a real design question and the
  tasks make it one rather than assuming.
- **[Bounded concurrency is a number this change invents]** → it will be
  wrong for somebody. It is small on purpose, and adaptive concurrency exists
  as a requirement precisely because a fixed number cannot be right.
- **[Tagging is easy to get almost right]** → an untagged event is silently
  wrong rather than loudly wrong, which is the worst failure mode available.
  The tasks call for an ablation on it.

## Open Questions

- **Where the panel lives** — over the listing, beside it, or a strip that
  expands. Deliberately left to the task that builds it, with the constraint
  that a queue of twenty must not hide the thing the user is browsing.
