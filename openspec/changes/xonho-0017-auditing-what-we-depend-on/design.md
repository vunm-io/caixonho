## Context

Everything below was read off the installed tools and the crate graph on
2026-08-21, not recalled. Where a fact contradicts `docs/planned-changes.md`,
the contradiction is stated rather than quietly corrected — that note is what a
reader will find first.

| Question | Answer | How it was established |
|---|---|---|
| What pulls the vulnerable crates | `aws-sdk-s3` and `aws-sdk-ssooidc` **default features** → `rustls` → `aws-smithy-runtime/tls-rustls` → `aws-smithy-http-client`'s `legacy-rustls-ring` + `hyper-014` | `cargo tree --edges features,no-dev`, and the crates' own `[features]` tables |
| Can they be dropped | Yes. All four advisories leave the build; `cargo audit` goes 4 → 0, lockfile 957 → 948 | Applied, built, tested, audited, reverted |
| Does anything need them | No. Every AWS client here is handed the one stack from `tls.rs` | `connection.rs:197`, `:280`, `sso_adapter.rs:71`, and the test at `sso_adapter.rs:289` |
| Does `cargo-deny` support an expiry on an ignore | **No.** 0.20.2 accepts `["id", "reason"]` only | `cargo deny check` rejects `expires` with `error[unexpected-keys]` |
| Is `unmaintained` scope configurable | Yes — `unmaintained = "workspace"` is accepted | Ran it against a probe crate |
| Where the seven warnings come from | Six from the frozen UI stack and its font/SVG subtree, one (`lru`, unsound) from `aws-sdk-s3` | `cargo tree -i` per crate |

**The first measurement's trace was wrong.** `docs/planned-changes.md` records
`__rustls` pulling `hyper-rustls 0.24.2` "with its `acceptor` feature — the
server-side TLS path". `__rustls` pulls the current `hyper-rustls`; the legacy
0.24/0.21 pair arrives through `legacy-rustls-ring`, a different feature,
enabled by two SDK crates' defaults. The conclusion drawn from it — "the work
is deciding policy, not wiring commands" — was right for the wrong reason, and
the right reason makes most of the work a deletion instead.

## Goals / Non-Goals

**Goals:** the dependency set is checked on every change; a known vulnerability
stops the change; an exception is individual, reasoned and dated.

**Non-Goals:**

- Upgrading the UI stack. `ADR-0001` freezes it, and six of the seven warnings
  live there. Bumping it is its own change, green on both targets.
- Release checksumming. §8 names it in the same sentence, and it belongs to
  releasing rather than to auditing.
- Reviewing licences of the whole graph as a gate today. `cargo-deny` can, and
  turning it on without reading the output first is how a pipeline goes red for
  a reason nobody has decided about.

## Decisions

**Remove the vulnerable crates before adding the job that reports them.** The
alternative — add the job, watch it go red, write four ignores — reaches the
same green tick having shipped the same vulnerable code. Removing them is two
lines in `Cargo.toml`, and it takes the crates out of the binary this project
distributes, not merely out of the report.

*Not accepted as unreachable.* The earlier note argued the vulnerable path is
compiled and never called, which is true today and is a statement about today's
call graph. The exception mechanism exists for advisories that cannot be
removed; this one can.

**`cargo-deny` is the gate, and `cargo-audit` is not also run.** A departure
from the brief's §5 wording, stated so it can be overruled rather than
discovered: `cargo deny check advisories` reads the same RUSTSEC database
against the same lockfile, and adds licence, ban and source checks under one
config file. Running both means one advisory fails the build twice, in two
formats, and the second failure teaches nobody anything. `cargo-audit` stays
useful as the thing a person runs locally, and the tasks say so.

**Expiry is ours to enforce, because the tool will not.** Measured above:
`cargo-deny` 0.20.2 rejects an `expires` key. So the expiry lives inside the
`reason` string, in a fixed form, and a small check in the same CI job fails
when a date has passed. It is a few lines, and without it the policy file
becomes what every policy file becomes.

*Alternative considered:* drop the expiry requirement and rely on review.
Rejected — an ignore list is exactly the file nobody re-reads, and this project
has already recorded twice this month what happens to a document that is only
ever appended to.

**The six unmaintained warnings are named individually, not scoped away.**
`unmaintained = "workspace"` would silence all six, because none is a direct
dependency. It is defensible — a project answers for what it chose — but it
converts six decisions into one silence, and the difference matters on the day
one of those crates stops being merely unmaintained. Each gets a line naming
it, why it is accepted, and when the acceptance runs out.

*This is the decision most worth overruling*, and it is cheap to reverse: one
setting against six lines. Recorded here so it is a choice rather than a
default.

## Risks / Trade-offs

**Dropping SDK default features is verified by tests that make no network
call.** Every client is handed an HTTP stack explicitly, and a test asserts it
for the sign-in path — but "no code path relies on the SDK's default client" is
a claim about the call graph, checked by reading it. The live check belongs
with the other live checks the owner runs, and the tasks say so rather than
implying `cargo test` settles it.

**A new CI job costs time on every push.** `cargo deny` fetches the advisory
database; on a cold cache that is tens of seconds. It runs on `ubuntu-latest`
beside `rustfmt` rather than inside the two build matrices, so it costs one job
rather than two.

**An advisory can appear with no change of ours.** That is the point, and it
means `main` can go red on a day nobody pushed. Preferable to finding out from
someone else.
