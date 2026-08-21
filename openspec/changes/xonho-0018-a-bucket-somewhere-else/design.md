## Context

Reading a bucket that lives outside the connection's region is answered with
HTTP 301 and an `x-amz-bucket-region` header naming where it really is. Today
that lands in `classify` with no code it recognises and becomes
`Error::Unexpected { detail: "the service reported PermanentRedirect (HTTP 301)" }`
— the exact shape `capability.rs` already uses as its fixture for "a
wrong-region redirect". The information needed to fix the call is in the
response and is discarded before anyone looks at it.

What the SDK provides was read out of the crates rather than recalled:

| Part | Where it is | Verdict |
|---|---|---|
| The header on a failed response | `SdkError::ServiceError` → `context.raw()` → `HttpResponse::headers()` (`aws-smithy-runtime-api-1.14.0/src/http/response.rs:168`), `Headers::get` (`headers.rs:112`) | Reachable |
| Automatic region-redirect retry | **Absent.** `x-amz-bucket-region` appears in exactly two files of `aws-sdk-s3 1.142.0`, both `HeadBucket` protocol-serde where it is modelled as an output field. No retry machinery references it | We must do it |
| A per-region client | `S3ObjectStore::client_for`, built for probes | Reuse as-is |
| A canned HTTP exchange for tests | `StaticReplayClient`, `aws-smithy-http-client-1.3.0/src/test_util/replay.rs:155`, behind the `test-util` feature | Makes the retry unit-testable |

So this change writes the capture, the single retry, and the correction of what
is displayed. It does not write region resolution, endpoint construction, or
signing.

## Goals / Non-Goals

**Goals:**

- A read of a bucket in another region succeeds, without the user doing
  anything.
- The region shown for that bucket becomes the region that served it.
- A redirect that names nowhere is reported as itself.

**Non-Goals:**

- Changing what a probe does. Probes already route through
  `client_for(bucket's region)`, so once a region is corrected they follow it
  for free; and `capability.rs` deliberately treats a redirect as *no evidence*
  about permission, a semantic this change has no reason to disturb.
- Guessing a region by trying several. It manufactures requests nobody asked
  for and can invent permission failures in regions that were never involved.
- Changing the connection's own region. The connection is the user's choice;
  this corrects a fact about one bucket, not a setting the user made.
- `HeadBucket` as a region oracle. It answers the question authoritatively but
  costs a request per bucket, and the brief asks for redirects to be followed,
  not for the redirect to be pre-empted.

## Decisions

**The redirect's region rides on `SdkFailure`, and the adapter asks before it
classifies.** `SdkFailure` gains `redirect_region: Option<String>`, filled in
`from_sdk`'s `ServiceError` arm when the status is 301. The adapter's error
path asks `failure.redirect_region()` first; `Some` means retry, `None` means
classify as usual.

*Alternative considered:* have `classify` return a distinct `Error::WrongRegion
{ region }` that the adapter catches and consumes. Rejected — it models a
**successful** path as an `Error`, constructed only to be swallowed one line
later, and every future reader has to work out why one variant of the error
enum is not a failure. `SdkFailure` already carries `code` and `status` as
typed fields for the classifier to consult; the region is the same kind of
thing, and putting it there keeps the promise that AWS errors are never
stringified early (AGENTS.md invariant 6).

*Alternative considered:* a smithy interceptor so the SDK retries by itself.
Rejected — it buys automatic coverage of every operation by moving the decision
into machinery this repository does not own and cannot test, which is the
opposite of what `ADR-0002` settled about deciding things by observation in our
own code.

**Followed once, never in a loop.** A service that redirects a request already
addressed to the region it named has contradicted itself, and following again
converts a wrong region into a hang. One retry, then the failure is reported.

**The discovered region is remembered per connection**, in a
`HashMap<String, String>` beside the `HashSet` that already remembers which
buckets are directory buckets. Same lifetime, same lock discipline, same reason:
the operation that answered is what knows, and the knowledge belongs to the
connection that learned it.

**A page says where it was served from.** `Page` gains
`served_from: Option<Region>`, `Some` only when it differs from the region the
call was addressed to. The window already receives `(Location, Result<Page,
Error>)` over a channel (`app.rs:51`, `:410`), so the correction arrives with
the data it corrects and the window updates that bucket's row.

*Alternative considered:* let the store hold the corrected region and have the
window query for it. Rejected — it makes the window ask a question it would have
to know to ask, on every page, forever, to catch a case that almost never
happens. A field that is `None` in the ordinary case costs nothing and cannot
be forgotten.

*Why `Option` and not always-populated:* a value that is always present is a
value every consumer must compare against something to know whether it matters.
`None` states "nothing to correct" once, at the source.

**A corrected region does not re-apply the region narrowing while the user is
inside that bucket.** The bucket list can be filtered by region, and a
correction can move a bucket out of the filter that is currently on. The row is
corrected; the view is not yanked. Pulling the screen out from under someone as
a reward for the application learning something true is a worse experience than
a filter that is briefly out of date, and the next listing settles it.

## Risks / Trade-offs

**The real case cannot be exercised locally.** Neither MinIO nor R2 emits a
region redirect, as `docs/planned-changes.md` already records about local rigs.
`StaticReplayClient` proves the mechanism against a canned exchange — that the
header is read, that the second request carries the named region, that a second
redirect stops — but a canned 301 is a 301 we wrote. Acceptance is a real
account with a bucket outside the connection's region, and it is the owner's to
run. The tasks say so rather than implying the unit tests settle it.

**`test-util` in dev-dependencies.** It enables `test-util` on
`aws-smithy-http-client`. Resolver 2 keeps dev-only features out of the release
build, and this crate is not part of the frozen stack of `ADR-0001`, so nothing
about the UI pins is disturbed. Verified by building both before and after.

**`Page` grows a field.** Three real construction sites, the same honest cost
`BucketKind` paid in `XONHO-0016`: every place that makes one now has to say
what it knows. The alternative — a defaulted field — would let a future
construction site forget silently, which is the failure mode the explicit cost
buys off.

**A remembered region outlives a bucket that moves.** A bucket cannot change
region, so the map cannot go stale in the way a cache does. It dies with the
connection regardless.
