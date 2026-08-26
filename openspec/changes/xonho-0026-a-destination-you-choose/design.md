# Design — a destination you choose

## Context

`upload_here` opens the platform's file dialog, takes the chosen path's file
name, and composes `format!("{}{name}", location.prefix)` (`app.rs:1150`).
The key is decided in one line that nothing can reach.

Everything else about an upload is already right: `XONHO-0020` made the
no-clobber guarantee with `If-None-Match` on the request itself rather than
with a check, so the collision answers — replace, keep both, abandon — work
against whatever key is sent. This change moves *which key* from a `format!`
to a field.

## Goals / Non-Goals

**Goals**

- Show the destination, defaulted to today's behaviour, and let it be edited.
- Make a folder on a directory bucket the only way that works there.
- Refuse a destination that cannot be a key, before sending.

**Non-Goals**

- Multiple files, folder uploads, a queue. All §4.4, all larger, and each
  needs the queue that does not exist.
- Browsing to pick a destination. The path bar navigates; a second navigator
  inside an upload answers a question twice.
- Remembering the last destination — see the proposal.

## Decisions

### The default is today's behaviour, made visible

Pre-filled with `<prefix><file name>`, which is exactly what `app.rs:1150`
composes now. Someone who wants what they got yesterday presses the same
button and gets the same key; the field is a place to intervene, not a
question to answer.

This is why the field goes in the **upload strip** rather than in a dialog
before the file picker: the picker is the platform's and cannot carry it, and
a dialog before the picker would make every upload a two-step act to serve
the minority of uploads that want a different key.

### Validation beside `folder::key_for`, not a copy of it

`XONHO-0024` put "what may a folder be called" in `core::folder`. This is the
same question about a different shape — an object key rather than a folder
name — and putting it anywhere else guarantees the two drift on the rules
they share.

Three refusals, and the third is the one worth arguing:

- **empty** — names nothing;
- **ends in `/`** — names a folder, and this operation writes an object.
  Sending it would create a zero-byte object that the listing renders as a
  folder, which is `XONHO-0024`'s marker arriving through the wrong door;
- **starts with `/`** — S3 permits it, and it produces an object inside a
  folder whose name is the empty string. It is legal, it is never intended,
  and every tool renders it strangely. Refused rather than silently trimmed,
  because trimming would send a key the user did not type and this spec says
  what is shown is what is sent.

### A directory bucket needs no special case here, and that is the point

`XONHO-0024` had to know the bucket's kind because an empty folder cannot
exist on a directory bucket. Writing an object into a path has no such
problem: the directories come into being as part of the `PutObject`, which is
AWS's documented behaviour and the same call this application already makes.

So this change has **no kind branch at all**. The feature that needed the
branch is the one that cannot work; the feature that works needs nothing.
Worth stating because the reflex, having just written `XONHO-0024`, is to
reach for the kind again.

## Risks / Trade-offs

- **[A typed destination silently walks away from where the user is looking]**
  → send to `elsewhere/report.csv` and the object is not in the listing on
  screen. The upload's own report already names the key it wrote, and that
  report is the mitigation; a listing that jumped to follow the upload would
  move the user without asking.
- **[Refusing a leading `/` will look pedantic to someone who meant it]** →
  they can still make that key by typing the folder name explicitly. The
  refusal names the rule, so it is arguable rather than mysterious.
- **[The field is one more thing on a strip that already reports progress]** →
  it appears only while a destination is being chosen, and the strip is
  already a phase machine. It is a phase, not a permanent control.

## Open Questions

None. The one factual question — whether writing into a non-existent path
creates the directories on a directory bucket — was answered from AWS's
documentation while planning `XONHO-0024`: directories are created during
`PutObject`.
