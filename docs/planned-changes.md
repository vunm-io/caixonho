# Planned changes

A staging list: what the next changes are, why they are separate, and what
order they go in. Entries leave this file when they become real OpenSpec
changes under `openspec/changes/`.

Requirements live in [`PROJECT_BRIEF.md`](PROJECT_BRIEF.md); this file only
decides how they are cut into changes.

**Entries go stale in two directions, and only one of them is obvious.** A
finding can be overtaken by work that fixed it, and a section written while a
decision was open can go on reading as though it still were. Both leave the
next reader planning work that exists — which is the opposite of what a
staging list is for.

*Audited 2026-08-22 against `XONHO-0015` through `XONHO-0019`.* Corrected:
the `cargo deny` entry (delivered by `XONHO-0017` the day before), the
`XONHO-0006` folder section (decided, shipped, and still titled *has to
decide*), and *Where the bucket list should live* (built, and built
deliberately differently from its own first bullet). Checked against the code
and found still true, so that the next audit can start after them: R2 reachable
on the profile path only — `StoredCredential` is still `{name, region,
access_key_id}` with nowhere for an endpoint; no filter by access and no sort,
so the buried-openable-buckets problem stands; no listing or object cache in
the brief; and both mechanisms deferred out of `XONHO-0016` still unstarted —
`Scope` is still `{bucket, prefix}`, and `shown_kind` is a badging rule rather
than the narrowing-by-kind that section describes.

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
| `XONHO-0020` | Uploading one file, and never silently replacing an object | §4.4 | M2 |
| `XONHO-0021` | Deleting one object deliberately, with the undo the service already offers | §4.5 | M3 |
| `XONHO-0022` | Reading a stored secret once per run instead of once per connection open | §4.1 | M1 |

`XONHO-0008` depends on `XONHO-0007`: a preview is the same download path
asking for the first N KB instead of the whole object.

## Order

**0009 → 0004 → 0006 → 0007 → 0008.**

> **`XONHO-0008` planned 2026-08-25** — its number and row have sat in this
> table since M1 was cut; the change directory now exists
> (`a-look-without-a-download`). One reading recorded at planning time: the
> brief's single preview line splits in two, because a ranged image does not
> decode — text previews by its first page, images whole under a size gate.
>
> **`XONHO-0020` issued 2026-08-24** for the upload direction, and planned
> the same day — the number is anchored in the table above rather than only
> in a change directory, which is what this table is for.
>
> **`XONHO-0007` landed 2026-08-24** — single-object download plus
> open-with-the-system; uploads and the queue are M2's remaining body. Noted
> here because this section reads as a future plan, and the two-directions
> lesson in the header is about exactly this sentence going stale.

### A declined prompt is a choice, not a failure (2026-08-25)

The owner, on meeting it for the fourth time: the panel is an eyesore. It
is, and the reason is a category error rather than a styling one.

When the OS credential prompt is declined, the app renders the full red
failure panel — the same weight it gives a network outage or a refused
credential. But declining a prompt is *the user answering a question*, not
something breaking. `XONHO-0009`'s own design language says an error is
"an inline message with the cause and a single action"; this has the cause
and the action right and the **severity** wrong, which no amount of correct
wording fixes.

Two things to weigh when this is scoped:

- `CredentialStoreProblem::Refused` should probably not share a surface with
  `Locked` and `Absent`. Those two are conditions of the machine; this one
  is a sentence the user just spoke.
- The retry loop is the actual irritant. A declined prompt today leaves the
  connection selected and failed, so the next click asks again — the app
  should probably remember the decline for the session rather than
  re-asking on every selection.

Not scoped here. §4.6, beside the copy gap above; both are quality-of-life
findings from the same sitting.

### Nothing on screen can be copied (2026-08-25)

Reported by the owner while trying to hand a bucket name to a command: the
name is on screen, and there is no way to get it into the clipboard. It
applies to every string the app draws — bucket names, object keys, the
breadcrumb, the log's own directory in the status bar, and the error panels
whose whole purpose is to be pasted into an issue.

