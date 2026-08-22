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
   demonstrated in `caixonho-gui/src/app.rs`.
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
- **One exception to that, and only one: removing what must never have been
  published.** A secret, or content that crosses the owner's knowledge
  boundary — anything identifying their employer, its accounts, or its
  infrastructure. Tidiness is never a reason; the line above still forbids
  rewriting to make history read better. Done once, on 2026-08-21, when
  `docs/design-language.md` was found carrying the owner's employer's name in
  a sidebar mockup: 82 commits rewritten, the name replaced by `work`, and the
  result checked to differ from the original tree in that one line alone.
  Before rewriting, take a `git bundle --all` outside the repository and write
  down the old-to-new commit mapping — session logs and planning documents
  cite commits by SHA, and every one of those citations dangles afterwards.
  **`filter-branch` drops commit signatures and says nothing about it**: the
  2026-08-21 rewrite left 82 of 129 commits unsigned, and it was nearly missed
  because `git log %G?` prints `N` both for an unsigned commit and for a signed
  one it cannot verify. Read the `gpgsig` header with `git cat-file commit` —
  that is the only answer that distinguishes them — and decide before pushing
  whether the signatures are being re-applied or given up.
  **Know what it does not buy.** A force-push removes nothing from GitHub:
  unreferenced commits stay reachable by SHA and are still served by the API.
  Measured rather than assumed — the old blob was fetched successfully after
  the push. Closing that takes a request to GitHub Support to purge the
  unreferenced objects, so the rewrite is the first half of the job and not
  the whole of it.
- Conventional Commits with a task ID in the scope: `feat(XONHO-0003): ...`.
  Small housekeeping may use the owner's global `OPS-NNNN` sequence — prefer it
  for anything that is not project work, so the project's own sequence stays
  meaningful. The launch-name slug `THUNG` is retired: `THUNG-0001` and
  `THUNG-0002` remain in commit history and are never reused, but no work
  continues under them — the in-flight change was renumbered to `XONHO-0003`
  rather than closing under its old ID. Every task uses `XONHO-NNNN`.
- **A commit is a decision, not a keystroke.** Written after 2026-08-20, when
  this repository took **24 documentation commits against 12 of code** in one
  day — twice as many commits *about* the work as commits *doing* it — with
  twelve touching `docs/planned-changes.md` and six of those inside 37 minutes.
  Three of the six were one investigation: a prediction, its correction, and
  its correction again, because the guess was committed before the command was
  run. That is thinking out loud into git, and the small commits are the
  symptom rather than the disease.
  - **Code**: one commit per slice that compiles, passes and could be reverted
    on its own. That granularity is right and is not what changed here.
  - **Notes and planning**: **one commit per discussion**, written when the
    discussion has reached a conclusion — not one per finding as it arrives.
    A reader of this history wants the conclusion, not the path to it.
  - **Measure before recording.** A prediction that a command could have
    settled does not belong in a commit at all. Run it, then write once.
  - Fear of losing work in a long session is not a reason: the working tree
    holds it, and a handoff log records it.
- AI provenance trailers on commits are allowed (owner decision 2026-08-20,
  aligning this repo with the workspace-wide policy): `Co-Authored-By:
  Claude <model>`, `AI-Assisted-By:`, `AI-Reviewed-By:` — use them when the
  involvement is material. The author of record stays Vu Nguyen; a self
  `Co-authored-by: Vu Nguyen` adds nothing and is not used.
- Branches: `<type>/xonho-NNNN-short-description`; merge to `main`.
- ADR for every irreversible choice, in `docs/adr/NNNN-title.md`.
- Change management from M1 onward: **OpenSpec** (`openspec/`, `/opsx:*`
  commands, the `passdown` schema — spec-driven plus dispatchable-task
  metadata; embedded at `openspec/schemas/passdown/`) — explore → propose →
  specify → apply → archive.
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
- **A dependency trace is read, never reconstructed.** Which crate arrives by
  which route comes off that crate's own `[features]` table and `cargo tree`,
  not from what a feature name sounds like. `XONHO-0017` found a trace in
  `docs/planned-changes.md` blaming `__rustls` for pulling a legacy TLS stack
  it does not pull, and the wrong mechanism produced the wrong remedy —
  "accept four advisories" instead of "delete two default-feature sets". Note
  also that `cargo tree` answers for the **host** target unless told otherwise:
  a crate that reaches the build only on Windows is invisible here until
  `--target all`, while `cargo audit` reads the lockfile and sees it.
