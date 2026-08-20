## Context

`ObjectStore` has two operations — `list_buckets` and `probe_list` — and the
window has one screen. `XONHO-0009` built the shell around that: a sidebar
holding connections, a main panel holding the bucket table. `XONHO-0005` built
the capability model around scopes that already carry an optional prefix,
though nothing has ever supplied one.

So the pieces this change needs mostly exist and have never been connected: a
port to extend, a capability model already prefix-shaped, and a shell built for
one screen that now needs two.

The constraint that shapes everything below is repo invariant 2 — nothing
network-shaped runs on the render thread — and it is why paging is a visible
part of the design rather than an implementation detail.

## Goals / Non-Goals

**Goals.** Open a bucket. Navigate prefixes as folders, lazily and page by
page. Know and state where you are. Reach a location directly by name,
including in an account whose buckets cannot be listed. Never confuse an empty
location with a refused one.

**Non-Goals.** Sorting, resizing or persisting columns. Client-side filtering
or server-side prefix search. Sort honesty. Downloading, uploading or deleting
anything. Previewing an object. Caching a listing between visits. A tree in the
sidebar. Back-and-forward history. Grouping or filtering the bucket list by
access.

Each is deliberately named because each is a plausible thing to drift into
while building this, and several are `[M]` requirements that this change is on
record as stepping over.

## Decisions

### Navigation lives in the main panel; the sidebar stays flat

The sidebar lists connections and, beneath the chosen one, its buckets. It does
not expand into prefixes. All movement through a bucket happens in the main
panel, with a trail above it.

*Alternative considered: a lazy tree in the sidebar, as S3 Browser and
Cyberduck offer.* Rejected for this change because it creates **two navigation
surfaces that must agree**: entering a folder in the panel has to open the
tree, selecting in the tree has to move the panel, and both paginate lazily
over the same data. That is a durable source of defects bought for a benefit
the trail already provides — knowing where you are and jumping back.

It is rejected, not foreclosed. Because one location is the single source of
truth, a tree added later is another way to set it rather than a second thing
to keep in step.

### The port gains one operation, returning one page

`ObjectStore` gains a single operation that takes a location and an optional
cursor and returns a page: the prefixes beneath, the objects within, and
whether more remains.

*Alternative considered: returning a stream, and letting the adapter paginate
internally.* Rejected because the requirement is that the interface can **say**
more is coming, and a stream hides exactly that. Paging is part of what the
user is told, so it belongs in the type the port returns, where the UI can read
it and a hand-written double can produce it.

The object type carries storage class and ETag even though only name, size and
last-modified are rendered. They arrive in the same response at no extra cost,
and admitting them now avoids changing the port when the remaining columns are
built.

### One location, and everything else derived from it

A location — connection, bucket, prefix — is the single answer to where the
user is. The breadcrumb trail is computed from it by splitting the prefix; the
path bar parses text into one and sets it. No second record of position exists.

*Alternative considered: a navigation stack owning position, with the location
derived from its top.* Rejected as premature: history is not in this change,
and when it arrives a stack of locations is the obvious shape — which the
single-source-of-truth arrangement permits and a second record would fight.

Parsing and rendering a location as text are pure functions, tested without a
network.

### Reaching a bucket by name is the path bar, not a feature of its own

An account whose credentials may work inside a bucket but may not enumerate the
account is ordinary, and today it is a dead end because listing is the only
door. The editable path bar is already required by §4.2. Once it exists, typing
a bucket name is entering a location like any other, and the dead end closes
without a mechanism of its own.

### The folder/file rules live in core, as pure functions

Three rules decide what a listing shows, and all three are decidable from the
service's own answer without any UI involved:

- an entry whose key equals the current prefix is that folder, not an entry
  within it;
- a prefix with no object behind it is still a folder, with empty object
  columns rather than substituted ones;
- an object and a prefix may share a name, and both are shown.

They live beside the adapter as pure functions because they are the difference
between a correct listing and a nameless zero-byte row inside every folder any
console ever created — and because that is a defect a unit test can hold, while
a UI cannot.

### Capability needs no new contract

`Scope` has carried `bucket + Option<prefix>` since `XONHO-0005`, and
`capability-awareness` is written in terms of scopes rather than buckets. A
prefix is therefore probed, cached and rendered by the machinery that already
exists. This change supplies prefixes; it does not extend the model.

### `app.rs` is split first, as a separate commit

It is 1113 lines and browsing lands in the middle of it. The split precedes the
feature and is a **pure move** — a diff a reviewer can confirm by reading
nothing but the line moves, as `XONHO-0009` did with `main.rs`.

## Risks / Trade-offs

- **Paging can hide truncation.** A listing that stops early and says nothing
  reads exactly like a small folder → the page type carries whether more
  remains, and the interface states it; a page that ends is distinguishable
  from a page that is merely the last one fetched.
- **A refused prefix rendering as empty.** The single most likely way to break
  the project's headline claim, one level down from where it is already
  proven → refusal is a distinct outcome in core, not an empty result, and a
  scenario in the spec covers it.
- **Probing prefixes multiplies requests.** Every folder entered is a new scope
  to observe → the existing budget applies unchanged: viewport-only, debounced,
  bounded, never automatic on write.
- **The layout change touches a shell just built.** `XONHO-0009` is closing
  with two tasks open → the sidebar gains a level and the main panel gains a
  branch; nothing 0009 established is removed.
- **The virtualized table has never rendered a real listing.** It was measured
  on a synthetic feed in M0 → this change gives it real data, and does not
  claim the 100k requirement is met.

## Migration Plan

Not applicable in the deployment sense: there is no stored state to migrate and
nothing published to roll back. The ordering that matters is internal — the
`app.rs` split lands before any browsing code, so that the feature's diff is
about the feature.

## Open Questions

- **What a listing shows while a page is in flight and the folder so far is
  empty.** A spinner over nothing, and a genuinely empty folder, are two states
  a user must be able to tell apart; the shell has a skeleton for exactly this
  and the answer is probably to reuse it, but it wants judging against real
  data rather than deciding here.
- **Whether the path bar accepts a bare `bucket/prefix` as well as the
  service's `s3://` form.** The spec requires the addressing form; accepting
  the shorter one is a convenience with an ambiguity attached, and can be
  decided during implementation without changing what the spec demands.
