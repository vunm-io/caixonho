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
- Conventional Commits with a task ID in the scope: `feat(XONHO-0002): ...`.
  Small housekeeping may use the owner's global `OPS-NNNN` sequence. The
  original slug `THUNG` (from the launch name *caithung*) is frozen: existing
  `THUNG-0001`/`THUNG-0002` identifiers keep their IDs and in-flight work
  closes under them; every new task uses `XONHO-NNNN` (first: `XONHO-0001`,
  the rename itself).
- No AI co-author trailers on commits (`Co-Authored-By: Claude ...`,
  `Generated with ...` are not used in this repo). Optional light provenance
  trailers `AI-Assisted-By:` / `AI-Reviewed-By:` are allowed when material.
- Branches: `<type>/xonho-NNNN-short-description`; merge to `main`.
- ADR for every irreversible choice, in `docs/adr/NNNN-title.md`.
- Change management from M1 onward: **OpenSpec** (`openspec/`, `/opsx:*`
  commands, spec-driven schema) — explore → propose → specify → apply → archive.
  Change directory names are lowercase and carry the task ID (`xonho-0003-...`;
  the pre-rename `thung-0002-...` keeps its frozen name). The CLI is installed
  automatically in web sessions by `.claude/hooks/session-start.sh`.
- Verify external facts (crate versions, AWS behavior) against current sources
  before shipping an identifier — the UI stack moves fast; do not trust this
  file or the brief over `cargo` and upstream docs, and say so when they drift.

## Current state

- **Milestone: M0** — UI-stack spike. `caixonho-gui` is the spike binary:
  100k-row virtualized DataTable fed asynchronously from tokio. Gates and
  measurements to fill in: `docs/adr/0001-ui-framework.md`.
- CI builds Windows + macOS on every push/PR and uploads the spike binaries as
  artifacts (`caixonho-spike-*`) for on-device M0b measurement.
