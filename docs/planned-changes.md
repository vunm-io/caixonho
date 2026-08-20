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
sends is one R2 does not define, and the trick it was sent for buys nothing
there.

**This is not a harmless difference. It is a defect, and it blocks R2
entirely.** Two earlier drafts of this note guessed that R2 would ignore an
unknown parameter, as services usually do. Measured on 2026-08-20, it does not:

    aws s3api list-buckets --max-buckets 1000    (against R2)
    NotImplemented: ListBuckets search parameter max-buckets not implemented

The same call against AWS returns `BucketRegion` for every bucket, which is
exactly what `XONHO-0005` established and why the parameter is sent. So the
first call this application makes on opening any R2 connection fails, and it
fails in the worst available way: `NotImplemented` is a cause `classify.rs`
does not know, so it lands in `FailureKind::Other` and reaches the user as
`Error::Unexpected` — the app saying it has no idea, about a condition that has
a precise cause and a precise fix. That is the same failure shape that was
already fixed once, for a rejected SSO session, and it is the thing §4.3 exists
to prevent.

Without the parameter R2 lists buckets fine, and reports **no region at all** —
only a name and a creation date. `HeadBucket` does answer `BucketRegion: APAC`,
a Cloudflare location hint rather than an S3 region name, so the region is
knowable per bucket but not from the listing. `RegionChoice::Unstated`
therefore gets its first exercise by a real service, which is the branch
working as designed.

**The fix should observe rather than declare.** Send the parameter, and on
`NotImplemented` retry without it — one extra round trip, only against services
that reject it, and the regions keep arriving from the ones that do not. The
alternative, branching on the provider chosen in the connection form, is the
anti-pattern described below under connection types: `ADR-0002`'s reasoning
about capability applies to API features word for word, because what an
endpoint implements is found out by asking it, not by knowing whose it is.
`NotImplemented` also needs a cause of its own in the classifier either way —
"this service does not implement that" is not "something unexpected happened".

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

## What a real account did to the bucket list

Recorded 2026-08-20, the first time the list ran against an account of any
size instead of three test buckets. Roughly two dozen buckets, most of them
refused. Two things follow, and neither could have been seen before.

**The few openable buckets are buried among the refused ones.** The status
vocabulary is right — refused rows carry the badge, openable rows carry none —
but nothing acts on it, so finding somewhere to work means reading every row.
The obvious fix is to sort or group by access, and the obvious fix is wrong on
its own: **access is discovered asynchronously**, viewport-first and debounced,
so a list that orders itself by access would reorder itself under the user's
hands as probes settle. Rows moving while being read is a worse defect than the
one being fixed.

Two shapes that do not have that problem:

- **A filter rather than a reorder** — "only what I can open" — which changes
  what is present without moving what stays. It composes with the region filter
  already there, and it must say what it is doing, exactly as the region filter
  does, because a filter that hides refused buckets while probes are still
  settling is hiding rows whose status is *not yet known* rather than known-bad.
- **An explicit sort the user asks for**, applied once on request rather than
  maintained live, with the not-yet-probed in a group of their own instead of
  being guessed into one end.

Either way the honest thing is the same: **unknown is not a third shade of
denied**, and whichever ordering exists must keep it visible as its own state.

## Where the bucket list should live

Asked 2026-08-20, together with the observation that clicking a bucket does
nothing: should buckets stay in the main panel, should they move left, and
should the connection list move elsewhere.

It is not a separate question from `XONHO-0006`. The moment a bucket can be
opened, the main panel has to show what is inside it, and the bucket list
cannot also be there. So browsing forces the layout decision rather than
following it, and deciding it inside `XONHO-0006` is cheaper than deciding it
twice.

The arrangement a file-explorer-grade client converges on, and the brief does
use that phrase:

- **Left, one column, two levels**: the connections, and under the chosen one
  its buckets — a bucket becoming a place you navigate into rather than a row
  in a table. This is where the grouping problem above actually bites, because
  two dozen entries in a sidebar is a scroll rather than a glance.
- **Main panel**: the contents of wherever you are, with a breadcrumb path
  above it and an editable path bar — both already `[M]` in §4.2.
- The bucket *table*, with created date, region and access, stops being the
  home screen and becomes what the main panel shows when a connection is
  selected but no bucket is — which is also the only place those columns have
  room to stay.

