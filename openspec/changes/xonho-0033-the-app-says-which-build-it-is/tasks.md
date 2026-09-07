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

- [ ] 1.1 Make the workspace version mean something [dispatch: main]
  - Paths: `Cargo.toml`
  - Done criteria: `[workspace.package] version = "0.1.0-beta.3"` — the version
    last released, since nothing newer has been. A comment states that this is
    the single source and that the release process bumps it, so the next reader
    does not "tidy" it back to `0.0.0`.
  - Both crates already inherit with `version.workspace = true`; check rather
    than assume.
  - Verification: `cargo metadata --format-version 1 --no-deps | python3 -c "import sys,json;print({p['name']:p['version'] for p in json.load(sys.stdin)['packages']})"`

## 2. The commit

- [ ] 2.1 A build script that reads the revision, and admits when it cannot
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

- [ ] 2.2 Say what it costs [dispatch: main]
  - Done criteria: the added time on a warm rebuild of `caixonho-gui`,
    measured and written here. If it is not free, the number is what says so.
  - Verification: `cargo build -p caixonho-gui` timed before and after

## 3. The window

- [ ] 3.1 The status bar states the build [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a `build_identity` element beside `log_location`, in the
    same muted type, reading `<version> (<commit>)`; a tooltip spelling it out
    the way `log_location`'s does. When the commit is `unknown` the version
    still shows and the parenthesis says `unknown` rather than being omitted.
  - The two sit together because they answer the same person at the same
    moment: *what is this, and where is its log.*
  - Verification: `cargo test -p caixonho-gui build_identity`, and the frame
    opened

- [ ] 3.2 A test that fails when the window stops saying it [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a test asserting the rendered identity contains
    `CARGO_PKG_VERSION` — not a hardcoded `"0.1.0-beta.3"`, which would pass
    for ever after the version moved on.
  - Verification: `cargo test -p caixonho-gui`

## 4. The macOS bundle

- [ ] 4.1 `Info.plist` derives, and the numeric field stays numeric
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

- [ ] 4.2 A gate for the drift this change exists to end [dispatch: external-ok]
  - Paths: `scripts/mac-app.sh`
  - Done criteria: the script fails if `CFBundleVersion` and the declared
    version disagree. The hardcoded `0.1.0` was wrong from the first release
    and nothing noticed for two of them; a second stated version is only safe
    while something checks it.
  - Verification: the script run normally passes; run with the version
    tampered, it fails

## 5. What is downloaded says what it is

- [ ] 5.1 CI names the artifacts with the version [dispatch: external-ok]
  - Paths: `.github/workflows/ci.yml`
  - Done criteria: the macOS zip and the Windows exe are produced as
    `caixonho-<version>-macos-arm64.zip` and
    `caixonho-<version>-windows-x86_64.exe`, with the version read on the
    runner from `cargo metadata` rather than written into the workflow.
  - Verification: a CI run, and the artifact contents listed

- [ ] 5.2 The release process loses a step and gains a step [dispatch: main]
  - Paths: `docs/releases/README.md` (new, if absent) or the process note
    wherever it lives
  - Done criteria: written down that the version bump is part of the release
    commit, and that assets are uploaded under the names CI produced rather
    than renamed by hand. The rename is where a mislabelled asset comes from,
    and `v0.1.0-beta.1` shipped two assets from two different commits.
  - Verification: the file, read against the last release's actual steps

## 6. Close-out

- [ ] 6.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - **And the platform-gated walk**: `build.rs` and the status bar are not
    gated, but local clippy compiles one target only — `XONHO-0032` learned
    that from a red Windows build after a clean local run.
  - Verification: the commands

- [ ] 6.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 6.3 Close-out review per `AGENTS.md` [dispatch: main]
  - Question 2 has a known shape here: this change alters what the window
    shows and what a downloaded file is called, so `README.md` and
    `docs/releases/` are both in scope, and the roadmap row for this change
    has to exist before it can go stale.
  - Verification: the recorded findings
