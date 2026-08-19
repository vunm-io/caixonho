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
- [ ] 4.4 [dispatch: main] Check every state against a profile that fails, one
      that is denied, and one that succeeds.

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
- [ ] 6.2 [dispatch: main] CI green on both targets.
- [ ] 6.3 [dispatch: main] Screenshots of every state for the owner to judge
      against `docs/design-language.md`, and the blurred-window comparison the
      design deferred.
