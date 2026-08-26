# Design — a folder you can make

## Context

S3 has no folders. A general purpose bucket fakes one with a zero-byte object
whose key ends in `/`, which the console does and which the listing already
renders as a folder because `ListObjectsV2` with `delimiter=/` returns it as
a common prefix.

A directory bucket does not fake it — it has real directories, and it deletes
them the instant they empty. Those are two different stores wearing one name,
and this change is mostly about not pretending otherwise.

## Goals / Non-Goals

**Goals**

- One level of folder, where the user is standing, on a general purpose
  bucket.
- A directory bucket told apart *before* the attempt, not by its refusal.
- A refusal that leaves the user able to do the thing they wanted.

**Non-Goals**

- Rename, copy, move, nested creation, bulk anything.
- Making an empty directory survive on a directory bucket. It cannot; the
  service removes it. Any design that appears to do this is hiding a delete.

## Decisions

### The marker is a zero-byte object, and only on a general purpose bucket

`PutObject` with key `<prefix><name>/` and an empty body. Nothing new: the
same call `XONHO-0020` already makes, without the `If-None-Match` guard's
keep-both branch, because a folder that already exists is a name collision to
refuse up front rather than a file to step aside from.

### A directory bucket is refused, and the refusal is the feature

AWS's own documentation, read before this was written:

> Directories are created during `PutObject` or `CreateMultiPartUpload`
> operations and automatically removed when they become empty after
> `DeleteObject` or `AbortMultiPartUpload` operations.

Its worked example is blunter still: deleting the last object in `reports/`
leaves the directory empty and "causing it to be deleted immediately".

Three shapes were considered.

1. **Write the marker anyway.** The object `reports/` is not empty — it *is*
   an object — so it would persist, and the folder would appear. Rejected:
   it puts a zero-byte object into a store whose whole model is that
   directories are structural, and it would show as a file to every other
   tool reading that bucket. Solving our display problem by writing rubbish
   into the user's bucket.
2. **A pending folder held only in the window**, materialising when something
   is uploaded into it. Rejected *for this change*: a folder that exists on
   screen and nowhere else is the kind of half-truth this project's filter
   and sort honesty rules exist to forbid, and it needs state that survives
   navigation. It is worth its own change if the refusal turns out to annoy.
3. **Refuse, and offer the act that works** — chosen. The button says a
   directory bucket keeps a folder only while something is in it, and points
   at uploading to a typed destination.

The honest reading of this is that on a directory bucket the useful feature
is not "create folder" at all: it is **choosing the destination key when
uploading**. `XONHO-0020` uploads under the local file's own name into the
current location. Letting the user type where it lands would give the owner's
daily account the organising power they asked for, and it is a smaller change
than a pending-folder model. Named here so the next proposal starts from it.

### The kind is already known, so ask the listing and not the service

The account listing already carries each bucket's kind — that is what
`XONHO-0016` put there and what the "All directory buckets" badge reads. So
the branch is taken locally, before any request. Deciding by attempting and
reading the failure would cost a round trip to learn something already on
screen, and would surface as an error where the answer was never in doubt.

## Risks / Trade-offs

- **[The owner's daily account gets a refusal, not a folder]** → this is the
  honest outcome, and it is worse than useless *only* if the alternative goes
  unbuilt. The design names that alternative rather than leaving it implied.
- **[A zero-byte marker is invisible to some tools]** → some clients render
  `reports/` as a strange empty file. That is what the console produces too,
  so this matches the ecosystem rather than inventing a convention.
- **[Name collision is checked against the loaded page, not the bucket]** →
  a folder that exists beyond the loaded rows would not be caught locally.
  The service is the authority: the put is what decides, and the refusal is
  reported from it. The local check is a courtesy, and must not be described
  as a guarantee.

## Open Questions

None blocking. The one question that mattered — whether a directory bucket
can hold an empty folder — was answered from AWS's documentation before the
spec was written, and it is the reason this change has two requirements
rather than one.