- **Scripts in `scripts/` run on the maintainer's macOS as well as on the Linux
  runner.** That means POSIX `awk` — macOS has neither gawk's three-argument
  `match` nor `{n}` interval expressions, and a script using them passes CI and
  fails on the machine the maintainer is holding.

## Planning gate (before choosing what to build)

The close-out review below checks that a change did what it promised. This one
checks that it was the right change. They are different failures, and only the
second one wastes a week.

A proposal states, in its own words:

- **which `[M]` requirements from `PROJECT_BRIEF.md` it delivers**, by section;
- **which `[M]` requirements are still unbuilt ahead of it**, from
  `docs/requirements-status.md`, and why this change goes first anyway.

The second is the one that matters. Choosing polish over a mandatory requirement
is allowed; choosing it without noticing is not. When a change lands,
`docs/requirements-status.md` is updated in the same change — a status file
nobody diffs against reality stops being read.

This exists because it was missing. The order of work was argued from what was
convenient on the machine it was being written on — "the person using the app
today already has working profiles" — while three mandatory requirements about
how anyone else would get credentials at all sat unstarted, all of them written
in the brief from the beginning. Nothing in the process ever compared the plan
to the requirements, so nothing caught it. The owner did.

## Close-out review (run it before calling a change done)

A change is not finished when its tasks are ticked. It is finished when someone
has asked these five questions out loud and written the answers down — in the
change's `tasks.md` while it is open, or in the session log. Ticking a checkbox
records that work happened; this records that nothing was left behind.

1. **Did we build what was asked, or what was convenient?** Re-read the change's
   own proposal and the requirements it names in `PROJECT_BRIEF.md` and
   `openspec/specs/`. Every departure is either an amendment written into the
   document it departs from, or a defect. Neither is a silent decision.
2. **Do the reader-facing documents still tell the truth?** `README.md`,
   `docs/architecture.md`, `docs/roadmap.md`, `docs/design-language.md`. A change
   that alters what the app does for a user carries its own documentation.
   Two readings, and the second is the one that gets skipped: *claims this
   change contradicts*, and *claims made wrong by this change's absence*.
   `docs/roadmap.md` carries a row per change and `docs/planned-changes.md`
   holds sections written while a decision was open — neither is caught by the
   first reading. Check your own row, then the rows either side of it: this
   question is asked per-change, so a cell belonging to a *different* change
   goes stale with nobody scheduled to look at it. On 2026-08-22 that was three
   cells and two sections, the oldest wrong for two days.
3. **Did we leave rubbish?** Dead code and unused API, throwaway scripts and
   diagnostic files, commented-out blocks, `TODO`s with nobody's name on them,
   duplicated constants that will drift. Delete it now — API kept "for later"
   is how a crate acquires functions nobody dares remove.
4. **What is asserted but not verified?** Name the paths with no test, the
   assumptions no test would catch, and anything ticked on the strength of unit
   tests alone. This project has already had a real failure that 105 green tests
   said nothing about, because the tests knew only what the double had been told.
5. **What is left, and where is it written?** Anything discovered and not done
   belongs in `docs/planned-changes.md` or a change of its own — never only in a
   conversation. A finding that lives in a transcript is a finding that is lost.

The point is not ceremony. Each of these has already caught something real
here: a README that described a milestone the repo had left, a spec requirement
delivered in half, a token module shipped with functions nothing called, a
diagnostic example left in the tree, and a defect that only appeared when a
person opened the app.

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
