//! The pieces more than one view needs, so a card is a card everywhere.
//!
//! Sizes come from `theme::space` and `theme::tile`; colours come from the
//! theme. Neither is written inline here — that is the habit this module
//! exists to break.

use gpui::{
    AnyElement, App, Hsla, IntoElement, ParentElement, Pixels, SharedString, Styled, div, px,
};
use gpui_component::{ActiveTheme, Icon, IconName, h_flex, skeleton::Skeleton, v_flex};

use crate::theme::{shadow, space, tile};

/// The rounded square that carries a glyph.
///
/// Tinted at 14% of its own colour, or filled when it is the subject of its
/// row. It casts a shadow in that same colour — the rule that carries most of
/// the reference's depth.
pub(crate) fn icon_tile(icon: IconName, size: Pixels, tint: Hsla, filled: bool) -> AnyElement {
    let (background, foreground) = if filled {
        (tint, gpui::white())
    } else {
        (tint.opacity(0.14), tint)
    };

    div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(size)
        .h(size)
        .rounded(tile::radius(size))
        .bg(background)
        .text_color(foreground)
        .shadow(shadow::chip(tint))
        .child(Icon::new(icon))
        .into_any_element()
}

/// A short state, said as a glyph and a word in a capsule.
///
/// Never a bare word in the default text colour: that is what the first pass
/// shipped, and it reads as data rather than as a state.
pub(crate) fn status_badge(
    icon: IconName,
    label: impl Into<SharedString>,
    tint: Hsla,
) -> AnyElement {
    h_flex()
        .gap(px(5.))
        .items_center()
        .flex_shrink_0()
        .px_2()
        .py(px(4.))
        .rounded_full()
        .bg(tint.opacity(0.12))
        .text_color(tint)
        .text_xs()
        .child(Icon::new(icon).size_3())
        .child(label.into())
        .into_any_element()
}

/// What a view shows when there is genuinely nothing to show.
pub(crate) fn empty_state(
    icon: IconName,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    cx: &App,
) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap(space::ROW)
        .p(space::SECTION)
        .child(icon_tile(
            icon,
            tile::LG,
            cx.theme().muted_foreground,
            false,
        ))
        .child(
            v_flex()
                .items_center()
                .gap(space::TIGHT)
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(title.into()),
                )
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(message.into()),
                ),
        )
        .into_any_element()
}

/// A cause and what to do about it, sized to its content.
///
/// Returns the message with room for one action, because an error with two
/// next steps is an error that has not been classified.
pub(crate) fn inline_message(
    icon: IconName,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    tint: Hsla,
    action: impl IntoElement,
    cx: &App,
) -> AnyElement {
    h_flex()
        .items_start()
        .gap(space::ROW)
        .p(space::CARD)
        .max_w(px(560.))
        .rounded(cx.theme().radius_lg)
        .bg(cx.theme().popover)
        .border_1()
        .border_color(tint.opacity(0.3))
        .shadow(shadow::chip(tint))
        .child(icon_tile(icon, tile::SM, tint, false))
        .child(
            v_flex()
                .flex_1()
                .gap(space::TIGHT)
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(tint)
                        .child(title.into()),
                )
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(message.into()),
                )
                // A row, so the action is as wide as its label: a column
                // stretches its children and the button spans the card.
                .child(h_flex().pt(space::INLINE).child(action)),
        )
        .into_any_element()
}

/// Rows in the shape of the content that is coming.
///
/// A skeleton says "this is a table, and it is nearly here"; a line of text
/// saying "Listing…" says only that something is happening somewhere.
pub(crate) fn skeleton_rows(count: usize) -> AnyElement {
    v_flex()
        .gap(space::INLINE)
        .p(space::CARD)
        .children((0..count).map(|_| {
            h_flex()
                .gap(space::ROW)
                .child(Skeleton::new().w(px(320.)).h(px(16.)))
                .child(Skeleton::new().w(px(160.)).h(px(16.)))
                .child(Skeleton::new().w(px(120.)).h(px(16.)))
        }))
        .into_any_element()
}
