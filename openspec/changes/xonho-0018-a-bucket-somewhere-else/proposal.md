## Why

A bucket that lives in a region other than the connection's is answered with a
redirect, and this application reports that redirect as an unexpected error. It
is not unexpected. The service said, in a header, exactly where the bucket is —
and we throw that away and tell the user something went wrong that we cannot
name.

`PROJECT_BRIEF.md` §4.1 asks for the opposite in as many words: *"Region
handling that does not surprise: follow `x-amz-bucket-region` redirects instead
of reporting a misleading error."* §7 states the rule it belongs to — never
render a cause the truth does not support. `docs/requirements-status.md` has
carried this row as **none** since the file was written.

Now, because it is the only mandatory row that is wrong for a user *today*
rather than absent: opening such a bucket fails, and the message tells them
nothing they can act on, while the answer sat in the response.

## What Changes

- A read that is redirected because the bucket lives elsewhere is **retried
  against the region the service named**, once, and succeeds. The user sees
  their objects rather than an error.
- The region presented for that bucket is **corrected to the one that actually
  served it**. The bucket list has a region column; following a redirect while
  continuing to display the region we guessed would replace one wrong answer
  with a quieter one.
- A redirect that names **no** region — which is what an S3-compatible service
  may do — is reported as its own cause, naming what happened and what would
  fix it, rather than as an unexpected service error.

No breaking change. A connection whose region is right for its buckets behaves
exactly as it does today, and no extra request is made where none is needed —
the retry happens only after a redirect has already arrived.

## Capabilities

### New Capabilities

None. What changes is what region a bucket is *said* to be in, and which causes
a failure can be reported as. Both belong to capabilities that already exist.

### Modified Capabilities

- `bucket-listing`: a bucket the service places in another region SHALL be
  followed there rather than refused, and the region presented for it SHALL be
  the one that served it.
- `connections`: the list of distinct causes SHALL cover a bucket that lives
  elsewhere where the service will not say where. That requirement already
  forbids reporting a wrong region as access denied; it does not yet give the
  condition a cause of its own, which is why it currently arrives as
  "unexpected".

`object-browsing` is deliberately untouched. The redirect is followed on the
read path, but nothing about *browsing* changes — the same page arrives, from a
different endpoint. What changes is the region contract and the cause list, and
those live in the two capabilities above.

## Impact

**Requirements delivered.** `PROJECT_BRIEF.md` §4.1 `[M]` — follow
`x-amz-bucket-region` redirects instead of reporting a misleading error.
`docs/requirements-status.md` moves that row from **none** to done.

**`[M]` requirements still unbuilt ahead of it**, and why this one goes first:

| Unbuilt `[M]` | Why this change goes first anyway |
|---|---|
| In-app OIDC device-flow login (§4.1) | `XONHO-0011`, 12/19 and **blocked**: its remaining tasks are live verification only the owner can run. This change does not jump it; it proceeds while 0011 waits |
| Sort honesty (§4.2) | Nothing sorts yet, so nothing lies yet. This row describes a lie that is not currently being told; the redirect describes one that is |
| KMS denial distinguished from an S3 denial (§4.3) | Needs object reads, which do not exist yet |
| Dependencies audited in CI (§7–8) | `XONHO-0017`, measured 2026-08-21 — 4 advisories, all in a TLS path compiled and never called. Real work, but nothing is failing for a user because of it |

The honest summary: of the mandatory rows available to work on, this is the one
where the application is **actively wrong in front of someone**. The others are
absent, blocked, or not yet possible.

**Code.** `crates/caixonho-core/`: `classify.rs` (carry the redirect's region on
the failure), `adapter.rs` (follow it once, remember it), `types.rs` (say which
region served a page), `error.rs` (the cause for a redirect that names nowhere).
`crates/caixonho-gui/`: the bucket row's region, corrected when a page says it
was served from somewhere else.

**Dependencies.** None added to the build. `aws-smithy-http-client`'s
`test-util` feature is added to `[dev-dependencies]` for `StaticReplayClient`,
which is what makes the retry testable without a network. Resolver 2 keeps a
dev-only feature out of the release build, and this crate is not part of the
stack ADR-0001 freezes.

**Testing.** The retry is unit-testable: a canned 301 carrying the header,
followed by a canned 200, proves the second request went to the named region.
What cannot be tested locally is the real thing — neither MinIO nor R2 emits a
region redirect — so acceptance is against an account holding a bucket outside
the connection's region, and that is the owner's to run.

**Documents.** `docs/requirements-status.md` (the §4.1 row and the count),
`README.md` if it describes what happens across regions, and
`docs/planned-changes.md` if anything is deferred.