What this costs: `XONHO-0009` built the shell around a sidebar that holds
connections only, so this extends that shell rather than replacing it. What it
buys: every later feature — prefix navigation, transfers, object operations —
has somewhere to live that does not need rearranging again.

## A requirement the brief does not have: caching what has been read

Asked for 2026-08-20, alongside browsing: listings and already-viewed files
should be kept so that going back somewhere is fast rather than fetched again.

`PROJECT_BRIEF.md` has nothing of the sort. §4.6 offers "persistent
per-connection state: last prefix, sort, column widths" `[S]`, which is where
the window was, not what it held; §4.3 caches *capability* per
`(profile, bucket, prefix)`. Nothing caches a listing or an object. So this is
a gap in the requirements rather than a change waiting to be cut, and it should
be added to the brief before it is planned — the brief is what
`requirements-status.md` is diffed against, and a requirement that never
entered it is one nothing will ever check.

Two things to settle when it is written, because they decide the shape:

- **What invalidates it.** A listing is a snapshot of a mutable store, and this
  project's whole posture is refusing to show something as true when it is not
  known to be. A cache that quietly serves a stale directory is that same lie
  with better latency. Time-based expiry, explicit refresh, and saying when
  what is shown was read are all defensible; silently serving old data is not.
- **Where a cached object may live.** An object's bytes are the user's data,
  and writing them to disk to make a second view fast puts them somewhere the
  user did not choose and may not know about. In memory for the session is a
  different promise from on disk between runs, and §8's security posture means
  the difference has to be decided rather than defaulted.

## Directory buckets are absent by design, not by defect

Reported 2026-08-20 as a bug: connecting with a static key listed no directory
buckets. It is not one. S3 Express One Zone directory buckets are **not
returned by `ListBuckets` at all** — they have their own operation,
`ListDirectoryBuckets`, against their own endpoint
(`s3express-control.<region>.amazonaws.com`, path-style only). An application
that calls `ListBuckets` and shows what comes back is behaving correctly.

`PROJECT_BRIEF.md` §4.2 already carries it as `[S]` at **M5**, and already
names the four parts: the listing operation, zonal endpoints, `CreateSession`
with silent refresh, and the `<name>--<az-id>--x-s3` naming. So this is
scheduled work rather than a gap — recorded here only so the next person to
notice it does not diagnose it a second time.

Worth knowing while it waits: an account holding directory buckets is now
available to test against, which is rarer than it sounds, and the brief calls
supporting them "a real differentiator" precisely because almost no GUI client
does.

## The window's views are methods, and that is why they cannot be tested

Found 2026-08-20 while splitting `app.rs` for `XONHO-0006`. Everything that
renders in that file is a method on `CaixonhoApp` reading its private fields —
72 uses of `self.`. Two consequences, and the second is the interesting one.

It cannot be moved to another module without either `pub(crate)` on some
eighteen fields or a parameter list per function, so the split stopped at the
two functions that touch no state. That is a line-count problem and a small
one.

The real cost is that **a view that reads `self` can only be exercised by
building the whole application**. `views/failure.rs` has five tests precisely
because its two functions take an error and return words; nothing else in the
window has that shape, so nothing else in the window is tested. Every rendering
defect this project has found — the table with no height, the full-width Retry
button, the empty Access cell — was found by a person looking at a screen.

The change worth making is therefore not "split the file" but "give the views
inputs": functions that take what they need and return an element, with the
application assembling them. Testability is the point and the smaller file is
a side effect, which is the opposite of how it was framed the first time.

Worth doing after `XONHO-0006` rather than before: browsing will add views, and
converting a set that is about to grow is cheaper once it has stopped growing.

## What a local server can and cannot be asked to prove

Asked 2026-08-20: could connecting, and directory buckets, be covered by tests
rather than by someone opening the application?

Partly, and the boundary is worth stating because getting it wrong buys a
feeling of safety about exactly the failures it does not cover.

**A double only knows what it was told.** This project has the definitive
example twice over: a real failure that 105 green tests said nothing about,
and then, on the day this was asked, 201 green tests beside an application
that could not list a single R2 bucket. No double had ever answered
`NotImplemented`, so no test could have.

