# caixonho

> From Vietnamese **"cái xô nhỏ"** — *the little pail* (that hauls big buckets).

A fast, native, cross-platform S3 client. GPU-rendered with [GPUI](https://gpui.rs)
(the framework behind Zed), written in Rust, built to feel instant: cold start under
half a second, buttery scrolling through prefixes with 100k+ objects, keyboard-first.

**Status: pre-alpha, milestone M1.** The app connects and reads; it does not yet
open anything. Watch the repo if you want to follow along.

Working today:

- Reads the profiles in `~/.aws` and connects with the credentials they name —
  an SSO profile with a valid cached token, `credential_process`, or static keys.
- Takes a credential you type in, keeps the secret in the system keychain, and
  offers it again next time. Nothing goes into a config file, a log, or `~/.aws`.
- Contacts nothing until you choose a connection, and says so when one cannot
  sign in rather than offering it as though it worked.
- Lists that account's buckets with name, creation date and region, and narrows
  the list to one region.
- Says which buckets your credentials can actually open, and names the IAM action
  a refused one would need. A bucket nobody has asked about yet says so, rather
  than guessing — and one still being checked says that, which is a different
  thing again.
- Tells failure causes apart — an expired session, rejected credentials, a
  network failure and a trust failure are each reported as themselves, with the
  action that fixes them, and none of them is ever reported as "access denied".
- Writes down what it decided and why, in your platform's own log location, and
  shows you that location in the status bar so a report can carry evidence
  instead of a description. The file is bounded and rolls daily; `CAIXONHO_LOG`
  turns the detail up for an investigation. No secret is ever written to it.

Not there yet: opening a bucket and browsing its objects, transfers, and signing
in to IAM Identity Center from the app rather than through the AWS CLI. Those
are the next changes — see [`docs/planned-changes.md`](docs/planned-changes.md),
and [`docs/requirements-status.md`](docs/requirements-status.md) for every
requirement and whether it is actually built.

## Why another S3 client

The existing GUI clients are some combination of feature-poor, slow, ugly,
Windows-only, or paid. caixonho aims to be *a file-explorer-grade S3 client that is
honest about permissions and fast on huge buckets*:

- **Honest about permissions.** Buckets and prefixes your credentials cannot touch
  are dimmed with the reason attached — and an expired token, a wrong region or a
  network failure are never misreported as "access denied".
- **Honest about data.** When a filter or a sort only covers what has been loaded
  so far, the UI says so instead of quietly lying — most clients get this wrong.
- **Fast on purpose.** Virtualized everything; nothing network-shaped ever runs on
  the render thread.
- **No telemetry. Ever.** No analytics, no phone-home, no exceptions. Crash reports
  are written to a local file that *you* choose to attach to an issue.

## Platforms

| Phase | Target |
|---|---|
| v1 | Windows 11 + macOS (Apple Silicon & Intel), first-class together |
| v2 | Linux |
| v3 | CLI sharing the same core crate |

## Building

Requires the Rust toolchain pinned in `rust-toolchain.toml`, which rustup installs
on its own the first time you build.

```sh
cargo run -p caixonho-gui
```

It opens on a profile from `~/.aws` and lists its buckets. With no profiles
configured it says so and does nothing else — it never invents credentials.

## Documentation

[`docs/`](docs/README.md) maps the rest: what the product is meant to be, what it
is contracted to do today, the decisions that are hard to reverse, and what each
task number in the commit log refers to.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
