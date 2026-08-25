//! The pieces more than one view needs, so a card is a card everywhere.
//!
//! Sizes come from `theme::space` and `theme::tile`; colours come from the
//! theme. Neither is written inline here — that is the habit this module
//! exists to break.

use gpui::{
    AnyElement, App, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels, SharedString,
    Styled, div, px,
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
        .debug_selector(|| "empty-state".into())
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
        .debug_selector(|| "inline-message".into())
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
                .debug_selector(|| "inline-message-body".into())
                .flex_1()
                // `min_w_0`, and it is load-bearing: a flex child will not
                // shrink below its own text by default, so a long cause —
                // one naming a connection, say — pushed this column past the
                // 560px cap above and the sentence ran out through the
                // panel's own border. `refusal_line` learned this once and
                // this shared component had not.
                .min_w_0()
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Render, TestAppContext, Window};

    /// Renders the empty state the way `contents()` does, varying only the
    /// parent, because the parent is what decides whether it is visible.
    #[derive(Clone, Copy, Debug)]
    enum Parent {
        /// What `contents()` does today.
        Flex,
        /// A bare div, no flex properties at all.
        BareDiv,
        /// A div that grows, but is not itself a flex container.
        GrowingDiv,
    }

    struct Harness {
        parent: Parent,
    }

    impl Render for Harness {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let body = empty_state(
                IconName::Folder,
                "This folder is empty.",
                "It was read successfully — there is simply nothing in it.",
                cx,
            );
            let parent = match self.parent {
                Parent::Flex => v_flex().flex_1().min_h_0().child(body).into_any_element(),
                Parent::BareDiv => div().child(body).into_any_element(),
                Parent::GrowingDiv => div().flex_1().min_h_0().child(body).into_any_element(),
            };

            v_flex().size_full().child(parent)
        }
    }

    fn draw(parent: Parent, cx: &mut TestAppContext) -> gpui::Bounds<Pixels> {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(move |_, _| Harness { parent });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.debug_bounds("empty-state")
            .expect("the empty state was never laid out at all")
    }

    /// The panel's own border must contain its text.
    ///
    /// Reported by the owner on 2026-08-25 with a screenshot: the border cut
    /// straight through the sentence. A flex child does not shrink below its
    /// own text by default, so a long cause pushed the body column past the
    /// panel's `max_w` and the words carried on outside it. `refusal_line`
    /// had already learned this; this shared component had not.
    ///
    /// Measured rather than eyeballed — the panel's bounds alone would not
    /// have caught it, since `max_w` clamps the panel while the text
    /// overflows *out of* it.
    #[gpui::test]
    fn a_long_cause_stays_inside_the_panel(cx: &mut TestAppContext) {
        struct Long;
        impl Render for Long {
            fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                v_flex().size_full().child(inline_message(
                    IconName::TriangleAlert,
                    // The owner's actual error, which is what overflowed.
                    "the credential store refused the request — allow caixonho to use it \
                     and try again (connection `a-connection-with-a-name`)",
                    "The system keychain did not hand back the secret for \
                     `a-connection-with-a-name`. If a prompt appeared, it may have been \
                     declined.",
                    cx.theme().danger,
                    div().child("Retry"),
                    cx,
                ))
            }
        }

        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|_, _| Long);
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let panel = cx
            .debug_bounds("inline-message")
            .expect("the panel was laid out");
        let body = cx
            .debug_bounds("inline-message-body")
            .expect("the body was laid out");

        assert!(
            body.right() <= panel.right(),
            "the sentence runs out through the panel's border: body ends at {:?}, \
             panel ends at {:?}",
            body.right(),
            panel.right()
        );
        assert!(
            panel.size.width <= px(560.),
            "the panel outgrew its own cap: {:?}",
            panel.size.width
        );
    }

    #[gpui::test]
    fn empty_state_fills_a_flex_parent(cx: &mut TestAppContext) {
        let bounds = draw(Parent::Flex, cx);
        assert!(
            bounds.size.height > px(0.) && bounds.size.width > px(0.),
            "the empty state should occupy its parent, got {:?}",
            bounds.size
        );
    }

    #[gpui::test]
    fn a_bare_div_parent_shrinks_the_empty_state_to_its_own_content(cx: &mut TestAppContext) {
        // The trap behind the 2026-08-20 defect, measured rather than
        // described: `size_full` resolves against a parent that is a flex
        // container with a height. A bare `div` in between is not one, so the
        // state stops filling and sits at its content height — 174px against
        // a 1080px window, which is what "floating in the middle of nothing"
        // actually is. A div that *grows* is enough; it need not be a flex
        // container itself.
        let flex = draw(Parent::Flex, cx);
        let bare = draw(Parent::BareDiv, cx);
        let growing = draw(Parent::GrowingDiv, cx);

        assert!(
            bare.size.height < flex.size.height / 2.,
            "a bare div should leave the state at content height: {:?} vs {:?}",
            bare.size,
            flex.size
        );
        assert_eq!(growing.size, flex.size);
    }
}