**A fake server tests you against the fake, not against every real service.**
MinIO would in all likelihood have accepted or ignored the `max-buckets`
parameter, so a MinIO rig would *not* have caught the R2 defect. Only R2
catches R2. A rig that is mistaken for coverage of "S3-compatible services" is
worse than no rig.

**Which is not an argument against one — the argument for it is different.**
A local server's value here is that it fails *on demand*. It enforces bucket
policies, so it can produce, deterministically and in CI, four things neither
of this project's real accounts can:

- a **refused** bucket, and a refused **prefix** — the headline feature's own
  case, and the part of `XONHO-0006` easiest to get wrong;
- an account holding **no buckets at all**, so the empty state stops being
  a rendering nobody has seen;
- a listing of **100k objects**, against which the virtualised table's claim
  can finally be measured rather than asserted from a synthetic feed in M0.

That turns four one-off manual checks into regression tests, which is a
different and better thing than fidelity.

**Directory buckets have no local option at all.** LocalStack lists S3 Express
directory bucket support as a backlog feature request — triaged, not being
worked on. The reason is structural rather than incidental: directory buckets
need a regional endpoint for `ListDirectoryBuckets`, zonal endpoints for object
operations, and `CreateSession` tokens that live five minutes and must be
refreshed silently. MinIO's commercial AIStor line documents an "S3 Express
mode" whose control-plane coverage has **not** been verified here, and should
not be planned against until it has been.

So `[S]` at M5 stays testable only against real AWS. Worth knowing before M5 is
planned, not during it.

Mechanically, when the rig is built: the repository already has a convention
for tests that touch this machine — `#[ignore = "<reason>"]`, used twice — and
integration tests extend it rather than inventing something. `testcontainers`
in `dev-dependencies` starts the server per run. There is no `tests/` directory
yet, so this would be the first, and it is its own change. **After
`XONHO-0006`**: browsing is what produces the listings worth testing, and
building the rig first means writing fixtures for code that does not exist.

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
- **Opening one stored connection can ask the keychain twice, and that is
  correct.** Recorded here on 2026-08-20 as a defect — that the second dialog
  asked for a session token the user never stored — and **measured on
  2026-08-20 to be wrong**. `credentials::load` reads two entries because a
  credential *has* two halves, and a connection saved with a session token
  genuinely holds both: `security find-generic-password` finds a session-token
  entry for the connection that has one and none for the connection that does
  not. Two secrets, two authorisation prompts, macOS working as designed.

  The correction is kept rather than deleted because it is the second
  prediction in this file that measurement overturned, and both were written
  before anyone ran the command that would have settled them.

  What remains unverified: whether a *lookup* of an absent entry prompts at
  all. If it does not, a connection without a session token already asks once
  and there is nothing here to fix.

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

## Why the keychain keeps refusing a build you just made

Found 2026-08-20 while testing `XONHO-0011`, and it will happen again to
whoever develops this next.

A stored connection failed with *"the credential store refused the request"*.
The item was there — `security find-generic-password -s "caixonho secret
access key" -a <connection>` found it in the login keychain — and the keychain
refused to hand it back anyway.

The cause is the binary, not the item. Measured on the machine:

- `codesign -dv target/debug/caixonho-gui` reports `adhoc, linker-signed`, with
  no TeamIdentifier. An ad-hoc signature carries no identity that survives a
  rebuild.
- The item was created at `20260819170800Z`. The binary asking for it was built
  the following evening.

macOS binds a keychain item's ACL to the application that created it. A rebuilt
ad-hoc binary is a different application, so the prompt returns, and a prompt
that is declined — or that appears behind the window and is dismissed — is
reported exactly as this application reports it: refused, with the item intact.

Two consequences worth keeping:

- **In development, expect it after every rebuild.** "Always Allow" grants the
  binary that exists at that moment and nothing later. This is also the honest
  explanation of the earlier note that the keychain "asks twice": it asks per
  item per binary.
- **It is an argument for signing, beyond distribution.** `docs/requirements-status.md`
  carries "One self-contained binary per platform" as `[M]`, currently *partial*
  with "nothing is signed". Unsigned is not only a download-warning problem:
  an application whose identity changes every build cannot hold onto the
  credential access a user granted it. A stable Developer ID is what makes
  "Always Allow" mean always.
