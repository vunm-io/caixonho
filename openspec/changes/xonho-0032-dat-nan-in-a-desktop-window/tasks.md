# Tasks — XONHO-0032 Đất Nặn, in a desktop window

> Every one of these is judged by **looking**, not by a test going green. A
> theme has no assertions worth writing: the harness can prove a frame was
> drawn and that two states differ, and it cannot prove the window looks right.
> So each task below ends at an image, and the images get opened.
>
> **Routing.** All `[dispatch: main]`. Two files carry almost all of it —
> `theme.json` and `theme.rs` — and the rest is judgement about a design system
> whose rules had to be read to be applied. `agy` remains this workspace's
> second-priority executor and earns nothing here.

## 1. The fonts, and the one thing that might not work

- [x] 1.1 Embed both families with their licences [dispatch: main]
  - Paths: `crates/caixonho-gui/assets/fonts/`
  - Done criteria: `Baloo2[wght].ttf`, three Be Vietnam Pro statics, and
    **both `OFL.txt` files**, from `google/fonts`. The licences are not
    optional — a binary that ships the glyphs and not the licence is
    distributing them wrongly, and this repository is public.
  - Verification: the files, and `README.md` naming them

- [x] 1.2 Load them, and find out whether the variable weight works
      [dispatch: main]
  - Paths: `crates/caixonho-gui/src/theme.rs`, `main.rs`
  - Done criteria: `text_system().add_fonts()` at startup, then **a frame with
    Baloo 2 at 400 beside Baloo 2 at 800** — and the frame is opened and
    compared.
  - **It failed, exactly as written.** The same `B` measured **28.864px at
    both 400 and 800** — gpui never reaches the variable font's `wght` axis, so
    Baloo 2 could only ever have rendered at one weight. A display family that
    cannot go bold is not a display family.
  - Fixed with three static cuts (regular / bold / extra-bold), which the
    Google Fonts CSS API serves per weight. Re-measured: **28.864 → 33.28px**.
  - The measurement stays as `the_display_font_actually_changes_shape_with_weight`
    rather than being deleted once green: the day gpui learns variable axes it
    still passes, and the day a font swap breaks weight selection it fails.
  - Verification: the measurement, and the frames
  - **Added 2026-09-05, after the week that followed.** The measurement above
    is macOS's. **Windows CI cannot make it**: `WindowsPlatform::new(headless)`
    installs `NoopTextSystem`, whose `add_fonts` returns `Ok` and does
    nothing, whose `font_id` answers `FontId(1)` for everything, and whose
    `typographic_bounds` is a constant 392/1000 em — 25.088px at 64px, for
    both families at every weight. Three commits read that number as a product
    defect (the RIBBI legacy-family limit, then "font loading is broken on
    Windows") before two unrelated typefaces agreeing to three decimals gave
    it away. The reason is now held by a test,
    `this_platforms_headless_text_system_is_a_noop`, which fails the day
    Windows headless gains a real text system — that is when to ungate the
    measurement there. One hypothesis for a *real*-Windows failure, **never
    observed**: `Baloo2-ExtraBold.ttf` and `BeVietnamPro-SemiBold.ttf`
    declare their own legacy families (`Baloo 2 ExtraBold`, `Be Vietnam Pro
    SemiBold`), since a legacy family holds only RIBBI; a platform matching on
    legacy names would not find 800 or 600. `load_fonts` now says when a
    family did not register, which its own comment warned about and nothing
    checked. **Still owed, and only a person at a Windows machine can pay
    it:** the two frames opened and looked at there. `v0.1.0-beta.2` carries
    no Đất Nặn; the first Windows artifact with it is CI run 33895255727's.

## 2. The tokens

- [x] 2.1 Colour and surface, app branch [dispatch: main]
  - Paths: `crates/caixonho-gui/assets/theme.json`
  - Done criteria: `--surface-app` `#F2F4F0` for the pane,
    `--surface-app-sidebar` `#EAEEE7`, `--surface-app-raised` `#FFFFFF`,
    `--app-line` for rules; `--text-title` forest, `--text-body` ink-2,
    `--text-muted` ink-4; accents from the clay ramp; `--status-danger`
    terracotta.
  - **Every colour paired with the ink the system names for it.** A diluted
    background needs a darker ink than its pure colour — the system says which,
    and picking a neighbour because it looks nicer is how a 13px label stops
    being readable.
  - Verification: the frames

- [x] 2.2 The dark theme goes [dispatch: main]
  - Paths: `crates/caixonho-gui/assets/theme.json`, `theme.rs`
  - Done criteria: one theme. Not a dark entry left stale — removed, and the
    mode handling with it.
  - The owner chose this: the system has no dark palette and inventing one is
    what `REPO-NOTES.md` forbids. Dark returns through Claude Design.
  - Verification: `cargo test -p caixonho-gui`, and the window opening light

- [ ] 2.3 Radius, elevation, spacing and the type scale [dispatch: main]
  - Paths: `crates/caixonho-gui/assets/theme.json`,
    `crates/caixonho-gui/src/theme.rs`
  - Done criteria: the system's radius steps; `--drop-sm/md` as warm drop
    shadows and `--clay-inset-sm/md` as the two-layer inset; the 4px spacing
    scale with `--gutter-app` 32px and `--sidebar-w` 248px; the type scale and
    its roles.
  - `space` in `theme.rs` is named for *what it separates*. Keep that — the
    system's names are sizes, and a size name is easier to misuse.
  - Verification: the frames

## 3. Clay, exactly where the kit puts it

- [ ] 3.1 The sidebar's current item [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the open bucket is a raised clay block; every other row
    flat. `AppSidebar.prompt.md`: *"Mục đang chọn là khối đất vàng nổi; các
    mục khác phẳng."*
  - Verification: the frames

- [x] 3.2 Buttons, chips and badges [dispatch: main]
  - Done criteria: inset + drop, asymmetric pixel radii. One `primary` per
    screen — the system says one, and this window has several candidates, so
    **decide which and write it here**.
  - `ghost` uses the tone's *ink* for text and border, not a fill, so it stays
    readable on the raised white surfaces — so **only filled buttons get
    clay**, which is eight of them.
  - A `Clay` extension trait rather than a helper called per site: every button
    here should be the same material, and a rule applied by remembering to
    apply it is a rule with exceptions nobody chose.
  - **The one `primary`, decided:** `Save` on the connection form. It is the
    only screen in this application with a single obvious commitment; the
    object pane's verbs all act on different things and none of them outranks
    the others, so none is primary there.
  - Verification: the frames

- [ ] 3.3 Everything else goes flat [dispatch: main]
  - Done criteria: both tables, every strip, the queue panel — thin
    `--app-line`, `--drop-sm`, **no inset**. No block inside a block.
  - Verification: the frames, looked at for nesting

- [ ] 3.4 A selected row [dispatch: main]
  - Done criteria: raised background + 1.5px `--clay-aqua` border. **Not** a
    fill, **not** lifted — the kit is explicit, and it matters more here than
    on the web: these rows can be deleted.
  - Verification: the frames

## 4. Looking at all of it

- [ ] 4.1 Regenerate every frame and open every one [dispatch: main]
  - Done criteria: all thirty-nine, plus the narrow one. **Opened, not just
    regenerated** — this change alters every frame, so a rendering defect has
    thirty-nine places to hide, and five times already in this project an image
    has caught what no assertion did.
  - Verification: what was seen, written here

- [ ] 4.2 Contrast, at the smallest size that ships [dispatch: main]
  - Done criteria: the smallest text in the window measured against its own
    background. `--ink-4` is the lightest text the system allows;
    `--ink-4-decor` is for strokes and **never** for text (2.9:1 on paper).
  - Verification: measured ratios, written here

- [ ] 4.3 `docs/design-language.md` rewritten [dispatch: main]
  - Done criteria: it describes Đất Nặn's app branch, names the system as the
    source, and **records the two signatures that did not survive** — blob
    radii and the squish — so a reader comparing the window to the system finds
    the difference explained rather than assumed to be a defect.
  - Verification: read against the system's own `readme.md`

## 5. Close-out

- [ ] 5.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - And the macOS-gate walk, which `XONHO-0030` learned to do the hard way.
  - Verification: the commands

- [ ] 5.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 5.3 Live: open it and look [dispatch: main]
  - Done criteria: the owner's own machine, a real connection. Fonts render,
    the sidebar's current bucket reads as the current one, nothing is a block
    inside a block, and the delete button still reads as dangerous at
    terracotta.
  - That last one is the only judgement here that a frame cannot settle,
    because it is about how a colour *feels* in the moment before someone
    presses it.
  - Verification: what was seen

- [ ] 5.4 Close-out review per `AGENTS.md` [dispatch: main]
  - Question 1 has a sharp form here — *did we build what was asked, or what
    was convenient?* Every departure from the system is either something the
    toolkit cannot do, or a defect. There is no third kind.
  - Verification: the recorded findings
