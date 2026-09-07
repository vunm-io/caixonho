# ADR-0005: What a key is called on disk when a folder arrives as a folder

- **Status:** Accepted
- **Date:** 2026-09-07
- **Deciders:** Vu Nguyen
- **Extends:** [ADR-0004](0004-key-to-filename-scheme.md) — which it does not change

## Context

`ADR-0004` settled what one object is called on disk: the key's **final**
`/`-separated segment, percent-encoded where a filesystem refuses a byte, with
a deterministic suffix for a segment that cannot serve, and a report of what
was done. It is deliberately a function of one key producing one *name*.

`XONHO-0034` downloads a folder, and the brief (§4.4 `[M]`) asks for the prefix
structure to be preserved. A subtree flattened into one directory is not the
subtree, and it manufactures collisions that did not exist: two objects told
apart only by their prefix would contend for one name.

So the unit has to grow from a name to a **relative path** — and that raises a
question `ADR-0004` never had to answer, because it only ever looked at the
last segment: **what does a broken segment in the middle do?**

**A new ADR rather than an edit.** `ADR-0004` closes with *"Changing the scheme
is a breaking change to every existing download folder and gets a new ADR, not
an edit to this one."* This change does not alter the scheme — `local_name`
returns exactly what it returned, and no existing download folder moves — so an
amendment would arguably have been within the rules. Deciding that its own
closing instruction does not apply to the person invoking it is not a habit
worth starting, and the finding below is new knowledge that deserves a record
of its own.

## Decision

`transfer::local_path(key, under) -> MappedPath` maps the part of `key` below
`under` to a relative path, and:

1. **Every segment goes through `ADR-0004`'s rules, not only the last.** A
   filesystem refuses a directory name for the same reasons it refuses a file
   name. The per-segment logic is *one* function — `map_segment`, which
   `local_name` also calls — so the two can never drift apart. The ADR's
   promise is one scheme; two copies of it would be two.

2. **A broken middle segment does what a broken last one does**: the same
   deterministic suffix, `-` plus FNV-1a/64 of the **whole key**. Hashing the
   whole key rather than the segment is what keeps two identical-looking
   segments from different keys apart, and it is the property `ADR-0004`'s
   injectivity already rested on.

3. **A key may name a parent directory and can never be one.** `.` and `..`
   have their trailing dot encoded before anything else inspects them, so they
   arrive as ordinary directory names. The returned path is relative and
   carries no parent component, whatever the key contains.

4. **The reported outcome is the worst across the segments** —
   `Suffixed` over `Substituted` over `Unchanged`. The user is being told
   something was done to *this object*; the strongest thing done is the honest
   headline, and a weaker one would understate it.

5. **A key ending in `/` names the directory it stands for**, not a file
   inside it. It is a zero-byte marker saying a folder exists; it has no
   content to write.

## Consequences

- **A collision `ADR-0004` left to the user is resolved by structure.** That
  ADR names `a/x.txt` against `b/x.txt` as something no function of one key can
  see, routed to the destination as an existing-file question. Under a path
  they no longer share a directory, so they no longer contend.

- **One collision is unresolvable, and it is the filesystem's, not the
  scheme's.** The folder marker `a/x.txt/` and the object `a/x.txt` map to the
  same path — one wanting to be a directory, one a file, of a single name. No
  mapping can separate them because no filesystem can hold both. **Found by the
  property test**, which had been written as *no two distinct keys share a
  path* and failed on that pair. The invariant is therefore stated over
  **object keys**, and the marker case stays where `ADR-0004` already puts what
  a pure function cannot see: a conflict reported at the destination.

- **Single-object download is untouched.** It still writes the final segment
  into the chosen folder. A file fetched alone and the same file fetched as
  part of its folder land in different places, and that is correct — they are
  different requests, and the folder one was asked for the folder.

- **The property test is known to have teeth.** Removing `%` from the refused
  set makes it fail with `12%3A30.log` and `12:30.log` colliding — the exact
  pair `ADR-0004`'s encoding exists to keep apart. Run as an ablation on
  2026-09-07, restored immediately.

## Alternatives considered

- **Flatten the subtree into the chosen folder.** Rejected by the brief, which
  says *preserving prefix structure*, and by arithmetic: it creates collisions
  the structure would have prevented, and each one becomes a question.

- **Map the path with a different scheme from the name** — for instance
  allowing `:` in directories on Unix. Rejected for `ADR-0004`'s own reason: a
  folder downloaded on macOS would change shape when it syncs to Windows, and
  the tests would need a platform matrix.

- **Strip broken middle segments instead of suffixing them.** Rejected: it
  loses depth, so `a//b` and `a/b` would land in one place. The suffix keeps
  them apart, which is the whole point of having one.
