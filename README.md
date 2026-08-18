# caixonho

> From Vietnamese **"cái xô nhỏ"** — *the little pail* (that hauls big buckets).

A fast, native, cross-platform S3 client. GPU-rendered with [GPUI](https://gpui.rs)
(the framework behind Zed), written in Rust, built to feel instant: cold start under
half a second, buttery scrolling through prefixes with 100k+ objects, keyboard-first.

**Status: pre-alpha.** The project is at milestone M0 — a spike proving the UI stack
on Windows 11 and macOS. Nothing here is usable yet. Watch the repo if you want to
follow along; the full requirements live in [`docs/PROJECT_BRIEF.md`](docs/PROJECT_BRIEF.md).

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

Requires stable Rust (see `rust-toolchain.toml`).

```sh
cargo run -p caixonho-gui   # currently: the M0 spike (100k-row virtualized table)
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
