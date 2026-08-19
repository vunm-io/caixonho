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
use gpui_component::{ActiveTheme, Theme, ThemeRegistry};

/// Compiled in rather than read from disk: the brief asks for one
/// self-contained binary per platform, and a theme that can go missing is a
/// window that can open unstyled.
const THEME: &str = include_str!("../assets/theme.json");

/// Install the caixonho themes over the toolkit's defaults.
///
/// Every colour the document omits is inherited, so this names only what
/// differs — the brand ramp — and leaves the neutral surfaces alone.
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
    // Both modes are applied so each is stored against its own mode; the last
    // one applied also sets the mode, which the change below corrects.
    let mode = cx.theme().mode;
    for config in ours {
        Theme::global_mut(cx).apply_config(&config);
    }
    Theme::change(mode, None, cx);
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
