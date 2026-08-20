use caixonho_core::{
    Abandon, ActiveOutcome, Bucket, ConfigPaths, ConnectionId, ConnectionSource, Cursor,
    DeviceAuthorization, Diagnostics, Error, HttpStack, Location, Outcome, Page, Prefix, Profile,
    RegionChoice, Scope, Session, SignInOutcome, StoredCredential, TaggedOutcome, region_choices,
};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, IndexPath, Side, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState},
    menu::PopupMenuItem,
    select::{SearchableVec, Select, SelectEvent},
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    status_bar::StatusBar,
    table::{DataTable, TableEvent, TableState},
    tooltip::Tooltip,
    v_flex,
};

use crate::components::{empty_state, icon_tile, inline_message, skeleton_rows};
use crate::scroll::{self, ScrollAccel};
use crate::theme::{space, tile};
use crate::views::buckets::{BucketsDelegate, RegionSelect, region_label};
use crate::views::credential_form::CredentialForm;
use crate::views::failure::{guidance_for, unavailable_reason};
use crate::views::objects::ObjectsDelegate;

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
    /// Where a location's page comes back, tagged with what was asked for.
    pages: flume::Sender<(Location, Result<Page, Error>)>,
    /// Where this run is writing its log, and whether it managed to.
    diagnostics: Diagnostics,
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
    /// The connection whose removal has been asked for and not yet confirmed.
    /// Removing a credential cannot be undone, so it takes a second deliberate
    /// act on a surface of its own — not a button that changes its label under
    /// the pointer.
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
    /// Where the user is, when they are inside a bucket.
    ///
    /// One value, and the single answer to the question. The breadcrumb trail
    /// is read from it rather than kept beside it, because a second record of
    /// where you are is a second thing that can be wrong. `None` means a
    /// connection is chosen and no bucket is — the bucket table.
    location: Option<Location>,
    /// What one location holds.
    objects: Entity<TableState<ObjectsDelegate>>,
    /// How the current location's listing is going.
    listing: Listing,
    /// Where to continue, when the service said there is more.
    more: Option<Cursor>,
    /// Set while a page is in flight, so reaching the end of the list twice
    /// does not ask for the same page twice.
    fetching: bool,
    /// The path bar's text, which is also how a bucket is opened by name in
    /// an account whose buckets cannot be listed.
    path: Entity<InputState>,
    /// Whether the trail has been turned into an editable path.
    editing_path: bool,
    /// Set while a sign-in the user asked for is running.
    ///
    /// A panel rather than a modal, decided 2026-08-20: a sign-in is a state
    /// the connection is in, not an interruption to something else, and the
    /// code the user has to read belongs where they were already looking.
    signing_in: Option<SignInAttempt>,
    /// Where a sign-in reports what it is doing and how it ended.
    sign_ins: flume::Sender<SignInEvent>,
}

/// A sign-in in progress, as the window needs to render it.
struct SignInAttempt {
    /// The session being signed in to, named while it happens.
    session_name: SharedString,
    /// `None` until the provider answers with something to show. The gap is
    /// short and real, and rendering "waiting for the browser" before there
    /// is anything to open would be a lie about which step we are on.
    shown: Option<Shown>,
    /// The user's way out. Held here so abandoning works from the button.
    abandon: Abandon,
}

/// What the user reads and copies while the browser is being waited on.
struct Shown {
    user_code: SharedString,
    verification_uri: SharedString,
}

/// What crosses back from a sign-in running on a runtime thread.
enum SignInEvent {
    /// There is a code to show. Arrives before any waiting.
    Started(DeviceAuthorization),
    /// It ended, one way or another.
    Settled(Result<SignInOutcome, Error>),
}

/// How reading the current location is going.
///
/// Four states and not three: a location that refused is not a location that
/// is empty, and neither is a location still being read. Collapsing any two
/// of them is the defect this whole project exists to avoid.
#[derive(Debug, Default)]
enum Listing {
    /// No location open.
    #[default]
    Idle,
    Loading,
    Failed(Error),
    Loaded,
}

