# ADR-0004: What a key is called on disk

- **Status:** Accepted
- **Date:** 2026-08-24
- **Deciders:** Vu Nguyen

## Context

`XONHO-0007` writes the first files. S3 keys admit names filesystems refuse:
Windows rejects `< > : " / \ | ? *`, control bytes, trailing dots and spaces,
and a family of device names (`CON`, `NUL`, `COM1`…) bare *or* with any
extension; every shipped filesystem caps a name at 255 bytes; keys may end in
`/` or contain `//`, leaving no usable final segment at all; and volumes are
case-insensitive on both primary targets, so `Report.pdf` and `report.pdf`
are distinct keys and one file.

The brief prices the failure mode exactly (§4.4): *downloads sanitize
deterministically and report every collision — silent data loss is the one
unforgivable file-manager bug.* The requirement it became says the same with
teeth: two distinct keys SHALL NOT silently produce one local file.

## Decision

One pure function, `transfer::local_name(key) -> Mapped`, one scheme on every
platform, no `cfg` in the mapping:

1. **The name is the key's final `/`-separated segment.**
2. **Refused bytes are percent-encoded, never replaced.** The set is
   Windows's refused punctuation, control bytes, the separator pair, a
   trailing `.` or space, and `%` itself. Encoding `%` is what keeps the
   scheme injective — with it, `12:30.log` → `12%3A30.log` and a key that
   already says `12%3A30.log` → `12%253A30.log`, and the two can never meet.
3. **A segment that cannot serve takes a deterministic suffix** — empty
   (trailing `/`), `.`/`..`, a reserved device stem, or overlong after
   encoding. The suffix is `-` plus FNV-1a/64 of the **full key**, so the
   distinctness the segment lost is carried by where the object came from.
   FNV-1a is implemented inline (eight lines): the suffix must be identical
   across platforms and releases, which the standard library's hasher
   explicitly does not promise. Overlong names are cut at a character
   boundary to fit 200 bytes including the suffix — margin under the
   255-byte floor for the writer's working extension.
4. **The mapping reports what it did** — `Unchanged`, `Substituted`, or
   `Suffixed` — and the UI surfaces anything that is not `Unchanged`. The
   report is the point: §4.4 forbids *silent*, not *changed*.

What the pure function deliberately does **not** solve: two different keys
with the *same* final segment (`a/x.txt`, `b/x.txt`), and case-collisions on
case-insensitive volumes. No function of one key can see the other. Both
surface at the destination as an existing-file question, which the
`object-transfer` spec routes to the user (replace / keep both / abandon) —
detected by the writer at the only place they are detectable.

## Alternatives considered

- **Replacement characters (`?` → `_`)** — how most GUIs do it, and lossy:
  `a?.txt` and `a*.txt` merge silently, which is the named unforgivable bug.
- **Percent-encoding without encoding `%`** — keeps names prettier, loses
  injectivity: a key that already contains `%3A` collides with the encoding
  of `:`. Chosen ugliness over ambiguity.
- **Full-key encoding instead of final segment** — injective without any
  suffix, but produces `reports%2F2026%2Fsummary.csv` for every ordinary
  download; the common case pays for the rare one. Declined.
- **A per-platform mapping** (`cfg(windows)` strictness only there) — a file
  downloaded on macOS would change name when the folder syncs to a Windows
  machine, and the property tests would need a platform matrix. One scheme
  costs Unix users a stricter alphabet and buys one behavior everywhere.
- **`sanitize-filename` (crate)** — replacement-based, so lossy (first
  alternative), and a dependency where the whole job is forty lines beside
  the tests that pin it.

## Consequences

- Names are stable: the same key produces the same file name on every
  machine, forever — safe to document, safe to script against.
- Encoded names are less pretty than replaced ones. Accepted; the report
  tells the user why the name looks the way it does.
- The 200-byte bound leaves 55 bytes of margin for the working-file
  extension and future qualifiers, so the bound never has to move in sync
  with them.
- Tests pin the scheme (`transfer::tests`), including determinism and the
  no-silent-merge property over a corpus dense in the characters the scheme
  touches. Changing the scheme is a breaking change to every existing
  download folder and gets a new ADR, not an edit to this one.
