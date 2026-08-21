## Why

`PROJECT_BRIEF.md` §5 asks for `cargo-deny` + `cargo-audit` in CI and §8 states
the guarantee they exist to keep: *"Dependencies audited in CI."*
`docs/requirements-status.md` has carried that row as **none** since the file
was written, and CI runs fmt, clippy and tests only.

It was assumed to be a two-line CI change. Measuring it on 2026-08-21 showed
otherwise: `cargo audit` reports **4 vulnerabilities and 7 warnings** against
the lockfile, so adding the step without deciding what to do about them turns
the pipeline red on its first run.

**A second measurement, 2026-08-21, changed what this change has to be.** The
first one recorded the wrong cause, and correcting it turns most of the work
from paperwork into a deletion.

## What the second measurement found

The note in `docs/planned-changes.md` traced the vulnerable crates to
`aws-smithy-http-client`'s `__rustls` feature pulling `hyper-rustls 0.24.2`
*with its `acceptor` feature* — "the server-side TLS path". Read off that
crate's own `[features]` table, `__rustls` does no such thing: it pulls the
current `hyper-rustls`. The legacy stack arrives by a different road:

```
aws-sdk-s3 / aws-sdk-ssooidc  (default features)
  └── feature "rustls"
      └── aws-smithy-runtime/tls-rustls
          ├── aws-smithy-http-client/hyper-014          → hyper 0.14, h2 0.3.27
          └── aws-smithy-http-client/legacy-rustls-ring → rustls 0.21.12
                                                          → rustls-webpki 0.101.7
```

It is the **legacy client** stack, not a server path — and it is enabled by the
default features of two crates this workspace names directly.

**Those features supply an HTTP client this application already replaces.**
Every AWS client here is built on the one stack in `tls.rs` and handed in
explicitly — `connection.rs:197`, `connection.rs:280`, `sso_adapter.rs:71`,
and a test already asserts the sign-in client carries it. Nothing reads the
SDK's default HTTPS client, so nothing loses anything when it stops being
compiled.

Measured rather than argued. With `default-features = false` on both crates,
keeping only the features actually used:

| | Before | After |
|---|---|---|
| `cargo audit` vulnerabilities | **4** | **0** |
| `cargo audit` warnings | 7 | 7 |
| Lockfile crates | 957 | 948 |
| `rustls` / `rustls-webpki` in build | 0.21.12 / 0.101.7 | gone |
| `h2` / `hyper` in build | 0.3.27 / 0.14.32 | gone |
| `cargo test --workspace` | 263 + 36 | 263 + 36 |
| `cargo build --release -p caixonho-gui` | ok | ok, 20.5s |

The four advisories are **removable**, not merely acceptable. They are compiled
into the shipped binary today; "not currently reachable" is a claim about
today's call graph and is exactly the claim that stops being true quietly.

## What Changes

- **Stop compiling the legacy TLS stack.** `default-features = false` on
  `aws-sdk-s3` and `aws-sdk-ssooidc`, with the features this workspace actually
  uses named explicitly.
- **Add the audit to CI**, as its own job, so a dependency problem reads as a
  dependency problem rather than a failed build.
- **Write the policy down in `deny.toml`.** A vulnerability fails the build. The
  seven warnings are each named individually, with the reason it is accepted and
  the date the acceptance expires — never a blanket ignore, which satisfies the
  requirement's letter and none of its purpose.
- `docs/requirements-status.md` §7–8 *Dependencies audited in CI* moves from
  **none**.

No behaviour changes for a user. No breaking change.

## Capabilities

### New Capabilities

- `supply-chain`: what this project guarantees about the code it ships that it
  did not write — that the dependency set is checked on every change, that a
  known vulnerability stops the change, and that an exception is a dated,
  reasoned, individual decision rather than a silence.

### Modified Capabilities

None. Nothing about what the application does for a user changes.

## Impact

**Requirements delivered.** `PROJECT_BRIEF.md` §8 `[M]` — *Dependencies audited
in CI*, and §5's `cargo-deny` + `cargo-audit`.

**`[M]` requirements still unbuilt ahead of it**, and why this one goes first:

| Unbuilt `[M]` | Why this change goes first anyway |
|---|---|
| In-app OIDC device-flow login (§4.1) | `XONHO-0011`, 12/19 and **blocked**: every remaining task is live verification only the owner can run |
| Region handling follows `x-amz-bucket-region` (§4.1) | `XONHO-0018`, 11/12 and **blocked** on the same wall — its live acceptance is the owner's |
| Sort honesty (§4.2) | Nothing sorts yet, so nothing lies yet |
| KMS denial distinguished from an S3 denial (§4.3) | Needs object reads, which do not exist |

The honest summary: the two changes ahead of this one are finished except for
checks only the owner can run, and of what is left this is the only row that is
both actionable and mandatory. It also stopped being paperwork when the second
measurement found four vulnerabilities that a two-line change removes from the
binary this project ships.

**Code.** `Cargo.toml` (workspace dependency features), `deny.toml` (new),
`.github/workflows/ci.yml` (a job), `docs/requirements-status.md`,
`docs/planned-changes.md` (the corrected trace).

**Dependencies.** Nothing added. Nine crates removed from the lockfile and four
vulnerable ones removed from the build.
