# AGENTS.md — caixonho

Source of truth for AI agents working in this repository. Read before changing
anything. The product requirements live in `docs/PROJECT_BRIEF.md`; irreversible
decisions live in `docs/adr/`.

## What this is

A fast, native, cross-platform S3 client in Rust + GPUI. Public open source,
dual-licensed MIT OR Apache-2.0. Windows 11 and macOS are both first-class v1
targets — Windows is the primary daily driver and must never be the lagging port.

## Invariants (do not break)

1. **Crate boundaries.** `caixonho-core` owns all product logic and never
   depends on UI. `caixonho-gui` never depends on `aws-sdk-s3` directly — if it
   needs an AWS-shaped type, core re-exports a domain type. This keeps the UI
   swappable and the core reusable by the future CLI.
2. **Nothing that touches the network runs on the render thread.** AWS calls
   live on a tokio runtime on background threads; results cross to the UI over
   channels; UI state updates happen on GPUI's executor. The bridge pattern is
   demonstrated in `caixonho-gui/src/main.rs` (M0 spike).
3. **Dependency pins are deliberate.** The UI stack tracks git (ADR-0001):
   `gpui-component` is pinned by `rev`, the zed commit is frozen by
   `Cargo.lock`, and `gpui`/`gpui_platform` specs must stay byte-identical to
   gpui-component's own declarations or Cargo will duplicate gpui and nothing
   will type-check. Bumping the stack is its own PR, green on both OS targets.
4. **No telemetry, no phone-home, ever.** Also: never add a "disable TLS
   verification" option — trust material is configurable, verification is not.
5. **Secrets never touch the config file or the logs.** OS keychain only;
   log redaction is covered by tests once logging lands.
6. **Errors stay structured** (`thiserror` in core). The permission-awareness
   feature depends on never stringifying AWS errors early, and on never mapping
   expired-token / wrong-region / network / missing-bucket errors to "denied".
7. **TDD for `caixonho-core`.** The GUI may be exploratory; the core may not.

## Conventions

- Everything in the repo is **English**: code, comments, docs, commits, issues.
- **This repository is public.** Its history is part of what it offers: commits
  are self-contained and reviewable, each message says what changed and why it
  changed rather than restating the diff, and history stays linear — no
  force-pushes to `main`, no rewriting published commits to tidy them. Prefer
  several focused commits over one that mixes concerns.
- Conventional Commits with a task ID in the scope: `feat(XONHO-0003): ...`.
  Small housekeeping may use the owner's global `OPS-NNNN` sequence — prefer it
  for anything that is not project work, so the project's own sequence stays
  meaningful. The launch-name slug `THUNG` is retired: `THUNG-0001` and
  `THUNG-0002` remain in commit history and are never reused, but no work
  continues under them — the in-flight change was renumbered to `XONHO-0003`
  rather than closing under its old ID. Every task uses `XONHO-NNNN`.
- No AI co-author trailers on commits (`Co-Authored-By: Claude ...`,
  `Generated with ...` are not used in this repo). Optional light provenance
  trailers `AI-Assisted-By:` / `AI-Reviewed-By:` are allowed when material.
- Branches: `<type>/xonho-NNNN-short-description`; merge to `main`.
- ADR for every irreversible choice, in `docs/adr/NNNN-title.md`.
- Change management from M1 onward: **OpenSpec** (`openspec/`, `/opsx:*`
  commands, spec-driven schema) — explore → propose → specify → apply → archive.
  Change directory names are lowercase and carry the task ID
  (`xonho-0003-...`). The CLI is installed automatically in web sessions by
  `.claude/hooks/session-start.sh`.
- **The reader-facing docs are part of the change, not paperwork after it.** A
  change that alters what the app does for a user updates `README.md` (the
  status paragraph and what works today) and, where the shape of the system
  moved, `docs/architecture.md` and `docs/roadmap.md` — in the same change, with
  a task for it in `tasks.md`. This rule exists because it was broken: the repo
  moved to M1 and the README went on describing the retired M0 spike, so the
  first thing a visitor read was false and the workflow had nothing that would
  ever have caught it.
- Verify external facts (crate versions, AWS behavior) against current sources
  before shipping an identifier — the UI stack moves fast; do not trust this
  file or the brief over `cargo` and upstream docs, and say so when they drift.

## Current state

- **Milestone: M1** — first real capability. `caixonho-gui` is the app now, not
  a spike: it resolves an AWS profile, lists that account's buckets, and reports
  each failure cause with the action that fixes it. The M0 spike (synthetic
  100k-row feed, FPS overlay) is retired; what survived it is the virtualized
  table, the tokio→channel→GPUI bridge and the scroll accelerator.
- M0's verdict and measurements stay in `docs/adr/0001-ui-framework.md`. The
  ADR is still `Proposed`: its last open cell is the Windows machine without a
  working Vulkan driver, where the required outcome is a graceful failure.
- CI builds Windows + macOS on every push/PR and uploads the app binaries as
  artifacts (`caixonho-app-*`) for on-device checks.
