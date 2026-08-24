# Design — a file you can send

## Context

`XONHO-0007` built the transfer machinery in one direction and left seams
shaped for the other: a port method, a session spawn, a `Cancel` flag, a
transfer line in the window, and a log vocabulary for outcomes. This change
uses all five rather than growing new ones.

What is genuinely new is the direction of harm. Every operation so far could
be wrong on screen; this one can be wrong in someone's account, permanently,
on an unversioned bucket. The design is mostly about that one sentence.

## Goals / Non-Goals

**Goals**

- One local file → one object under the location on screen.
- No object is ever replaced except by a user answering a question about
  that object, and the guarantee comes from the service.
- Cancel, and outcomes in the log in the established shape.
- An honest refusal above the single-request size limit.

**Non-Goals**

- Byte progress (see the decision below — it belongs with multipart).
- Multipart, folders, the queue, drag-and-drop, storage class or metadata on
  upload, delete.
- Uploading to a bucket other than the one on screen.

## Decisions

### The guarantee is `If-None-Match: *`, not a look-then-write

`PutObject` replaces silently. The obvious defence — `HeadObject`, then
`PutObject` if absent — is a check whose answer is stale the moment it
arrives: between the two calls another writer can create the object this
application then destroys. It is a race that loses rarely, which is the worst
frequency, because it will not show up in any live check.

`put_object().if_none_match("*")` moves the decision inside the request the
service executes. A taken key comes back `412 PreconditionFailed`, and
`SdkFailure` already carries the status code — the same mechanism
`classify.rs` uses to recognise a `301` redirect (`classify.rs:223`,
`:236`). So `412` is read where `301` already is, and becomes an *outcome*
(`KeyTaken`), never an `Error`: a precondition that did its job is not a
failure and must not reach the failure panel's vocabulary.

Replace is then a second call with the condition omitted. That is the only
place in this codebase that issues an unconditional `PutObject`, and it is
reachable only from the user's answer.

**Alternatives:** `HeadObject` first (raced, above); versioning-only
(unavailable on unversioned buckets and not this application's to enable);
never replacing at all (declined — the user's own file, their call).

### Progress and the size cap are the same change, and it is not this one

Measured during planning rather than assumed:

- `ByteStream::from_path` / `FsBuilder` (`aws-smithy-types` 1.6.2) expose
  `path`, `file`, `length`, `offset`, `buffer_size` — no counting hook.
- `ByteStream::from_body_1_x` **is** public and the `http-body-1-x` feature
  is already enabled in this graph, so a hand-written counting
  `http_body::Body` is possible.
- But the SDK may **rebuild the body on retry**. A counter that jumps
  backwards, or forwards twice, is a worse lie than an honest indeterminate
  state — and this project's whole posture is that a wrong number costs more
  than a missing one.

Multipart dissolves the problem: one tick per completed part, each part
retried independently, and the 5 GiB single-request cap gone. So this change
shows the file's total size with an indeterminate running state, and refuses
above the cap **before** sending, naming multipart. `PutObject`'s limit is a
constant here rather than a probe — it is the service's documented figure,
and discovering it by being refused after uploading gigabytes is the
behaviour the requirement exists to prevent.

### A derived object key is not a derived filename

ADR-0004's scheme exists because filesystems refuse things. S3 refuses almost
nothing: a key is bytes. Reusing `local_name` here would percent-encode
characters the service is perfectly happy with and produce keys that do not
match the file that was sent.

So keep-both derives an *object key* by its own small rule — ` (n)` before
the last dot, first free n from 2, the same numbering the local side uses
because it is the numbering people know — and it is a separate function with
its own tests. The shared thing is the numbering convention, not the code.

Freedom to pick `n` needs to know which keys are taken, and the only honest
source is the service: each candidate is attempted conditionally, so
"choosing" a free key is a small loop of conditional writes rather than a
listing that could be stale. Bounded, and the bound is reported if reached.

### The endpoint that will not do preconditions

An endpoint answering `501`/`NotImplemented` to the condition is detectable,
and the spec says what happens: stop, tell the user this endpoint cannot
guarantee the object is left alone, and let proceeding be their explicit act.

**The residual risk, stated because the spec cannot cover it:** an endpoint
that *silently ignores* `If-None-Match` is undetectable from a successful
response — the write looks exactly like a write to a free key. Nothing in
this design closes that, and no check-then-write closes it either. It is
named here so the next reader does not think the requirement covers it.
AWS S3 and R2 both implement conditional writes; the exposure is
S3-compatible endpoints generally, which §5 already calls a supported
configuration rather than a tested one.

### The window reuses the transfer line

`Transfer` grows a direction rather than being joined by an `Upload` twin.
The states are the same five with different words, the collision question is
the same three buttons against a key instead of a filename, and the queue
change will replace one holder rather than two.

## Risks / Trade-offs

- **[No byte progress on a slow link]** → the total is shown and cancel
  works, so the user is never trapped; multipart is the fix and is next.
- **[Keep-both loops conditional writes]** → bounded attempts, and the bound
  is reported rather than silently giving up.
- **[An endpoint that ignores the condition]** → undetectable; named above,
  not papered over.
- **[Upload is the first destructive-capable operation]** → `capability`
  stays untouched: the model's rule is that write capability leaves unknown
  only through an operation the user asked for, and this is that operation.
  Nothing here adds a write probe.

## Open Questions

None. The three that mattered — how to prevent silent replacement, whether
byte progress is available, and where the size cap belongs — were settled by
reading the SDK during planning, and each answer is recorded above with what
was read.
