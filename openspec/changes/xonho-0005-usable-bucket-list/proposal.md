## Why

The app lists buckets and then stops. Every bucket looks equally usable, though
some are only visible and not enterable, and every region reads `unknown` — not
because the account is unusual, but because a `ListBuckets` request carrying no
parameters is answered without `BucketRegion`. A list that cannot be narrowed
and cannot tell a usable bucket from an unusable one is a dead end for anyone
holding more than a handful of buckets.

This change makes the bucket list a surface you can act on, and lands the first
working piece of the brief's headline feature: capability is observed, never
declared.

## What Changes

- Bucket regions are populated for real. The listing request carries a page
  size, which is what makes the service return `BucketRegion` at all; the
  `unknown` presentation stays for buckets the service still says nothing
  about, and nothing is ever inferred from the connection's own region.
- A region selector above the list. It offers only the regions actually present
  in the account's buckets, plus "all regions", and filters the rows already
  retrieved rather than re-querying — a server-side region filter would require
  the request to go to an endpoint in that same region, which costs a client per
  region and buys nothing at this size.
- The list distinguishes buckets whose contents can be listed from those that
  can only be seen in the account listing. The distinction is drawn only from
  observed evidence: a bucket is never presented as denied on the strength of a
  guess.
- Capability probing lands: cheap, non-destructive, lazy per viewport, under its
  own concurrency budget, cached per `(profile, bucket)` and invalidated on
  profile switch. A row that has not been probed yet reads as probing, not as
  unknown and not as denied — the flicker the brief calls out.
- Write, delete and read capability stay untouched here. This change probes
  `list` only.

Not in this change: opening a bucket and browsing its objects, and reaching a
bucket by typing its name. Both are `XONHO-0006` — until there is a bucket view
to land in, a name box has nowhere to go.

## Capabilities

### New Capabilities

- `capability-awareness`: what the app knows about what the current credentials
  may do, how it comes to know it, and the states it may present. Covers the
  three-valued observation model, the probe rules (non-destructive, lazy,
  budgeted, never automatic for write), caching and invalidation, and the rule
  that no other failure may be rendered as a denial.

### Modified Capabilities

- `bucket-listing`: region is now reported in the ordinary case rather than
  being routinely unknown; the list can be filtered by region; and buckets are
  presented differently according to observed list capability.

## Impact

- `caixonho-core`: `capability.rs` grows probing, caching and invalidation
  (the module already carries the three-valued model and says this is where it
  lands). `adapter.rs` learns to ask for a page size so regions come back, and
  gains a probe call. The S3 port and its test double grow one method.
- `caixonho-gui`: the bucket table gains a region selector, a capability column
  or badge, and a second group for list-only buckets; rows update as probes
  resolve rather than waiting for them.
- No new dependencies. No change to the connection or credential path.
