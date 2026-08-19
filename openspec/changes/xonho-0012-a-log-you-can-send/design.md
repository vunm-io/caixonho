## Context

See `proposal.md`. What shapes the approach:

- `tracing` 0.1.44 is already in the tree: the AWS SDK emits through it. A
  subscriber therefore captures the SDK's own diagnostics for free, which is
  exactly what was missing when a credential was refused and the reason lived
  only inside the SDK's error chain.
- `directories` 6.0.0 is already a dependency and knows the platform's log
  location.
- Invariant 5 is absolute and has no exception for a debug build.

## Goals / Non-Goals

**Goals:**

- A file a user can send, holding enough to explain a failure they have already
  had.
- The redaction rule enforced by tests, not by discipline.

**Non-Goals:**

- A crash handler. The brief wants one; it is its own work.
- Telemetry of any kind. There is no network path out of this, and there never
  will be.
- Tracing every function. A log that records everything is one nobody reads.

## Decisions

### `tracing` with a file subscriber, not a logger of our own

Writing a small logger would avoid a dependency and lose the reason for having
one: the AWS SDK already emits through `tracing`, so a subscriber picks up its
diagnostics without a line of glue. That is the half of the picture the
application cannot otherwise see — the failure that started this change had its
truth inside an SDK error chain, not in anything we would have written.

### Redaction is structural, not editorial

A secret is never handed to the logging layer at all: the types that hold one do
not implement the formatting traits that would print it, and events name the
connection rather than the credential. Filtering secrets out at the point of
writing is the alternative, and it is one missed call site away from failing.

The test is the same shape as the credential store's, and for the same reason
that test earned: three spellings — readable, raw bytes, and escaped by the
format — because a naive check passed a verbatim disclosure once already.

### Our events are informative; the SDK's are quiet

The default keeps this crate at a level that records decisions and everything
else at warnings and above. The SDK's detailed levels carry request and header
material, and writing that to a file unasked would be exactly the sort of
quiet accumulation this project refuses elsewhere.

One environment variable raises the level for an investigation, which is the
`tracing` convention and needs no interface of its own.

### A bounded file, in the platform's log location

Rolling by day, with a small number kept. `directories` gives the location.

### Failing to log is not a failure

If the file cannot be opened the application runs without it. A client that
refuses to start because it could not write a diagnostic has mistaken the
diagnostic for the product.

## Risks / Trade-offs

- **A log is a new place secrets could reach** → which is why the rule is
  structural and tested rather than reviewed. This is the risk of the whole
  change and the reason its spec is mostly about what must not appear.
- **The SDK could log something sensitive at a raised level** → the raised level
  is a deliberate act by the user for an investigation, the default is quiet,
  and what the SDK does with its own detail is documented rather than assumed to
  be safe.
- **Rolling files leave data on disk after the fact** → they are local, bounded,
  and the user can delete them; nothing is ever sent anywhere.
