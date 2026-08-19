use caixonho_core::{
    ActiveOutcome, Bucket, ConfigPaths, ConnectionId, ConnectionSource, ConnectionsProblem,
    CredentialStoreProblem, Error, HttpStack, Outcome, Profile, RegionChoice, Scope, Session,
    SessionProblem, StoredCredential, TaggedOutcome, region_choices,
};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, IndexPath, Side, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    select::{SearchableVec, Select, SelectEvent},
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    status_bar::StatusBar,
    table::{DataTable, TableState},
    tooltip::Tooltip,
    v_flex,
};

use crate::components::{empty_state, icon_tile, inline_message, skeleton_rows};
use crate::scroll::{self, ScrollAccel};
use crate::theme::{space, tile};
use crate::views::buckets::{BucketsDelegate, RegionSelect, region_label};
use crate::views::credential_form::CredentialForm;

/// What to do about a failure. The advice belongs to the cause rather than
/// to the panel that happens to show it, now that two surfaces need it.
fn guidance_for(error: &Error) -> SharedString {
    match error {
                Error::Network { .. } => "The endpoint could not be reached. Check the connection and try again.".into(),
                Error::SessionRejected { profile, sso_session, problem } => match (problem, sso_session) {
                    (_, Some(session)) => format!("Sign in again: `aws sso login --sso-session {session}`").into(),
                    (SessionProblem::Expired, None) => format!("Sign in again for profile `{profile}`, then retry.").into(),
                    (SessionProblem::Invalid, None) => format!(
                        "The service does not recognise these credentials. Check the access key and secret for profile `{profile}`."
                    )
                    .into(),
                },
                Error::TlsTrust { endpoint } => format!(
                    "The certificate chain for {endpoint} is not trusted. Add the issuing CA to the \
                     system trust store, or point AWS_CA_BUNDLE at the bundle your network uses."
                )
                .into(),
                Error::AccessDenied { iam_action } => format!(
                    "This profile is not allowed to list buckets. It needs the `{iam_action}` permission."
                )
                .into(),
                Error::NoCredentials { profile } => {
                    format!("No credentials resolved for `{profile}`. Check the profile's keys, role or SSO session.").into()
                }
                Error::MissingConfiguration { .. } => {
                    "Complete the profile's configuration — a region is required — and try again.".into()
                }
                Error::CredentialStore { connection, problem } => match problem {
                    CredentialStoreProblem::Locked => format!(
                        "The system keychain is locked, so the secret for `{connection}` cannot be \
                         read. Unlock it and try again."
                    )
                    .into(),
                    CredentialStoreProblem::Refused => format!(
                        "The system keychain did not hand back the secret for `{connection}`. If a \
                         prompt appeared, it may have been declined."
                    )
                    .into(),
                    CredentialStoreProblem::Absent => {
                        "This system has no credential store for caixonho to use, so credentials \
                         entered here cannot be kept. Use a profile in ~/.aws instead."
                            .into()
                    }
                },
                Error::Connections { problem, path } => {
                    let where_it_is = path
                        .as_ref()
                        .map(|path| format!(" ({})", path.display()))
                        .unwrap_or_default();
                    match problem {
                        ConnectionsProblem::Unreadable => format!(
                            "The file of saved connections could not be read{where_it_is}. The \
                             connections kept in it are not shown; the ones from ~/.aws are \
                             unaffected."
                        )
                        .into(),
                        ConnectionsProblem::Malformed => format!(
                            "The file of saved connections is not in a form caixonho understands\
                             {where_it_is}. It has been left exactly as it is — repair or remove it, \
                             and nothing in it will be overwritten meanwhile."
                        )
                        .into(),
                        ConnectionsProblem::NotWritable => format!(
                            "The file of saved connections could not be written{where_it_is}, so the \
                             change was not kept."
                        )
                        .into(),
                        ConnectionsProblem::NoLocation => {
                            "This machine offers nowhere to keep saved connections, so a credential \
                             entered here cannot be remembered. Use a profile in ~/.aws instead."
                                .into()
                        }
                    }
                }
                Error::Unexpected { .. } => "The call failed for an unrecognised reason.".into(),
    }
}

