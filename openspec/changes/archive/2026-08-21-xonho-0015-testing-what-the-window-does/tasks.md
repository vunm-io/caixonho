## 1. Let a test reach the doubles core already has

- [x] 1.1 A `test-support` feature on `caixonho-core` [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test --workspace` 264
        core + 36 window, clippy clean at `-D warnings`,
        `cargo check -p caixonho-core --features test-support` compiles with
        the feature alone (not only under `cfg(test)`).
      - **The default build is unchanged, measured on both sides.**
        `cargo tree -p caixonho-gui --edges features,no-dev` reports
        `caixonho-core` with **no features** before and after; with dev edges
        it reports `test-support`. The release binary is
        **39,949,456 bytes — byte-identical** to the pre-change build.
      - **Deviation from the done criteria, narrowed on evidence.** The task
        said `with_secret_store` and `with_connection_file` would open up too.
        They did not, and clippy is why: making them `pub` leaks
        `credentials::SecretStore` and `connections::ConnectionFile`, both
        `pub(crate)`, into the public API — `private_interfaces` fails the
        build. The right question then was whether the seam needs them at all,
        and it does not: a frontend is **handed** its connections by the world
        (task 2.1) rather than reading them, and neither recorded gap touches
        the keychain. So the surface is `StoreDouble` and
        `install_object_store`, and nothing speculative.
      - **A real defect the doc build caught.** Both injectors already carried
        `#[cfg(test)]`; adding a second `#[cfg(any(test, feature = ...))]`
        **ANDs** them, so the feature alone would never have compiled those
        functions. Nothing failed — the tests still ran under `cfg(test)`. It
        surfaced as two unresolved rustdoc links, which is the only signal that
        was ever going to appear.
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

- [x] 1.2 A way to put an object store into a session [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test -p caixonho-core
        session::` — 20 pass, one new.
      - `Session::install_object_store` is a gated wrapper over
        `install_scheduler`, the door production uses, so the probe scheduler
        is installed with the store. A double wired past it would leave every
        row in a window test saying "checking…" for ever, and a test asserting
        on that would be asserting on the absence of an answer.
      - The test opens no connection, resolves no profile and touches no
        keychain, and a location still reads — asserted by the page carrying a
        cursor only the double could have produced, rather than by it merely
        being `Ok`.
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

- [x] 1.3 Say what the feature is for, where a reader will look [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo doc -p caixonho-core
        --no-deps --features test-support` builds with no unresolved links.
      - The crate docs name the feature, what it exposes, what it deliberately
        does **not** expose and why, and that resolver 2 keeps it out of a
        build without dev targets.
  - Paths: `crates/caixonho-core/src/lib.rs`
  - Done criteria: the crate docs name `test-support`, what it exposes, and
    that it is not part of the shipped surface. A feature nobody can find is a
    feature the next person re-invents.
  - Verification: `cargo doc -p caixonho-core --no-deps`

## 2. Give the window its world

- [x] 2.1 A world the application is handed [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test --workspace` 264
        core + 38 window, clippy clean.
      - The greppable criterion holds: **none of** `from_env`, `discover(`,
        `ConfigPaths`, `HttpStack`, `stored_connections` or
        `tokio::runtime::Builder` appears anywhere inside the constructor.
        Checked by script over the function body rather than by eye.
      - `World` owns the runtime, as the task required. It is destructured at
        the top of `new`, so the fields are named once and the rest of the
        constructor reads exactly as it did.
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

- [x] 2.2 `main.rs` reads the world, as the application always did [dispatch: main]
      - Done in `main` (2026-08-21).
      - **Run, not just compiled.** A fresh debug build was started, stayed up
        1m52s at 80 MB RSS, and stopped cleanly on `SIGTERM`. It got past
        `open_window(...).expect("failed to open window")`, which is what a
        failed window would have panicked at, and nothing new appeared in the
        log at `~/Library/Logs/caixonho`.
      - **What that does and does not prove.** Nothing appearing in the log is
        the *expected* result, not a weak one: the default filter records
        `WARN` and above, so a clean startup is silent by design — checked
        against the previous days' files, which contain only warnings. What is
        **not** proven is what the window looked like. The process passing the
        window-open expect is an inference from the code, not a screenshot,
        and the visual pass belongs with the owner's other live checks.
      - `diagnostics::start()` still runs first in `main`, before
        `gpui_component::init` and before the world is read — the ordering that
        comment exists to protect is untouched.
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

- [x] 2.3 A world a test can write, in one line [dispatch: main]
      - Done in `main` (2026-08-21); used by both tests in 3.1.
      - `World::scripted(store)` — a current-thread runtime, trust material
        from the OS store, config paths naming nothing, and the store double
        where the S3 adapter goes. No profile and no remembered connection: a
        test that wants either should say so rather than inherit it.
      - **The session is real, not `None`.** A world with no session *and* no
        startup error is a state the application never reaches — `new` only
        ever produces `session: None` alongside `startup_error: Some(_)` — and
        a test standing in one would be testing a shape nobody ships.
      - It needed one thing core did not offer: `Diagnostics` has no
        constructor but `start()`, which opens a real log file. Added
        `Diagnostics::without_a_log()` under the same feature gate, returning
        `NoLocation` rather than all-`None` — because all-`None` is a fourth
        shape `start` never returns.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: under `#[cfg(test)]`, a constructor giving a world with a
    current-thread runtime, a session over doubles, and no profiles — so a test
    reads as what it is about rather than as six lines of scaffolding. Every
    later window test starts here, so it is worth being short.
  - Verification: it is used by task 3.1

## 3. The tests the seam exists for

- [x] 3.1 The wiring `XONHO-0018` left open [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test -p caixonho-gui
        app::tests` — 2 pass.
      - **Both were watched failing before being believed.** These tests were
        written after the code they cover, so passing proves nothing on its
        own. Removing the `correct_region` call from `apply_page` fails
        `a_page_served_from_elsewhere_corrects_that_bucket_and_no_other` and
        leaves the other green; removing the early return fails
        `a_page_for_a_location_already_left_corrects_nothing` and leaves the
        first green. Each test fails for its own reason and no other, then both
        were restored.
      - They drive the **real view** through `apply_page`, not a delegate
        lifted out of it — which is the half `XONHO-0018` recorded it could
        not reach.
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

- [x] 3.2 A screenshot of a real view — `XONHO-0009` task 6.3 [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test -p caixonho-gui
        a_real_view_renders` passes on macOS, and the whole chain is real —
        a `CaixonhoApp` built over `World::scripted`, opened in a
        `HeadlessAppContext`, rendered through Metal, captured as an
        `RgbaImage` with non-transparent pixels in it.
      - **It needed one dependency line, and the reason is measured.**
        `current_headless_renderer` lives behind `gpui_platform`'s
        `test-support` feature (`gpui_platform.rs:84`), which this workspace
        did not enable — the compiler said `cannot find value`, which is how
        that was found rather than assumed. Added under `[dev-dependencies]`,
        the same shape and the same argument as the `gpui` line above it:
        `ADR-0001`'s byte-identical rule is about the *source*, and a feature
        is not a different source.
      - **Windows is not put at risk by it**, checked rather than hoped:
        `test-support` expands to `gpui_macos/test-support`, and `gpui_macos`
        sits under `[target.'cfg(target_os = "macos")'.dependencies]` in that
        crate, so it is inert elsewhere. This workspace already enables
        `font-kit` and `runtime_shaders`, which name the same crate the same
        way, and Windows CI has been green throughout.
      - **The shipped binary is unaffected, and that is measured both ways.**
        Non-dev features of `gpui_platform` are
        `default,font-kit,runtime_shaders,wayland,x11` — no `test-support`;
        with dev edges it appears. Building the release binary with both
        dev-dependencies removed and again with them restored produced
        **byte-identical** output, 39,946,848 bytes each time.
      - **Found by running it: the image is in device pixels.** A 1280x800
        window came back 2560x1600 on this 2x display. The first assertion
        hard-coded the logical size and failed — corrected to assert a
        whole-number scale of the window, so it says the same thing on a 1x
        display.
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

- [x] 3.3 Close `XONHO-0009` 6.3, and say what it does not cover [dispatch: main]
      - Done in `main` (2026-08-21) — **but not as this task described it, and
        the difference matters.** The task said 6.3 would be ticked and
        `XONHO-0009` would move to 19/19. It was not, and it did not.
      - Reading 6.3 rather than remembering it: it asks for screenshots of
        **every state**, judged against `docs/design-language.md`. The seam is
        what was blocking it, and the seam is now open — but judging a
        rendering against a visual reference is the owner's work, not a test's,
        and one screenshot of one view is not every state. Ticking it would
        have made a real defect-finding exercise disappear into a green
        checkbox, which is the failure this project keeps writing down about
        itself.
      - What was done instead: 6.3 carries a dated note recording that the
        blocker is gone, that every condition its earlier note predicted held,
        the device-pixel surprise, and exactly what is left. `XONHO-0009` stays
        at 18/19.
  - Paths: `openspec/changes/xonho-0009-app-shell-and-visual-foundation/tasks.md`
  - Done criteria: 6.3 ticked with what was captured and on which platform, and
    with the macOS-only limit stated. `XONHO-0009` moves to 19/19.
  - Verification: `openspec list`

## 4. Close-out

- [x] 4.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-21); verified: all three exit zero.
        264 core + 39 window tests, clippy clean at `-D warnings`.
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [x] 4.2 CI green on every job, and the run id recorded [dispatch: main]
      - Done in `main` (2026-08-21); run **32496372817**, conclusion `success`:
        `rustfmt`, `dependency audit`, `build (windows-latest)` and
        `build (macos-latest)` all successful.
      - **The absence on Windows is clean, and that is counted rather than
        assumed.** Comparing the Windows job's own numbers across the two runs:

        | | before this change | after | delta |
        |---|---|---|---|
        | Windows core | 262 | 263 | **+1** |
        | macOS core (local) | 263 | 264 | **+1** |
        | Windows window | 36 | **38** | +2 |
        | macOS window (local) | 36 | **39** | +3 |

        The new **core** test runs on Windows too — both platforms gained one,
        so the standing one-test gap in core is pre-existing and nothing to do
        with this change. The window crate gained two on Windows and three
        locally, which is exactly the screenshot test being absent. Its name
        appears nowhere in the Windows log, so it was excluded rather than run.
      - Resolution had already been checked here — `cargo tree --target
        x86_64-pc-windows-msvc --edges all` contains `gpui_macos` zero times —
        and this run answers the different question resolution could not: that
        the Windows build compiles and links with the test module `cfg`-ed out.
      - Windows took 18m21s, in line with the runs before it; the added
        features brought no new crates, and the lockfile is unchanged since
        `e176ec9` at 948.
  - Paths: this file
  - Done criteria: `rustfmt`, `dependency audit`, and both builds successful for
    the tip; the run id written here. **Windows is the one that matters here**:
    it is the target the new test is absent from, and the only proof the
    absence is clean rather than a compilation error nobody saw.
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [x] 4.3 Update the reader-facing documents in this change [dispatch: main]
      - Done in `main` (2026-08-21).
      - `docs/architecture.md` gains the shape change: the window is given its
        world rather than fetching one, stated as a testability property
        before a tidiness one.
      - `docs/planned-changes.md` §*"The window's views are methods, and that
        is why they cannot be tested"* is **corrected in place, title and all,
        with the old reasoning left standing**. Every observation in it still
        holds; the conclusion did not. It would have bought a rewrite of every
        view in the window to obtain what a constructor signature already gave
        — so the habit is recorded with it: when something is called
        untestable, find the one thing that makes it so before rewriting what
        surrounds it. Here it was six lines of environment reading that had sat
        in a constructor since M0.
      - `docs/roadmap.md` gains the row. `README.md` needs nothing: nothing a
        user sees has changed.
  - Paths: `docs/architecture.md`, `docs/planned-changes.md`, `docs/roadmap.md`
  - Done criteria: `docs/architecture.md` records that the application is given
    its world rather than fetching it, because that is a shape change and the
    file is about shape. `docs/planned-changes.md`'s section *"The window's
    views are methods, and that is why they cannot be tested"* is **corrected
    in place with the correction visible** — it is now half wrong, and this
    repository has just had two lessons about what a stale note costs.
    `README.md` needs nothing: no user-facing behaviour changes.
  - Verification: the corrected section names what changed and what is still true

- [x] 4.4 Close-out review per `AGENTS.md` [dispatch: main]
      - Done in `main` (2026-08-21). The five answers:

      **1. Did we build what was asked, or what was convenient?** What was
      asked, and it turned out smaller than the proposal feared because most of
      the seam already existed. **Three departures, each forced by evidence and
      each written into the document it departs from:** task 1.1 opened two
      seams instead of four, because clippy refused to let `pub(crate)` traits
      leak and the honest follow-up question was whether the seam needed them
      at all (it did not); task 3.3 did **not** tick `XONHO-0009` 6.3, because
      reading 6.3 rather than remembering it shows it asks for every state
      judged against the design language, which is the owner's work; and
      `Diagnostics::without_a_log` was added, which no task named — the window
      cannot be constructed without a diagnostics handle and `start()` opens a
      real log file.

      **2. Do the reader-facing documents still tell the truth?** Yes, and one
      had stopped: a section of `planned-changes.md` titled *"...and that is
      why they cannot be tested"*. Corrected in place. `architecture.md` and
      `roadmap.md` updated here rather than after.

      **3. Did we leave rubbish?** No. Clippy is clean at `-D warnings`, which
      is what would have caught an unused `World::scripted`. Both new
      dev-dependency lines are load-bearing — remove either and a test stops
      compiling. `deny.toml` and the audit job from `XONHO-0017` still pass
      with the two added features. Nothing was left commented out and no
      `TODO` was added.

      **4. What is asserted but not verified?**
      - **The screenshot has no real text in it.** `NoopTextSystem` answers
        `FontId(1)` for every font and returns fabricated metrics — read from
        `gpui/src/platform.rs:1090`, not assumed — so the image proves layout,
        colour and that something was drawn, and proves nothing about type. A
        real text system means another dependency (`gpui_macos::MacTextSystem`
        or `gpui_wgpu`), and that is a decision about the `ADR-0001` stack
        rather than a detail. **It is also the strongest argument for 6.3
        staying open**: an image with no text cannot be judged against
        `docs/design-language.md`.
      - **The screenshot proves one platform of two.** macOS only, by
        construction. Windows — this project's primary daily driver — has no
        equivalent, and never will while the renderer is Metal-only.
      - **`main.rs` moved and no test covers it.** The application was run, it
        stayed up, it stopped cleanly and it logged nothing (which is what a
        clean start does). Nobody looked at the window.
      - ~~**That `gpui_macos/test-support` is inert on Windows is read from a
        `Cargo.toml`, not observed.**~~ **Observed.**
        `cargo tree -p caixonho-gui --target x86_64-pc-windows-msvc --edges all`
        contains `gpui_macos` **zero** times; the same command for
        `aarch64-apple-darwin` contains it seven. Feature resolution per target
        needs no toolchain, so this was checkable here rather than only in CI.
        What remains for CI is whether the Windows build *compiles and links*
        with the test module `cfg`-ed out — resolution and compilation are
        different questions.
      - **`World::scripted` has one shape.** Every window test written after
        this one inherits its choices — no profiles, no stored connections, one
        store double. A test needing otherwise will have to say so, and the
        first one to need it will find out whether the helper bends.

      **5. What is left, and where is it written?**
      - Task 4.2, CI — the only task left here.
      - `XONHO-0009` 6.3: capturing every state and judging it. The dated note
        on that task says what changed and what remains.
      - Real glyphs in a captured image, which is a dependency decision — in
        answer 4 above.
      - *Give the views inputs*, still worth doing for its own reasons and no
        longer the only road to testing them — recorded in the corrected
        section of `docs/planned-changes.md`.
  - Paths: this file
  - Done criteria: the five questions answered in writing, including what is
    asserted but not verified — at minimum, that `main.rs` moved and no test
    covers it, and that the screenshot proves one platform of two
  - Verification: the answers exist and name specifics, not reassurances
