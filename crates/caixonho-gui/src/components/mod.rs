use gpui::{Context, IntoElement, ParentElement, SharedString, Styled, div};
use gpui_component::{ActiveTheme, v_flex};

use crate::app::CaixonhoApp;

/// A centred message with an optional explanation under it.
pub(crate) fn notice(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    cx: &mut Context<CaixonhoApp>,
) -> impl IntoElement {
    let detail = detail.into();
    let mut notice = v_flex().gap_1().p_4().child(div().child(title.into()));
    if !detail.is_empty() {
        notice = notice.child(div().text_color(cx.theme().muted_foreground).child(detail));
    }
    notice
}
