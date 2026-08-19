//! Entering a credential, so that connecting does not require editing a file
//! by hand or installing another tool first.

use caixonho_core::{CredentialSecret, StoredCredential};
use gpui::{
    AnyElement, App, AppContext, Entity, IntoElement, ParentElement, SharedString, Styled, Window,
    div, px,
};
use gpui_component::{
    ActiveTheme, IconName,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    v_flex,
};

use crate::components::{icon_tile, inline_message};
use crate::theme::{space, tile};

/// What the user is typing, and what is wrong with it so far.
pub(crate) struct CredentialForm {
    name: Entity<InputState>,
    region: Entity<InputState>,
    access_key_id: Entity<InputState>,
    secret: Entity<InputState>,
    session_token: Entity<InputState>,
    /// Set when saving was attempted and refused — by us for being incomplete,
    /// or by the credential store.
    pub(crate) problem: Option<SharedString>,
    /// True while the store is being written, so the form cannot be submitted
    /// twice and the button can say what is happening.
    pub(crate) saving: bool,
}

impl CredentialForm {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let mut field = |placeholder: &'static str, masked: bool, cx: &mut App| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(masked)
            })
        };
        Self {
            name: field("production", false, cx),
            region: field("ap-southeast-1", false, cx),
            access_key_id: field("AKIA…", false, cx),
            secret: field("the secret access key", true, cx),
            session_token: field("only for temporary credentials", true, cx),
            problem: None,
            saving: false,
        }
    }

    /// What was typed, if it is enough to connect with.
    ///
    /// The check is deliberately shallow: an access key's shape is AWS's to
    /// judge, and guessing at it here would reject credentials that work. What
    /// cannot be guessed at is a field left empty.
    pub(crate) fn entered(
        &self,
        cx: &App,
    ) -> Result<(StoredCredential, CredentialSecret), SharedString> {
        let read = |field: &Entity<InputState>| field.read(cx).value().trim().to_owned();
        let (name, region) = (read(&self.name), read(&self.region));
        let (access_key_id, secret) = (read(&self.access_key_id), read(&self.secret));
        let session_token = read(&self.session_token);

        for (value, missing) in [
            (&name, "a name for the connection"),
            (&region, "a region"),
            (&access_key_id, "an access key id"),
            (&secret, "a secret access key"),
        ] {
            if value.is_empty() {
                return Err(format!("This needs {missing}.").into());
            }
        }

        Ok((
            StoredCredential::new(name, region, access_key_id),
            CredentialSecret::new(secret, (!session_token.is_empty()).then_some(session_token)),
        ))
    }

    pub(crate) fn render(
        &self,
        save: impl Fn(&mut Window, &mut App) + 'static,
        cancel: impl Fn(&mut Window, &mut App) + 'static,
        cx: &App,
    ) -> AnyElement {
        let saving = self.saving;
        v_flex()
            .max_w(px(520.))
            .gap(space::ROW)
            .p(space::CARD)
            .rounded(cx.theme().radius_lg)
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .child(
                gpui_component::h_flex()
                    .gap(space::ROW)
                    .child(icon_tile(
                        IconName::Plus,
                        tile::SM,
                        cx.theme().primary,
                        false,
                    ))
                    .child(
                        v_flex()
                            .gap(space::TIGHT)
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Add a connection"),
                            )
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .text_xs()
                                    .child(
                                        "The secret is kept in this system's keychain. It is \
                                         never written to a file, a log, or ~/.aws.",
                                    ),
                            ),
                    ),
            )
            .child(field("Name", &self.name, cx))
            .child(field("Region", &self.region, cx))
            .child(field("Access key ID", &self.access_key_id, cx))
            .child(field("Secret access key", &self.secret, cx))
            .child(field("Session token (optional)", &self.session_token, cx))
            .children(self.problem.clone().map(|problem| {
                inline_message(
                    IconName::TriangleAlert,
                    "Not saved",
                    problem,
                    cx.theme().danger,
                    div(),
                    cx,
                )
            }))
            .child(
                gpui_component::h_flex()
                    .gap(space::INLINE)
                    .child(
                        Button::new("save-credential")
                            .label(if saving { "Saving…" } else { "Save" })
                            .primary()
                            .on_click(move |_, window, cx| save(window, cx)),
                    )
                    .child(
                        Button::new("cancel-credential")
                            .label("Cancel")
                            .ghost()
                            .on_click(move |_, window, cx| cancel(window, cx)),
                    ),
            )
            .into_any_element()
    }
}

/// A label above its input, which is the only arrangement that survives a
/// narrow window without the label being cut off.
fn field(label: &'static str, state: &Entity<InputState>, cx: &App) -> AnyElement {
    v_flex()
        .gap(space::TIGHT)
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(Input::new(state))
        .into_any_element()
}
