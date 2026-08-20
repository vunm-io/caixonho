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

## What `XONHO-0006` has to decide: S3 has no folders

Recorded 2026-08-20, while putting a test fixture together. `ListObjectsV2`
with `delimiter=/` answers in two separate fields — `CommonPrefixes` are the
folders and `Contents` are the objects — so the application never has to guess
which a row is. What it does have to decide is what to show when the two
disagree, and they disagree in ways that are ordinary rather than exotic:

- **A folder that is also an object.** "Create folder" in the AWS console, and
  most other clients, writes a zero-byte object whose key ends in `/`. It comes
  back in `CommonPrefixes` *and* in `Contents`. Showing both is a folder with a
  mysterious 0-byte file inside it that the user did not create and cannot
  explain.
- **A folder that is no object at all.** A single key `photos/cat.jpg` makes
  `photos/` appear as a prefix with nothing behind it. It cannot be selected,
  has no size, no last-modified, no storage class and no ETag — so every column
  the brief asks for is empty for it, and that emptiness is the honest answer
  rather than a gap to fill with placeholders.
- **An object and a prefix sharing a name.** `notes` and `notes/meeting.md` can
  both exist. Two rows called `notes`, one openable and one not.
- **A folder that is empty.** Only visible at all because a marker object was
  written for it; delete the marker and the folder ceases to exist.

This is the same principle as the sort and filter honesty the brief already
asks for, applied to hierarchy: the folders are inferred, and the UI should not
pretend they are a thing S3 stores.

A fixture covering all four, plus deep nesting, a multi-megabyte object for
size formatting and a key with spaces and non-ASCII characters, lives in the
R2 bucket `caixonho-test` (see the endpoint note below). It is deliberately
richer than the three S3 test buckets, which are empty.

Two of the four are already visible in what the service returns for it. At the
root, with `delimiter=/`, `notes` comes back as a 35-byte object *and* `notes/`
as a prefix — one name, two rows, one of them openable. And a marker does not
appear beside its own folder but *inside* it: listing `prefix=photos/` returns

    CommonPrefixes: photos/vacation/
    Contents:       photos/ (0 bytes), photos/cat.jpg, photos/dog.jpg

so the key `photos/` is an entry within `photos/` whose name — everything after
the prefix — is the empty string. Rendered without thought that is **a row with
no name and no size inside every folder anyone created from a console**. It is
the first thing to get right, and it costs nothing to get right: the entry
whose key equals the prefix is the folder itself and is never its own child.

## Testing against something that is not AWS

R2 is reachable **today on the profile path and not on the stored-credential
path**, which is worth knowing before anyone plans work around it.

`adapter.rs` already honours a configured endpoint over any region, and both
connection paths build their SDK config through `aws_config::defaults`, which
reads `endpoint_url` from the profile and `AWS_ENDPOINT_URL` from the
environment. So a profile in `~/.aws/config` with an `endpoint_url` and
`region = auto` connects to R2 with no code change at all.

A connection *typed into the app* cannot: `connections.toml` holds a name, a
region and an access key id, and has nowhere to put an endpoint — so an R2 key
entered in the app is sent to AWS. Giving it somewhere is the same shape of
work as `XONHO-0013` and the session-token field above: three separate reasons
now to widen that file, which argues for widening it once, deliberately.

Worth doing early rather than at M5, where "S3-compatible endpoints" currently
sits: a second implementation is the cheapest way to find every place the code
assumed AWS rather than S3, and R2's free tier (10 GB, 1M class A operations
and 10M class B per month, no egress charge) covers this project's testing
without a bill.

### The first AWS assumption it found, before a single connection was made

`adapter.rs` sends `ListBuckets` with `max_buckets(1000)`, and the constant's
comment explains why: AWS reports each bucket's `BucketRegion` only when the
request carries at least one valid parameter, so the page size is what buys the
regions inside a call already being made. That was established live against AWS
in `XONHO-0005`, and it is true of AWS.

R2 names the same idea differently. Its `ListBuckets` takes the `ListObjectsV2`
search parameters — `prefix`, `start-after`, `continuation-token` and
**`max-keys`** — with `cf-`-prefixed header equivalents, and a default and
maximum of 1000. There is no `max-buckets`. So the parameter the application
sends is one R2 does not define, the trick it was sent for buys nothing there,
and R2 has no per-bucket region to report in the first place because every
bucket is `auto`.

The expected consequences, to be confirmed against the running application
rather than assumed: the listing still works, because the page size R2 ignores
is also the default it would have used and `continuation-token` is common to
both; and every R2 bucket arrives with no region, landing in
`RegionChoice::Unstated` — the branch that exists so such buckets do not vanish
from every filter, and which no real service had exercised until now.

### R2 tokens hand out exactly the shape this project is about

R2's token permissions split along the same line the application does. An
**Object Read & Write** or **Object Read only** token can list and read objects
inside buckets but **cannot enumerate the buckets** — and widening its scope to
"all buckets" does not change that, because the limit is the permission class
rather than the resource set. Only **Admin Read only** and above can list
buckets. Confirmed against Cloudflare's token documentation on 2026-08-20,
after an object-scoped token was widened to every bucket and `ListBuckets`
went on being denied.

That makes an object-scoped token the cheapest fixture this repository has for
two things it currently has no way to see:

- `XONHO-0009` needs "a profile that is denied" to check the error state
  against, and this is one that is denied for a real reason rather than a
  broken key.
- It is exactly the "key scoped to a bucket, no permission to enumerate the
  account" case that `XONHO-0006` has to answer with *open a bucket by name* —
  and against such a token the application today is a dead end, because
  `ListBuckets` is its only door in.

