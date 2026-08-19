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
| `XONHO-0004` | Credentials the user enters, the OS keychain, in-app SSO sign-in and inline re-login | §4.1 | M1 |
| `XONHO-0007` | Downloading objects to disk | §4.4 | M2 |
| `XONHO-0008` | Previewing text and images without a full download (ranged GET) | §4.5 `[S]` | M3 |
| `XONHO-0013` | Editing a saved connection: its region, its key, and renaming it | §4.1 | M1 |

`XONHO-0008` depends on `XONHO-0007`: a preview is the same download path
asking for the first N KB instead of the whole object.

## Order

**0009 → 0004 → 0006 → 0007 → 0008.**

Credential entry moved ahead of browsing on 2026-08-19, reversing the earlier
decision. The argument for browsing first was that a bucket list alone is a dead
end and opening objects is why anyone launches an S3 client. That is still true,
and it is still the argument for putting `XONHO-0006` immediately after.

What changed is the standing of the counter-argument. Credential entry was
treated as work for a hypothetical future user, on the grounds that the person
using the app today already has working profiles in `~/.aws`. Those profiles
reach a password manager through an external process, which is one developer's
test scaffolding — it is not how anyone else will hold credentials, and it is
not what the brief describes. Ordering around it meant ordering around a
temporary local arrangement, and it showed: the first thing anyone notices about
the app is a wait that only that arrangement produces.

`XONHO-0004` is also where a connection stops pretending. Today a profile whose
sign-in fails is offered like any other and explains itself only after seven
seconds of trying. With sign-in in the app, a connection that cannot authenticate
is simply unavailable, which is both truthful and instant.

`XONHO-0004` inherits one item from `XONHO-0003`: confirming the expired-session
path. That path is now half-closed — the classifier names an unusable session as
of 2026-08-19 — and what remains is offering the re-login, which is `0004`'s
subject.

## Editing a saved connection

Asked for on 2026-08-20, once removing them had a home. Managing connections is
now a surface of its own, and editing belongs on it beside removal — but it
needs requirements before it needs code, because its three parts are not the
same operation:

- **Changing the region** touches only the configuration file. Safe.
- **Replacing the key** touches only the credential store. Also safe, and the
  access key id and the secret must move together — a new id against an old
  secret is a credential that fails in a way that reads like a typo.
- **Renaming** is a move: read the secret, write it under the new name, delete
  the old. It is the one that can strand a secret if it fails halfway, and the
  ordering rule the store already follows — the residue is always something the
  application can name — decides how.

## Connecting is something the user asks for

Recorded 2026-08-19, after the owner watched the app take seven seconds to show
anything and asked what it was doing.

The app opens, picks a profile on its own, and resolves that profile's
credentials immediately — the code says so: *"Open on the default profile when
there is one, so the first screen shows data rather than an instruction."* That
sentence is the defect. Nobody asked for a listing yet, and on a machine whose
credentials come from an external process the wait is **7 seconds, or 26 on the
first run of the day**, measured. The window looks frozen doing work nobody
requested.

**Startup should show the connections and stop there.** Nothing resolves until a
connection is chosen. This removes the wait rather than hiding it, and it is a
smaller change than the caching that was briefly planned to paper over it.

### And the credential story was upside down

Credentials on the development machine come from a password manager through
`credential_process`. That is **test scaffolding for one developer**, not the
product. The brief has said so from the start, and all three of these are `[M]`
in §4.1 with none of them built:

- **Static credentials typed into the app**, stored in the OS keychain. This is
  the answer to "why is it slow" for anyone who is not that one developer — there
  is no external process to wait for.
- **In-app SSO sign-in** via the OIDC device flow, "so the AWS CLI is not a hard
  dependency".
- **Inline re-login when a token is spent.** As of today the app *detects* an
  unusable session and names it; it still offers nothing to do about it. A
  connection that cannot sign in is simply a connection that is not available,
  and should read that way rather than being listed as though it worked.

`XONHO-0004` therefore covers entry, the keychain, sign-in and re-login, and
moves ahead of the caching idea. Caching what an external credential process
returned is still worth doing for profile switching, but it optimises a path the
product does not depend on, so it waits.

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