/// The remove control, which changes what it says once it has been asked.
///
/// Removing a credential cannot be undone, so it takes two deliberate acts
/// rather than one that could be a mis-click — and the second one is coloured
/// like what it does.
fn remove_button(row: usize, confirming: bool) -> Button {
    let button = Button::new(("remove", row)).label(if confirming {
        "Really remove"
    } else {
        "Remove"
    });
    if confirming {
        button.danger()
    } else {
        button.ghost()
    }
}

/// Why a connection cannot be used at all, if that is what happened.
///
/// Only a failure to authenticate makes a *connection* unusable. A network
/// failure belongs to the network and will pass; a denial means the connection
/// worked perfectly and the permission did not. Marking either would say
/// something untrue about the connection.
fn unavailable_reason(error: &Error) -> Option<SharedString> {
    match error {
        Error::SessionRejected { problem, .. } => Some(match problem {
            SessionProblem::Expired => "sign-in expired".into(),
            SessionProblem::Invalid => "credentials refused".into(),
        }),
        Error::NoCredentials { .. } => Some("no credentials".into()),
        // The secret exists somewhere the app cannot reach, which for the
        // purpose of connecting is the same as not having one.
        Error::CredentialStore { problem, .. } => Some(match problem {
            CredentialStoreProblem::Locked => "keychain locked".into(),
            CredentialStoreProblem::Refused => "keychain refused".into(),
            CredentialStoreProblem::Absent => "no keychain".into(),
        }),
        // A configuration file that will not parse says nothing about whether
        // any particular credential works, so it marks no connection.
        Error::Connections { .. }
        | Error::Network { .. }
        | Error::TlsTrust { .. }
        | Error::AccessDenied { .. }
        | Error::MissingConfiguration { .. }
        | Error::Unexpected { .. } => None,
    }
}

/// Everything the window shows.
pub(crate) struct CaixonhoApp {
    /// The one runtime for this process. Held so it outlives every spawned
    /// call; core is handed its handle and never builds one of its own.
    _runtime: tokio::runtime::Runtime,
    session: Option<Session>,
    /// Set when trust material or the runtime could not be prepared at all,
    /// which is a startup failure rather than a connection failure.
    startup_error: Option<Error>,
    profiles: Vec<Profile>,
    active_profile: Option<usize>,
    outcome: ActiveOutcome,
    next_connection: u64,
    table: Entity<TableState<BucketsDelegate>>,
    accel: Entity<ScrollAccel>,
    inbox: flume::Sender<TaggedOutcome>,
    /// Set when the remembered connections could not be read. Deliberately
    /// not a startup failure: the profiles in `~/.aws` are unaffected and the
    /// application is perfectly usable, so this is said above the content
    /// rather than instead of it.
    connections_error: Option<Error>,
    /// The connections this application holds credentials for. Kept beside the
    /// discovered profiles rather than in a list of their own: to someone
    /// connecting, both are just somewhere to connect.
    stored: Vec<StoredCredential>,
    /// Open while a credential is being entered.
    form: Option<CredentialForm>,
    /// Open while connections are being managed rather than used.
    managing: bool,
    /// The connection whose removal has been asked for and not yet confirmed.
    /// Removing a credential cannot be undone, so it takes two deliberate acts
    /// rather than one that can be a mis-click.
    confirming: Option<String>,
    /// Connections that could not authenticate, and the short reason each
    /// gave. A connection that cannot sign in is not a connection, so it is
    /// marked where it is chosen rather than only where its listing would have
    /// been.
    unavailable: std::collections::HashMap<usize, SharedString>,
    /// Which region the list is narrowed to.
    region: RegionChoice,
    /// The choices currently on offer, in the order the selector shows them.
    /// Held so a confirmed label can be turned back into the choice it means.
    region_options: Vec<RegionChoice>,
    region_select: Entity<RegionSelect>,
}

