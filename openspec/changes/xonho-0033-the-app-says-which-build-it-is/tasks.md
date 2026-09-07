# Tasks — XONHO-0033 the app says which build it is

> The measure of this change is a question stopping being expensive: *what are
> you running?* It is answered when a screenshot of the window contains the
> answer, and when a file in someone's Downloads folder contains it too.
>
> **Routing.** Mostly `[dispatch: main]`, and three tasks are not — 4.1, 4.2
> and 5.1 go to `agy`.
>
> The workspace rule says Rust and GPUI are not the frontend it hands over, and
> `AGENTS.md` adds a measured reason for this repository in particular: its
> standards for comments and prose make reviewing a delegated draft dearer than
> writing one. The owner widened that boundary on 2026-09-07 ("giao task nhỏ
> cho agy"), and these three are what the widening actually covers — a shell
> script and a workflow, each fully specified here, none of them Rust and none
> of them prose. **The dispatch prompt says not to write explanatory comments**:
> the mechanism is delegable, the voice is not, and mixing them is how the
> review cost comes back.
>
> Everything else stays: `build.rs`'s fallback semantics, the GPUI element, the
> tests, the release-process note (English source is Claude's by the same
> rule), and the close-out.

## 1. The source

- [x] 1.1 Make the workspace version mean something [dispatch: main]
  - Paths: `Cargo.toml`
  - Done criteria: `[workspace.package] version = "0.1.0-beta.3"` — the version
    last released, since nothing newer has been. A comment states that this is
    the single source and that the release process bumps it, so the next reader
    does not "tidy" it back to `0.0.0`.
  - Both crates already inherit with `version.workspace = true`; check rather
    than assume.
  - Verification: `cargo metadata --format-version 1 --no-deps | python3 -c "import sys,json;print({p['name']:p['version'] for p in json.load(sys.stdin)['packages']})"`
  - **Done.** `cargo metadata` reports `0.1.0-beta.3` for both crates.
    `Cargo.lock` had to be regenerated too — it still said `0.0.0`, and
    `cargo pkgid` reads the lock rather than the manifest, so CI would have
    named its artifacts after the old version.

## 2. The commit

- [x] 2.1 A build script that reads the revision, and admits when it cannot
      [dispatch: main]
  - Paths: `crates/caixonho-gui/build.rs` (new), `crates/caixonho-gui/Cargo.toml`
  - Done criteria: emits `cargo:rustc-env=CAIXONHO_COMMIT=<short sha>`;
    emits the literal `unknown` when `git` is missing, the directory is not a
    repository, or the command fails — never an empty string, which renders as
    though the field were absent. `cargo:rerun-if-changed` on `.git/HEAD` and
    on the file `HEAD` points at, so a checkout rebuilds.
  - **The failure to design against is a build script that breaks the build.**
    A missing `git` is not an error here; it is a fact to report.
  - Verification: `cargo build -p caixonho-gui` in the repository, then again
    with `git` removed from `PATH` — both succeed, and the second yields
    `unknown`
  - **Done.** With `git`: `CAIXONHO_COMMIT=3d2afc0`. With `git` replaced by
    a stub that exits 127: `CAIXONHO_COMMIT=unknown`, and the build succeeds.
    Both read off `cargo build -vv`, not inferred.

- [x] 2.2 Say what it costs [dispatch: main]
  - Done criteria: the added time on a warm rebuild of `caixonho-gui`,
    measured and written here. If it is not free, the number is what says so.
  - Verification: `cargo build -p caixonho-gui` timed before and after
  - **Measured 2026-09-07.** The three `git` calls cost **28 ms**. A rebuild
    that reruns the script is 1786 ms against 345 ms for one that does not —
    the 1.4 s is `caixonho-gui` recompiling because an embedded value may have
    changed, not the script. So: free while `HEAD` stands still, and about a
    second and a half on the first build after a commit or a checkout.

## 3. The window

- [x] 3.1 The status bar states the build [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a `build_identity` element beside `log_location`, in the
    same muted type, reading `<version> (<commit>)`; a tooltip spelling it out
    the way `log_location`'s does. When the commit is `unknown` the version
    still shows and the parenthesis says `unknown` rather than being omitted.
  - The two sit together because they answer the same person at the same
    moment: *what is this, and where is its log.*
  - Verification: `cargo test -p caixonho-gui build_identity`, and the frame
    opened
  - **Done.** `build_identity` beside `log_location`, `space::INLINE`
    between them, reading `0.1.0-beta.3 (3d2afc0)`.

- [x] 3.2 A test that fails when the window stops saying it [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a test asserting the rendered identity contains
    `CARGO_PKG_VERSION` — not a hardcoded `"0.1.0-beta.3"`, which would pass
    for ever after the version moved on.
  - Verification: `cargo test -p caixonho-gui`
  - **Done.** `the_window_states_the_version_and_the_revision_it_was_built_from`
    asserts against `CARGO_PKG_VERSION` rather than a literal, and separately
    that the revision never renders as an empty `()`.

## 4. The macOS bundle

- [x] 4.1 `Info.plist` derives, and the numeric field stays numeric
      [dispatch: external-ok]
  - Paths: `scripts/mac-app.sh`
  - Done criteria: version read with `cargo metadata` (not a `grep` of
    `Cargo.toml`, whose `version` line is not the only one);
    `CFBundleShortVersionString` = the numeric core (`0.1.0`), because macOS
    expects period-separated numerals there; `CFBundleVersion` = the full
    declared version (`0.1.0-beta.3`), which is where the pre-release part
    survives.
  - Verification: `scripts/mac-app.sh --no-open` then
    `plutil -p target/Caixonho.app/Contents/Info.plist | grep -i version`
  - Dispatched: agy (2026-09-07) — done as specified; verified: `sh -n` clean,
    script run, `plutil -p` shows `CFBundleShortVersionString => "0.1.0"` and
    `CFBundleVersion => "0.1.0-beta.3"`. Read `cargo metadata` via `python3`,
    numeric core by `${version%%-*}`, heredoc unquoted so both substitute. No
    comments added, as instructed.

- [x] 4.2 A gate for the drift this change exists to end [dispatch: external-ok]
  - Paths: `scripts/mac-app.sh`
  - Done criteria: the script fails if `CFBundleVersion` and the declared
    version disagree. The hardcoded `0.1.0` was wrong from the first release
    and nothing noticed for two of them; a second stated version is only safe
    while something checks it.
  - Verification: the script run normally passes; run with the version
    tampered, it fails
  - Dispatched: agy (2026-09-07) — done as specified; verified by causing the
    regression it guards: re-quoting the heredoc as `<<'PLIST'` makes the
    script exit 1 with `CFBundleVersion ($version) does not match declared
    version (0.1.0-beta.3)`. Reviewed for a hole where both values are empty
    and the comparison passes — `set -e` aborts the assignment first, checked
    with `sh -eu -c 'v="$(false)"; echo LỌT'`, so no extra guard is needed.

## 5. What is downloaded says what it is

- [x] 5.1 CI names the artifacts with the version [dispatch: main]
  - **Routed to `agy` and taken back before dispatch (2026-09-07)**, after
    reading the job rather than the task: the macOS side is one changed
    `ditto` line, but Windows has no rename step at all and its runner shell is
    `pwsh`, so this is a new step in a second shell plus a version read on both.
    Two shells is not the "fully specified and mechanical" this delegation
    covers.
  - Paths: `.github/workflows/ci.yml`
  - Done criteria: the macOS zip and the Windows exe are produced as
    `caixonho-<version>-macos-arm64.zip` and
    `caixonho-<version>-windows-x86_64.exe`, with the version read on the
    runner from `cargo metadata` rather than written into the workflow.
  - Verification: a CI run, and the artifact contents listed
  - **Done, in `main` after all.** One `shell: bash` step reads the version
    with `cargo pkgid | sed 's/.*[#@]//'` — no JSON parser, because which one a
    runner has differs by image — and the macOS and Windows steps name their
    own file. The upload matches by glob so the names are stated once.

- [x] 5.2 The release process loses a step and gains a step [dispatch: main]
  - Paths: `docs/releases/README.md` (new, if absent) or the process note
    wherever it lives
  - Done criteria: written down that the version bump is part of the release
    commit, and that assets are uploaded under the names CI produced rather
    than renamed by hand. The rename is where a mislabelled asset comes from,
    and `v0.1.0-beta.1` shipped two assets from two different commits.
  - Verification: the file, read against the last release's actual steps
  - **Done**, `docs/releases/README.md`. Carries the bump step, the
    no-rename rule, the `codesign --verify` check that two releases needed and
    did not have, and the instruction to open a browser-downloaded copy and
    read `syspolicyd` before describing a dialog.

## 6. Close-out

- [x] 6.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - **And the platform-gated walk**: `build.rs` and the status bar are not
    gated, but local clippy compiles one target only — `XONHO-0032` learned
    that from a red Windows build after a clean local run.
  - Verification: the commands
  - **Done 2026-09-07.** fmt clean; `clippy --workspace --all-targets -D
    warnings` clean; 540 tests pass. Gate walk: this change added nothing
    platform-gated — `build.rs`, the status bar element and its test compile on
    both targets, and CI confirms it. The one platform-specific piece,
    `mac-app.sh`, is guarded by `runner.os == 'macOS'` as it already was.

- [x] 6.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`
  - **Run 34092006851** — `build (windows-latest)` and `build (macos-latest)`
    both `success`, `rustfmt` and `dependency audit` too. Artifacts checked
    rather than assumed: `caixonho-0.1.0-beta.3-windows-x86_64.exe` and
    `caixonho-0.1.0-beta.3-macos-arm64.zip`, and inside the zip
    `CFBundleShortVersionString => "0.1.0"`, `CFBundleVersion =>
    "0.1.0-beta.3"`, `codesign --verify --deep --strict` valid, bundle still
    `Caixonho.app` / `io.vunm.caixonho`.

- [x] 6.3 Close-out review per `AGENTS.md` [dispatch: main]
  - Question 2 has a known shape here: this change alters what the window
    shows and what a downloaded file is called, so `README.md` and
    `docs/releases/` are both in scope, and the roadmap row for this change
    has to exist before it can go stale.
  - Verification: the recorded findings
  - **1. Asked or convenient?** Asked, with one departure recorded where it
    happened: 5.1 was routed to `agy` and taken back before dispatch, because
    reading the job showed two shells rather than one changed line.
  - **2. Reader-facing documents.** `docs/releases/README.md` is new and is the
    first written record of the release process. `docs/roadmap.md` gains this
    change's row. `README.md` unchanged: it describes what the app does for a
    user, and this adds a line to a status bar rather than a capability. The
    three release notes already carry their corrections from 2026-09-05 and
    09-06 and are not touched again.
  - **3. Rubbish.** No dead code. `build_identity_text` exists apart from
    `build_identity` because a string is assertable without a renderer and an
    `AnyElement` is not — one caller each, the test being the second. The
    hardcoded `0.1.0` is gone rather than left beside its replacement.
  - **4. Asserted but not verified.** The status bar element is asserted
    through its text, not a rendered frame: a change that rendered it invisibly
    would pass. `build.rs`'s `unknown` path is exercised with a stub `git` that
    exits 127, which is the same code path but a simulated environment. And the
    Windows artifact's *name* is verified while nothing here can run the
    executable inside it.
  - **5. Left, and where.** Whether the version bump belongs before or after
    the tag stays open in `design.md` until the second release under this
    scheme. Embedding a `VERSIONINFO` resource so the Windows executable states
    its version in Explorer, as the macOS bundle does, is a named non-goal
    there and is the obvious follow-up.
