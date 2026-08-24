# Design — a delete you can take back

## Context

The first operation whose success destroys. Downloads could be wrong on
disk, uploads were made unable to destroy by `XONHO-0020`'s conditional
write; delete destroys *on purpose*, so the design question inverts: not
"how do we prevent the harm" but "how do we make sure the harm was meant,
and give back what the service lets us give back".

Everything rides existing seams: a port method pair, a session spawn, the
established outcome/diagnostics shapes, and the window's second-act
confirmation pattern (`confirming`, from connection removal).

## Goals / Non-Goals

**Goals**

- One object deleted through a named-key confirmation.
- The marker-aware outcome: Undo exactly when the response proves it exists.
- Failures classified as themselves; the listing re-read after success.

**Non-Goals**

- Bulk / recursive / folder delete and their counted confirmation.
- Versions browsing, restore-to-version, Object Lock display (§4.5 `[L]`).
- Re-delete from the outcome line (a toggle invites carelessness).
- Detecting versioning *before* the delete (`GetBucketVersioning` is an
  extra call and permission for information the delete's own response
  already carries).

## Decisions

### The response is the oracle, not the bucket's configuration

Whether a delete is reversible is answered by `DeleteObjectOutput` itself:
`delete_marker: Option<bool>` plus `version_id` (measured,
`_delete_object_output.rs:25,:31`). So the port returns what the service
said — `Deleted { marker: Option<MarkerId> }` — and the window offers Undo
exactly when `marker` is `Some`. No `GetBucketVersioning` probe: it would
cost a call and a permission to predict what the response states as fact,
and a prediction can be stale where the response cannot.

The alternative — asking versioning up front to word the *confirmation*
("this is permanent" vs "a marker will be placed") — was declined: the
confirmation always uses the strong wording, because promising a safety net
before the service has produced one is the lie this application keeps
refusing to tell. The net is announced when it exists.

### Undo is `remove_marker`, a separate port method

`DeleteObject` with `version_id` set to the marker's id removes the marker
(the SDK builder carries `version_id`, measured at `builders.rs:287`). It is
a distinct method rather than an optional parameter on `delete_object`
because the two have different permission surfaces (`s3:DeleteObject` vs
`s3:DeleteObjectVersion`), different failure wordings, and only one of them
may ever be reachable without a confirmation — Undo restores, so it needs no
second act.

### Deletion does not rent the transfer line

The outcome renders on its own strip. Reusing `Transfer` would put
"Downloading…" vocabulary one enum arm away from a destructive verb and
would make the queue change — which replaces the transfer holder — also a
change to deletion for no reason. The *pattern* is shared (a line under the
listing, a dismiss), the state is not.

### The confirmation reuses the second-act shape, not its field

Connection removal's `confirming: Option<String>` established the pattern:
a dedicated surface, two buttons, nothing changes labels under the pointer.
Deletion gets its own state rather than overloading that field — the two
confirmations can in principle be pending at once, and a `String` that
sometimes means a connection and sometimes a key is the kind of reuse this
codebase keeps declining.

### Idempotent success is reported as the service's answer

S3 answers 204 for a key that holds nothing. No "not found" is invented;
the outcome says the delete succeeded, the re-read shows the listing as it
is. The log records the outcome the service gave.

## Risks / Trade-offs

- **[Undo can fail after a successful delete]** → the outcome then shows the
  classified refusal (`s3:DeleteObjectVersion` named) and keeps saying the
  marker exists; nothing pretends the object came back.
- **[The outcome line outlives the location]** → outcome state is dropped on
  location change and connection switch, the discipline `XONHO-0019`
  established; a stale Undo against a switched connection must be
  impossible, so the outcome carries the connection it belongs to.
- **[A second delete while an outcome shows]** → starting a new delete
  replaces the outcome; the marker id it held is gone with it. Acceptable
  for one-object scope; the queue era revisits.
- **[R2 and versioning]** → R2 supports versioning with the same marker
  semantics; where versioning is off the response simply carries no marker,
  which is the unversioned path. No special-casing.

## Open Questions

None. Both SDK facts this design rests on were read before writing it, and
the two candidate extra behaviours (pre-delete versioning probe, re-delete
toggle) were considered and declined above rather than left open.
