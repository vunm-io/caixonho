# Planned changes

A staging list: what the next changes are, why they are separate, and what
order they go in. Entries leave this file when they become real OpenSpec
changes under `openspec/changes/`.

Requirements live in [`PROJECT_BRIEF.md`](PROJECT_BRIEF.md); this file only
decides how they are cut into changes.

## Why these four, separately

Each owns a subsystem with its own interface, and each is testable on its own.
Cutting them into one change would produce a spec nobody can review and a
branch nobody can land.

| Change | Scope | Brief | Milestone |
|---|---|---|---|
| `XONHO-0004` | Static credentials entered in the app and stored in the OS keychain | §4.1 | M1 |
| `XONHO-0005` | Opening a bucket and browsing its objects as folders (prefix navigation) | §4.2 | M1 |
| `XONHO-0006` | Downloading objects to disk | §4.4 | M2 |
| `XONHO-0007` | Previewing text and images without a full download (ranged GET) | §4.5 `[S]` | M3 |

`XONHO-0007` depends on `XONHO-0006`: a preview is the same download path
asking for the first N KB instead of the whole object.

## Order

Default: **0004 → 0005 → 0006 → 0007**.

The argument for it: credential entry is the only one of the four that makes
the app usable without the AWS CLI. Today it can only read profiles that
already exist in `~/.aws`, which is a hard requirement to place on someone
opening a GUI client.

The argument against, worth revisiting before starting: a bucket list alone
does nothing useful — browsing objects is what a person opens the app to do,
so `0005` delivers real value sooner. Owner's call.

## Two additions, and the evidence for them

**Permission awareness (§4.3, M1, the brief's headline feature).** A live
session on 2026-08-19 hit `AccessDenied` three times from three different
causes — a scoped static key, an SSO role without S3 list permission, and a
key the service rejected outright — and told them apart only because someone
read the raw API errors. Saying so on the surface, rather than leaving the
user to guess between "wrong key" and "missing policy", is the differentiator
the brief already claims. It belongs early, not late.

**Opening a bucket by name, even when `ListBuckets` is denied.** Part of
`XONHO-0005`. A key scoped to one bucket is an ordinary way for an
organisation to hand out access, and it is exactly the shape the same session
ran into: permission to work inside a bucket, none to enumerate the account.
Without this the app is a dead end for those keys — it can only offer a
listing the credential is not allowed to make.

## Storing credentials: keychain, not an app-managed cipher

`XONHO-0004` uses the OS keychain (macOS Keychain, Windows Credential
Manager). Encrypting the secret ourselves would need a key of our own, which
would then have to live on the same disk — moving the problem rather than
solving it. The keychain already provides encryption at rest, per-application
access control, and unlock tied to the OS session. Repo invariant 5 says the
same thing in one line: secrets never touch the config file or the logs.
