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
    select::{SearchableVec, Select, SelectState},
    v_flex,
};

use crate::components::{icon_tile, inline_message};
use crate::theme::{space, tile};

/// What the user is typing, and what is wrong with it so far.
pub(crate) struct CredentialForm {
    name: Entity<InputState>,
    /// A list rather than a free-text box. A region is a closed set with exact
    /// spellings, and a typo in one does not fail loudly — it signs requests
    /// for somewhere else and surfaces later as a bucket that "does not exist".
    region: Entity<SelectState<SearchableVec<SharedString>>>,
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
        fn field(
            placeholder: &'static str,
            masked: bool,
            window: &mut Window,
            cx: &mut App,
        ) -> Entity<InputState> {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(masked)
            })
        }
        let regions: Vec<SharedString> = REGIONS.iter().map(|r| SharedString::from(*r)).collect();
        let default = REGIONS
            .iter()
            .position(|region| *region == DEFAULT_REGION)
            .unwrap_or(0);
        let region = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(regions),
                Some(gpui_component::IndexPath::new(default)),
                window,
                cx,
            )
            .searchable(true)
        });
        Self {
            name: field("production", false, window, cx),
            region,
            access_key_id: field("AKIA…", false, window, cx),
            secret: field("the secret access key", true, window, cx),
            session_token: field("only for temporary credentials", true, window, cx),
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
        let name = read(&self.name);
        let region = self
            .region
            .read(cx)
            .selected_value()
            .map(ToString::to_string)
            .unwrap_or_default();
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
            .child(
                v_flex()
                    .gap(space::TIGHT)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Region"),
                    )
                    .child(Select::new(&self.region)),
            )
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

/// Where this application offers to connect.
///
/// Not every AWS region — the list is a convenience, not a catalogue, and one
/// long enough to scroll past is worse than one that covers where people
/// actually keep data. It is searchable for the rest.
const REGIONS: &[&str] = &[
    "ap-southeast-1",
    "ap-southeast-2",
    "ap-southeast-3",
    "ap-northeast-1",
    "ap-northeast-2",
    "ap-south-1",
    "ap-east-1",
    "us-east-1",
    "us-east-2",
    "us-west-1",
    "us-west-2",
    "eu-west-1",
    "eu-west-2",
    "eu-central-1",
    "eu-north-1",
    "sa-east-1",
    "ca-central-1",
    "me-south-1",
    "af-south-1",
];

/// Singapore: nearest to where this is being built, and the region its own
/// test buckets are in.
const DEFAULT_REGION: &str = "ap-southeast-1";

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
