## Why

The M1 screens were judged unfinished, and the judgement was right. Sizes were
written inline wherever each element happened to be added, so nothing shares a
scale; status is rendered as bare words in the default colour; the profile
switcher is loose buttons in the title bar; and the error state is red text
above a button stretched across the window.

The cause is not taste applied badly, it is that there was no foundation to
apply it to — and `caixonho-gui` reached for a bare `div` while the toolkit it
already depends on ships a sidebar, a status bar, badges, alerts, skeletons and
a theme carrying every semantic colour this app needs.

Doing this before `XONHO-0006` is the point. Browsing objects means a second
table, a breadcrumb and a detail surface; built against no shell they land the
same way the first screens did, and cost twice to correct afterwards.

## What Changes

- The window gains a shell: a sidebar holding profiles, a content area, and a
  status bar. Profiles leave the title bar, and the counts leave it too.
- The app ships a theme rather than naming colours in components. The accent is
  the reference's, which is Tailwind's violet ramp, already present in the
  toolkit's palette.
- Elevation arrives as a rule rather than a decoration: a shadow is tinted with
  its element's own colour, never black.
- Status becomes a vocabulary — a glyph, a short label and a colour. A bucket
  that cannot be entered gets the lock badge `PROJECT_BRIEF.md` §4.3 asked for
  and the first pass did not deliver.
- Loading, empty and error stop being improvised: skeleton rows in the shape of
  what is coming, an empty state that says why it is empty, and an inline error
  sized to its content.
- `caixonho-gui` gains a module structure. `main.rs` currently holds the window,
  the app state, the table delegate, every rendering function and `main()` in
  723 lines.

No behaviour contract changes. Every requirement in `connections` and
`bucket-listing` stays satisfied exactly as written — the spec already says a
bucket that cannot be entered is "dimmed, or grouped apart" and states its cause
on request, and this delivers that better rather than differently. The change
therefore declares `skip_specs`, and is held instead to
[`docs/design-language.md`](../../../docs/design-language.md), which is binding
on UI work.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. This changes how the existing contracts are presented, not what they
promise.

## Impact

- `caixonho-gui`: split into modules; a theme document loaded at startup; a
  token module for what the theme does not carry; components for badge, empty
  state, error and skeleton; views for the sidebar and the bucket table.
- `caixonho-core`: unchanged. If a rendering need reaches for core, that is the
  signal the boundary is being crossed and the change is wrong.
- No new dependencies: the sidebar, status bar, badge, alert and skeleton all
  come from `gpui-component`, which is already a dependency.
