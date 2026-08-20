## Why

The application lists an account's buckets and then stops. Clicking a bucket
does nothing, because there is nothing behind it: `ObjectStore` has two
operations, and neither of them reads a bucket's contents. Opening objects is
what a person launches an S3 client to do, so every other feature — transfers,
previews, object operations — is queued behind a door that does not open.

It was put behind `XONHO-0004` deliberately, on the argument that a client
nobody can give a credential to has nothing to browse. That argument has been
paid off: credentials can be entered, stored and chosen. What remains is the
dead end itself, and an owner who opened the app against a real account and
found that the only thing it can do is name the rooms.

## What Changes

- **A bucket can be opened, and a prefix navigated into.** `ListObjectsV2` with
  `delimiter=/`, one page at a time, fetched as the list is scrolled rather
  than gathered before anything is drawn.
- **The application knows where it is.** A single location — connection,
  bucket, prefix — from which the breadcrumb trail is derived rather than
  separately maintained, and which an editable path bar can set directly.
- **A bucket can be reached by name without listing the account.** This falls
  out of the path bar rather than being built: a credential scoped to one
  bucket cannot enumerate the account, which is an ordinary way for an
  organisation to hand out access and currently a dead end.
- **The main panel changes what it shows.** With a connection chosen but no
  bucket, it is the bucket table it is today. With a bucket chosen, it is that
  bucket's contents. Buckets move into the sidebar beneath their connection,
  flat — the sidebar does not become a tree.
- **Folders are inferred, and the inference is stated honestly.** S3 has no
  directories: a prefix with nothing behind it, an object that is also a
  folder, and an object sharing a name with a prefix are all ordinary, and each
  is rendered as what it is rather than smoothed over.
- **An empty prefix and a refused prefix never look alike.** The headline
  feature, one level down from where it works today.
- `caixonho-gui/src/app.rs` is split before this lands in it. It is 1113 lines,
  and browsing would land squarely in the middle.

## Requirements this delivers

From `PROJECT_BRIEF.md` §4.2, recorded in `docs/requirements-status.md`:

- **Prefix navigation as folders (`ListObjectsV2`, `delimiter=/`), paginated
  and lazy** — currently *none*. This is the whole of it.
- **Breadcrumbs plus an editable path bar** — currently *none*. The whole of
  it.
- **Columns name, size, last modified, storage class, ETag; sortable,
  resizable, persisted** — currently *none*, and this delivers **part**: the
  columns render. Sorting, resizing and persistence do not, and saying so here
  is the point of saying it at all.
- **Virtualized table, 100k+ rows** — currently *partial*, measured once on a
  synthetic feed. This gives it a real listing to render, though not yet a
  large one.

From §4.3:

- **Dimming at bucket/prefix granularity** — currently *partial*, "buckets
  only; there are no prefixes yet". This makes prefixes exist. The capability
  model needs no change to accommodate them: `Scope` has carried an optional
  prefix since `XONHO-0005`.

## Requirements it steps over, deliberately

Still unbuilt and mandatory, from `docs/requirements-status.md`:

- **In-app SSO sign-in (device flow)** — `XONHO-0011`. The oldest unpaid debt
  in M1: until it lands the AWS CLI is a hard dependency, which the brief says
  it must not be. It is stepped over again here, and this is the second change
  in a row to do so.
- **Region handling that follows `x-amz-bucket-region`** and **MFA prompting**
  remain untouched.
- **Sortable, resizable, persisted columns**, **client-side filter and
  server-side prefix search**, and **sort honesty** are `[M]` in §4.2 and are
  deliberately not here. Each is a property of a listing that already works,
  and none of them means anything while there is no listing at all. Taken
  together they are larger than this change; taken into it they would produce
  something nobody can review.
- **Grouping or filtering the bucket list by access** — asked for on
  2026-08-20 after a real account showed most of its buckets refused. Recorded
  in `docs/planned-changes.md`, and left there: it is a property of the bucket
  list rather than of browsing, and its difficulty is that access is
  discovered asynchronously, which deserves its own thought.

## Capabilities

### New Capabilities

- `object-browsing`: reading what a bucket contains — prefixes as folders,
  objects as their contents, one page at a time; where the application is and
  how that is stated; and reaching a location directly, including a bucket the
  credentials cannot enumerate.

### Modified Capabilities

None. `capability-awareness` already speaks in scopes that may carry a prefix,
and `bucket-listing` keeps every requirement it has — the bucket table moves,
but nothing it must do changes.

## Impact

- `caixonho-core`: one new operation on the `ObjectStore` port and its
  implementation in `adapter`; domain types for a location, a page of results,
  a folder and an object. The rule that an entry whose key equals the current
  prefix is that folder rather than its own child lives here, as a pure
  function, because it is the difference between a correct listing and a
  nameless zero-byte row inside every folder ever made from a console.
- `caixonho-gui`: `app.rs` split first; the sidebar gains buckets beneath the
  connection; the main panel selects its view from the location; new views for
  the object list and the path.
- No new dependencies.
