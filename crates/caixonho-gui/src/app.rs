use caixonho_core::{
    ActiveOutcome, Bucket, ConfigPaths, ConnectionId, Error, HttpStack, Outcome, Profile,
    RegionChoice, Scope, Session, SessionProblem, TaggedOutcome, region_choices,
};
use gpui::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    div, px,
};
use gpui_component::{
    ActiveTheme, IconName, IndexPath, Side, TitleBar,
    button::Button,
    h_flex,
    select::{SearchableVec, Select, SelectEvent},
    sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    status_bar::StatusBar,
    table::{DataTable, TableState},
    v_flex,
};

use crate::components::{empty_state, inline_message, skeleton_rows};
use crate::scroll::{self, ScrollAccel};
use crate::theme::space;
use crate::views::buckets::{BucketsDelegate, RegionSelect, region_label};

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

        let mut app = Self {
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
            region: RegionChoice::All,
            region_options: vec![RegionChoice::All],
            region_select,
        };

        // Open on the default profile when there is one, so the first screen
        // shows data rather than an instruction.
        if let Some(index) = app.profiles.iter().position(|profile| profile.is_default) {
            app.select_profile(index, window, cx);
        } else if !app.profiles.is_empty() {
            app.select_profile(0, window, cx);
        }
        app
    }

    /// Switch to a profile and start listing it.
    fn select_profile(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(profile) = self.profiles.get(index).map(|profile| profile.name.clone()) else {
            return;
        };
        self.active_profile = Some(index);
        self.next_connection += 1;
        let id = ConnectionId(self.next_connection);
        // Clears the previous profile's rows and error before anything of the
        // new one's arrives.
        self.outcome.switch_to(id);
        self.set_rows(Vec::new(), window, cx);
        self.issue(id, profile, cx);
    }

    /// Try the active profile again on the same connection.
    fn retry(&mut self, cx: &mut Context<Self>) {
        let (Some(index), id) = (self.active_profile, self.outcome.active()) else {
            return;
        };
        let Some(profile) = self.profiles.get(index).map(|profile| profile.name.clone()) else {
            return;
        };
        self.outcome.switch_to(id);
        self.issue(id, profile, cx);
    }

    /// Ask core for a listing; the answer arrives through the inbox.
    fn issue(&mut self, id: ConnectionId, profile: String, cx: &mut Context<Self>) {
        if let Some(session) = self.session.clone() {
            let inbox = self.inbox.clone();
            session.spawn_listing(id, profile, move |tagged| {
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
        if let Outcome::Loaded(buckets) = self.outcome.state() {
            let buckets = buckets.clone();
            self.set_rows(buckets, window, cx);
        }
        cx.notify();
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
        let active = self.active_profile;
        Sidebar::new("connections")
            .side(Side::Left)
            .w(px(220.))
            .header(
                SidebarHeader::new()
                    .child(div().font_weight(gpui::FontWeight::BOLD).child("caixonho")),
            )
            .child(
                SidebarGroup::new("Connections").child(SidebarMenu::new().children(
                    self.profiles.iter().enumerate().map(|(index, profile)| {
                        let label = if profile.is_default {
                            format!("{} (default)", profile.name)
                        } else {
                            profile.name.clone()
                        };
                        SidebarMenuItem::new(label)
                            .icon(IconName::CircleUser)
                            .active(Some(index) == active)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.select_profile(index, window, cx);
                            }))
                    }),
                )),
            )
    }

    /// What to say about a failure, and what the user can do about it.
    ///
    /// Each cause gets its own next action, which is the whole reason the
    /// error type keeps them apart.
    fn failure_panel(&self, error: &Error, cx: &mut Context<Self>) -> impl IntoElement {
        let guidance: SharedString = match error {
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
            Error::Unexpected { .. } => "The call failed for an unrecognised reason.".into(),
        };

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
        if self.profiles.is_empty() {
            return empty_state(
                IconName::Inbox,
                "No AWS profiles found.",
                "caixonho reads ~/.aws/config and ~/.aws/credentials (or the files named by \
                 AWS_CONFIG_FILE and AWS_SHARED_CREDENTIALS_FILE). Add a profile there to begin.",
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
                h_flex().flex_1().min_h_0().child(self.sidebar(cx)).child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .p(space::WINDOW)
                                .child(self.body(cx)),
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
