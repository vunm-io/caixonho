# Roadmap

What is built, what is being built, and what comes after. The near-term detail
lives in [`planned-changes.md`](planned-changes.md); this is the shape of the
whole thing.

## Milestones

| | Milestone | State |
|---|---|---|
| **M0a** | The stack builds — CI green on Windows and macOS, artifacts uploaded | **done** |
| **M0b** | The stack is smooth — measured on real hardware, ADR-0001 filled in | **done bar one cell**: a Windows machine without a working Vulkan driver, where the required outcome is a graceful failure. ADR-0001 stays `Proposed` until then |
| **M1** | Read-only browser — credentials, bucket list, prefix navigation, permission awareness | **in progress** |
| **M2** | Transfers — up and download including folders, multipart, a queue with progress, cancel and retry | not started |
| **M3** | Object operations, safe subset first — create folder, delete with a counted confirmation, properties, presigned URLs | not started |
| **M4** | Ship v0.1 — installers for both platforms, a README with a recording, public announcement | not started |
| **M5** | Extras — S3-compatible endpoints as a supported configuration, sync, versions | not started |
| **M6** | Reach — Linux, then the CLI crate over the same core | not started |

**v0.1 is M1 + M2 + the safe part of M3.** Copy, move and rename wait for v0.2:
they are the operations where a bug costs someone their data, and they deserve
to ship after the safe subset has been exercised.

## M1 in detail

M1 is the milestone that turns a window into a client. It is cut into changes
that each own a subsystem and land on their own:

| Change | Delivers | State |
|---|---|---|
| `XONHO-0003` | Connecting to an account, listing its buckets, telling failure causes apart | landed |
| `XONHO-0005` | Regions on the list, filtering by region, and the first working piece of permission awareness | landed |
| `XONHO-0004` | Entering credentials in the app, storing them in the OS keychain, and connecting only when asked | landed |
| `XONHO-0012` | A log on disk that says what the app decided and why, and never holds a secret | landed |
| `XONHO-0009` | The app shell, the palette, and loading, empty and error states that were improvised before | in progress |
| `XONHO-0006` | Opening a bucket and browsing objects by prefix, including reaching a bucket by name when the account listing is denied | built, awaiting live acceptance |
| `XONHO-0011` | Signing in to IAM Identity Center from the app, via the OIDC device flow | after browsing |
| `XONHO-0013` | Editing a saved connection: its region, its key, and renaming it | after browsing |
| `XONHO-0016` | S3 Express One Zone directory buckets: listed beside ordinary ones, opened, and refused in their own words | landed |
| `XONHO-0018` | Following a bucket to the region it lives in, instead of reporting the redirect as an unexplained error | built, awaiting live acceptance |
| `XONHO-0017` | Auditing what the project depends on, on every change — and removing the four advisories that were being shipped | landed |
| `XONHO-0015` | A seam that lets a test build the real window, and the first tests that use it | landed |
| `XONHO-0019` | A pane that cannot outlive the connection it was read on: switching connection ends the location instead of leaving the previous account's bucket named | built, awaiting live acceptance |

Three cells were corrected on 2026-08-22, all in the same direction — a row
that was easier to write than to check:

- `XONHO-0015` said **in progress** after it had closed 13/13 and been
  archived on 2026-08-21.
- `XONHO-0006` said **landed** while its live acceptance (task 5.3) was still
  open and `openspec/specs/` still had no `object-browsing`. That one was not
  cosmetic: `XONHO-0019` must sync its delta only *after* `XONHO-0006`
  archives, so a reader trusting this table would have thought that unblocked.
- `XONHO-0019` had no row at all, having landed the same day.

The pattern is worth more than the three fixes: this table is written when a
change *starts* and is true only if someone returns to it when the change
stops. The close-out review's second question is where that return is supposed
to happen — and it is answered per-change, which is exactly why a row about a
*different* change goes stale unseen.

`XONHO-0016` was pulled forward out of **M5** on 2026-08-20. It is an `[S]`
taken ahead of four `[M]`s, which the planning gate exists to make deliberate
rather than accidental — the reasoning is in that change's `proposal.md`. The
short version: the only account available to verify against cannot list
ordinary buckets at all, so directory buckets are the difference between a
connection that works and a connection that shows an error, and `XONHO-0011`'s
own acceptance was waiting on a listing that account could serve.

Credential entry was ordered **ahead** of browsing on 2026-08-19, reversing an
earlier decision that had put browsing first. The argument for browsing first
still holds on its own terms — a bucket list alone is a dead end — and it is why
`XONHO-0006` comes immediately next. What did not hold was the counter-argument:
credential entry had been treated as work for a hypothetical future user, on the
grounds that the developer's own machine already had working profiles in
`~/.aws`. Those profiles reach a password manager through an external process,
which is one developer's scaffolding rather than the product, and ordering around
it showed — the first thing anyone noticed about the app was a seven-second wait
that only that arrangement produced. The full reasoning is in
[`planned-changes.md`](planned-changes.md).

## What this project will not do

Saying no early is cheaper than saying it later:

- **No telemetry, ever.** No analytics, no phone-home, no crash upload. Crash
  reports are written to a local file that you choose to attach to an issue.
- **No option to disable TLS verification.** Trust material is configurable —
  an enterprise CA in the OS trust store, `AWS_CA_BUNDLE`, `SSL_CERT_FILE` —
  but verification itself is not a setting.
- **No pretending about permissions.** If the app has not established that
  something is allowed, it says so rather than guessing either way.

The full requirements, including the ones with no milestone yet, are in
[`PROJECT_BRIEF.md`](PROJECT_BRIEF.md).
