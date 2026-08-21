## 1. Make room

- [x] 1.1 [dispatch: main] Split `caixonho-gui` into `main.rs` (window and
      entry), `app.rs` (state and messages), `views/` and `components/`, moving
      code without changing it. Commit this before any restyling, so the move
      and the redesign are reviewable apart.
- [x] 1.2 [dispatch: main] `cargo fmt --all`, clippy and tests green on the
      move alone.

## 2. Theme and tokens

- [x] 2.1 [dispatch: main] Ship a theme document with the reference's accent —
      `violet-600` light, `violet-400` dark, `violet-800` for pressed and deep —
      and load it at startup through `ThemeRegistry::load_themes_from_str`.
- [x] 2.2 [dispatch: main] Add `tokens.rs` for what the theme does not carry:
      the spacing scale, icon-tile sizes, and the three tinted shadows from
      `docs/design-language.md`. Read radius and colour from the theme, never
      from a local copy.
      - Landed in `theme.rs` beside the theme installation rather than a file of
        its own, and carries only the shadow level this app actually casts. The
        reference's heavier two are kept in the design language until a surface
        needs them, rather than shipped as unused API.
- [x] 2.3 [dispatch: main] Remove every hard-coded colour and every loose `px()`
      from rendering code; a grep for both is the check.

## 3. The shell

- [x] 3.1 [dispatch: main] Build the sidebar from `gpui-component`'s
      `sidebar/`: a header, a `CONNECTIONS` group with one row per profile —
      icon tile, name, active state — and a footer.
- [x] 3.2 [dispatch: main] Move profile switching out of the title bar into
      those rows, keeping the existing switch behaviour exactly.
- [x] 3.3 [dispatch: main] Add the status bar and move the counts into it,
      keeping "N of M buckets" while a region is chosen.
- [x] 3.4 [dispatch: main] Lay the content area out with the window margin from
      the tokens, so the table stops touching the window chrome.

## 4. The four states

- [x] 4.1 [dispatch: main] Loading: skeleton rows in the shape of the bucket
      table, not a line of text.
- [x] 4.2 [dispatch: main] Empty: icon tile, headline, one sentence — for an
      account that genuinely holds no buckets.
- [x] 4.3 [dispatch: main] Error: an inline message carrying the cause and one
      action, sized to its content. The full-width Retry button goes.
- [x] 4.4 [dispatch: main] Check every state against a profile that fails, one
      that is denied, and one that succeeds.
      - **Succeeds**: a stored connection listed its account and the rows
        rendered as designed.
      - **Fails**: a stored connection whose secret the credential store
        refused produced the inline error carrying the cause and one action,
        and the log recorded the same cause — `connection refused … the
        credential store refused the request`.
      - **Denied**: done at last, and by a real account rather than a rig.
        Roughly two dozen buckets came back from one credential, most of them
        refused and a minority openable, and the refused ones carried the
        badge while the openable ones carried none. **This is the first time
        the headline feature of §4.3 has been seen against anything real** —
        every earlier sighting was a test double. It reads correctly: the
        cause is a genuine authorisation boundary, nothing is misreported as
        denied, and no bucket vanished from the list for being refused.
      - The account is not this project's to describe, so no names, regions or
        counts from it belong in this repository. What is recorded here is
        the behaviour, which is the part that is ours.
      - **One defect the real data exposed, invisible on a list of three test
        buckets**: when most rows are refused, the few that are not are buried
        among them, and the list offers no way to bring them together. The
        status vocabulary is right; what is missing is anything that acts on
        it. Recorded in `docs/planned-changes.md`.

## 5. Status vocabulary

- [x] 5.1 [dispatch: main] One function from `Access` to glyph, label, tint and
      whether the row dims.
- [x] 5.2 [dispatch: main] Render it: no badge when a bucket can be entered, a
      lock badge tinted from `danger` when it cannot, a skeleton while probing,
      an em dash when nothing is known.
- [x] 5.3 [dispatch: main] Keep the cause and the IAM action reachable on the
      refused row.
      - The glyph is `CircleX`, not the lock `PROJECT_BRIEF.md` §4.3 asks for:
        the toolkit's icon set has no lock, and adding one needs an asset source
        of the app's own. Recorded as a departure in `docs/design-language.md`.

## 6. Close-out

- [x] 6.1 [dispatch: main] `cargo fmt --all`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo test --workspace` green.
- [x] 6.2 [dispatch: main] CI green on both targets.
      - Run `32333149778` on `e57015c`: `build (windows-latest)` success,
        `build (macos-latest)` success, `rustfmt` success. The shell commits
        were green when pushed; `32322529073` is the most recent of them.
- [ ] 6.3 [dispatch: main] Screenshots of every state for the owner to judge
      against `docs/design-language.md`, and the blurred-window comparison the
      design deferred.
      - **2026-08-21: what blocks this, checked rather than assumed.** The
        obvious automation exists and is not the obstacle. `gpui` really does
        render offscreen — `HeadlessAppContext::capture_screenshot` calls
        `Window::render_to_image` (`gpui/src/window.rs:2430`) and returns an
        `RgbaImage`, and `gpui_platform::current_headless_renderer()` supplies
        a Metal renderer for it. Two conditions come with it: the factory is
        behind `gpui_platform`'s `test-support` feature, which this workspace
        does not enable, and it answers `Some` only on macOS — `None`
        everywhere else, so it can never be a gate on both CI targets.
      - Neither of those is the blocker. The blocker is that a screenshot is
        only worth judging if it is of the **real** view, and there is no seam
        for building one in a test yet. A test that reconstructs a fragment
        photographs its own reconstruction: that was measured on 2026-08-20,
        when a rebuilt fragment failed to reproduce the white-window defect it
        was written for. **That seam is `XONHO-0015`'s subject** — the number
        is issued and anchored, but no change is planned under it yet.
      - So this task waits on `XONHO-0015`, or it is done by hand: the owner
        runs the application and captures the states themselves. Nothing here
        needs deciding again — the capability inventory is in
        `vunm-knowledge-base/topics/gpui-capabilities.md` §Hạ tầng test.
