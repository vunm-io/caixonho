# ADR-0001: UI framework — GPUI + gpui-component, tracked from git

- **Status:** Proposed — becomes Accepted or Rejected on the M0 verdict (§Gates)
- **Date:** 2026-08-18
- **Deciders:** Vu Nguyen

## Context

caithung needs a GPU-rendered, low-latency, cross-platform (Windows 11 + macOS)
UI with a virtualized table as its centerpiece. GPUI (Zed's framework) was
chosen for the target feel; the practical question is *which distribution* of
GPUI to build on. Facts verified against crates.io and upstream sources on
2026-08-18:

- `gpui` on crates.io: latest **0.2.2, published 2025-10-22** — frozen for ~10
  months. The published crate *does* contain the Windows backend
  (`src/platform/windows/`, `windows` 0.61 bindings) despite docs only
  advertising macOS/Linux.
- On Windows, GPUI renders through **Blade → Vulkan**, not DirectX. This moves
  the Windows risk from "does it build" to "does it start on machines without a
  working Vulkan driver" (VMs, RDP sessions, old enterprise Intel drivers).
- `gpui-component` (longbridge, Apache-2.0) provides the virtualized
  `DataTable`, theming, and 60+ components, and is shipped in a commercial app.
  Its crates.io release (0.5.1, 2026-02-05) still targets crates.io `gpui
  ^0.2.2`, but its `main` branch has switched to `gpui` **from the zed git
  repo** — the actively developed line assumes git gpui.
- Both `gpui` and `gpui-component` are Apache-2.0. (The Zed *editor* is GPL;
  that license does not apply to the framework crates.)

Two viable stacks:

- **A — crates.io only:** `gpui 0.2.2` + `gpui-component 0.5.1`. Reproducible,
  but frozen at 2025-10/2026-02; forgoes every Windows fix since.
- **B — git, pinned:** `gpui-component@main` (pinned rev) + transitive `gpui`
  from the zed repo. Current, including Windows fixes; costs pin discipline and
  exposure to pre-1.0 breakage.

## Decision

**Stack B**, with three rules that make it reproducible in practice:

1. **`gpui-component` and `gpui-fps` are pinned by `rev`** in the workspace
   `Cargo.toml`. Currently `7a9ac172e804ce89aebac644a02f09813dc9e793`.
2. **`gpui` / `gpui_platform` carry no `rev` on purpose.** Their dependency
   specs must be byte-identical to gpui-component's own declarations
   (`{ version = "0.2.2", git = "https://github.com/zed-industries/zed" }`),
   otherwise Cargo treats them as different sources and links two copies of
   gpui, which fails type-checking everywhere. The actual zed commit is frozen
   by the committed **`Cargo.lock`** — kept at the commit gpui-component's own
   lockfile uses (currently `e0931d5a9dbf4f781b336fdf448739e74a2ac0b5`), i.e.
   the combination upstream actually develops against.
3. **Bumping the stack is a deliberate, standalone PR**: update the
   `gpui-component` rev, re-lock zed to *their* locked commit
   (`cargo update gpui --precise <sha>` retargets the whole zed source), and
   merge only when CI is green on both Windows and macOS. Never bump as a side
   effect and never run a blind `cargo update`.
4. **The Rust toolchain is pinned to the version zed pins** at the locked
   revision (`rust-toolchain.toml` in the zed checkout), currently `1.97.1`,
   and bumps together with the stack. Learned the hard way: gpui at the locked
   rev uses freshly-stabilized std APIs (`std::hint::cold_path`), so a merely
   "recent" stable (1.94) fails with E0658 — a floating `channel = "stable"`
   would make builds depend on how fresh each machine's rustup happens to be.

Insurance policy: the crate split keeps 100% of S3 logic in `caithung-core`
with no UI dependency, so a framework exit costs the GUI crate only.

## Gates (M0)

M0 is split so the expensive half runs on machines other than the developer's:

- **M0a — builds and links (CI):** `cargo clippy -D warnings`, `cargo test`,
  and a release build of the spike must pass on `windows-latest` and
  `macos-latest`. CI uploads the binaries as artifacts.
- **M0b — opens and is smooth (real hardware):** run the artifact on Windows 11
  and macOS and record the table below. Must also be attempted on one "dirty"
  Windows machine (VM or RDP, i.e. weak/no Vulkan) — the *required* outcome
  there is a graceful failure message, not a working window.

| Measurement | Windows 11 | macOS | Bar |
|---|---|---|---|
| First paint (shown in-app) | _TBD_ | _TBD_ | < 500 ms |
| FPS while flick-scrolling 100k rows (in-app HUD) | _TBD_ | _TBD_ | ≈ 60 fps, no hitches |
| RSS with all 100k rows loaded | _TBD_ | _TBD_ | no runaway growth |
| Async feed fills while scrolling | _TBD_ | _TBD_ | no stalls, counter reaches 100k |
| Behavior without Vulkan (Win VM/RDP) | _TBD_ | n/a | graceful error, no silent crash |

**Failure rule:** if Windows cannot be made to work within one focused day of
effort, stop and record the result here — do not silently continue and do not
silently switch frameworks. Fallbacks to evaluate in that order of promise:
egui, iced, Slint, Dioxus, Tauri.

## Consequences

- We inherit upstream breakage on every deliberate bump; the pin rules bound
  when, not whether.
- The Vulkan requirement on Windows becomes a support reality: the app must
  detect renderer-init failure and say so plainly (tracked as a product
  requirement, brief §7).
- `Cargo.lock` is load-bearing and always committed; CI must never regenerate
  it silently.