impl CaixonhoApp {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| TableState::new(BucketsDelegate::new(), window, cx));
        let accel = cx.new(|_| ScrollAccel::default());

        let region_select = cx.new(|cx| {
            RegionSelect::new(
                SearchableVec::new(vec![region_label(&RegionChoice::All)]),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        cx.subscribe(
            &region_select,
            |app, _, event: &SelectEvent<SearchableVec<SharedString>>, cx| {
                let SelectEvent::Confirm(label) = event;
                let Some(label) = label else { return };
                // The control confirms a label; the choice it stands for is
                // whichever offered choice renders as that label.
                let chosen = app
                    .region_options
                    .iter()
                    .find(|choice| region_label(choice) == *label)
                    .cloned();
                if let Some(choice) = chosen {
                    app.region = choice;
                    app.narrow_rows(cx);
                }
            },
        )
        .detach();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("caixonho-aws")
            .build()
            .expect("failed to start the tokio runtime");

        let paths = ConfigPaths::from_env();
        let profiles = caixonho_core::discover(&paths).unwrap_or_default();

        // Trust material is prepared once, at startup: it belongs to the
        // process, not to a profile, and failing here is not a connection
        // failure to be shown against whichever profile happens to be first.
        let (session, startup_error) = match HttpStack::from_env() {
            Ok(http) => (
                Some(Session::new(runtime.handle().clone(), http, paths)),
                None,
            ),
            Err(error) => (None, Some(error)),
        };

        // Connections remembered from a previous run. Reading them is local
        // and cheap — no credential is resolved and nothing is contacted, so
        // this does not reintroduce the wait that startup just stopped paying.
        let mut stored = Vec::new();
        let mut connections_error = None;
        if let Some(session) = &session {
            match session.stored_connections() {
                Ok(remembered) => stored = remembered,
                // Not shown as "no connections": a machine whose connections
                // could not be read is not a machine without any, and saying
                // the second invites entering a credential on top of one that
                // is already there.
                Err(error) => connections_error = Some(error),
            }
        }

        // The table reads observations straight from the session, and reports
        // the rows on screen back to it for probing.
        if let Some(session) = &session {
            table.update(cx, |state, _| {
                state.delegate_mut().session = Some(session.clone());
            });

            // Probes settle on runtime threads and write into the capability
            // store, which nothing watches — without this the window would keep
            // showing "checking…" over an answer that had already arrived.
            let (settled, arrivals) = flume::unbounded::<Scope>();
            session.on_probe_settled(move |scope| {
                let _ = settled.send(scope);
            });
            cx.spawn(async move |this, cx| {
                while arrivals.recv_async().await.is_ok() {
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break; // The window is gone.
                    }
                }
            })
            .detach();
        }

        // The bridge: results cross from runtime threads to the UI as
        // messages, and are applied on GPUI's executor.
        let (inbox, results) = flume::unbounded::<TaggedOutcome>();
        // `spawn_in` rather than `spawn`: applying an outcome replaces the
        // region choices on offer, and that control needs a window.
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(tagged) = results.recv_async().await {
                let applied = this.update_in(cx, |app, window, cx| app.apply(tagged, window, cx));
                if applied.is_err() {
                    break; // The window is gone.
                }
            }
        })
        .detach();

        let app = Self {
            _runtime: runtime,
            session,
            startup_error,
            profiles,
            active_profile: None,
            outcome: ActiveOutcome::new(ConnectionId(0)),
            next_connection: 0,
            table,
            accel,
            inbox,
            connections_error,
            stored,
            form: None,
            managing: false,
            confirming: None,
            unavailable: std::collections::HashMap::new(),
            region: RegionChoice::All,
            region_options: vec![RegionChoice::All],
            region_select,
        };

        // Nothing is contacted until a connection is chosen. Opening on a
        // profile of our own choosing used to spare the first screen an
        // instruction; it bought that with seconds of a window that looks
        // frozen, resolving credentials for work nobody had asked for.
        app
    }

    /// Switch to a profile and start listing it.
    fn select_profile(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, source)) = self.connections().into_iter().nth(index) else {
            return;
        };
        self.active_profile = Some(index);
        self.next_connection += 1;
        let id = ConnectionId(self.next_connection);
        // Clears the previous profile's rows and error before anything of the
        // new one's arrives.
        self.outcome.switch_to(id);
        self.set_rows(Vec::new(), window, cx);
        self.issue(id, source, cx);
    }

    /// Try the active profile again on the same connection.
    fn retry(&mut self, cx: &mut Context<Self>) {
        let (Some(index), id) = (self.active_profile, self.outcome.active()) else {
            return;
        };
        let Some((_, source)) = self.connections().into_iter().nth(index) else {
            return;
        };
        self.outcome.switch_to(id);
        self.issue(id, source, cx);
    }

    /// Ask core for a listing; the answer arrives through the inbox.
    fn issue(&mut self, id: ConnectionId, source: ConnectionSource, cx: &mut Context<Self>) {
        if let Some(session) = self.session.clone() {
            let inbox = self.inbox.clone();
            session.spawn_listing(id, source, move |tagged| {
                let _ = inbox.send(tagged);
            });
        }
        cx.notify();
    }

    /// Apply an outcome, unless it belongs to a connection we left behind.
    fn apply(&mut self, tagged: TaggedOutcome, window: &mut Window, cx: &mut Context<Self>) {
        if !self.outcome.accept(tagged) {
            return; // Stale: a late answer from a profile the user left.
        }
        match self.outcome.state() {
            Outcome::Loaded(buckets) => {
                let buckets = buckets.clone();
                if let Some(index) = self.active_profile {
                    self.unavailable.remove(&index);
                }
                self.set_rows(buckets, window, cx);
            }
            Outcome::Failed(error) => {
                // Only a connection that could not authenticate is marked
                // unavailable. A network failure is the network's, and a denial
                // means the connection worked and the permission did not — both
                // would be a lie about the connection itself.
                let reason = unavailable_reason(error);
                if let (Some(index), Some(reason)) = (self.active_profile, reason) {
                    self.unavailable.insert(index, reason);
                }
            }
            Outcome::Loading => {}
        }
        cx.notify();
    }

    /// Everything there is to connect to, in the order the sidebar shows it.
    ///
    /// Discovered profiles first, then the credentials this application holds.
    /// One list because to someone connecting there is one question — where
    /// do I want to connect — and where the secret lives is an answer to a
    /// different one.
    fn connections(&self) -> Vec<(SharedString, ConnectionSource)> {
        let profiles = self.profiles.iter().map(|profile| {
            let label = if profile.is_default {
                format!("{} (default)", profile.name)
            } else {
                profile.name.clone()
            };
            (
                SharedString::from(label),
                ConnectionSource::from(profile.name.clone()),
            )
        });
        let stored = self.stored.iter().map(|credential| {
            (
                SharedString::from(credential.name().to_owned()),
                ConnectionSource::Stored(credential.clone()),
            )
        });
        profiles.chain(stored).collect()
    }

    /// Remove a stored connection, and what the credential store holds for it.
    fn forget(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        // Where it sits in the sidebar, if it is the one currently open.
        let index = self
            .stored
            .iter()
            .position(|credential| credential.name() == name)
            .map(|position| position + self.profiles.len());

        let (done, arrivals) = flume::bounded::<Result<(), Error>>(1);
        session.spawn_forget_credential(name.clone(), move |result| {
            let _ = done.send(result);
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = arrivals.recv_async().await else {
                return;
            };
            let _ = this.update_in(cx, |app, window, cx| {
                match result {
                    // Removed from the list only once the store has actually
                    // let go of the secret: dropping it here on a failure would
                    // leave a secret nobody can name.
                    Ok(()) => {
                        app.stored.retain(|credential| credential.name() != name);
                        if app.active_profile == index {
                            app.active_profile = None;
                            app.outcome.switch_to(ConnectionId(app.next_connection));
                            app.set_rows(Vec::new(), window, cx);
                        }
                        if let Some(index) = index {
                            app.unavailable.remove(&index);
                        }
                    }
                    Err(error) => {
                        if let Some(index) = index {
                            app.unavailable.insert(index, error.to_string().into());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The connections this application holds, and what can be done to them.
    ///
    /// Only its own: a profile in `~/.aws` is not ours to remove, and offering
    /// to would be offering to edit a file shared with every other AWS tool on
    /// the machine.
    fn manage_connections(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.stored.is_empty() {
            return empty_state(
                IconName::Settings,
                "No saved connections.",
                "Connections you add here appear in this list. The ones read from ~/.aws are not \
                 managed by caixonho and are left alone.",
                cx,
            );
        }

        let rows: Vec<_> = self
            .stored
            .iter()
            .map(|credential| {
                (
                    credential.name().to_owned(),
                    credential.region().to_owned(),
                    credential.access_key_id().to_owned(),
                )
            })
            .collect();

        v_flex()
            .gap(space::ROW)
            .max_w(px(680.))
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Saved connections"),
            )
            .children(
                rows.into_iter()
                    .enumerate()
                    .map(|(row, (name, region, access_key_id))| {
                        let confirming = self.confirming.as_deref() == Some(name.as_str());
                        let for_remove = name.clone();
                        let for_edit = name.clone();
                        h_flex()
                            .gap(space::ROW)
                            .items_center()
                            .px(space::CARD)
                            .py(space::ROW)
                            .rounded(cx.theme().radius_lg)
                            .bg(cx.theme().popover)
                            .border_1()
                            .border_color(if confirming {
                                cx.theme().danger.opacity(0.4)
                            } else {
                                cx.theme().border
                            })
                            .child(icon_tile(
                                IconName::CircleUser,
                                tile::SM,
                                cx.theme().primary,
                                false,
                            ))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(space::TIGHT)
                                    .child(div().child(name.clone()))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            // The access key id is not a secret. The
                                            // secret is in the keychain, and is shown
                                            // neither here nor anywhere else.
                                            .child(format!("{region} · {access_key_id}")),
                                    ),
                            )
                            .children(confirming.then(|| {
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().danger)
                                    .child("This cannot be undone.")
                            }))
                            .child(
                                h_flex()
                                    .gap(space::INLINE)
                                    .flex_shrink_0()
                                    .child(
                                        Button::new(("edit", row))
                                            .label("Edit")
                                            .outline()
                                            .on_click(cx.listener(move |app, _, window, cx| {
                                                app.edit_connection(&for_edit, window, cx);
                                            })),
                                    )
                                    .child(remove_button(row, confirming).on_click(cx.listener(
                                        move |app, _, window, cx| {
                                            if app.confirming.as_deref()
                                                == Some(for_remove.as_str())
                                            {
                                                app.confirming = None;
                                                app.forget(for_remove.clone(), window, cx);
                                            } else {
                                                app.confirming = Some(for_remove.clone());
                                            }
                                            cx.notify();
                                        },
                                    ))),
                            )
                    }),
            )
            .child(
                h_flex().child(
                    Button::new("done-managing")
                        .label("Done")
                        .outline()
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.managing = false;
                            app.confirming = None;
                            cx.notify();
                        })),
                ),
            )
            .into_any_element()
    }

    /// Open the form on an existing connection.
    ///
    /// Everything but the secret is filled in. The secret is not, and cannot
    /// be: it is in the keychain and this application does not read secrets
    /// back to show them. So editing asks for it again — which is honest, and
    /// is also the thing to improve once core can rewrite a connection's
    /// configuration without touching what the store holds.
    fn edit_connection(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(existing) = self
            .stored
            .iter()
            .find(|credential| credential.name() == name)
            .cloned()
        else {
            return;
        };
        self.confirming = None;
        self.managing = false;
        self.form = Some(CredentialForm::editing(&existing, window, cx));
        cx.notify();
    }

    /// Save what was typed, if it is enough to connect with.
    fn save_credential(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &mut self.form else {
            return;
        };
        if form.saving {
            return; // A second click while the store is being written.
        }

        let entered = form.entered(cx);
        let Some(form) = &mut self.form else {
            return;
        };
        let (credential, secret) = match entered {
            Ok(entered) => entered,
            Err(problem) => {
                form.problem = Some(problem);
                cx.notify();
                return;
            }
        };

        form.problem = None;
        form.saving = true;
        cx.notify();

        let Some(session) = self.session.clone() else {
            return;
        };
        let (saved, arrivals) = flume::bounded::<Result<StoredCredential, Error>>(1);
        session.spawn_save_credential(credential, secret, move |result| {
            let _ = saved.send(result);
        });
        cx.spawn(async move |this, cx| {
            let Ok(result) = arrivals.recv_async().await else {
                return;
            };
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(credential) => {
                        app.stored.push(credential);
                        app.form = None;
                    }
                    Err(error) => {
                        if let Some(form) = &mut app.form {
                            form.saving = false;
                            form.problem = Some(error.to_string().into());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Take a new listing, and re-offer the region choices it supports.
    fn set_rows(&mut self, rows: Vec<Bucket>, window: &mut Window, cx: &mut Context<Self>) {
        // A choice the new listing cannot satisfy would render an empty table
        // whose only cure is guessing which control emptied it.
        self.region = self.region.clone().retained_for(&rows);
        self.region_options = region_choices(&rows);

        let labels: Vec<SharedString> = self.region_options.iter().map(region_label).collect();
        let selected = self
            .region_options
            .iter()
            .position(|choice| *choice == self.region)
            .unwrap_or(0);
        self.region_select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(labels), window, cx);
            select.set_selected_index(Some(IndexPath::new(selected)), window, cx);
        });

        self.table.update(cx, |state, cx| {
            state.delegate_mut().rows = rows;
            cx.notify();
        });
        self.narrow_rows(cx);
    }

    /// Apply the region choice to the listing already held.
    ///
    /// No request is made: the service can filter a listing by region, but only
    /// for a request sent to an endpoint in that same region, which would cost
    /// a client per region to narrow a list already in hand.
    fn narrow_rows(&mut self, cx: &mut Context<Self>) {
        let choice = self.region.clone();
        self.table.update(cx, |state, cx| {
            let delegate = state.delegate_mut();
            delegate.shown = delegate
                .rows
                .iter()
                .enumerate()
                .filter(|(_, bucket)| choice.matches(bucket))
                .map(|(index, _)| index)
                .collect();
            cx.notify();
        });
        self.report_visible_rows(cx);
        cx.notify();
    }

    /// Report the rows on screen after the rows themselves changed.
    ///
    /// The table only announces a *changed* range, and switching accounts or
    /// narrowing by region can leave the range identical while every row in it
    /// is different — which would leave a whole screen unprobed.
    fn report_visible_rows(&self, cx: &mut Context<Self>) {
        let Some(session) = &self.session else {
            return;
        };
        let table = self.table.read(cx);
        let rows = table.visible_range().rows().clone();
        session.submit_viewport(&table.delegate().targets(rows));
    }

    /// The region selector, offering only regions this account uses.
    fn region_picker(&self) -> impl IntoElement {
        h_flex().gap_2().pb_2().child(
            div()
                .w(px(240.))
                .child(Select::new(&self.region_select).title_prefix("Region: ")),
        )
    }

    /// One button per profile, the active one filled in.
    /// The connections this machine knows about.
    ///
    /// Profiles used to be loose buttons in the title bar. They belong in a
    /// sidebar group, which is where every later connection-shaped thing goes
    /// too — so adding one does not mean redesigning the window.
    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Sidebar::new("connections")
            .side(Side::Left)
            .w(px(220.))
            .header(
                SidebarHeader::new()
                    .child(div().font_weight(gpui::FontWeight::BOLD).child("caixonho")),
            )
            .footer(
                // One child holding a column. The footer lays its children out
                // in a row, which squeezed the second button down to its icon
                // and pushed it past the sidebar's edge.
                SidebarFooter::new().child(
                    v_flex()
                        .w_full()
                        .gap(space::TIGHT)
                        .child(
                            Button::new("add-connection")
                                .label("Add a connection")
                                .icon(IconName::Plus)
                                .ghost()
                                .w_full()
                                .on_click(cx.listener(|app, _, window, cx| {
                                    app.form = Some(CredentialForm::new(window, cx));
                                    cx.notify();
                                })),
                        )
                        // Managing connections is a different activity from
                        // choosing one, so it lives behind its own control
                        // rather than as a destructive button beside the row
                        // it would destroy.
                        .child(
                            Button::new("manage-connections")
                                .label("Manage connections")
                                .icon(IconName::Settings)
                                .ghost()
                                .w_full()
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.managing = !app.managing;
                                    app.confirming = None;
                                    cx.notify();
                                })),
                        ),
                ),
            )
            // Two groups rather than a label beside each name: where a
            // connection's secret is kept is worth seeing, and a label would
            // compete with the name for the little room a sidebar row has.
            .children(
                [
                    self.connection_group("From ~/.aws", 0..self.profiles.len(), cx),
                    self.connection_group(
                        "Saved in caixonho",
                        self.profiles.len()..self.profiles.len() + self.stored.len(),
                        cx,
                    ),
                ]
                .into_iter()
                .flatten(),
            )
    }

    /// One group of the sidebar, over a slice of [`Self::connections`].
    fn connection_group(
        &self,
        title: &'static str,
        range: std::ops::Range<usize>,
        cx: &mut Context<Self>,
    ) -> Option<SidebarGroup<SidebarMenu>> {
        let active = self.active_profile;
        let rows: Vec<_> = self
            .connections()
            .into_iter()
            .enumerate()
            .filter(|(index, _)| range.contains(index))
            .collect();
        // A heading over nothing is worse than no heading.
        if rows.is_empty() {
            return None;
        }

        Some(
            SidebarGroup::new(title).child(SidebarMenu::new().children(rows.into_iter().map(
                |(index, (label, _))| {
                    let item = SidebarMenuItem::new(label)
                        .icon(IconName::CircleUser)
                        .active(Some(index) == active)
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.select_profile(index, window, cx);
                        }));
                    // Marked, not hidden, and not removed: a connection
                    // that cannot sign in is still a fact about this
                    // machine, and hiding it would leave the user
                    // wondering where it went.
                    match self.unavailable.get(&index).cloned() {
                        None => item,
                        // An icon, not a sentence. Spelled out, the badge
                        // ate the name it was describing and left a row
                        // reading "v" beside a warning about it.
                        Some(reason) => item.suffix(move |_, cx| {
                            div()
                                .id("unavailable")
                                .text_color(cx.theme().warning)
                                .child(Icon::new(IconName::TriangleAlert).size_3())
                                .tooltip({
                                    let reason = reason.clone();
                                    move |window, cx| Tooltip::new(reason.clone()).build(window, cx)
                                })
                        }),
                    }
                },
            ))),
        )
    }

    /// What to say about a failure, and what the user can do about it.
    ///
    /// Each cause gets its own next action, which is the whole reason the
    /// error type keeps them apart.
    fn failure_panel(&self, error: &Error, cx: &mut Context<Self>) -> impl IntoElement {
        let guidance = guidance_for(error);
        inline_message(
            IconName::TriangleAlert,
            SharedString::from(error.to_string()),
            guidance,
            cx.theme().danger,
            Button::new("retry")
                .label("Retry")
                .outline()
                .on_click(cx.listener(|app, _, _, cx| app.retry(cx))),
            cx,
        )
    }

    /// The body: whatever the active connection's latest outcome deserves.
    fn body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(error) = &self.startup_error {
            return v_flex()
                .child(self.failure_panel(error, cx))
                .into_any_element();
        }
        if self.managing {
            return self.manage_connections(cx);
        }
        if let Some(form) = &self.form {
            let this = cx.entity().downgrade();
            let save = {
                let this = this.clone();
                move |window: &mut Window, cx: &mut App| {
                    let _ = this.update(cx, |app, cx| app.save_credential(window, cx));
                }
            };
            let cancel = move |_: &mut Window, cx: &mut App| {
                let _ = this.update(cx, |app, cx| {
                    app.form = None;
                    cx.notify();
                });
            };
            return form.render(save, cancel, cx);
        }
        if self.connections().is_empty() {
            return empty_state(
                IconName::Inbox,
                "No connections yet.",
                "caixonho reads the profiles in ~/.aws/config and ~/.aws/credentials (or the \
                 files named by AWS_CONFIG_FILE and AWS_SHARED_CREDENTIALS_FILE). Add one there \
                 to begin.",
                cx,
            );
        }
        if self.active_profile.is_none() {
            // The first screen states what has happened, which is nothing.
            return empty_state(
                IconName::CircleUser,
                "Choose a connection.",
                "Nothing has been contacted yet. Pick a connection on the left and caixonho will \
                 sign in to it and list its buckets.",
                cx,
            );
        }

        match self.outcome.state() {
            // Rows in the shape of the table that is coming, rather than a
            // line of text announcing that something is happening somewhere.
            Outcome::Loading => skeleton_rows(6),
            Outcome::Failed(error) => {
                // Cloned so the panel can borrow the app immutably.
                let rendered = error.to_string();
                let panel = self.failure_panel_from(rendered, error, cx);
                panel.into_any_element()
            }
            Outcome::Loaded(buckets) if buckets.is_empty() => empty_state(
                IconName::Folder,
                "This account has no buckets.",
                "The listing succeeded — there is simply nothing in it yet.",
                cx,
            ),
            Outcome::Loaded(_) => v_flex()
                .size_full()
                .child(self.region_picker())
                .child(
                    div()
                        .relative()
                        .min_h_0()
                        .flex_1()
                        .child(DataTable::new(&self.table))
                        .child(scroll::accelerator(self.table.clone(), self.accel.clone())),
                )
                .into_any_element(),
        }
    }

    /// Borrow-splitting helper: the panel needs `&self` while `self.outcome`
    /// is already borrowed by the match in [`Self::body`].
    fn failure_panel_from(
        &self,
        _rendered: String,
        error: &Error,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.failure_panel(error, cx)
    }
}

