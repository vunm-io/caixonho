## Why

An account whose buckets are all S3 Express One Zone directory buckets looks,
in this application today, like an account that denied you. `ListBuckets` does
not return directory buckets at all — they answer to `ListDirectoryBuckets`
against their own endpoint — so the window shows a red access-denied panel and
nothing else. The owner's work account is exactly that account: it holds eight
directory buckets in a single local zone, and no permission to list ordinary
ones.

Two things make this the right moment rather than a later one.

The first is that it is the only way that account becomes usable at all. This
is not polish on a working screen; it is the difference between a connection
that shows eight buckets and a connection that shows an error.

The second is that `XONHO-0011` needs it. That change's acceptance — a listing
served by a session this application obtained for itself — cannot be
demonstrated on this machine, because the one account that can sign in through
the app is the one that cannot list buckets. This change is what makes that
proof possible.

## What Changes

- The bucket listing covers directory buckets as well as ordinary ones, so a
  connection presents everything it can actually see.
- A denied `ListBuckets` no longer empties the screen when directory buckets
  are visible: the account lists what it can, and says what it could not.
- Directory buckets are identifiable as such in the list, and their
  `<name>--<az-id>--x-s3` names are presented so the name is readable without
  hiding the zone that distinguishes them.
- Opening a directory bucket and browsing it works through the paths
  `XONHO-0006` already built.
- A denial of `s3express:ListAllMyDirectoryBuckets` or `s3express:CreateSession`
  is reported as its own cause, naming the action that was refused, rather than
  as an S3 denial or an unrecognised failure.

No breaking change: an account with no directory buckets behaves exactly as it
does today, and the extra call is not made where it cannot apply.

## Capabilities

### New Capabilities

None. What changes is what "the buckets a connection can see" means, which is
an existing capability's business.

### Modified Capabilities

- `bucket-listing`: the listing SHALL cover directory buckets, a partial
  denial SHALL be reported as partial rather than as an empty account or a
  total failure, and a directory bucket SHALL be distinguishable from an
  ordinary one in what is presented.

`capability-awareness` is untouched. Two mechanisms that would have reached it
— remembering a refusal against the credentials that earned it, and narrowing
the list by kind — were designed and then deferred so this change lands the
capability first. `design.md` keeps the reasoning and names what deferring
them costs.

## Impact

**Requirements delivered.** `PROJECT_BRIEF.md` §4.1 `[S]` — S3 Express One Zone
/ directory buckets. This delivers no `[M]` requirement, and the planning gate
in `AGENTS.md` exists to make that admission explicit rather than accidental.

**`[M]` requirements still unbuilt ahead of it**, from
`docs/requirements-status.md`:

| Unbuilt `[M]` | Why this change goes first anyway |
|---|---|
| In-app OIDC device-flow login (§4.1) | `XONHO-0011`, in flight and 12/19 — this change does not jump it, it *unblocks its acceptance*. 0011 closes first |
| Region handling that follows `x-amz-bucket-region` (§4.1) | Untouched by this change and unaffected by it. Directory buckets are single-region by construction |
| Sort honesty (§4.2) | Nothing sorts yet; this adds no sorting and so tells no new lie |
| KMS denial distinguished from an S3 denial (§4.3) | Needs object reads, which this change does not add |
| Dependencies audited in CI (§4.3) | Unrelated; this change adds no dependency — the SDK already carries the operations |

The honest summary: one `[S]` is being taken ahead of four `[M]`s. The reason
is that the `[M]`s are either in flight, blocked on work this change does not
touch, or — in the case of `XONHO-0011` — waiting on precisely what this
change provides.

**Code.** `crates/caixonho-core/`: `adapter.rs` (the control-plane call),
`types.rs` (what a bucket is), `listing.rs`, `classify.rs` (the `s3express`
causes), `error.rs`. `crates/caixonho-gui/`: the bucket list's presentation of
a long zonal name.

**Dependencies.** None added. `aws-sdk-s3 1.142.0` already carries
`list_directory_buckets`, `create_session`, the `sigv4-s3express` auth scheme
with its own expiring identity cache, and the endpoint rules for both the
regional control plane and the zonal endpoints. Three of the four parts
`PROJECT_BRIEF.md` names are provided by the SDK; only the listing call and the
presentation are ours.

**Testing.** There is no local rig: LocalStack does not implement directory
buckets, as `docs/planned-changes.md` records. Verification is against a real
account, and the one available belongs to the owner's employer — so no bucket
name, account id or zone id from it enters this repository. Fixtures use names
of the same *shape* (`example--usw2-az1--x-s3`).

**Documents.** `docs/planned-changes.md` carries a section saying directory
buckets are absent by design and scheduled at M5; when this lands, that section
is replaced by the fact that they are present. `docs/roadmap.md` and
`docs/requirements-status.md` move the `[S]` row to done.
