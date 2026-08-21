## 1. Let a test reach the doubles core already has

- [ ] 1.1 A `test-support` feature on `caixonho-core` [dispatch: main]
  - Paths: `crates/caixonho-core/Cargo.toml`, `crates/caixonho-core/src/store.rs`,
    `crates/caixonho-core/src/session.rs`
  - Done criteria: a `test-support` feature, off by default. Under
    `#[cfg(any(test, feature = "test-support"))]`, `store::double::StoreDouble`
    and `Session::{with_secret_store, with_connection_file}` become reachable
    from outside the crate. **`pub` without the gate is the wrong answer** and
    the close-out review in `AGENTS.md` says why: API kept "for later" is how a
    crate acquires functions nobody dares remove.
  - Done criteria: the default build is **unchanged, measured** — compare
    `cargo tree -p caixonho-core --edges features,no-dev` and a release build
    before and after, the same way `XONHO-0017` task 1.2 did. A feature that
    is off must cost nothing.
  - Verification: `cargo build --release -p caixonho-gui`; `cargo test --workspace`

- [ ] 1.2 A way to put an object store into a session [dispatch: main]
  - Paths: `crates/caixonho-core/src/session.rs`
  - Done criteria: under the same gate, a session can be given an
    `Arc<dyn ObjectStore>` for a stated `CredentialsId`, filling the slot
    `install_scheduler` fills for a real connection. It goes through whatever
    that function does about the probe scheduler rather than around it — a
    double that skips the scheduler would make every capability observation in
    a window test a fiction.
  - Done criteria (test): a session holding a double answers `list_buckets`
    from it, with no network and no keychain touched.
  - Verification: `cargo test -p caixonho-core session::`

- [ ] 1.3 Say what the feature is for, where a reader will look [dispatch: main]
  - Paths: `crates/caixonho-core/src/lib.rs`
  - Done criteria: the crate docs name `test-support`, what it exposes, and
    that it is not part of the shipped surface. A feature nobody can find is a
    feature the next person re-invents.
  - Verification: `cargo doc -p caixonho-core --no-deps`

## 2. Give the window its world

- [ ] 2.1 A world the application is handed [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a `World` carrying what `CaixonhoApp::new` reads from the
    machine today — the tokio runtime, the optional `Session`, the discovered
    profiles, the remembered connections, and the startup and connections
    errors. `CaixonhoApp` takes it. **The runtime is owned by the world**, not
    built inside the constructor: it must outlive the spawns, and a test that
    has to remember that separately will one day not.
  - Done criteria: no `*_from_env` call and no `discover` call remains inside
    the constructor. That is the whole point, and it is greppable.
  - Verification: `cargo test --workspace`

- [ ] 2.2 `main.rs` reads the world, as the application always did [dispatch: main]
  - Paths: `crates/caixonho-gui/src/main.rs`, `crates/caixonho-gui/src/app.rs`
  - Done criteria: `main.rs` builds the world from the environment and hands it
    over; startup behaviour is **byte-for-byte what it was**, including the two
    failure paths — trust material that will not prepare, and a connections
    file that will not read. Neither is a connection failure and neither may
    become one.
  - Done criteria: run the app once and see the window come up as before. This
    is a refactor of the one path no test covers, so it is looked at.
  - Verification: `cargo run -p caixonho-gui`, and the log in the platform's
    log directory

- [ ] 2.3 A world a test can write, in one line [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: under `#[cfg(test)]`, a constructor giving a world with a
    current-thread runtime, a session over doubles, and no profiles — so a test
    reads as what it is about rather than as six lines of scaffolding. Every
    later window test starts here, so it is worth being short.
  - Verification: it is used by task 3.1

## 3. The tests the seam exists for

- [ ] 3.1 The wiring `XONHO-0018` left open [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a `#[gpui::test]` that drives `apply_page` with a `Page`
    carrying `served_from: Some(region)` and asserts **that bucket's row** now
    reports that region while the others are untouched. `correct_region` is
    already tested; what is not is that `apply_page` calls it with the right
    bucket, which is the half a user would see.
  - Done criteria (test): a page for a location the window has **already left**
    changes nothing — `apply_page` returns early for those, and a test that
    never exercised the early return would pass against a version that dropped
    it.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 3.2 A screenshot of a real view — `XONHO-0009` task 6.3 [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs` or a test module beside it
  - Done criteria: a real view is constructed over the world from 2.3, drawn,
    and captured with `capture_screenshot`; the image is non-empty and has the
    window's dimensions.
  - Done criteria: **gated to macOS, and the gate says why.** Read from
    `gpui_platform/src/gpui_platform.rs:85`: `current_headless_renderer()`
    returns `Some` only on `target_os = "macos"` and `None` everywhere else, so
    this cannot run on the Windows target — which `AGENTS.md` calls the primary
    daily driver. The comment names that, so the next reader does not take a
    green suite for two-platform coverage.
  - Verification: `cargo test -p caixonho-gui` on macOS; `cargo test --workspace`
    on Windows CI must stay green **because the test is absent there, not
    because it silently passed**

- [ ] 3.3 Close `XONHO-0009` 6.3, and say what it does not cover [dispatch: main]
  - Paths: `openspec/changes/xonho-0009-app-shell-and-visual-foundation/tasks.md`
  - Done criteria: 6.3 ticked with what was captured and on which platform, and
    with the macOS-only limit stated. `XONHO-0009` moves to 19/19.
  - Verification: `openspec list`

## 4. Close-out

- [ ] 4.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` green [dispatch: main]
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [ ] 4.2 CI green on every job, and the run id recorded [dispatch: main]
  - Paths: this file
  - Done criteria: `rustfmt`, `dependency audit`, and both builds successful for
    the tip; the run id written here. **Windows is the one that matters here**:
    it is the target the new test is absent from, and the only proof the
    absence is clean rather than a compilation error nobody saw.
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 4.3 Update the reader-facing documents in this change [dispatch: main]
  - Paths: `docs/architecture.md`, `docs/planned-changes.md`, `docs/roadmap.md`
  - Done criteria: `docs/architecture.md` records that the application is given
    its world rather than fetching it, because that is a shape change and the
    file is about shape. `docs/planned-changes.md`'s section *"The window's
    views are methods, and that is why they cannot be tested"* is **corrected
    in place with the correction visible** — it is now half wrong, and this
    repository has just had two lessons about what a stale note costs.
    `README.md` needs nothing: no user-facing behaviour changes.
  - Verification: the corrected section names what changed and what is still true

- [ ] 4.4 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: this file
  - Done criteria: the five questions answered in writing, including what is
    asserted but not verified — at minimum, that `main.rs` moved and no test
    covers it, and that the screenshot proves one platform of two
  - Verification: the answers exist and name specifics, not reassurances