impl Render for CaixonhoApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Nothing is in flight until a profile is actually selected, so the
        // status stays empty rather than claiming a listing that never began.
        let status: SharedString = match (self.active_profile, self.outcome.state()) {
            (None, _) => "".into(),
            (Some(_), Outcome::Loading) => "Listing buckets…".into(),
            (Some(_), Outcome::Loaded(buckets)) => {
                // Says both numbers while a region is chosen: reporting the
                // account's total beside a narrowed table reads as rows lost.
                let shown = self.table.read(cx).delegate().shown.len();
                let total = buckets.len();
                if shown == total {
                    format!("{total} buckets").into()
                } else {
                    format!("{shown} of {total} buckets").into()
                }
            }
            (Some(_), Outcome::Failed(_)) => "Listing failed".into(),
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(
                TitleBar::new().child(div().font_weight(gpui::FontWeight::BOLD).child("caixonho")),
            )
            .child(
                h_flex()
                    // Stretch, not centre. `h_flex` centres its children by
                    // default, which left both columns only as tall as their
                    // own content, floating in the middle of the window — and
                    // a table with `flex_1` inside a column that had collapsed
                    // to its content height had no height to render into.
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_h_0()
                                    .gap(space::ROW)
                                    .p(space::WINDOW)
                                    .children(self.connections_error.as_ref().map(|error| {
                                        inline_message(
                                            IconName::TriangleAlert,
                                            "Saved connections could not be read",
                                            guidance_for(error),
                                            cx.theme().warning,
                                            div(),
                                            cx,
                                        )
                                    }))
                                    .child(div().flex_1().min_h_0().child(self.body(cx))),
                            )
                            .child(
                                StatusBar::new().child(
                                    div()
                                        .text_color(cx.theme().muted_foreground)
                                        .text_xs()
                                        .child(status),
                                ),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    //! `connections` spec — "A connection that cannot authenticate is not
    //! offered as usable". Which failures mean that, and which do not.

    use super::*;

    #[test]
    fn a_session_the_service_will_not_accept_makes_the_connection_unusable() {
        let expired = Error::SessionRejected {
            profile: "work".into(),
            sso_session: Some("corp".into()),
            problem: SessionProblem::Expired,
        };
        let refused = Error::SessionRejected {
            profile: "work".into(),
            sso_session: None,
            problem: SessionProblem::Invalid,
        };

        assert_eq!(unavailable_reason(&expired), Some("sign-in expired".into()));
        assert_eq!(
            unavailable_reason(&refused),
            Some("credentials refused".into())
        );
    }

    #[test]
    fn a_connection_with_nothing_to_sign_in_with_is_unusable() {
        let error = Error::NoCredentials {
            profile: "work".into(),
        };

        assert_eq!(unavailable_reason(&error), Some("no credentials".into()));
    }

    #[test]
    fn a_denial_does_not_make_the_connection_unusable() {
        // The whole point: the connection worked and the permission did not.
        // Marking it would say something untrue about the connection, and send
        // the user to fix a sign-in that is fine.
        let error = Error::AccessDenied {
            iam_action: "s3:ListAllMyBuckets",
        };

        assert_eq!(unavailable_reason(&error), None);
    }

    #[test]
    fn a_secret_the_app_cannot_reach_makes_the_connection_unusable() {
        // Whether the store is locked, refusing or missing, the effect on
        // connecting is the same: there is nothing to sign in with.
        for problem in [
            CredentialStoreProblem::Locked,
            CredentialStoreProblem::Refused,
            CredentialStoreProblem::Absent,
        ] {
            let error = Error::CredentialStore {
                connection: "my-key".into(),
                problem,
            };

            assert!(
                unavailable_reason(&error).is_some(),
                "a secret that cannot be read leaves nothing to connect with: {problem:?}"
            );
        }
    }

    #[test]
    fn a_failure_of_the_environment_is_not_the_connection_s_fault() {
        let network = Error::Network {
            detail: "connection reset".into(),
        };
        let trust = Error::TlsTrust {
            endpoint: "s3.example.com".into(),
        };

        assert_eq!(unavailable_reason(&network), None);
        assert_eq!(unavailable_reason(&trust), None);
    }
}
