## Context

`ListBuckets` does not return S3 Express One Zone directory buckets. They
answer to `ListDirectoryBuckets`, against the regional control plane
(`s3express-control.<region>.amazonaws.com`), and their objects are reached
through zonal endpoints authorised by a short-lived session from
`CreateSession` rather than by the caller's own credentials.

The current listing path is `S3ObjectStore::list_buckets`, one call on the
connection's own client, whose result is either a list or an `Error`. A
connection permitted only to list directory buckets therefore produces an
access-denied panel and nothing else. That case is not hypothetical: it is the
account this change is verified against, and its shape — eight directory
buckets in one zone, `ListBuckets` refused — is what the design has to serve.

What the SDK already provides was read out of `aws-sdk-s3-1.142.0` rather than
recalled, and it is most of the work:

| Part | Where it is |
|---|---|
| `ListDirectoryBuckets` + paginator | `src/operation/list_directory_buckets/` |
| `CreateSession` | `src/operation/create_session/` |
| The `sigv4-s3express` auth scheme, and a session cache that refreshes before expiry | `src/s3_express.rs` — `ExpiringCache`, 10s buffer, installed by default via `S3ExpressRuntimePlugin` in `Client::from_conf` |
| Control-plane and zonal endpoint resolution | `src/config/endpoint.rs` |

So this change writes a listing call, a bucket kind, and a partial-denial
outcome. It does not write session handling, and should not.

## Goals / Non-Goals

**Goals:**

- One list, both kinds of bucket, from one act of listing.
- A denial of one listing never discards the other's result.
- A directory bucket is identifiable as one, and its zone is not lost.
- A refusal names the action it actually required.

**Non-Goals:**

- Implementing `CreateSession`, its caching or its refresh. The SDK does this;
  duplicating it would mean maintaining a second answer to the same problem.
- Creating, deleting or configuring directory buckets.
- Any zone-aware behaviour beyond presenting the zone: no zone filter, no
  placement decisions.
- Directory-bucket support against S3-compatible endpoints. The construct is
  AWS's; a custom endpoint means the call is not made at all.

## Decisions

**Both listings are issued together, not one after the other.** They are
independent requests to different endpoints, and running them concurrently
makes the pair cost what the slower one costs. Sequential would double the
latency of the most common screen in the application for no benefit.

*Alternative considered:* fetch directory buckets lazily, only after
`ListBuckets` is denied. Rejected — it makes the good case (an account with
both kinds) permanently incomplete, and defines the feature in terms of a
failure.

**Choosing a kind, and remembering a refusal, are deferred — deliberately, and
with the reasoning kept.** Two mechanisms were designed and then taken back out
of this change so it can land the capability first and be refined against a
working screen rather than against an argument:

- *Remembering a refusal against the credentials that earned it*, so a listing
  observed to be refused is not issued again until those credentials change.
  `Scope` is `{bucket, prefix}` today and would grow an account level; the
  credential-keyed retention and its invalidation already exist.
- *Narrowing the list by kind* — all / ordinary / directory — applied to the
  buckets already retrieved and issuing no request, the same shape as the
  region narrowing the `bucket-listing` spec already defines.

Neither is in this change's spec delta or its tasks, because a spec that claims
behavior nobody built is the failure `AGENTS.md` keeps a status file to
prevent. They are carried into `docs/planned-changes.md` when this change
lands, with what is written here.

**What deferring them costs, stated so it is recognised and not re-diagnosed:**
an account that will never hold directory buckets pays one wasted request and
shows one refusal **on every connect**. A refusal shown every time is how a
user learns to stop reading refusals. Task 4.3 is where that becomes visible on
real accounts; if it grates, the first mechanism above is the answer and it is
already designed.

**A connection-level switch is not the answer, and that decision stands.** It
was proposed: choose when connecting between ordinary buckets, directory
buckets, or all. Rejected for a specific failure rather than a general
principle. A connection set to "ordinary" on an account that holds only
directory buckets renders *"this account has no buckets"* — precisely the
sentence `bucket-listing` forbids for an account that in fact holds buckets,
with no signal that a setting is what emptied the screen. It also asks the
account holder to declare what one request can observe, the inversion
`ADR-0002` exists to prevent, and it changes the on-disk shape of
`connections.toml` to store the answer.

The instinct behind it was right about two real costs — the wasted request and
the repeated refusal. Those are answered by remembering the refusal, not by
asking the user.

## Risks / Trade-offs

- **No local rig.** LocalStack does not implement directory buckets, so nothing
  here can be exercised offline end to end. → Ports and doubles cover the
  logic; the real path is proven by a live run, and by an `#[ignore]`d test in
  the manner of `profiles.rs::this_machine` that can be pointed at whatever
  account the runner has.
- **The only account available belongs to the owner's employer.** → No bucket
  name, account id or zone id from it enters this repository. Fixtures use the
  same *shape* (`example--usw2-az1--x-s3`).
- **Session handling is the SDK's, so a stack bump can change it silently.** →
  The live test is what would catch it; note in the code that
  `disable_s3_express_session_auth` is deliberately left alone.
- **Zone ids are not all the same shape.** A directory bucket in a local zone
  carries a three-segment az-id — `usw2-lax1-az1` is the public shape — where
  one in a plain availability zone carries two, as in `usw2-az1`. The account
  this change is verified against holds local-zone buckets, so this is the
  case being exercised rather than a hypothetical. → Nothing may parse an
  az-id by segment count; treat it as an opaque string between the
  delimiters.
- **One more request per connection.** → Concurrent, and skipped entirely for
  custom endpoints.

## Migration Plan

Not applicable. Nothing stored changes shape, and an account with no directory
buckets sees exactly what it sees today: one extra request that returns an
empty list, or none at all if the connection has a custom endpoint.

Rollback is removing the second call.

## Open Questions

- **Does capability probing behave the same on a directory bucket?** A probe
  reads a bucket to observe whether it may be listed; on a directory bucket
  that read is authorised by a session, so a refusal can arrive from
  `CreateSession` rather than from the read. Whether that is one observation or
  two is decided when the probe path is touched, not before.
- **How a long zonal name is rendered** is the GUI task's to answer against
  `docs/design-language.md`, and is deliberately not settled here.
