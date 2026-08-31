//! The app's own theme, and the tokens the toolkit's theme does not carry.
//!
//! What lives where was read out of `gpui-component` rather than assumed
//! (`docs/design-language.md` records the reasoning):
//!
//! - **Colour, radius and typography belong to the theme.** They are declared
//!   in `assets/theme.json` and installed at startup. A component asks the
//!   theme for `primary`; it never names a colour.
//! - **Spacing, tile sizes and shadows belong here.**
//!   `Theme::apply_semantic_config` hands its spacing and shadow tokens back to
//!   the caller for application-owned UI rather than storing them, and a
//!   shadow tinted with its own element's colour cannot be one global value
//!   anyway.

use gpui::{App, BoxShadow, Hsla, Pixels, point, px};
use gpui_component::{Theme, ThemeMode, ThemeRegistry};

/// Compiled in rather than read from disk: the brief asks for one
/// self-contained binary per platform, and a theme that can go missing is a
/// window that can open unstyled.
const THEME: &str = include_str!("../assets/theme.json");

/// The families the design system names, embedded for the same reason the
/// theme is (`XONHO-0032`).
///
/// Baloo 2 for display — titles, labels, the text on a button — and Be Vietnam
/// Pro for anything read at length. The system forbids mixing the two inside
/// one block of text.
///
/// **Static instances, not the variable file**, and that was measured rather
/// than assumed. `google/fonts` ships Baloo 2 as one variable `Baloo2[wght]`,
/// and gpui — selecting a weight through font-kit — never reaches its `wght`
/// axis: the same glyph came back **28.864px wide at both 400 and 800**. A
/// display family that cannot go bold is not a display family, and the whole
/// point of Baloo 2 here is the heavy cut.
///
/// So three static cuts, from the same family, served per weight by the Google
/// Fonts CSS API. `the_display_font_actually_changes_shape_with_weight` is the
/// measurement, and it stays: the day gpui learns variable axes, it still
/// passes, and the day a font swap breaks weight selection it fails.
///
/// Both families are SIL Open Font License; `assets/fonts/OFL-*.txt` travel
/// with them, which is not optional in a public repository that ships the
/// glyphs.
const BALOO_REGULAR: &[u8] = include_bytes!("../assets/fonts/Baloo2-Regular.ttf");
const BALOO_BOLD: &[u8] = include_bytes!("../assets/fonts/Baloo2-Bold.ttf");
const BALOO_EXTRABOLD: &[u8] = include_bytes!("../assets/fonts/Baloo2-ExtraBold.ttf");
const BE_VIETNAM_REGULAR: &[u8] = include_bytes!("../assets/fonts/BeVietnamPro-Regular.ttf");
const BE_VIETNAM_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/BeVietnamPro-SemiBold.ttf");
const BE_VIETNAM_BOLD: &[u8] = include_bytes!("../assets/fonts/BeVietnamPro-Bold.ttf");

/// What the design system calls display type: titles, labels, button text.
pub(crate) const FONT_DISPLAY: &str = "Baloo 2";
/// And what it calls body type: anything read at length.
pub(crate) const FONT_BODY: &str = "Be Vietnam Pro";

/// Hand the platform the families this application draws with.
///
/// Before the theme, because a theme naming a family the text system has never
/// heard of falls back silently — and a silent fallback is how a window ships
/// looking almost right.
pub(crate) fn load_fonts(cx: &mut App) {
    if let Err(error) = cx.text_system().add_fonts(vec![
        std::borrow::Cow::Borrowed(BALOO_REGULAR),
        std::borrow::Cow::Borrowed(BALOO_BOLD),
        std::borrow::Cow::Borrowed(BALOO_EXTRABOLD),
        std::borrow::Cow::Borrowed(BE_VIETNAM_REGULAR),
        std::borrow::Cow::Borrowed(BE_VIETNAM_SEMIBOLD),
        std::borrow::Cow::Borrowed(BE_VIETNAM_BOLD),
    ]) {
        // Not a reason to refuse to open, for `install`'s reason: the
        // platform's own families are a working fallback, and a window nobody
        // can open explains nothing.
        eprintln!("caixonho: could not load its fonts, falling back: {error}");
    }
}

