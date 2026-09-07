## Context

Three places state a version and none of them is the source: the workspace
`Cargo.toml` says `0.0.0`, `scripts/mac-app.sh` writes `0.1.0` into every
`Info.plist`, and the newest tag says `0.1.0-beta.3`. The window states none of
them.

The status bar already carries the neighbouring fact. `log_location` renders
the log directory with a tooltip, deliberately always visible, and its comment
gives the reason this change borrows: *"the moment it is wanted is the moment
something has gone wrong, and hunting for it then is the worst time."*

Two constraints come from outside this repository. `CFBundleShortVersionString`
is a marketing version macOS expects to be numeric; `CFBundleVersion` is the
build identifier and is treated loosely for software distributed outside the
App Store. And a released version alone cannot identify a build: `main` moves
between tags, so every build made after a release would claim to be that
release.

## Goals / Non-Goals

**Goals:**

- One declared version, and everything else derived from it.
- The window identifies the running build precisely enough to fetch the same one.
- A downloaded file identifies itself by name.
- A build made outside a repository still builds, and says what it does not know.

**Non-Goals:**

- Renaming the app bundle or its identifier. `Caixonho.app` and
  `io.vunm.caixonho` are what keychain grants, TCC permissions and Gatekeeper's
  *Open Anyway* record key on; a per-version identity re-prompts for all of it
  on every update.
- Automating the version bump. Deciding that a release is `beta.4` is a
  judgement, and this change makes the decision take effect in one edit rather
  than making it for anyone.
- An About window. The status bar is where the neighbouring fact already lives.
- Windows executable metadata. `caixonho-gui.exe` gains its version in its
  file name here; embedding a `VERSIONINFO` resource is its own change.

## Decisions

### The version source is the workspace `Cargo.toml`

`[workspace.package] version`, inherited by both crates, and reachable in code
as `CARGO_PKG_VERSION` with no plumbing. The alternative — a `VERSION` file
read by everything — needs a reader in Rust, in `sh`, and in the workflow;
Cargo's own field already has readers in all three (`env!`, `cargo metadata`,
and the same via the runner).

It holds **the version being developed**, which after a release is the version
just released. A build from `main` after `v0.1.0-beta.3` therefore says
`0.1.0-beta.3` plus a commit that is not that tag's — which is why the commit
is not optional.

### The commit comes from a build script, not from CI

`build.rs` in `caixonho-gui` runs `git rev-parse --short HEAD` and emits it as
a compile-time variable, with `cargo:rerun-if-changed` on `.git/HEAD` and the
current ref so a rebuild follows a checkout.

Considered and rejected: reading a CI environment variable. It would leave
every local build unable to identify itself, which is exactly the case that has
already cost this project a session — a developer looking at a window that is
not the code they just changed. The build script treats CI and a laptop the
same.

When `git` is absent or the directory is not a repository, the value is the
literal `unknown`. Not an empty string, which renders as though the field were
missing, and not a guess.

### `Info.plist` takes two fields, and they are not the same string

`CFBundleShortVersionString` gets the numeric core (`0.1.0`), because macOS
expects a period-separated numeric there and tooling has been known to reject
otherwise. `CFBundleVersion` gets the full declared version
(`0.1.0-beta.3`), which is where the pre-release part survives.

`mac-app.sh` reads the version with `cargo metadata`, not by grepping
`Cargo.toml`: the manifest's `version` line is not the only line matching
`version` and a grep would eventually pick the wrong one.

### The file names are made by CI, and the release step stops renaming

CI produces `caixonho-<version>-macos-arm64.zip` and
`caixonho-<version>-windows-x86_64.exe`. The release step then uploads what CI
built under the name CI gave it.

This removes the hand-rename that has stood between the artifact and the
release asset. That step is where a mislabelled asset would come from, and the
evidence it can go wrong is already on record: `v0.1.0-beta.1` shipped its two
assets from two different commits.

### The window shows `version (commit)`, with the detail in a tooltip

`0.1.0-beta.3 (6a6b923)` beside the log directory, in the same muted type. The
tooltip carries the whole thing spelled out, as the log entry's tooltip does.
Two facts, one line, and the short commit is what makes them different builds.

## Risks / Trade-offs

- **[A build script that shells out to `git` slows every build]** → It runs
  once per crate compilation, not per file, and only when `.git/HEAD` or the
  current ref changes. Measured before and after; if it is not free, it is
  recorded as what it costs.
- **[`rerun-if-changed` on git internals is not airtight]** → A commit made
  without moving `HEAD`'s file — some worktree and rebase paths — could leave a
  stale value. The failure is a wrong short commit, which is visible in the
  window rather than silent, and `touch build.rs` clears it. Accepted rather
  than solved with a heavier mechanism.
- **[The declared version goes stale between releases]** → It says the last
  released version, deliberately, and the commit distinguishes builds. The
  release process gains a bump step, which is a thing to forget; the release
  notes' own checksum step already fails loudly if assets and notes disagree.
- **[Two more places read a version]** → That is the opposite of the
  requirement, unless they derive it. Both do, and the tasks check it: a build
  whose `Info.plist` disagrees with `CARGO_PKG_VERSION` fails.

## Migration Plan

Nothing to migrate: no persisted data and no format anyone else reads. The
version in `Cargo.toml` moves from `0.0.0` to `0.1.0-beta.3`, which is the
current released version, so the first build after this change tells the truth
about itself.

Rollback is reverting the change; the app loses the line and the artifacts
lose their version suffix.

## Open Questions

- **Whether the release process should bump the version before or after the
  tag.** The tasks assume before — the bump is part of the release notes
  commit, and the tag then names a commit whose version matches it. The
  alternative, bumping immediately after a release to a `-dev` suffix, makes
  every intermediate build honest but adds a version nobody released. Not worth
  deciding until the second release under this scheme.