impl CaixonhoApp {
    pub(crate) fn new(
        diagnostics: Diagnostics,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let table = cx.new(|cx| TableState::new(BucketsDelegate::new(), window, cx));
        let objects = cx.new(|cx| TableState::new(ObjectsDelegate::new(), window, cx));
        let path = cx.new(|cx| InputState::new(window, cx).placeholder("s3://bucket/prefix/"));
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

        // Double click, not single: a single click selects, and a file
        // explorer that navigated on selection would move the ground under
        // anyone walking a list with the keyboard.
        cx.subscribe_in(
            &objects,
            window,
            |app, _, event: &TableEvent, window, cx| {
                if let TableEvent::DoubleClickedRow(row) = event {
                    app.enter(*row, window, cx);
                }
            },
        )
        .detach();

        // Opening a bucket is the same gesture, one level up.
        cx.subscribe_in(&table, window, |app, _, event: &TableEvent, window, cx| {
            if let TableEvent::DoubleClickedRow(row) = event {
                app.open_bucket(*row, window, cx);
            }
        })
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
        // A second channel for locations, tagged with the location asked
        // about: a page that arrives after the user has walked somewhere else
        // belongs to a screen nobody is looking at, and is dropped rather than
        // rendered over the one they are.
        let (pages, arrived) = flume::unbounded::<(Location, Result<Page, Error>)>();
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

        cx.spawn_in(window, async move |this, cx| {
            while let Ok((asked, outcome)) = arrived.recv_async().await {
                let applied = this.update_in(cx, |app, _, cx| app.apply_page(asked, outcome, cx));
                if applied.is_err() {
                    break; // The window is gone.
                }
            }
        })
        .detach();

        // A third channel, for the one operation that reports twice: once when
        // there is a code to put on screen, and once when it is over.
        let (sign_ins, signing) = flume::unbounded::<SignInEvent>();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = signing.recv_async().await {
                let applied = this.update_in(cx, |app, _, cx| app.apply_sign_in(event, cx));
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
            pages,
            diagnostics,
            connections_error,
            stored,
            form: None,
            confirming: None,
            unavailable: std::collections::HashMap::new(),
            region: RegionChoice::All,
            region_options: vec![RegionChoice::All],
            region_select,
            location: None,
            objects,
            listing: Listing::Idle,
            more: None,
            fetching: false,
            path,
            editing_path: false,
            signing_in: None,
            sign_ins,
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

    /// Go to `location` and read it.
    ///
    /// Everything shown about position is derived from the location this sets,
    /// so there is nowhere else to keep in step.
    fn go_to(&mut self, location: Location, window: &mut Window, cx: &mut Context<Self>) {
        self.path.update(cx, |state, cx| {
            state.set_value(location.to_string(), window, cx);
        });
        self.listing = Listing::Loading;
        self.more = None;
        self.fetching = true;
        self.objects.update(cx, |table, cx| {
            table
                .delegate_mut()
                .show(location.prefix.clone(), Vec::new(), Vec::new());
            cx.notify();
        });
        self.location = Some(location.clone());
        self.read(location, None, cx);
    }

    /// Ask core for a page; the answer arrives through the channel.
    fn read(&mut self, location: Location, cursor: Option<Cursor>, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let pages = self.pages.clone();
        let asked = location.clone();
        session.spawn_objects(location, cursor, move |outcome| {
            // On a runtime thread: send, and do nothing else here.
            let _ = pages.send((asked, outcome));
        });
        cx.notify();
    }

    /// Apply a page, unless it belongs to a location we have already left.
    fn apply_page(
        &mut self,
        asked: Location,
        outcome: Result<Page, Error>,
        cx: &mut Context<Self>,
    ) {
        if self.location.as_ref() != Some(&asked) {
            return; // A page for a screen nobody is looking at.
        }
        self.fetching = false;

        match outcome {
            Err(error) => {
                // Never an empty folder. A refusal keeps its cause, and the
                // panel says what would lift it.
                self.listing = Listing::Failed(error);
            }
            Ok(page) => {
                let continuing = self.more.is_some();
                self.more = page.more.clone();
                self.listing = Listing::Loaded;
                self.objects.update(cx, |table, cx| {
                    if continuing {
                        table.delegate_mut().extend(page.folders, page.objects);
                    } else {
                        table
                            .delegate_mut()
                            .show(asked.prefix.clone(), page.folders, page.objects);
                    }
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    /// Ask for the next page, if there is one and none is already in flight.
    fn read_more(&mut self, cx: &mut Context<Self>) {
        let (Some(location), Some(cursor)) = (self.location.clone(), self.more.clone()) else {
            return;
        };
        if self.fetching {
            return; // Reaching the end twice must not ask twice.
        }
        self.fetching = true;
        self.read(location, Some(cursor), cx);
    }

    /// Enter the row at `index`, when it is something that can be entered.
    fn enter(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(location), Some(entry)) = (
            self.location.clone(),
            self.objects.read(cx).delegate().row(index).cloned(),
        ) else {
            return;
        };
        let Some(prefix) = entry.into_prefix() else {
            // An object is not a place. Opening one is XONHO-0007's business,
            // and doing nothing is better than pretending to navigate.
            return;
        };
        self.go_to(Location::at(location.bucket, prefix), window, cx);
    }

    /// Open the bucket in the table's row `index`.
    fn open_bucket(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.table.read(cx).delegate().name_at(index) else {
            return;
        };
        self.go_to(Location::bucket(name), window, cx);
    }

    /// Leave the bucket entirely, back to the account's listing.
    fn leave_bucket(&mut self, cx: &mut Context<Self>) {
        self.location = None;
        self.listing = Listing::Idle;
        self.more = None;
        self.fetching = false;
        cx.notify();
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

    /// Where this run is writing its log, said quietly and always.
    ///
    /// In the status bar rather than behind a menu: the moment it is wanted is
    /// the moment something has gone wrong, and hunting for it then is the
    /// worst time. The directory rather than the file, because the file's name
    /// changes when the log rolls.
    fn log_location(&self, cx: &Context<Self>) -> AnyElement {
        match (self.diagnostics.directory(), self.diagnostics.problem()) {
            (Some(directory), _) => {
                let shown = directory.display().to_string();
                div()
                    .id("log-location")
                    .child(format!("log: {shown}"))
                    .tooltip(move |window, cx| {
                        Tooltip::new(format!(
                            "caixonho writes what it decides to {shown}. It holds no secrets. \
                             Set CAIXONHO_LOG=debug for more detail."
                        ))
                        .build(window, cx)
                    })
                    .into_any_element()
            }
            // Not an error panel: nothing the user was doing has failed.
            (None, Some(_)) => div()
                .text_color(cx.theme().warning)
                .child("no log this run")
                .into_any_element(),
            (None, None) => div().into_any_element(),
        }
    }

    /// The name of the connection at `index`, when it is one this application
    /// holds rather than a profile read from `~/.aws`.
    fn stored_name(&self, index: usize) -> Option<String> {
        self.stored
            .get(index.checked_sub(self.profiles.len())?)
            .map(|credential| credential.name().to_owned())
    }

    /// Confirming a removal, on a surface of its own.
    ///
    /// Not a button that changes its label under the pointer: the second click
    /// of a two-step control lands in the same place as the first, which is
    /// exactly how a mis-click becomes a deletion.
    fn confirm_removal(&mut self, name: String, cx: &mut Context<Self>) -> AnyElement {
        let for_remove = name.clone();
        inline_message(
            IconName::TriangleAlert,
            format!("Remove {name}?"),
            "Its secret is deleted from this system's keychain. The bucket and everything in it \
             are untouched — this removes how caixonho signs in, not what it signs in to.",
            cx.theme().danger,
            h_flex()
                .gap(space::INLINE)
                .child(
                    Button::new("confirm-remove")
                        .label("Remove")
                        .danger()
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.confirming = None;
                            app.forget(for_remove.clone(), window, cx);
                        })),
                )
                .child(
                    Button::new("cancel-remove")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.confirming = None;
                            cx.notify();
                        })),
                ),
            cx,
        )
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
                        // Replace, never append. The store keys a connection by
                        // its name and rewrites it, so saving over one that
                        // exists — which is what editing is — must do the same
                        // here or the same connection appears twice.
                        match app
                            .stored
                            .iter()
                            .position(|existing| existing.name() == credential.name())
                        {
                            Some(position) => app.stored[position] = credential,
                            None => app.stored.push(credential),
                        }
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
                        // choosing one, and it acts on a connection — so it
                        // lives on the connection, in its own context menu,
                        // rather than in a second list of the same things.
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Right-click a saved connection to edit or remove it."),
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
                    self.bucket_group(cx),
                ]
                .into_iter()
                .flatten(),
            )
    }

    /// The buckets of the connection in use, under it.
    ///
    /// Flat: a bucket is a place to go, and the sidebar does not expand into
    /// prefixes. Walking a bucket happens in the panel, with one trail, so
    /// there is never a second navigation surface to keep in step
    /// (`design.md`, "Navigation lives in the main panel").
    ///
    /// Its purpose is that entering a bucket no longer costs sight of the
    /// others — the main panel gives itself over to the contents, and without
    /// this the account disappears while you are inside one of its buckets.
    fn bucket_group(&self, cx: &mut Context<Self>) -> Option<SidebarGroup<SidebarMenu>> {
        self.active_profile?;
        let names = self.table.read(cx).delegate().shown_names();
        if names.is_empty() {
            return None;
        }
        let here = self.location.as_ref().map(|at| at.bucket.clone());

        Some(
            SidebarGroup::new("Buckets").child(SidebarMenu::new().children(names.into_iter().map(
                |name| {
                    let active = here.as_deref() == Some(name.as_str());
                    SidebarMenuItem::new(name.clone())
                        .icon(IconName::Folder)
                        .active(active)
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.go_to(Location::bucket(name.clone()), window, cx);
                        }))
                },
            ))),
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
                    // Editing and removing act on a connection, so they live on
                    // the connection rather than in a second list of the same
                    // things. Only what this application holds: a profile in
                    // ~/.aws is not ours to edit or remove.
                    let item = match self.stored_name(index) {
                        None => item,
                        Some(name) => {
                            let entity = cx.entity().downgrade();
                            item.context_menu(move |menu, _, _| {
                                let (edit, remove) = (entity.clone(), entity.clone());
                                let (for_edit, for_remove) = (name.clone(), name.clone());
                                menu.item(PopupMenuItem::new("Edit…").on_click(
                                    move |_, window, cx| {
                                        let _ = edit.update(cx, |app, cx| {
                                            app.edit_connection(&for_edit, window, cx);
                                        });
                                    },
                                ))
                                .separator()
                                .item(
                                    PopupMenuItem::new("Remove…").on_click(move |_, _, cx| {
                                        let _ = remove.update(cx, |app, cx| {
                                            app.confirming = Some(for_remove.clone());
                                            cx.notify();
                                        });
                                    }),
                                )
                            })
                        }
                    };
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

    /// Begin a sign-in the user asked for.
    ///
    /// Nothing here happens on its own: no connection opens a browser because
    /// it was selected, and none does at startup (`sso-sign-in`, "Signing in
    /// is something the user asks for").
    fn sign_in(&mut self, profile: String, cx: &mut Context<Self>) {
        let Some(session) = &self.session else { return };

        // Where to sign in comes from the profile, and a profile that does
        // not say is reported as exactly that rather than as an attempt that
        // failed. No request is made.
        let at = match caixonho_core::sign_in_location(session.paths(), &profile) {
            Ok(at) => at,
            Err(error) => {
                self.outcome.accept(TaggedOutcome {
                    connection: self.outcome.active(),
                    outcome: Outcome::Failed(error),
                });
                cx.notify();
                return;
            }
        };

        let abandon = Abandon::default();
        self.signing_in = Some(SignInAttempt {
            session_name: at.session_name.clone().into(),
            shown: None,
            abandon: abandon.clone(),
        });

        let (started, settled) = (self.sign_ins.clone(), self.sign_ins.clone());
        session.spawn_sign_in(
            at,
            abandon,
            move |authorization| {
                let _ = started.send(SignInEvent::Started(authorization));
            },
            move |outcome| {
                let _ = settled.send(SignInEvent::Settled(outcome));
            },
        );
        cx.notify();
    }

    /// Apply what a running sign-in reported.
    fn apply_sign_in(&mut self, event: SignInEvent, cx: &mut Context<Self>) {
        match event {
            SignInEvent::Started(authorization) => {
                // Opened here, and only here: this arm is reached from a
                // sign-in the user started, which is what makes opening a
                // browser theirs rather than ours. Never on selection, never
                // at startup (`sso-sign-in`, "Signing in is something the user
                // asks for").
                //
                // Guarded on the attempt still wanting it. The provider
                // answers fast, but not instantly, and a user who pressed
                // Cancel in that gap should not have a browser tab open on
                // them for something they just stopped.
                let wanted = self
                    .signing_in
                    .as_ref()
                    .is_some_and(|attempt| !attempt.abandon.asked());
                if wanted {
                    // A convenience, never the mechanism: nothing checks
                    // whether it worked, because the code and the address are
                    // already on screen and the sign-in stays completable by
                    // hand either way.
                    cx.open_url(&authorization.verification_uri_complete);
                }
                if let Some(attempt) = &mut self.signing_in {
                    attempt.shown = Some(Shown {
                        user_code: authorization.user_code.into(),
                        verification_uri: authorization.verification_uri.into(),
                    });
                }
            }
            SignInEvent::Settled(outcome) => {
                self.signing_in = None;
                match outcome {
                    // The session is already in the token cache by the time
                    // this arrives. Retrying is what turns it into a listing,
                    // and doing it here means the user does not have to.
                    Ok(SignInOutcome::Session(_)) => self.retry(cx),
                    // Abandoning was deliberate. Saying anything about it
                    // would be telling the user what they just did.
                    Ok(SignInOutcome::Abandoned) => {}
                    Err(error) => {
                        self.outcome.accept(TaggedOutcome {
                            connection: self.outcome.active(),
                            outcome: Outcome::Failed(error),
                        });
                    }
                }
            }
        }
        cx.notify();
    }

    /// The panel shown while a sign-in is running.
    ///
    /// Everything the spec requires to be visible is here rather than only in
    /// the browser: the code, the address it belongs to, that we are waiting,
    /// and the way out. A browser that did not open leaves this usable.
    fn sign_in_panel(&self, attempt: &SignInAttempt, cx: &mut Context<Self>) -> AnyElement {
        let waiting = match &attempt.shown {
            None => v_flex()
                .gap(space::TIGHT)
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Starting the sign-in…"),
                )
                .into_any_element(),
            Some(shown) => v_flex()
                .gap(space::ROW)
                .items_center()
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Finish signing in, in your browser"),
                )
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(shown.verification_uri.clone()),
                )
                // The code, big enough to read off the screen and onto
                // another device. This is the whole reason the panel exists.
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded(px(8.))
                        .bg(cx.theme().muted)
                        .text_2xl()
                        .font_weight(gpui::FontWeight::BOLD)
                        .debug_selector(|| "user-code".into())
                        .child(shown.user_code.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Waiting for you to approve it there."),
                )
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap(space::ROW)
            .p(space::SECTION)
            .debug_selector(|| "sign-in-panel".into())
            .child(icon_tile(
                IconName::Globe,
                tile::LG,
                cx.theme().primary,
                false,
            ))
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Signing in to {}", attempt.session_name)),
            )
            .child(waiting)
            .child(
                Button::new("abandon-sign-in")
                    .label("Cancel")
                    .outline()
                    .on_click(cx.listener(|app, _, _, cx| {
                        if let Some(attempt) = &app.signing_in {
                            attempt.abandon.now();
                        }
                        // The flow answers with `Abandoned` and clears the
                        // panel; nothing is torn down from here, so a sign-in
                        // that completes in the same instant still lands.
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// What to say about a failure, and what the user can do about it.
    ///
    /// Each cause gets its own next action, which is the whole reason the
    /// error type keeps them apart.
    fn failure_panel(&self, error: &Error, cx: &mut Context<Self>) -> impl IntoElement {
        let guidance = guidance_for(error);
        let retry = Button::new("retry")
            .label("Retry")
            .outline()
            .on_click(cx.listener(|app, _, _, cx| app.retry(cx)));

        // The offer, where the cause is already stated — and only when it
        // could succeed. A profile that declares no `sso_session` has nowhere
        // to sign in, and a button that cannot work is worse than no button:
        // it moves the failure from the message to the click.
        let offer = self.offerable_sign_in(error).map(|profile| {
            Button::new("sign-in")
                .label("Sign in")
                .primary()
                .debug_selector(|| "sign-in-button".into())
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.sign_in(profile.clone(), cx);
                }))
        });

        inline_message(
            IconName::TriangleAlert,
            SharedString::from(error.to_string()),
            guidance,
            cx.theme().danger,
            h_flex().gap(space::TIGHT).children(offer).child(retry),
            cx,
        )
    }

    /// The profile a sign-in would be for, when one is worth offering.
    ///
    /// Two conditions, both required. The cause has to be one a sign-in would
    /// actually change — an expired or missing session, never a denial or a
    /// dead network — and the profile has to declare where to sign in.
    fn offerable_sign_in(&self, error: &Error) -> Option<String> {
        let session = self.session.as_ref()?;
        let profile = match error {
            Error::SessionRejected { profile, .. } => profile.clone(),
            Error::NoCredentials { profile } => profile.clone(),
            _ => return None,
        };
        caixonho_core::sign_in_location(session.paths(), &profile)
            .ok()
            .map(|_| profile)
    }

    /// The trail from the bucket down to here, and the path bar beside it.
    ///
    /// The trail is *read* from the location — `Prefix::segments` — rather
    /// than stored, so it cannot drift from where the user actually is.
    fn path_bar(&self, location: &Location, cx: &mut Context<Self>) -> AnyElement {
        let steps: Vec<String> = location.prefix.segments().map(ToOwned::to_owned).collect();

        let mut trail = h_flex().items_center().gap(space::TIGHT).child(
            Button::new("leave-bucket")
                .label("All buckets")
                .ghost()
                .on_click(cx.listener(|app, _, _, cx| app.leave_bucket(cx))),
        );

        trail = trail.child(div().text_color(cx.theme().muted_foreground).child("/"));
        trail = trail.child(
            Button::new("bucket-root")
                .label(location.bucket.clone())
                .ghost()
                .on_click({
                    let bucket = location.bucket.clone();
                    cx.listener(move |app, _, window, cx| {
                        app.go_to(Location::bucket(bucket.clone()), window, cx);
                    })
                }),
        );

        // Each step goes to the prefix that *ends* at it, which is why the
        // trail is built by accumulating rather than by slicing text.
        let mut walked = Prefix::root();
        for (index, step) in steps.iter().enumerate() {
            walked = walked.child(step);
            let target = Location::at(location.bucket.clone(), walked.clone());
            trail = trail
                .child(div().text_color(cx.theme().muted_foreground).child("/"))
                .child(
                    Button::new(("step", index))
                        .label(step.clone())
                        .ghost()
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.go_to(target.clone(), window, cx);
                        })),
                );
        }

        // One row, not two. The trail and the path say the same thing, and
        // showing both permanently is a second answer to a question already
        // answered — so the path bar is a *mode* the trail turns into, the way
        // every file manager does it.
        if !self.editing_path {
            return h_flex()
                .w_full()
                .items_center()
                .gap(space::TIGHT)
                .child(div().flex_1().child(trail))
                .child(
                    Button::new("edit-path")
                        .label("Type a location")
                        .ghost()
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.editing_path = true;
                            cx.notify();
                        })),
                )
                .into_any_element();
        }

        h_flex()
            .w_full()
            .gap(space::TIGHT)
            .items_center()
            .child(div().flex_1().child(Input::new(&self.path)))
            .child(
                Button::new("go")
                    .label("Go")
                    .primary()
                    .on_click(cx.listener(|app, _, window, cx| app.go_typed(window, cx))),
            )
            .child(
                Button::new("cancel-path")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.editing_path = false;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// Go where the path bar says, or say that it says nowhere.
    fn go_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.path.read(cx).value().to_string();
        match Location::parse(&typed) {
            // This is also how a bucket is opened in an account whose buckets
            // cannot be listed: typing its name needs no listing first.
            Some(location) => {
                self.editing_path = false;
                self.go_to(location, window, cx);
            }
            None => {
                self.listing = Listing::Failed(Error::MissingConfiguration {
                    profile: None,
                    detail: format!("`{typed}` does not name a bucket to open"),
                });
                cx.notify();
            }
        }
    }

    /// What one location holds, or why it does not say.
    fn contents(&mut self, location: Location, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.listing {
            Listing::Loading | Listing::Idle => skeleton_rows(6),
            Listing::Failed(error) => {
                let panel = self.failure_panel(error, cx);
                v_flex().child(panel).into_any_element()
            }
            Listing::Loaded if self.objects.read(cx).delegate().rows.is_empty() => empty_state(
                IconName::Folder,
                "This folder is empty.",
                "It was read successfully — there is simply nothing in it. A folder \
                 you are not allowed to read says so instead.",
                cx,
            ),
            Listing::Loaded => {
                let truncated = self.more.is_some();
                let fetching = self.fetching;
                v_flex()
                    .size_full()
                    .child(
                        div()
                            .relative()
                            .min_h_0()
                            .flex_1()
                            .child(DataTable::new(&self.objects))
                            .child(scroll::accelerator(
                                self.objects.clone(),
                                self.accel.clone(),
                            )),
                    )
                    // Said, not hidden: a listing that stops early without
                    // saying so is indistinguishable from a small folder.
                    .children(truncated.then(|| {
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap(space::TIGHT)
                            .p(space::TIGHT)
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if fetching {
                                        "Reading more…"
                                    } else {
                                        "More to come."
                                    }),
                            )
                            .child(
                                Button::new("read-more")
                                    .label(if fetching { "Reading…" } else { "Load more" })
                                    .ghost()
                                    .on_click(cx.listener(|app, _, _, cx| app.read_more(cx))),
                            )
                    }))
                    .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .gap(space::TIGHT)
            .child(self.path_bar(&location, cx))
            // `v_flex`, not a bare `div`: the states below size themselves
            // with `size_full`, which resolves against a parent that is a
            // flex container with a height. A plain div here left the empty
            // state with nowhere to be and drew nothing at all — the same
            // family of bug as the `h_flex` one in `design-language.md`.
            .child(v_flex().flex_1().min_h_0().child(body))
    }

    /// The body: whatever the active connection's latest outcome deserves.
    fn body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        // Before anything else a connection could be showing: a sign-in the
        // user asked for is the state this pane is in until it resolves.
        if let Some(attempt) = self.signing_in.take() {
            let panel = self.sign_in_panel(&attempt, cx);
            self.signing_in = Some(attempt);
            return v_flex().size_full().child(panel).into_any_element();
        }
        if let Some(error) = &self.startup_error {
            return v_flex()
                .child(self.failure_panel(error, cx))
                .into_any_element();
        }
        if let Some(name) = self.confirming.clone() {
            return self.confirm_removal(name, cx);
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

        // Inside a bucket, the panel shows what that location holds. This is
        // the one branch browsing adds here; everything it renders lives in
        // `views/objects.rs` (task 1.1, amended).
        if let Some(location) = self.location.clone() {
            return self.contents(location, cx).into_any_element();
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
                                    h_flex()
                                        .w_full()
                                        .justify_between()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(status)
                                        .child(self.log_location(cx)),
                                ),
                            ),
                    ),
            )
    }
}