/// Install the caixonho theme over the toolkit's defaults.
///
/// **One theme, and it is light** (`XONHO-0032`). Đất Nặn defines a single
/// light palette; its own notes record dark as a decision the owner deferred,
/// and building one here from values the system has never specified would be
/// this project inventing brand. It comes back through the design system, not
/// through this file.
pub(crate) fn install(cx: &mut App) {
    if let Err(error) = ThemeRegistry::global_mut(cx).load_themes_from_str(THEME) {
        // A malformed theme is a styling problem, not a reason to refuse to
        // open: the toolkit's default is a working theme.
        eprintln!("caixonho: could not load its theme, falling back: {error}");
        return;
    }

    let ours: Vec<_> = ThemeRegistry::global(cx)
        .themes()
        .values()
        .filter(|config| config.name.starts_with("caixonho"))
        .cloned()
        .collect();
    for config in ours {
        Theme::global_mut(cx).apply_config(&config);
    }
    // Light, explicitly, rather than following whatever the OS reports. There
    // is no dark palette to follow it into, and a window that honours a dark
    // preference by showing light-theme colours on a dark-theme *mode* is the
    // worst of both.
    Theme::change(ThemeMode::Light, None, cx);
}

/// Spacing, named for what it separates rather than by t-shirt size.
///
/// A name that says where a value belongs is harder to misuse than one that
/// says how big it is: the first pass had loose numbers written wherever an
/// element happened to be added, which is how nothing came to share a scale.
pub(crate) mod space {
    use super::{Pixels, px};

    /// A title and the line under it.
    pub(crate) const TIGHT: Pixels = px(4.);
    /// An icon and the text it labels.
    pub(crate) const INLINE: Pixels = px(8.);
    /// Between the parts of one row.
    pub(crate) const ROW: Pixels = px(12.);
    /// Inside a card, an inline message, a tile.
    pub(crate) const CARD: Pixels = px(14.);
    /// Between the window and what it holds.
    pub(crate) const WINDOW: Pixels = px(16.);
    /// Inside something standing alone in an empty area.
    pub(crate) const SECTION: Pixels = px(24.);
}

/// The rounded square that carries a glyph.
///
/// The most recognisable element of the reference, and the cheapest way to
/// make a list read as designed rather than dumped. The radius rule is the
/// reference's own.
pub(crate) mod tile {
    use super::{Pixels, px};

    /// Beside a message.
    pub(crate) const SM: Pixels = px(34.);
    /// Standing alone in an empty area.
    pub(crate) const LG: Pixels = px(58.);

    /// A tile's corner radius: proportional until it would look like a pill.
    pub(crate) fn radius(size: Pixels) -> Pixels {
        px((f32::from(size) / 3.).min(14.))
    }
}

/// Elevation.
///
/// One rule carries the effect: **a shadow is tinted with the colour of the
/// thing casting it, never black.** The scale fixes blur, offset and opacity;
/// the colour arrives from the element.
///
/// The blur figures are doubled from the reference's, because SwiftUI's shadow
/// radius is roughly half a CSS-style blur radius. They are a starting point to
/// be trimmed against a screenshot, not a settled conversion.
///
/// Only the level this app actually casts lives here. The reference's heavier
/// two arrive with the surfaces that need them rather than sitting unused —
/// `docs/design-language.md` keeps their values.
pub(crate) mod shadow {
    use super::{BoxShadow, Hsla, point, px};

    /// A chip or a small tile.
    pub(crate) fn chip(color: Hsla) -> Vec<BoxShadow> {
        vec![tinted(color, 0.18, 2., 8.)]
    }

    fn tinted(color: Hsla, alpha: f32, y: f32, blur: f32) -> BoxShadow {
        BoxShadow {
            color: color.opacity(alpha),
            offset: point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(0.),
            inset: false,
        }
    }
}

/// The clay tier: what a thing you press is made of (`XONHO-0032`).
///
/// Đất Nặn divides every surface in two and states the rule — *if you press
/// it, it is clay; if you read it for a long time, it is flat.* This is the
/// first half. The second half is everywhere else, and it is the default: no
/// inset, a thin line, a light drop shadow.
///
/// The DNA is **two inset shadows** — light from the top-left, dark from the
/// bottom-right — which is what makes a surface read as pressed out of a
/// material rather than drawn. gpui paints inset shadows, so this is the one
/// signature of the system that survives the port intact.
pub(crate) mod clay {
    use super::{BoxShadow, Pixels, point, px};
    use gpui::{Hsla, hsla};

