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
| `XONHO-0006` | Opening a bucket and browsing its objects as folders (prefix navigation), including reaching a bucket by name | §4.2 | M1 |
| `XONHO-0004` | Static credentials entered in the app and stored in the OS keychain, and session lifetime | §4.1 | M1 |
| `XONHO-0007` | Downloading objects to disk | §4.4 | M2 |
| `XONHO-0008` | Previewing text and images without a full download (ranged GET) | §4.5 `[S]` | M3 |

`XONHO-0008` depends on `XONHO-0007`: a preview is the same download path
asking for the first N KB instead of the whole object.

## Order

**0005 (in flight) → 0006 → 0004 → 0007 → 0008.**

Browsing comes before credential entry because a bucket list alone is a dead
end: the app can name an account's buckets and then do nothing with them, and
opening objects is what a person launches an S3 client to do. `XONHO-0006` also
carries reaching a bucket by name, which makes a credential scoped to a single
bucket usable at all — a shape this project has already met. It waits for
`XONHO-0005` only because a name box needs a bucket view to land in.

Credential entry was the earlier default, on the argument that it is the only
one of these that makes the app usable without the AWS CLI. That argument is
about a user who does not have one; it buys nothing for the person using the app
today, whose working profiles already exist in `~/.aws`. It is required before
anyone else can use this, and it keeps its place ahead of the M2 work.

`XONHO-0004` also inherits one item from `XONHO-0003`: confirming the
expired-session path. It was left unverified there because no expired SSO token
was available and clearing one on the development machine revokes every other
profile's token as well. Session lifetime is `0004`'s subject, and the check
belongs in a test rather than a manual step.

## Two additions, and the evidence for them

**Permission awareness (§4.3, M1, the brief's headline feature).** A live
session on 2026-08-19 hit `AccessDenied` three times from three different
causes — a scoped static key, an SSO role without S3 list permission, and a
key the service rejected outright — and told them apart only because someone
read the raw API errors. Saying so on the surface, rather than leaving the
user to guess between "wrong key" and "missing policy", is the differentiator
the brief already claims. It belongs early, not late — and it is now landing:
`XONHO-0005` carries the observed-capability model for bucket listing, and
`XONHO-0006` extends it to prefixes.

**Opening a bucket by name, even when `ListBuckets` is denied.** Part of
`XONHO-0006`. A key scoped to one bucket is an ordinary way for an
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
