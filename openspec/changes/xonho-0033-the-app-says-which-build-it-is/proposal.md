## Why

**Nobody can tell which build they are running, including its author.** The
owner installed `v0.1.0-beta.3`, could not tell it apart from the beta.2 it
replaced, and reported the old interface — correctly, since the theme is not in
either, but there was nothing on screen to settle it. Three files named
`caixonho-macos-arm64.zip`, `(1)` and `(2)` sat in Downloads the same evening
and only a `sha256` said which was which.

The application already answers the neighbouring question — the status bar
names the log directory, "said quietly and always", because the moment it is
wanted is the moment something has gone wrong. *Which build is this* is the
same question asked one step earlier, and a bug report without it cannot be
acted on. Members are being handed builds now, so the reports start now.

**And the answer would be three different numbers.** `Cargo.toml` says
`0.0.0`, the macOS `Info.plist` hardcodes `0.1.0`, the newest tag says
`0.1.0-beta.3`. None of them is wrong on purpose; there is simply no source.

This project has already lost a whole session to a stale binary that looked
current (`docs/` records it), and the fix recorded then was a habit — check
mtimes — rather than something the window says.

## What Changes

- **One version source**: `version` in the workspace `Cargo.toml`, set to the
  version being developed. Everything else derives from it; nothing else
  states a version of its own.
- **The window says which build it is**, in the status bar beside the log
  directory: the version, and the short commit it was built from. A tooltip
  carries the full detail, as the log entry beside it already does.
- **The macOS bundle's `Info.plist` is generated from that source** instead of
  carrying a hardcoded `0.1.0` that has been wrong since the first release.
- **The files CI produces carry their own version in their names**, so a
  download says what it is without a checksum — and so the release step stops
  renaming files by hand, which is where a mislabelled asset would come from.
- **The app bundle is deliberately NOT renamed.** `Caixonho.app` and
  `io.vunm.caixonho` are identity: keychain grants, TCC permissions and
  Gatekeeper's *Open Anyway* record all key on them. A per-version name would
  re-prompt for everything on every update and leave a row of near-identical
  apps in Applications. Version belongs in what is downloaded, not in what is
  installed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `diagnostics`: gains a requirement that the application can state which build
  it is, unprompted and without the user knowing where to look. The capability
  exists so that "a failure can be explained after it has happened, by someone
  who was not watching when it did", and the first thing such a person needs is
  which build failed. Its neighbouring requirement — the system can state where
  its log is — is the same promise about a different fact.

## Impact

- **`crates/caixonho-gui`**: a build script to capture the commit; the status
  bar gains one element; `Cargo.toml` inherits the workspace version.
- **`Cargo.toml`** (workspace): `version` becomes meaningful and is the source.
- **`scripts/mac-app.sh`**: reads the version rather than hardcoding it.
- **`.github/workflows/ci.yml`**: artifacts are named with the version.
- **Release process**: one step fewer — no manual rename — and one step more:
  bump the source version in the release commit. `docs/releases/` gains the
  rule.
- **Dependencies**: none. The commit is read by the build script from `git`,
  with a stated fallback when there is no repository to read.

### Planning gate

**`[M]` requirements this change delivers:** none. It is quality-of-life, and
saying so is the point of this line.

**`[M]` requirements still unbuilt ahead of it** (`docs/requirements-status.md`):
in-app IAM Identity Center sign-in (`XONHO-0011`, §4.1); sort honesty (§4.2);
KMS denial told apart from an S3 denial (§4.3); multipart upload (§4.4); retry
with backoff and adaptive concurrency (§4.4); drag and drop app → OS (§4.4).

**Why this goes first anyway.** It is hours, not days, and it is the first
change written after builds started leaving this machine. Every one of those
mandatory requirements will be reported on by someone running a build nobody
can identify — including the three occasions in the last two days where the
answer to "what are you running" cost real time. `XONHO-0011` remains the
nearest mandatory gap and is unblocked by nothing here; it waits on live checks
only the owner can run, which is unchanged by this.
