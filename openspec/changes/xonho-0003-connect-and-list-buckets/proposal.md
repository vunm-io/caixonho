> Opened as `THUNG-0002` before the project was renamed from *caithung* to
> *caixonho*, and renumbered to `XONHO-0003` on 2026-08-19. Commits made
> before the renumber carry the `THUNG-0002` scope; everything after carries
> `XONHO-0003`.

## Why

M0 proved the UI stack can render 100k rows at 60fps, but every row it shows is
synthetic. caixonho cannot yet talk to S3 at all. This change opens the
narrowest end-to-end path — real credentials → a real AWS call → a domain type →
the existing table — so the architecture in `docs/PROJECT_BRIEF.md` §6 is proven
by working code rather than by intention, and every later M1 slice (prefix
navigation, capability probing) has a connection to stand on.

Authentication is also the riskiest unknown left in M1: profile resolution,
SSO token reuse and corporate TLS interception all fail in ways that look
identical to the user ("access denied") unless the error model separates them
from the start. Doing that first, with tests, is cheaper than retrofitting it
under a feature.

## What Changes

- **A connection model in `caixonho-core`.** A connection resolves credentials
  for one named profile (or the default chain) and produces a ready S3 client.
  Resolution covers what the AWS SDK already understands: named profiles from
  `~/.aws/config` and `~/.aws/credentials`, static keys, `role_arn` +
  `source_profile` chains, and **SSO tokens already cached by the AWS CLI**
  (`aws sso login` having been run). No credential material is ever logged.
- **A trait-based S3 port.** All AWS access sits behind a trait in core, so
  listing logic is testable with a double and no AWS account. The AWS-backed
  implementation is one adapter behind it.
- **`ListBuckets` exposed as a domain type** — name, creation date, and region
  where known — with no `aws-sdk-s3` type crossing the crate boundary.
- **OS trust store for TLS**, wired through the shared HTTP client so it applies
  to service calls *and* to credential/SSO calls, which use a separate client
  and are the classic place this is forgotten. Corporate TLS-inspecting proxies
  then work by construction. No "disable verification" switch exists.
- **A structured error enum that separates causes users confuse**: no
  credentials found, expired/invalid SSO token, TLS trust failure, network
  failure, access denied, and unexpected service errors — each carrying what the
  UI needs to say something actionable. Ordering matters: TLS-trust failures are
  classified before the generic credentials path, because their messages
  overlap.
- **The spike table becomes the bucket list.** The synthetic feed is deleted;
  the GUI lists buckets for a chosen profile, shows the resolved profile, and
  renders each error kind as its own message with a retry or re-login hint. The
  tokio↔GPUI bridge proven in M0 carries the real calls.
- **Profile picker** limited to what exists in the user's AWS config files: pick
  a profile, switch without restarting.

Not in this change, deliberately, each its own later change: in-app SSO device
flow, static credentials entered in-app and stored in the OS keychain, prefix
navigation, capability probing and dimming, custom endpoints / S3-compatible
services, directory buckets.

## Capabilities

### New Capabilities

- `connections`: resolving a named profile into usable credentials, reporting
  precisely why resolution or a first call failed, and switching profiles
  without restarting.
- `bucket-listing`: listing the buckets a connection can see, with region and
  creation date, and honest reporting when the account has none or the caller
  lacks `s3:ListAllMyBuckets`.

### Modified Capabilities

None — this is the first behavioural change in the project.

## Impact

- **Crates**: `caixonho-core` gains its first real modules (connection,
  credentials, S3 port, errors, bucket listing) and its first async surface;
  `caixonho-gui` loses the synthetic feed and gains a profile picker plus error
  states.
- **Dependencies** (all pre-authorised by the project brief §5.2–5.3):
  `aws-config` with the `sso` feature, `aws-sdk-s3`, `rustls-platform-verifier`,
  a tokio runtime handle owned by the app rather than created ad hoc in the UI,
  and a test double (hand-written or `mockall`) for the S3 port.
- **Testing**: core is TDD — error classification and bucket mapping are unit
  tested against the port double, with no AWS account required. Live
  verification against a real account stays manual for this change; the
  MinIO-in-Docker rig arrives with custom endpoints.
- **Risk**: the AWS SDK's credential provider chain is the one part that cannot
  be fully unit tested; its behaviour is verified by hand against a real profile
  and recorded in the change's validation notes.
