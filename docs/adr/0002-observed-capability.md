# ADR-0002: Capability is observed, never declared

- **Status:** Accepted
- **Date:** 2026-08-19
- **Deciders:** Vu Nguyen

## Context

The brief's headline feature (§4.3) is that buckets and prefixes the current
credentials cannot use are visibly distinct, and that clicking one explains
why. Delivering it honestly runs into a fact about S3: **no API enumerates a
caller's effective permissions.**

What exists instead:

- `iam:SimulatePrincipalPolicy` answers the question directly, but calling it
  is itself a permission most credentials do not have, and it reasons about the
  caller's own policies — not about a bucket policy, an SCP, or an ACL that may
  deny independently.
- Reading policy documents requires `s3:GetBucketPolicy` and `iam:GetPolicy`,
  which are rarer than the operations they would describe, and interpreting the
  result correctly means reimplementing IAM evaluation, including deny
  precedence and condition keys.
- Trying the operation is the only answer that is always available and always
  true, and it costs a request.

The decision is therefore not *whether* to guess, but what the app is allowed
to claim when it has not tried.

## Decision

**Every capability, on every scope, is a record of what has been observed —
never an inference.** The model is three-valued and defaults to not knowing:

```
Unknown | Allowed | Denied
```

- `Unknown` is the default and must render as unknown, never as denied.
- Only a completed operation or a probe moves a capability out of `Unknown`.
- Only an authorization denial produces `Denied`. An expired session, rejected
  credentials, an unreachable network, a wrong region, a missing bucket and a
  throttled request each keep their own cause and record nothing.
- No scope infers from another, and nothing infers from a policy, a role name
  or an account.
- Observations belong to the credentials that produced them and are discarded
  when those change, including re-authenticating the same profile.

Probes are cheap and non-destructive — a listing for at most one key — issued
only for what the user is looking at, under a small concurrency budget, and
never automatically for write, because a write probe creates an object.

"Being probed" is deliberately **not** a fourth variant. The three states are
claims about the world; probing is a fact about our own activity, so the
in-flight set lives beside the model and the presentation layer combines them.

## Consequences

**What this buys.** The failure that sends people to rewrite an IAM policy over
an expired token cannot happen: the mapping from a result to an observation is
an allowlist, so adding an error variant cannot invent a new way to accuse the
user. This is the difference the brief claims over other clients, and it is
structural rather than a matter of care.

**What it costs.** Knowing anything costs a request, so the interface must be
honest about not knowing yet — a row that has not been probed says so. An
account with hundreds of buckets cannot be fully known without hundreds of
requests, which is why probing follows the viewport rather than the listing.

**What it forbids.** No optimistic rendering. A bucket is never shown as
enterable because it usually would be, and never dimmed because a name looks
like a production account.

**What stays open.** `iam:SimulatePrincipalPolicy` remains a legitimate
*accelerator* where it is permitted — it can seed observations without spending
a request per scope — but it can never become the source of truth, because it
cannot see every policy that might deny. Adding it does not amend this ADR; it
would supply evidence into the same model.

## Alternatives considered

**Declare capability from policy documents.** Rejected: requires permissions
rarer than the ones being described, and correctness means reimplementing IAM
evaluation. A subtly wrong answer here is worse than no answer, because it
looks authoritative.

**Two states, treating unknown as denied.** Rejected: it is the exact
mistake the product exists to avoid, and it makes a slow probe
indistinguishable from a refusal.

**Two states, treating unknown as allowed.** Rejected more mildly: it renders
nothing until the user hits the wall, which is what every other client does and
what the brief is a reaction to.

## Where this lives

`caixonho-core::capability` holds the model and the store;
`caixonho-core::probe` decides what to ask about next. The behaviour contract
is `openspec/specs/capability-awareness/spec.md`.