    /// The light that falls on a lump of clay from above-left.
    /// `--clay-hi`, `rgba(255, 252, 240, .9)`.
    fn highlight() -> Hsla {
        hsla(45. / 360., 1.0, 0.971, 0.9)
    }

    /// And the shade it throws below-right. `--clay-lo`,
    /// `rgba(150, 120, 70, .38)`.
    fn shade() -> Hsla {
        hsla(37.5 / 360., 0.364, 0.431, 0.38)
    }

    /// The warm drop the whole piece sits in. `--drop-sm`,
    /// `0 8px 16px rgba(90, 80, 50, .16)` — warm, never blue-grey.
    fn drop() -> Hsla {
        hsla(45. / 360., 0.286, 0.275, 0.16)
    }

    fn inset(x: f32, y: f32, blur: f32, color: Hsla) -> BoxShadow {
        BoxShadow {
            color,
            offset: point(px(x), px(y)),
            blur_radius: px(blur),
            spread_radius: px(0.),
            inset: true,
        }
    }

    /// `--clay-inset-sm` plus `--drop-sm`: a button, a chip, a badge.
    ///
    /// The system's `--elev-button`, in the order CSS writes it — the two
    /// insets first, the drop behind them.
    pub(crate) fn button() -> Vec<BoxShadow> {
        vec![
            inset(3., 4., 5., highlight()),
            inset(-3., -5., 7., shade()),
            BoxShadow {
                color: drop(),
                offset: point(px(0.), px(8.)),
                blur_radius: px(16.),
                spread_radius: px(0.),
                inset: false,
            },
        ]
    }

    /// The four corners of a lump nobody rolled perfectly round.
    ///
    /// Đất Nặn's `--blob-*` are **elliptical percentages**, and gpui's corners
    /// are four `Pixels` — so this is the nearest honest thing rather than the
    /// same thing: four different radii around one control, so no two corners
    /// agree. It reads as hand-made at a glance and it is not a blob.
    /// `docs/design-language.md` records the difference.
    pub(crate) const CORNERS: [Pixels; 4] = [px(14.), px(11.), px(13.), px(10.)];
}

/// Give anything that can be styled the clay treatment.
///
/// An extension trait rather than a helper function per call site: every
/// button in this window should be the same material, and a rule applied by
/// remembering to apply it is a rule with exceptions nobody chose.
pub(crate) trait Clay: gpui::Styled + Sized {
    fn clay(self) -> Self {
        let [tl, tr, br, bl] = clay::CORNERS;
        self.shadow(clay::button())
            .rounded_tl(tl)
            .rounded_tr(tr)
            .rounded_br(br)
            .rounded_bl(bl)
    }
}

impl<T: gpui::Styled + Sized> Clay for T {}

/// A quiet button: no surface until the pointer is on it, and then a wash.
///
/// **Why not `ghost`.** The toolkit computes a ghost's hover as
/// `secondary.darken(0.1).opacity(0.8)`, and `darken` multiplies lightness —
/// so with `secondary` at `#EAEEE7` the hover lands on `#DBDED9`, a solid
/// block that reads as a *different kind of button* rather than as a pointer
/// resting on one. Lightening `secondary` barely moves it: at `#F7F9F6` it is
/// `#E5E7E4`, and even pure white gives `#EBEBEB` — a dead grey outside the
/// palette. The formula cannot produce a wash, whatever it is fed.
///
/// So the hover is stated instead of derived. `rgba(95, 110, 98, .07)` is the
/// value Đất Nặn's own desktop kit uses for a hover on an app surface, which
/// is exactly this situation.
///
/// The foreground is `--ink-2`, the body ink — a quiet button is a secondary
/// action and the system says it wears its tone's **ink**, not a fill.
pub(crate) fn quiet(cx: &App) -> gpui_component::button::ButtonCustomVariant {
    use gpui_component::ActiveTheme as _;
    gpui_component::button::ButtonCustomVariant::new(cx)
        .color(gpui::transparent_black())
        .foreground(cx.theme().foreground)
        .hover(gpui::hsla(133. / 360., 0.073, 0.402, 0.07))
        .active(gpui::hsla(133. / 360., 0.073, 0.402, 0.12))
}
