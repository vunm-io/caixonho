// A GUI app must not spawn a console window on Windows. Debug builds keep one
// so `println!` and panic output stay visible while developing.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

//! caixonho — a fast, native S3 client.
//!
//! This crate is a thin frontend: it owns the window, the runtime and the
//! rendering, and nothing else. Every decision about credentials, calls and
//! failure causes lives in `caixonho-core`, which this crate reaches only
//! through domain types — no `aws-sdk-s3` type appears here.

mod scroll;

use caixonho_core::{
    ActiveOutcome, Bucket, ConfigPaths, ConnectionId, Error, HttpStack, Outcome, Profile, Region,
    Session, SessionProblem, TaggedOutcome,
};
use gpui::{
    App, AppContext, Bounds, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::{
    ActiveTheme, Root, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    table::{Column, DataTable, TableDelegate, TableState},
    v_flex,
};

use scroll::ScrollAccel;

/// Displayed instead of a region the service never stated. A first-class
/// value, not a placeholder: the alternative is showing the connection's own
/// region, which would be a guess that reads as fact.
const UNKNOWN_REGION: &str = "unknown";

/// The bucket table.
struct BucketsDelegate {
    columns: Vec<Column>,
    rows: Vec<Bucket>,
}

impl BucketsDelegate {
    fn new() -> Self {
        Self {
            columns: vec![
                Column::new("name", "Bucket").width(px(420.)),
                Column::new("created", "Created").width(px(200.)),
                Column::new("region", "Region").width(px(180.)),
            ],
            rows: Vec::new(),
        }
    }
}

impl TableDelegate for BucketsDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        let text: SharedString = match col_ix {
            0 => row.name.clone().into(),
            1 => row.created.clone().unwrap_or_else(|| "—".into()).into(),
            2 => match &row.region {
                Region::Known(region) => region.clone().into(),
                Region::Unknown => UNKNOWN_REGION.into(),
            },
            _ => "".into(),
        };
        div().child(text)
    }
}

/// Everything the window shows.
struct CaixonhoApp {
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
}

impl CaixonhoApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| TableState::new(BucketsDelegate::new(), window, cx));
        let accel = cx.new(|_| ScrollAccel::default());

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

        // The bridge: results cross from runtime threads to the UI as
        // messages, and are applied on GPUI's executor.
        let (inbox, results) = flume::unbounded::<TaggedOutcome>();
        cx.spawn(async move |this, cx| {
            while let Ok(tagged) = results.recv_async().await {
                let applied = this.update(cx, |app, cx| app.apply(tagged, cx));
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
        };

        // Open on the default profile when there is one, so the first screen
        // shows data rather than an instruction.
        if let Some(index) = app.profiles.iter().position(|profile| profile.is_default) {
            app.select_profile(index, cx);
        } else if !app.profiles.is_empty() {
            app.select_profile(0, cx);
        }
        app
    }

    /// Switch to a profile and start listing it.
    fn select_profile(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(profile) = self.profiles.get(index).map(|profile| profile.name.clone()) else {
            return;
        };
        self.active_profile = Some(index);
        self.next_connection += 1;
        let id = ConnectionId(self.next_connection);
        // Clears the previous profile's rows and error before anything of the
        // new one's arrives.
        self.outcome.switch_to(id);
        self.set_rows(Vec::new(), cx);
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
    fn apply(&mut self, tagged: TaggedOutcome, cx: &mut Context<Self>) {
        if !self.outcome.accept(tagged) {
            return; // Stale: a late answer from a profile the user left.
        }
        if let Outcome::Loaded(buckets) = self.outcome.state() {
            let buckets = buckets.clone();
            self.set_rows(buckets, cx);
        }
        cx.notify();
    }

    fn set_rows(&mut self, rows: Vec<Bucket>, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| {
            state.delegate_mut().rows = rows;
            cx.notify();
        });
    }

    /// One button per profile, the active one filled in.
    fn profile_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_profile;
        h_flex().gap_1().children(
            self.profiles
                .iter()
                .enumerate()
                .map(|(index, profile)| {
                    let label = if profile.is_default {
                        format!("{} (default)", profile.name)
                    } else {
                        profile.name.clone()
                    };
                    let button = Button::new(("profile", index)).label(label).compact();
                    let button = if Some(index) == active {
                        button.primary()
                    } else {
                        button.ghost()
                    };
                    button.on_click(cx.listener(move |app, _, _, cx| {
                        app.select_profile(index, cx);
                    }))
                })
                .collect::<Vec<_>>(),
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

        v_flex()
            .gap_2()
            .p_4()
            .child(
                div()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(cx.theme().danger)
                    .child(SharedString::from(error.to_string())),
            )
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(guidance),
            )
            .child(
                Button::new("retry")
                    .label("Retry")
                    .outline()
                    .on_click(cx.listener(|app, _, _, cx| app.retry(cx))),
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
            return notice(
                "No AWS profiles found.",
                "caixonho reads ~/.aws/config and ~/.aws/credentials (or the files named by \
                 AWS_CONFIG_FILE and AWS_SHARED_CREDENTIALS_FILE). Add a profile there to begin.",
                cx,
            )
            .into_any_element();
        }

        match self.outcome.state() {
            Outcome::Loading => notice("Listing buckets…", "", cx).into_any_element(),
            Outcome::Failed(error) => {
                // Cloned so the panel can borrow the app immutably.
                let rendered = error.to_string();
                let panel = self.failure_panel_from(rendered, error, cx);
                panel.into_any_element()
            }
            Outcome::Loaded(buckets) if buckets.is_empty() => notice(
                "This account has no buckets.",
                "The listing succeeded — there is simply nothing in it yet.",
                cx,
            )
            .into_any_element(),
            Outcome::Loaded(_) => div()
                .relative()
                .size_full()
                .child(DataTable::new(&self.table))
                .child(scroll::accelerator(self.table.clone(), self.accel.clone()))
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

/// A centred message with an optional explanation under it.
fn notice(
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

impl Render for CaixonhoApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Nothing is in flight until a profile is actually selected, so the
        // status stays empty rather than claiming a listing that never began.
        let status: SharedString = match (self.active_profile, self.outcome.state()) {
            (None, _) => "".into(),
            (Some(_), Outcome::Loading) => "listing…".into(),
            (Some(_), Outcome::Loaded(buckets)) => format!("{} buckets", buckets.len()).into(),
            (Some(_), Outcome::Failed(_)) => "failed".into(),
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .w_full()
                        .pr_2()
                        .gap_3()
                        .justify_between()
                        .child(
                            h_flex()
                                .gap_3()
                                .child(div().font_weight(gpui::FontWeight::BOLD).child("caixonho"))
                                .child(self.profile_picker(cx)),
                        )
                        .child(div().text_color(cx.theme().muted_foreground).child(status)),
                ),
            )
            .child(div().min_h_0().flex_1().px_3().pb_3().child(self.body(cx)))
    }
}

fn main() {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx| {
            gpui_component::init(cx);

            cx.spawn(async move |cx| {
                let options = cx.update(|cx| WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1280.), px(800.)),
                        cx,
                    ))),
                    ..TitleBar::window_options()
                });

                cx.open_window(options, |window, cx| {
                    window.set_window_title("caixonho");
                    let view = cx.new(|cx| CaixonhoApp::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("failed to open window");
            })
            .detach();
        });
}
