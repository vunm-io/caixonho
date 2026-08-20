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
| **M5** | Extras — S3-compatible endpoints as a supported configuration, directory buckets, sync, versions | not started |
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
| `XONHO-0006` | Opening a bucket and browsing objects by prefix, including reaching a bucket by name when the account listing is denied | landed |
| `XONHO-0011` | Signing in to IAM Identity Center from the app, via the OIDC device flow | after browsing |
| `XONHO-0013` | Editing a saved connection: its region, its key, and renaming it | after browsing |

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