So the useful arrangement is two tokens, not one fixed token: an **Admin Read
only** token for browsing, and the object-scoped one kept deliberately as the
denied fixture.

## Choosing what kind of service a connection points at

Asked on 2026-08-20: the connection form should let the user pick AWS S3 or
Cloudflare R2, with more added later. Yes — with one boundary that has to hold,
because the idea sits right next to a decision this project has already made.

**A preset declares configuration. It must never declare capability.**
`ADR-0002` says capability is observed and never declared, and that stands: what
a credential may do is found out by trying, not by knowing which company runs
the endpoint. Configuration is the opposite kind of fact — an endpoint cannot be
discovered, the user has to say it — so declaring *that* is not a retreat from
the ADR, it is the other half of it.

The failure mode to design against is the dropdown quietly becoming a place
where behaviour branches: `if provider == R2 { … }`. That is capability by
brand, it is wrong on its own terms — a token's permissions vary far more than
its provider does — and it rots, because what a service supports changes and a
match arm does not.

The `max-buckets` difference above is the worked example of both fixes:

- **Wrong**: stop sending the parameter when the provider is R2.
- **Right**: notice it is an optimisation for AWS whose absence costs nothing
  anywhere, keep sending it, and let a bucket with no region be `Unstated` —
  which the code already does, for a branch written before R2 was in the
  picture.

So the shape is: **a preset fills in fields, and then gets out of the way.**
Picking "Cloudflare R2" writes an endpoint template, `region = auto` and an
addressing style into an ordinary form the user can see and edit. What is
stored is those fields. The provider may be kept as a label — an icon, a way to
group the sidebar — but it must not be load-bearing at connect time: a
connection has to remain openable from its own fields alone, or a preset
edited in some later version silently redirects connections already saved.

Two consequences worth stating now:

- **"S3-compatible (custom endpoint)" is the general case, and the named
  providers are shortcuts to it** — not the other way around. Otherwise every
  new service is a code change, and MinIO, Backblaze, Wasabi and Ceph queue up
  behind a release. It also hands M1 the MinIO rig it has been missing for
  free.
- This is the **fourth** reason to widen `connections.toml`, after the
  session-token flag, `XONHO-0013`'s editing, and the endpoint: addressing
  style, and possibly a provider label. Four reasons is no longer an argument
  for widening it — it is an argument for deciding its schema once, deliberately,
  and only then writing any of them.

Where it goes in the plan is a real question, not a formality: `roadmap.md`
puts "S3-compatible endpoints as a supported configuration" at **M5**, and this
pulls a piece of it into M1. That needs a proposal saying which `[M]`
requirements it delivers and which it steps over, per the planning gate in
`AGENTS.md` — the argument for it being that a second implementation is what
finds the AWS assumptions, and one has already been found without connecting.

## Smaller things, found at close-out and not yet cut into changes

Recorded 2026-08-20, closing out `XONHO-0004` and `XONHO-0012`. None of these
is large enough to be a change on its own yet; all of them are large enough to
be lost if they stay in a session log.

- **The keychain has only ever been exercised on macOS.** Windows Credential
  Manager is reached through the same `keyring` API and compiles in CI, but
  nobody has stored, read and forgotten a secret on Windows — and Windows is
  the primary daily driver by the repo's own account. It belongs to whichever
  change first has a Windows machine in front of it, and it should be a task
  in that change rather than a hope.
- **The refused credential is still unexplained.** Two saved connections carry
  the same access key id and only one works, so the difference is in the
  keychain rather than in anything the app does. It was not diagnosable when
  it appeared; it is now, and it is the natural first case for the log.
- **Opening one stored connection asks the keychain twice.** `credentials::load`
  reads two separate items — `caixonho secret access key` and then
  `caixonho session token` — and each is its own macOS authorization subject,
  so each raises its own password dialog. A static credential has no session
  token, so the second read is asking for something the user never stored. The
  fix is to know from the connection's own configuration whether a session
  token exists and not to go looking when it does not; it needs a field in
  `connections.toml`, which is why it is a change rather than an edit.
  Observed on 2026-08-20 by a user who answered the dialog and got it again.
  - Worth checking while doing it: if a `caixonho session token` entry exists
    for a connection that was saved without one, the delete-on-save path at
    `credentials.rs` is leaving residue — and a stale entry beside a current
    key is a candidate explanation for the refused connection above.
- **The macOS bundle is unsigned**, which `scripts/mac-app.sh` says plainly.
  A keychain ACL is granted to a code identity, so an unsigned binary that is
  rebuilt is a new applicant every time: "Always Allow" cannot stick, and every
  run re-asks. It is dev-convenience packaging and real signing is its own
  milestone — but it means keychain prompt behaviour seen today is not the
  behaviour a shipped build will have, and neither is evidence about the other.
- **`block v0.1.6` will be rejected by a future Rust.** It arrives through
  `cocoa` → `gpui` at the pinned zed commit, so it is macOS-only, upstream,
  and movable only by bumping the UI stack — which `ADR-0001` already makes a
  change of its own, green on both targets. Worth a line in that change rather
  than a change of its own.
- **`cargo deny` is promised by the brief (§8) and absent from CI**, which
  runs fmt, clippy and tests only. The `block` warning above is exactly the
  class of thing it would have surfaced without anyone reading clippy output.

## Storing credentials: keychain, not an app-managed cipher

`XONHO-0004` uses the OS keychain (macOS Keychain, Windows Credential
Manager). Encrypting the secret ourselves would need a key of our own, which
would then have to live on the same disk — moving the problem rather than
solving it. The keychain already provides encryption at rest, per-application
access control, and unlock tied to the OS session. Repo invariant 5 says the
same thing in one line: secrets never touch the config file or the logs.
