## Context

See `proposal.md` for motivation. What shapes the approach:

- `caixonho-core::capability` already carries the three-valued observation model
  and states that probing, caching and invalidation land in M1. This change is
  that landing.
- The S3 port is one trait with a test double behind it; `caixonho-gui` may not
  reach for `aws-sdk-s3` types. Anything the UI needs about a bucket must be a
  domain type.
- One connection today means one region. Buckets are not region-scoped
  resources but object operations are: a request for a bucket in another region
  is answered with a redirect, not with data.
- Credentials on the development machine come from `credential_process`, which
  costs seconds per resolution. Anything that rebuilds the provider chain
  multiplies that cost.
- `tls.rs` owns one HTTP client shared by the credential loaders and S3, so
  trust material stays consistent.

## Goals / Non-Goals

**Goals:**

- Region data good enough to filter on, from the listing itself.
- A probe path that is cheap, bounded, and honest about what it did not learn.
- The capability model usable by the future CLI, not just this screen.

**Non-Goals:**

- Read, write and delete capability. This change probes list only.
- Prefix-level scopes. The cache is keyed for them, but nothing below a bucket
  is probed until `XONHO-0006` has prefixes to probe.
- Persisting observations across app restarts. In-memory per session.

## Decisions

### Regions come from the listing request, not from per-bucket calls

`BucketRegion` is documented as returned "if the request contains at least one
valid parameter" — and the account this project develops against returns no
region for a parameterless request, which is what makes every row read
`unknown` today. Sending an explicit page size on the listing therefore buys the
whole account's regions in the calls we already make.

Alternative: `GetBucketLocation` (or `HeadBucket`) per bucket. It is the
documented way to ask about one bucket, but it costs a request per bucket at
open time — the exact startup cost the brief's probe budget forbids — to learn
something the listing will hand over for free. It stays as the fallback if the
listing turns out not to carry regions for some account shape.

### Filtering is client-side

The service can filter a listing by region, but only when the request goes to an
endpoint in that same region. Honouring the selector server-side would mean a
client per region and a round trip per selection, to narrow a list already held
in memory. The selector filters retrieved rows; the offered regions are the
distinct regions among them, so the control cannot offer an empty result.

### Probing uses a client for the bucket's own region

A list probe is `ListObjectsV2` with a maximum of one key: it creates nothing,
returns almost nothing, and is direct evidence for the capability being claimed.
Sent to the wrong region it produces a redirect, which is not evidence about
permission — so probes go through a client for that bucket's region, built on
demand and kept for the session.

Those per-region clients share the credentials provider and the HTTP client
from `tls.rs`. Building a fresh provider chain per region would re-run
`credential_process` for each one, turning a probe budget into a login storm.

Where a bucket's region is unknown, the probe goes through the connection's own
client, and a wrong-region answer is recorded as no evidence rather than as a
denial.

### Probing is scheduled in core, driven by the viewport from the UI

The UI reports which buckets are visible, debounced; core decides what to probe.
It skips scopes already observed or already in flight, and holds a small fixed
number of probes in flight. Putting the scheduler in core keeps the budget with
the model it protects, and keeps it available to the CLI.

### The probing state stays out of the observation model

`Observation` remains `Unknown | Allowed | Denied` — the three states that are
claims about the world. "Being probed" is a fact about our own activity, so the
in-flight set lives beside the model and the view combines them. This keeps the
invariant that only evidence moves a capability, and keeps a transient UI state
out of a type the CLI will also consume.

### Failures that are not denials leave the model untouched

The classifier from `XONHO-0003` already separates denial, rejected session,
expired session, wrong region, network and missing bucket. A probe that fails
with anything but a denial records nothing, so the scope stays unknown and can
be probed again. Throttling is treated the same way, so a slow account does not
turn into a wall of locks.

## Risks / Trade-offs

- **The region assumption is documented but not yet confirmed live on this
  account** (the credential store locked mid-verification) → the first task is
  the confirmation, with `GetBucketLocation` per bucket as the fallback. Both
  paths satisfy the same spec, so only the task list changes.
- **Probes cost requests, and an account can be large** → viewport-only,
  debounced, deduplicated, and capped in flight. The list renders before any
  probe returns.
- **Per-region clients accumulate** → one per region actually seen, created
  lazily, sharing credentials and HTTP client; an account spanning many regions
  costs a handful of small objects, not a provider chain each.
- **A dimmed bucket is a strong claim to make from one probe** → only an
  authorization denial may produce it, it is reversible on the next observation,
  and the row stays visible with its cause and required IAM action.
- **Observations can go stale** while the app is open if a policy changes → this
  change does not add a TTL; observations are discarded on profile switch and
  re-authentication, and a real operation always overrides what was observed.