Worth more than its size: `XONHO-0011` already made a point of the sign-in
code being *selectable* ("nothing about the attempt is only in the
browser"), and `XONHO-0012` put the log's location in the status bar so a
report could carry evidence — both of which assume a person can copy what
they are shown. This is the general form of that assumption, unbuilt.

Not scoped here. Two candidate shapes, and the choice wants a look at what
the toolkit offers rather than a guess: text selection on the labels, or a
context-menu Copy on the row (which the connection list already has a
right-click menu for, so the affordance exists). §4.6 quality-of-life is
where it belongs.

### Opening a connection re-runs `credential_process` every time (2026-08-25)

**This supersedes the 2026-08-23 note below, which guessed wrong.** That note
blamed R2's slowness on a directory-bucket listing burning a timeout. It is
not that. Measured today across 36 connection opens:

| Connection kind | open → listed |
|---|---|
| Stored credential (secret from the keychain) | 0.06 – 0.3s |
| Profile using `credential_process` | **4.3 – 8.8s** |

And the credential process itself, timed directly: **3.99s warm, 16.36s
cold.** That is the whole difference. Both slow profiles resolve through a
script that shells out to the 1Password CLI, and `Session::open` builds a
fresh SDK config on every open — so every connection click pays for a
subprocess, a vault lookup, and its round trip again.

**`XONHO-0022` does not fix this**, and that is worth saying loudly because
the two problems look alike. That change caches the *keychain* read, which
is the path already running in 0.1s. The 4-second path is the SDK's own
provider chain, which this application never sees.

The shape underneath both is one thing: `open()` rebuilds everything from
nothing each time. Remembering the opened connection per name would fix
both at once — and it is not free, because `Session::open` deliberately
resets the store, the scheduler and the credentials id, and `XONHO-0019`'s
switching guarantees and the capability model both lean on that reset. So:
a real change with a real design question, not a tweak.

Evidence to gather first: whether the AWS SDK's own provider chain would
cache the process credentials if the config outlived one open. If it would,
the fix is smaller than it looks.

### The superseded note (2026-08-23), kept

Listing the R2 connection took **12.3 seconds for 2 buckets** (log,
2026-08-23: `connection opened` 07:03:01.6 → `listed the account` 07:03:13.9)
while the same build listed an AWS account in 0.7s and stored-credential
connections in ~60ms. Suspicion, unverified: the parallel directory-bucket
listing against an endpoint that does not implement it may be burning a
timeout before the combine.

Kept rather than deleted, because the *shape* of the mistake is worth more
than the guess was: it reached for the most interesting explanation
available — a subtle protocol interaction this project had already written
a change about — when the answer was a four-second subprocess nobody had
timed. The note even said "measure before deciding", and then two days
passed before anyone did.

### "From ~/.aws" says where, and is read as what (2026-08-25)

The sidebar groups profile connections under **From ~/.aws**, which is
true: that is the file they are read from. The owner pointed out what it
*reads* as — `r2-caixonho` sits under it, and is Cloudflare R2, not AWS.

The label describes the source and the reader hears the service. Not a
wrong statement, but a misleading one, which is the failure mode this
project spends most of its care on elsewhere. The material to do better is
already in hand: a profile carrying an `endpoint_url` is by definition not
talking to AWS, and `XONHO-0016` already taught the bucket list to name
what a thing is rather than let its shape imply it.

§4.6, with the copy gap and the declined-prompt severity — three findings
from one sitting, all of them about the interface saying something other
than what is true.

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

## What `XONHO-0006` had to decide: S3 has no folders

> **Decided and shipped. Retitled 2026-08-22** — it read *has to decide* for
> two days after the deciding was done. Each of the four now has a test
> carrying it in `caixonho-core/src/listing.rs`, and the names are the map:
> `a_folder_marker_is_the_folder_rather_than_an_entry_inside_it`,
> `a_prefix_nothing_stands_behind_is_still_a_folder`,
> `an_object_and_a_prefix_may_share_a_name_and_both_survive`, and
> `the_marker_rule_does_not_touch_a_key_that_merely_starts_the_same` — the
> last being a guard nothing below asked for, added because the marker rule is
> a prefix comparison and prefix comparisons over-reach.
>
> The reasoning is kept in full, in the present tense it was written in. It is
> why the code looks the way it does, and a reader who meets only the tests
> gets the rule without the four ordinary situations that produced it.

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

> **Decided and built, and not quite as proposed. Noted 2026-08-22.** What
> shipped follows this section everywhere except its first bullet. The rail
> does *not* hold the chosen connection's buckets whenever a connection is
> chosen: it appears **only while inside a bucket**, and at account level the
> main-panel table carries them alone.
>
> The reason is better than the proposal and lived only in a code comment on
> `bucket_group` until now — at account level the table already lists every
> bucket with room for the full name, so a rail there repeats it in a third of
> the width. The rail exists for the case the table cannot serve: once the main
> panel gives itself over to a bucket's contents, the account would otherwise
> disappear from the screen entirely.
>
> Written down because a proposal left standing next to a different
> implementation reads as a plan not yet carried out, and the next person to
> "finish" it would widen the rail back into the case that was deliberately
> declined.

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

## Directory buckets: what "absent by design" turned into

Reported 2026-08-20 as a bug: connecting with a static key listed no directory
buckets. It was not one, and the diagnosis is kept because it is still the
reason the code looks the way it does. S3 Express One Zone directory buckets
are **not returned by `ListBuckets` at all** — they have their own operation,
`ListDirectoryBuckets`, against their own endpoint
(`s3express-control.<region>.amazonaws.com`). An application that called
`ListBuckets` and showed what came back was behaving correctly.

**`XONHO-0016` built them the same day**, pulled forward out of M5 because the
one account available to verify against cannot list ordinary buckets at all:
directory buckets were the difference between a connection that works and a
connection that shows an error. What that change found, and what is worth
knowing before touching this again:

- **Three of the four parts are the SDK's.** `aws-sdk-s3` resolves both the
  control-plane and the zonal endpoints, and obtains and refreshes the session
  itself through an expiring identity cache installed by default. Only the
  listing call and the presentation were ours. Anyone reimplementing
  `CreateSession` here is duplicating something that already works.
- **A refusal of that session does not arrive as a denial.** It happens inside
  the SDK before our request is dispatched, so what reaches the classifier is a
  dispatch failure with no code and no status, carrying the service error in
  its chain. It read as "unexpected error: the call failed without a reportable
  cause" until rule 4 of `classify` learned to read a denial out of the chain
  when there is no response to read instead.
- **`ListObjectsV2` on a directory bucket answers without `KeyCount` or
  `IsTruncated`.** Pagination that depends on either will silently misbehave;
  ours follows `NextContinuationToken` and was unaffected.
- **A zone id is not two segments.** A local zone's has three. Nothing may
  parse one by counting.

### Still deferred, with the reasoning

Two mechanisms were designed during `XONHO-0016` and taken back out so it could
land the capability first. Both are worth doing, and neither is started:

- **Remember a refusal against the credentials that earned it**, so a listing
  observed to be refused is not issued again until those credentials change.
  Today an account that will never hold directory buckets pays one wasted
  request and shows one refusal on every connect — and a refusal shown every
  time is how a user learns to stop reading refusals. `Scope` is
  `{bucket, prefix}` and would grow an account level; the credential-keyed
  retention and its invalidation already exist.
- **Narrow the bucket list by kind**, applied to what has been retrieved and
  issuing no request, the same shape as the region narrowing.

A connection-level switch — choose ordinary, directory or all when connecting —
was proposed and rejected, and the reason is worth keeping: set to "ordinary"
on an account holding only directory buckets it renders "this account has no
buckets", with no signal that a setting emptied the screen. It also asks the
account holder to declare what one request can observe, which is the inversion
`ADR-0002` exists to prevent.

## The window's views are methods, and that is why they cannot be tested

> **Corrected 2026-08-21 by `XONHO-0015`. The title is wrong and the section is
> kept because how it was wrong is worth more than the correction.**
>
> Every observation below still holds: the views are methods, they read
> `self`, and exercising one does mean building the whole application. What
> does not follow — and what this section concluded — is that they therefore
> cannot be tested. **Building the whole application in a test was the thing to
> fix, and it was one seam wide.** `CaixonhoApp::new` read `~/.aws`, the
> keychain and the trust store inside itself; once those became a value handed
> in, a test could construct the real view, drive `apply_page` through it, and
> render it to an image. Neither view was rewritten.
>
> The recommendation — *give the views inputs* — is still worth doing, but for
> its own reasons now (a smaller blast radius per test, and functions a reader
> can follow), not because it is the only road to testing them. That
> distinction matters, because the section as written would have bought a
> refactor of every view in the window to obtain something a constructor
> signature already gave.
>
> The habit: **when something is called untestable, find the one thing that
> makes it so before rewriting what surrounds it.** Here it was six lines of
> environment reading, and they had been sitting in a constructor since M0.

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
- ~~**`cargo deny` is promised by the brief (§8) and absent from CI**, which
  runs fmt, clippy and tests only.~~ **Done by `XONHO-0017` on 2026-08-21**:
  CI has a `dependency audit` job on `cargo-deny-action@v2`, and the four
  advisories that were being shipped were removed rather than accepted. Struck
  through on 2026-08-22 rather than deleted, because the entry had gone stale
  in place for a day — this file is where findings are kept so they are not
  lost, and a finding that has since been *fixed* is a different kind of loss:
  the next reader plans work that already exists.

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

## What auditing the dependencies actually finds (measured 2026-08-21)

`cargo-deny` + `cargo-audit` in CI is promised by the brief (§5) and by §8's
"Dependencies audited in CI"; `docs/requirements-status.md` still carries that
row as *none*. It was assumed to be a two-line CI change. **It is not**, and
this is the measurement rather than the assumption. `XONHO-0017` is issued for
it and unplanned.

`cargo audit` against the current lockfile — **944 crates** — reports **4
vulnerabilities and 7 warnings**. Adding the step to CI without deciding what
to do about them turns the pipeline red on the first run.

**Amended the same day, after `XONHO-0018`: the lockfile is now 957 crates.**
That change added `aws-smithy-http-client`'s `test-util` feature under
`[dev-dependencies]`, and it brought 13 crates with it — `pretty_assertions`,
`ciborium`, `aws-smithy-protocol-test` and the rest of a test-support tree.
The four vulnerabilities and seven warnings are unchanged; re-measured, not
assumed. But the number is worth carrying because of what it implies for the
job being planned: **`cargo audit` scans the lockfile, and the lockfile holds
dev-dependencies.** A policy written against "what we ship" and a tool reading
"what we resolve" disagree by an entire test-support tree, and the difference
will show up as advisories in crates that never reach a user. Deciding how the
job treats them — `--ignore`, a `deny.toml` scope, or accepting that dev trees
are audited too — is part of the policy work, not a detail of the wiring.

| Advisory | Crate | Patched in |
|---|---|---|
| RUSTSEC-2026-0098 | `rustls-webpki 0.101.7` | `>=0.103.12` |
| RUSTSEC-2026-0099 | `rustls-webpki 0.101.7` | `>=0.103.12` |
| RUSTSEC-2026-0104 | `rustls-webpki 0.101.7` | `>=0.103.13` |
| RUSTSEC-2026-0258 | `h2 0.3.27` | `>=0.4.16` |

The seven warnings are six *unmaintained* crates (`bincode`, `instant`,
`paste`, `rustls-pemfile`, `rustybuzz`, `ttf-parser`) and one *unsound*
(`lru 0.16.4`, a use-after-free on panic in `LruCache::pop`). The font crates
arrive with the UI stack; the rest with the SDK.

**Where they come from matters more than the count, and it is good news.**
Both vulnerable crates arrive through `aws-smithy-http-client 1.3.0`, not
through the frozen UI stack — so fixing them is not blocked behind ADR-0001.

> **Corrected the same day, and left visible on purpose.** The paragraph that
> stood here was wrong about *how* they arrive, and it is left described rather
> than deleted because the next reader will otherwise trust the next paragraph
> just as readily.
>
> It said: this workspace asks for `rustls-aws-lc`, that turns on `__rustls`,
> and `__rustls` pulls `hyper-rustls 0.24.2` **with its `acceptor` feature** —
> the server-side TLS path — so the advisories sit in code compiled and never
> called, because an S3 client accepts no TLS connections.
>
> `aws-smithy-http-client`'s own `[features]` table says `__rustls` pulls the
> *current* `hyper-rustls`. There is no `acceptor` feature in the chain and no
> server-side path. The real route, traced with
> `cargo tree --edges features,no-dev`:
>
> ```
> aws-sdk-s3 / aws-sdk-ssooidc  (default features)
>   └── feature "rustls"
>       └── aws-smithy-runtime/tls-rustls
>           ├── aws-smithy-http-client/hyper-014          → hyper 0.14, h2 0.3.27
>           └── aws-smithy-http-client/legacy-rustls-ring → rustls 0.21.12
>                                                           → rustls-webpki 0.101.7
> ```
>
> It is the **legacy client** stack, enabled by the default features of two
> crates this workspace names directly — and those features supply an HTTP
> client `tls.rs` already replaces at every construction site.
>
> **The conclusion the error supported was the expensive part.** "Compiled but
> unreachable" made this look like something to accept with a reason. It was
> something to delete: `default-features = false` on both crates takes
> `cargo audit` from four vulnerabilities to none and the lockfile from 957
> crates to 948, with every test still passing (`XONHO-0017` task 1.2).
>
> The habit worth keeping: **a dependency trace is read off the crate's
> `[features]` table and `cargo tree`, never reconstructed from what the
> feature names sound like.** `__rustls` sounds like the thing that pulls
> rustls, and it does — just not that rustls.

That is a reason to rank it, **not** a reason to dismiss it. "Compiled but
unreachable" is a claim about today's call graph, and it is exactly the claim
that stops being true quietly — which is why `XONHO-0017` removed the crates
rather than recording them as accepted.

The policy work remained, for the seven warnings that **cannot** be removed:
six live in the UI stack `ADR-0001` freezes. A `deny.toml` full of blanket
ignores would satisfy the requirement's letter and none of its purpose, so each
is named with a reason and a date. Two things that only appeared while building
it, both worth carrying:

- **`cargo-deny` has no `expires` key.** It takes `id` and `reason` and rejects
  anything else, so a dated acceptance needs enforcing outside the tool —
  `scripts/check-advisory-expiry.sh`.
- **`cargo-deny`'s default does not report a transitive `unsound` advisory.**
  Without `unsound = "all"`, `cargo deny check advisories` passed clean while
  `cargo audit` reported a use-after-free against `lru`. The two tools disagree
  by default, and the disagreement is silent in the direction that matters.

## A test can go on passing after it stops testing anything (found 2026-08-21)

`XONHO-0018` gave a wrong-region redirect its own cause, `BucketElsewhere`.
Nothing broke. Every test stayed green, including one in `capability.rs` whose
case was named "a wrong-region redirect" — and which built its fixture by hand:

```rust
Error::Unexpected { detail: "the service reported `PermanentRedirect` (HTTP 301)" }
```

That is the shape the classifier used to produce and no longer does. The test
asserts that such a failure is no evidence about permission, and `Unexpected`
still satisfies that, so it passed — while no longer exercising the case its own
name claims. It would have passed for ever.

The mechanism is worth naming because it is not specific to this file. **A
hand-built fixture is a copy of what the code produced on the day it was
written.** When the production shape moves, the copy stays behind, and a test
whose assertion is broad enough — here, "anything that is not `AccessDenied`
means unknown" — keeps agreeing with the copy. Nothing fails. The coverage is
gone and the green tick is unchanged.

Two things make it findable rather than a matter of luck:

- **A fixture that names a scenario should be built by the thing that produces
  that scenario**, or the change that moves the shape has to visit it. The four
  new tests in `classify.rs` go through `from_sdk` with a real
  `SdkError::ServiceError` for exactly this reason — a misspelled header name
  or a dropped status check fails them, where a hand-built `SdkFailure` would
  have accepted either.
- **When a change adds a variant, grep the test tree for the old shape**, not
  just for compile errors. The exhaustive matches in `error.rs` consumers
  caught every *display* site automatically, which is the design working; they
  cannot catch a fixture that constructs a still-valid variant for a case that
  has moved to a new one.

Cheap, and worth doing at every close-out that adds a cause.

