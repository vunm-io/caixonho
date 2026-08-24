# XONHO-0021 — A delete you can take back

## Why

The third verb the owner named, in order: download, upload, delete. It is
also the only one of the three whose *success* destroys something. A wrong
download wastes disk; a wrong upload was made safe by `XONHO-0020`'s
conditional write; a wrong delete on an unversioned bucket is gone, and no
later change can give it back.

The brief prices this exactly (§4.5): delete is `[M]` **with a confirmation
that states what is being deleted**, and one line further down it names what
it calls the *cheapest trust feature in the list* — on a versioned bucket a
delete creates a marker, so the honest thing to do is offer **Undo** right
after, and hide it where it would be a lie. That feature was priced during
planning rather than assumed: `DeleteObjectOutput` carries `delete_marker`
and `version_id` (`aws-sdk-s3` 1.142.0, `_delete_object_output.rs:25,:31`),
so *whether this delete is reversible, and how,* arrives in the response of
the delete itself — no extra call, no extra permission until Undo is
actually pressed.

v0.1 is M1 + M2 + the safe part of M3, and this is the safe part of M3
beginning: one object, deliberately, with the way back shown whenever the
bucket offers one.

## What Changes

- **Delete one object, as a two-act operation.** A Delete action on the
  selected object row leads to a confirmation that names the exact key and
  does not soften the verb; only the confirmation deletes. Same shape as the
  connection-removal confirmation the window already has — a second
  deliberate act on a surface of its own, never a button that changes its
  label under the pointer.
- **The undo the service already offers, surfaced.** When the response says
  a delete marker was created, the outcome line says so and carries
  **Undo** — which removes that marker by its version id, restoring the
  object exactly. When the response says nothing, no Undo is shown and the
  line says the object is gone: an Undo that appears everywhere and works
  somewhere is worse than none.
- **Deleting is honest about idempotence.** S3 answers success for a key
  that holds nothing; the outcome line reports what the service said rather
  than inventing a "not found" the protocol does not have.
- **The listing catches up.** After a delete the location re-reads through
  the existing path, so the row leaves the screen because the service says
  it is gone, not because the window assumed.
- **The log records delete outcomes** in the transfer shape: bucket,
  outcome, cause on failure — never the key.
- **Refusals classify as themselves**: `s3:DeleteObject` denied is a denial
  with that action named; Undo denied names `s3:DeleteObjectVersion`.

### What is deliberately absent

- Bulk, recursive and folder delete — the counted confirmation for many
  objects is real work (a listing walk with an honest count) and is where a
  mistake multiplies; it composes with the queue machinery, not with this
  slice.
- Create-folder, rename, copy/move, properties, presigned URLs — their own
  rows in §4.5.
- Any second Undo level: removing a delete marker restores the object; this
  change does not offer to re-delete from the outcome line, because a
  toggle invites the exact carelessness the confirmation exists to prevent.

## Capabilities

### New Capabilities

- `object-deletion`: removing one object deliberately — the named-key
  confirmation, the delete itself, the marker-aware outcome with its Undo,
  and what the log records. Bulk and recursive extend this capability
  later rather than creating another.

### Modified Capabilities

None. `object-transfer` moves content between machine and service; deletion
moves nothing, and folding it in would make one capability answer two
different questions about harm.

## Impact

- **`caixonho-core`**: `ObjectStore` grows `delete_object` (returning what
  the service said about markers) and `remove_marker`; the adapter maps them
  to `DeleteObject` — the second with the marker's `version_id`; the double
  scripts versioned and unversioned shapes; `diagnostics` gains the delete
  outcome. TDD as everywhere in core.
- **`caixonho-gui`**: a Delete action enabled on object selection; a
  confirmation strip naming the key; an outcome line with conditional Undo.
  The transfer line is not reused — deletion is not a transfer, and renting
  its widget would put "Downloading…" vocabulary one enum away from a
  destructive verb.
- **Dependencies**: none.
- **Docs**: `README.md`, `docs/roadmap.md` (M3's safe subset begins),
  `docs/requirements-status.md` §4.5 opens as a table the way `XONHO-0007`
  opened §4.4.
- **Capability model untouched**: no write probe exists and none is added;
  delete capability leaves *unknown* only through this user-asked operation,
  which is the model's standing rule.
