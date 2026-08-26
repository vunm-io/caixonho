use caixonho_core::{
    Abandon, ActiveOutcome, Bucket, BucketKind, ConfigPaths, ConnectionId, ConnectionSource,
    Cursor, DeviceAuthorization, Diagnostics, Error, HttpStack, Location, Outcome, Page, Prefix,
    Profile, RefusedListing, RegionChoice, Scope, Session, SignInOutcome, StoredCredential,
    TaggedOutcome, region_choices,
};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, IconName, IndexPath, Side, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::PopupMenuItem,
    select::{SearchableVec, Select, SelectEvent},
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    status_bar::StatusBar,
    table::{DataTable, TableEvent, TableState},
    tooltip::Tooltip,
    v_flex,
};

use crate::components::{empty_state, icon_tile, inline_message, skeleton_rows, status_badge};
use crate::scroll::{self, ScrollAccel};
use crate::theme::{space, tile};
use crate::views::buckets::{BucketsDelegate, KindChoice, Narrowing, RegionSelect, region_label};
use crate::views::credential_form::CredentialForm;
use crate::views::failure::{guidance_for, refusal_detail, refusal_headline, unavailable_reason};
use crate::views::format::split_zonal_name;
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
    /// The one download in flight or just settled, if any (`XONHO-0007`).
    transfer: Option<Transfer>,
    /// Where a download's progress and outcome come back.
    transfers: flume::Sender<TransferEvent>,
    /// The one deletion being confirmed, in flight, or just settled
    /// (`XONHO-0021`).
    deletion: Option<Deletion>,
    /// The folder being made, if one is (`XONHO-0024`).
    making_folder: Option<MakingFolder>,
    /// Where a folder's outcome arrives, off the runtime.
    folder_inbox: flume::Sender<caixonho_core::session::FolderOutcome>,
    /// What the folder is being called. Its own input rather than a `String`
    /// on the phase, because the control is what the user is typing into and
    /// two places holding one answer is how they come to disagree.
    folder_name: Entity<InputState>,
    /// Where delete and undo outcomes come back.
    deletions: flume::Sender<DeleteEvent>,
    /// The preview on screen or in flight, if any (`XONHO-0008`).
    preview: Option<Preview>,
    /// Where preview outcomes come back.
    previews: flume::Sender<caixonho_core::preview::PreviewOutcome>,
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
    /// Everything the account listing is narrowed by — region, kind, name and
    /// accessibility, in one value so the count is of the final set.
    ///
    /// Held only in the window, and reset when the connection changes. Nothing
    /// persists it, and the design says why at length: half the connections are
    /// profiles discovered in `~/.aws` with no record anywhere to hang a
    /// setting on, and a narrowing set on one account silently applying to
    /// another is how someone comes to believe a bucket has gone missing.
    narrowing: Narrowing,
    /// The kind choices, in the order the selector offers them.
    kind_select: Entity<RegionSelect>,
    /// The name to match, as its own control.
    filter: Entity<InputState>,
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
    ///
    /// Read through [`CaixonhoApp::location`] rather than directly: a position
    /// is the current one only while the connection it was read on is still
    /// active, and saying so is the accessor's job.
    position: Option<Position>,
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

/// What one transfer reports back to the window.
enum TransferEvent {
    Progress {
        bytes: u64,
        total: Option<u64>,
    },
    Settled(caixonho_core::transfer::DownloadOutcome),
    /// Uploads settle once and report no progress — see `XONHO-0020`
    /// design: the SDK offers no counting hook that survives a retried
    /// body, and a counter that jumps backwards is worse than none.
    UploadSettled(caixonho_core::transfer::UploadOutcome),
}

/// The one transfer this slice allows, and everything the window says about
/// it. One, deliberately: the queue is the rest of M2, and this struct is
/// what that change replaces (`XONHO-0007` design, "One transfer at a time").
struct Transfer {
    /// Where the object came from — shown, and needed again if a collision
    /// answer re-issues the download.
    bucket: String,
    key: String,
    /// Where it is going.
    directory: std::path::PathBuf,
    /// Open with the system once finished, and clean up the question of
    /// where: `true` only for downloads into the open-cache.
    then_open: bool,
    /// Which way the bytes are going. A direction rather than a second
    /// struct: the states are the same five, the collision question is the
    /// same three buttons, and the queue change should replace one holder
    /// rather than two.
    direction: Direction,
    /// The local file being sent, for an upload.
    source: Option<std::path::PathBuf>,
    bytes: u64,
    total: Option<u64>,
    cancel: caixonho_core::transfer::Cancel,
    phase: TransferPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Down,
    Up,
}

enum TransferPhase {
    Running,
    /// The destination already has a file of this name; the user decides.
    NameTaken {
        name: String,
    },
    /// The bucket already has an object at this key; the user decides.
    /// Distinct from `NameTaken` because what is in the way is someone
    /// else's data rather than a file on this machine, and the words differ.
    KeyTaken {
        key: String,
    },
    /// This endpoint will not do the conditional write, so nothing can
    /// promise the object at that key survives. Proceeding is the user's
    /// explicit second act.
    ConditionUnsupported {
        key: String,
    },
    /// The object is up, at `key` — which is not always the key asked for.
    Sent {
        key: String,
        stepped_aside: bool,
    },
    Finished {
        name: String,
        mapped: caixonho_core::transfer::MappingOutcome,
    },
    Cancelled,
    Failed(Error),
}

/// One deletion, from confirmation to its aftermath (`XONHO-0021`).
///
/// Its own state and its own strip — deliberately not a `Transfer` arm:
/// deletion moves nothing, and "Downloading…" vocabulary does not belong one
/// enum away from a destructive verb. The connection rides along so a stale
/// outcome can never offer an Undo against an account the user has left
/// (`XONHO-0019`'s discipline).
struct Deletion {
    connection: ConnectionId,
    bucket: String,
    key: String,
    phase: DeletePhase,
}

enum DeletePhase {
    /// The second act has not happened; nothing has been deleted.
    Confirming,
    /// The delete is in flight.
    Deleting,
    /// The service accepted it. `marker` is the undo's proof and token.
    Gone { marker: Option<String> },
    /// The undo is in flight. The marker travelled with the spawn; nothing
    /// here needs it again, and a field nobody reads is how retry-shaped
    /// ideas sneak in unreviewed.
    Restoring,
    /// The marker is gone; the object is back.
    Restored,
    /// A delete or undo failed. `during_undo` picks the words.
    Failed { error: Error, during_undo: bool },
}

/// Making a folder, from naming it to what became of it (`XONHO-0024`).
///
/// Its own state rather than a phase on `Deletion`, for that type's own
/// reason: nothing here is destructive, and a confirmation strip written in
/// the danger colour is not where a benign act belongs. The connection rides
/// along for `XONHO-0019`'s discipline — an answer that arrives after the
/// user has switched accounts must not report a folder into a bucket of the
/// same name somewhere else.
struct MakingFolder {
    connection: ConnectionId,
    bucket: String,
    at: Prefix,
    kind: BucketKind,
    phase: FolderPhase,
}

enum FolderPhase {
    /// The name is being typed; nothing has been sent.
    Naming,
    /// The marker is in flight.
    Making,
    /// It is there.
    Made { key: String },
    /// The name could not be one. Nothing was sent.
    BadName(caixonho_core::folder::BadFolderName),
    /// A directory bucket keeps a folder only while something is in it.
    /// Nothing was sent, and nothing failed.
    NotOnADirectoryBucket,
    /// The service refused it.
    Failed(Error),
}

/// What a deletion reports back to the window.
enum DeleteEvent {
    Settled(caixonho_core::session::DeleteOutcome),
    UndoSettled(caixonho_core::session::UndoOutcome),
}

/// One preview, in flight or on screen (`XONHO-0008`).
///
/// Carries its connection for the `XONHO-0019` discipline: a page fetched
/// under one account must never render under another's name.
struct Preview {
    connection: ConnectionId,
    key: String,
    phase: PreviewPhase,
}

enum PreviewPhase {
    /// The fetch is in flight.
    Loading,
    /// A first page of text, with the numbers for the truncation line.
    Text {
        content: SharedString,
        shown: u64,
        total: Option<u64>,
    },
    /// A whole raster, ready to draw.
    Image(std::sync::Arc<gpui::Image>),
    /// The name said text; the bytes said otherwise.
    Binary,
    /// Over the gate; nothing was fetched.
    TooLarge { size: u64 },
    /// No preview serves this kind.
    NoPreview,
    /// The fetch failed, with its classified cause.
    Failed(Error),
}

/// Where the user is, and the connection they got there on.
///
/// The two travel together because the spec's location *is* the pair: the
/// `object-browsing` requirement names a connection, a bucket and a prefix as
/// one answer, while `Location` — core's addressing form — carries only the
/// last two. Keeping the connection *beside* the location instead of with it
/// is what let a pane outlive the connection it belonged to (`XONHO-0019`).
struct Position {
    /// The connection the location was read on.
    connection: ConnectionId,
    /// Bucket and prefix, in core's addressing form.
    at: Location,
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

/// Everything `CaixonhoApp` needs from outside itself.
///
/// It exists so the application can be **given** its world instead of reading
/// it. `CaixonhoApp::new` used to build the runtime, resolve `~/.aws`, prepare
/// trust material and read the connections file inline, which meant a test
/// that constructed a window read the developer's own machine and answered
/// differently on each one (`XONHO-0015`).
///
/// The runtime lives here rather than being built in the constructor because
/// it must outlive every spawn, and a caller that has to remember that
/// separately will one day not.
pub(crate) struct World {
    pub(crate) runtime: tokio::runtime::Runtime,
    pub(crate) session: Option<Session>,
    /// Set when trust material or the runtime could not be prepared at all.
    pub(crate) startup_error: Option<Error>,
    pub(crate) profiles: Vec<Profile>,
    pub(crate) stored: Vec<StoredCredential>,
    pub(crate) connections_error: Option<Error>,
}

impl World {
    /// Read from the machine this process is running on. What `main` calls,
    /// and the only place in this crate that touches the environment.
    pub(crate) fn from_env() -> Self {
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

        Self {
            runtime,
            session,
            startup_error,
            profiles,
            stored,
            connections_error,
        }
    }
}

#[cfg(test)]
impl World {
    /// A world with no machine in it.
    ///
    /// Every window test starts here, so it is worth being one line: a
    /// current-thread runtime, a session whose trust material comes from the
    /// OS store rather than from `AWS_CA_BUNDLE`, config paths that name
    /// nothing, and `store` where the S3 adapter would be. No profile is
    /// discovered and no connection is remembered, because a test that wants
    /// either should say so rather than inherit it.
    ///
    /// The session is real, not `None`. A world with no session and no startup
    /// error is a state the application never reaches in production, and a
    /// test standing in one is testing a shape nobody ships.
    pub(crate) fn scripted(store: std::sync::Arc<dyn caixonho_core::ObjectStore>) -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a current-thread runtime");
        let session = Session::new(
            runtime.handle().clone(),
            HttpStack::with_ca_bundle(None).expect("the OS trust store alone builds a client"),
            ConfigPaths {
                config: None,
                credentials: None,
            },
        );
        let credentials = session.credentials_changed("test");
        session.install_object_store(store, credentials);

        Self {
            runtime,
            session: Some(session),
            startup_error: None,
            profiles: Vec::new(),
            stored: Vec::new(),
            connections_error: None,
        }
    }
}

impl CaixonhoApp {
    pub(crate) fn new(
        diagnostics: Diagnostics,
        world: World,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let World {
            runtime,
            session,
            startup_error,
            profiles,
            stored,
            connections_error,
        } = world;
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
        // The kind choices never change — there are two kinds of bucket and
        // there always will be — so unlike the region selector this one is
        // built once and never re-offered.
        let kind_select = cx.new(|cx| {
            RegionSelect::new(
                SearchableVec::new(
                    KindChoice::all()
                        .iter()
                        .map(KindChoice::label)
                        .collect::<Vec<SharedString>>(),
                ),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        cx.subscribe(
            &kind_select,
            |app, _, event: &SelectEvent<SearchableVec<SharedString>>, cx| {
                let SelectEvent::Confirm(label) = event;
                let Some(label) = label else { return };
                let Some(chosen) = KindChoice::all()
                    .into_iter()
                    .find(|choice| choice.label() == *label)
                else {
                    return;
                };
                app.narrowing.kind = chosen;
                app.narrow_rows(cx);
            },
        )
        .detach();
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by name"));
        let folder_name = cx.new(|cx| InputState::new(window, cx).placeholder("Folder name"));
        cx.subscribe(&filter, |app, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                app.narrowing.name = state.read(cx).value().to_string();
                app.narrow_rows(cx);
            }
        })
        .detach();
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
                    app.narrowing.region = choice;
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

        // Downloads report progress per chunk and one settlement; both cross
        // to the window here (`XONHO-0007`).
        let (transfers, transferring) = flume::unbounded::<TransferEvent>();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = transferring.recv_async().await {
                let applied = this.update_in(cx, |app, _, cx| app.apply_transfer(event, cx));
                if applied.is_err() {
                    break; // The window is gone.
                }
            }
        })
        .detach();

        // Deletions get their own channel rather than riding the transfer
        // one — same reasoning as the strip: plumbing is cheap, and mixed
        // vocabulary is not (`XONHO-0021`).
        let (deletions, deleting) = flume::unbounded::<DeleteEvent>();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = deleting.recv_async().await {
                let applied =
                    this.update_in(cx, |app, window, cx| app.apply_delete(event, window, cx));
                if applied.is_err() {
                    break; // The window is gone.
                }
            }
        })
        .detach();

        // And a channel for folders, for the same reason again (`XONHO-0024`).
        let (folders, foldering) = flume::unbounded::<caixonho_core::session::FolderOutcome>();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(outcome) = foldering.recv_async().await {
                let applied = this.update_in(cx, |app, window, cx| {
                    app.folder_settled(outcome, window, cx)
                });
                if applied.is_err() {
                    break; // The window is gone.
                }
            }
        })
        .detach();

        let (previews, previewing) = flume::unbounded::<caixonho_core::preview::PreviewOutcome>();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(outcome) = previewing.recv_async().await {
                let applied = this.update_in(cx, |app, _, cx| app.apply_preview(outcome, cx));
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
            transfer: None,
            transfers,
            deletion: None,
            making_folder: None,
            folder_name,
            folder_inbox: folders,
            deletions,
            preview: None,
            previews,
            confirming: None,
            unavailable: std::collections::HashMap::new(),
            narrowing: Narrowing::default(),
            kind_select,
            filter,
            region_options: vec![RegionChoice::All],
            region_select,
            position: None,
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
        // The narrowings go with the account they were chosen for. A filter
        // set on one account and silently applied to the next is how someone
        // comes to believe a bucket has gone missing — and the region choice
        // could not survive anyway, since the regions on offer are derived
        // from whichever listing arrives.
        self.clear_narrowing(window, cx);
        // Clears the previous profile's rows and error before anything of the
        // new one's arrives.
        self.outcome.switch_to(id);
        self.set_rows(Vec::new(), window, cx);
        // Before the new listing is asked for, not after it arrives: the
        // previous connection's bucket should not be on screen during the
        // wait either.
        self.end_location(cx);
        self.issue(id, source, cx);
    }

    /// Where the user is, or nothing when they are not inside a bucket.
    ///
    /// Nothing, too, when the position was read on a connection that is no
    /// longer active: a location reached on one account is not a location on
    /// the next, so it is not shown as though it were. The guard is what makes
    /// a forgotten reset harmless rather than merely unlikely — every reader
    /// of position comes through here.
    fn location(&self) -> Option<&Location> {
        self.position
            .as_ref()
            .filter(|position| position.connection == self.outcome.active())
            .map(|position| &position.at)
    }

    /// Go to `location` and read it.
    ///
    /// Everything shown about position is derived from the location this sets,
    /// so there is nowhere else to keep in step.
    fn go_to(&mut self, location: Location, window: &mut Window, cx: &mut Context<Self>) {
        // Walking somewhere else takes the deletion strip along — its key
        // belongs to the location it was deleted at. A re-read of the *same*
        // location keeps it, because that re-read is how the strip's own
        // outcome refreshes the listing (`XONHO-0021`).
        if self.location() != Some(&location) {
            self.deletion = None;
            self.preview = None;
        }
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
        self.position = Some(Position {
            connection: self.outcome.active(),
            at: location.clone(),
        });
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
        if self.location() != Some(&asked) {
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
                // The bucket answered from somewhere other than where it was
                // looked for. The correction arrives with the data it
                // corrects, so the row is put right here rather than by the
                // window asking after every page whether anything moved.
                if let Some(region) = &page.served_from {
                    self.table.update(cx, |state, cx| {
                        state.delegate_mut().correct_region(&asked.bucket, region);
                        cx.notify();
                    });
                }

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
        let (Some(location), Some(cursor)) = (self.location().cloned(), self.more.clone()) else {
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
            self.location().cloned(),
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

    /// The selected object's key, when the selection is an object.
    fn selected_object_key(&self, cx: &Context<Self>) -> Option<String> {
        let index = self.objects.read(cx).selected_row()?;
        match self.objects.read(cx).delegate().row(index)? {
            crate::views::objects::Entry::Object(object) => Some(object.key.clone()),
            crate::views::objects::Entry::Folder(_) => None,
        }
    }

    /// Download the selected object to a directory the user chooses
    /// (`XONHO-0007` task 4.1).
    fn download_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(location), Some(key)) = (self.location().cloned(), self.selected_object_key(cx))
        else {
            return;
        };
        let ask = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Download here".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            // Cancelled dialogs and platform errors both mean "no destination
            // was chosen", and choosing nothing starts nothing.
            let Ok(Ok(Some(mut directories))) = ask.await else {
                return;
            };
            let Some(directory) = directories.pop() else {
                return;
            };
            let _ = this.update_in(cx, |app, _, cx| {
                app.start_download(
                    location.bucket.clone(),
                    key,
                    directory,
                    caixonho_core::transfer::Collision::Ask,
                    false,
                    cx,
                );
            });
        })
        .detach();
    }

    /// Open the selected object with the system's own application for it
    /// (`XONHO-0007` task 4.3): download to the open-cache, then hand over.
    fn open_selected(&mut self, cx: &mut Context<Self>) {
        let (Some(location), Some(key)) = (self.location().cloned(), self.selected_object_key(cx))
        else {
            return;
        };
        let Some(cache) = caixonho_core::transfer::open_cache_dir() else {
            // A machine with no resolvable cache directory: say so as a
            // failed transfer, because nothing was transferred.
            self.transfer = Some(Transfer {
                bucket: location.bucket.clone(),
                key,
                directory: std::path::PathBuf::new(),
                then_open: true,
                direction: Direction::Down,
                source: None,
                bytes: 0,
                total: None,
                cancel: caixonho_core::transfer::Cancel::default(),
                phase: TransferPhase::Failed(Error::MissingConfiguration {
                    profile: None,
                    detail: "this machine offers no cache directory to open through".into(),
                }),
            });
            cx.notify();
            return;
        };
        if let Err(error) = std::fs::create_dir_all(&cache) {
            self.transfer = Some(Transfer {
                bucket: location.bucket.clone(),
                key,
                directory: cache,
                then_open: true,
                direction: Direction::Down,
                source: None,
                bytes: 0,
                total: None,
                cancel: caixonho_core::transfer::Cancel::default(),
                phase: TransferPhase::Failed(Error::Destination {
                    detail: error.to_string(),
                }),
            });
            cx.notify();
            return;
        }
        // Replace, not ask: the cache is ours, its contents are re-downloads
        // by definition, and a question about clobbering a stale cached copy
        // would be the application asking permission to do its job.
        self.start_download(
            location.bucket.clone(),
            key,
            cache,
            caixonho_core::transfer::Collision::Replace,
            true,
            cx,
        );
    }

    /// Send a local file into the location on screen (`XONHO-0020` task
    /// 4.1). Unlike Open and Download…, this needs no selection — the
    /// destination is the location, not a row.
    /// Start naming a folder to be made here (`XONHO-0024`).
    ///
    /// The bucket's kind is read now, from the listing already held, rather
    /// than at the moment of the request: a directory bucket removes a folder
    /// as soon as it is empty, so there is no point sending anything, and
    /// discovering that by attempting it would sometimes *succeed* and leave a
    /// zero-byte object in a store whose model is that directories are
    /// structural.
    fn new_folder_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = self.location().cloned() else {
            return;
        };
        let kind = self
            .table
            .read(cx)
            .delegate()
            .kind_of(&location.bucket)
            .unwrap_or(BucketKind::General);
        self.folder_name.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.making_folder = Some(MakingFolder {
            connection: self.outcome.active(),
            bucket: location.bucket.clone(),
            at: location.prefix.clone(),
            kind,
            phase: FolderPhase::Naming,
        });
        cx.notify();
    }

    /// Send it, once there is a name.
    fn confirm_new_folder(&mut self, cx: &mut Context<Self>) {
        let Some(making) = self.making_folder.as_mut() else {
            return;
        };
        if !matches!(making.phase, FolderPhase::Naming) {
            return;
        }
        let name = self.folder_name.read(cx).value().to_string();
        let (bucket, at, kind) = (making.bucket.clone(), making.at.clone(), making.kind);
        making.phase = FolderPhase::Making;

        if let Some(session) = self.session.clone() {
            let inbox = self.folder_inbox.clone();
            session.spawn_create_folder(bucket, at, kind, name, move |outcome| {
                let _ = inbox.send(outcome);
            });
        }
        cx.notify();
    }

    /// Apply what became of it, unless the user has left that connection.
    fn folder_settled(
        &mut self,
        outcome: caixonho_core::session::FolderOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use caixonho_core::session::FolderOutcome;
        let Some(making) = self.making_folder.as_mut() else {
            return;
        };
        if making.connection != self.outcome.active() {
            // A folder made into an account the user has left must not be
            // announced over the one they are looking at (`XONHO-0019`).
            self.making_folder = None;
            cx.notify();
            return;
        }
        making.phase = match outcome {
            FolderOutcome::Made { key } => FolderPhase::Made { key },
            FolderOutcome::BadName(bad) => FolderPhase::BadName(bad),
            FolderOutcome::NotOnADirectoryBucket => FolderPhase::NotOnADirectoryBucket,
            FolderOutcome::Failed(cause) => FolderPhase::Failed(cause),
        };
        // A folder nobody can see is a folder nobody believes in.
        if matches!(making.phase, FolderPhase::Made { .. })
            && let Some(location) = self.location().cloned()
        {
            self.go_to(location, window, cx);
        }
        cx.notify();
    }

    fn upload_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = self.location().cloned() else {
            return;
        };
        let ask = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Upload".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(mut chosen))) = ask.await else {
                return; // Cancelled dialog, or the platform refused it.
            };
            let Some(path) = chosen.pop() else {
                return;
            };
            let Some(name) = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToOwned::to_owned)
            else {
                return;
            };
            let key = format!("{}{name}", location.prefix.as_str());
            let _ = this.update_in(cx, |app, _, cx| {
                app.start_upload(
                    location.bucket.clone(),
                    key,
                    path,
                    caixonho_core::transfer::Collision::Ask,
                    cx,
                );
            });
        })
        .detach();
    }

    /// Start one upload and hold it as the window's transfer.
    fn start_upload(
        &mut self,
        bucket: String,
        key: String,
        path: std::path::PathBuf,
        collision: caixonho_core::transfer::Collision,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let total = std::fs::metadata(&path).ok().map(|meta| meta.len());
        let inbox = self.transfers.clone();
        let cancel = session.spawn_upload(
            bucket.clone(),
            key.clone(),
            path.clone(),
            collision,
            move |outcome| {
                let _ = inbox.send(TransferEvent::UploadSettled(outcome));
            },
        );
        self.transfer = Some(Transfer {
            bucket,
            key,
            directory: path.parent().map(ToOwned::to_owned).unwrap_or_default(),
            then_open: false,
            direction: Direction::Up,
            source: Some(path),
            bytes: 0,
            // Known up front and shown as a total, with no fraction: an
            // upload reports no progress in this slice, and the size is
            // still worth saying.
            total,
            cancel,
            phase: TransferPhase::Running,
        });
        cx.notify();
    }

    /// Answer the taken-*key* question by sending again with the answer.
    fn answer_key_collision(
        &mut self,
        collision: caixonho_core::transfer::Collision,
        cx: &mut Context<Self>,
    ) {
        let Some(transfer) = self.transfer.take() else {
            return;
        };
        let Some(source) = transfer.source else {
            return;
        };
        self.start_upload(transfer.bucket, transfer.key, source, collision, cx);
    }

    /// Preview the selected object (`XONHO-0008` task 4.1).
    fn preview_selected(&mut self, cx: &mut Context<Self>) {
        let (Some(location), Some(index)) = (
            self.location().cloned(),
            self.objects.read(cx).selected_row(),
        ) else {
            return;
        };
        let Some(crate::views::objects::Entry::Object(object)) =
            self.objects.read(cx).delegate().row(index).cloned()
        else {
            return;
        };
        let Some(session) = self.session.clone() else {
            return;
        };
        self.preview = Some(Preview {
            connection: self.outcome.active(),
            key: object.key.clone(),
            phase: PreviewPhase::Loading,
        });
        let inbox = self.previews.clone();
        session.spawn_preview(location.bucket, object.key, object.size, move |outcome| {
            let _ = inbox.send(outcome);
        });
        cx.notify();
    }

    /// Apply a preview outcome, unless its preview has left the screen.
    fn apply_preview(
        &mut self,
        outcome: caixonho_core::preview::PreviewOutcome,
        cx: &mut Context<Self>,
    ) {
        let Some(preview) = self.preview.as_mut() else {
            return; // Backed out while the fetch was in flight.
        };
        if preview.connection != self.outcome.active() {
            // Fetched under an account the user has left; render nothing
            // under the new one's name (`XONHO-0019`).
            self.preview = None;
            cx.notify();
            return;
        }
        use caixonho_core::preview::{PreviewOutcome, RasterKind};
        preview.phase = match outcome {
            PreviewOutcome::Text {
                content,
                shown,
                total,
            } => PreviewPhase::Text {
                content: content.into(),
                shown,
                total,
            },
            PreviewOutcome::Image { bytes, format } => {
                let format = match format {
                    RasterKind::Png => gpui::ImageFormat::Png,
                    RasterKind::Jpeg => gpui::ImageFormat::Jpeg,
                    RasterKind::Gif => gpui::ImageFormat::Gif,
                    RasterKind::Webp => gpui::ImageFormat::Webp,
                    RasterKind::Bmp => gpui::ImageFormat::Bmp,
                };
                PreviewPhase::Image(std::sync::Arc::new(gpui::Image::from_bytes(format, bytes)))
            }
            PreviewOutcome::Binary => PreviewPhase::Binary,
            PreviewOutcome::ImageTooLarge { size } => PreviewPhase::TooLarge { size },
            PreviewOutcome::NoPreview => PreviewPhase::NoPreview,
            PreviewOutcome::Failed(error) => PreviewPhase::Failed(error),
        };
        cx.notify();
    }

    /// Ask to delete the selected object (`XONHO-0021` task 3.1).
    ///
    /// Asks. The first act deletes nothing: it puts the named-key
    /// confirmation on screen, and only the confirmation's own button
    /// issues the delete.
    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let (Some(location), Some(key)) = (self.location().cloned(), self.selected_object_key(cx))
        else {
            return;
        };
        self.deletion = Some(Deletion {
            connection: self.outcome.active(),
            bucket: location.bucket,
            key,
            phase: DeletePhase::Confirming,
        });
        cx.notify();
    }

    /// The second act: the confirmation's own button.
    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(deletion) = self.deletion.as_mut() else {
            return;
        };
        if !matches!(deletion.phase, DeletePhase::Confirming) {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        deletion.phase = DeletePhase::Deleting;
        let inbox = self.deletions.clone();
        session.spawn_delete(
            deletion.bucket.clone(),
            deletion.key.clone(),
            move |outcome| {
                let _ = inbox.send(DeleteEvent::Settled(outcome));
            },
        );
        cx.notify();
    }

    /// Undo: remove the marker the delete's own response reported.
    fn undo_delete(&mut self, cx: &mut Context<Self>) {
        let Some(deletion) = self.deletion.as_mut() else {
            return;
        };
        let DeletePhase::Gone {
            marker: Some(marker),
        } = &deletion.phase
        else {
            return; // No proof, no undo — the button only exists on proof.
        };
        let Some(session) = self.session.clone() else {
            return;
        };
        let marker = marker.clone();
        deletion.phase = DeletePhase::Restoring;
        let inbox = self.deletions.clone();
        session.spawn_undo_delete(
            deletion.bucket.clone(),
            deletion.key.clone(),
            marker,
            move |outcome| {
                let _ = inbox.send(DeleteEvent::UndoSettled(outcome));
            },
        );
        cx.notify();
    }

    /// Apply a delete or undo outcome, unless the deletion it belongs to has
    /// left the screen — dismissed, or the connection was switched.
    fn apply_delete(&mut self, event: DeleteEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(deletion) = self.deletion.as_mut() else {
            return; // Dismissed while in flight; nothing to apply to.
        };
        if deletion.connection != self.outcome.active() {
            // A switch happened. An outcome — and above all an Undo — from
            // the account the user left must not render under the new one's
            // name (`XONHO-0019`).
            self.deletion = None;
            cx.notify();
            return;
        }
        use caixonho_core::session::{DeleteOutcome, UndoOutcome};
        match event {
            DeleteEvent::Settled(DeleteOutcome::Gone { marker }) => {
                deletion.phase = DeletePhase::Gone { marker };
                // The row leaves because the service says so: re-read.
                if let Some(location) = self.location().cloned() {
                    self.go_to(location, window, cx);
                }
            }
            DeleteEvent::Settled(DeleteOutcome::Failed(error)) => {
                deletion.phase = DeletePhase::Failed {
                    error,
                    during_undo: false,
                };
            }
            DeleteEvent::UndoSettled(UndoOutcome::Restored) => {
                deletion.phase = DeletePhase::Restored;
                if let Some(location) = self.location().cloned() {
                    self.go_to(location, window, cx);
                }
            }
            DeleteEvent::UndoSettled(UndoOutcome::Failed(error)) => {
                deletion.phase = DeletePhase::Failed {
                    error,
                    during_undo: true,
                };
            }
        }
        cx.notify();
    }

    /// Start one download and hold it as the window's transfer.
    fn start_download(
        &mut self,
        bucket: String,
        key: String,
        directory: std::path::PathBuf,
        collision: caixonho_core::transfer::Collision,
        then_open: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let progress_inbox = self.transfers.clone();
        let settled_inbox = self.transfers.clone();
        let cancel = session.spawn_download(
            bucket.clone(),
            key.clone(),
            directory.clone(),
            collision,
            move |bytes, total| {
                let _ = progress_inbox.send(TransferEvent::Progress { bytes, total });
            },
            move |outcome| {
                let _ = settled_inbox.send(TransferEvent::Settled(outcome));
            },
        );
        self.transfer = Some(Transfer {
            bucket,
            key,
            directory,
            then_open,
            direction: Direction::Down,
            source: None,
            bytes: 0,
            total: None,
            cancel,
            phase: TransferPhase::Running,
        });
        cx.notify();
    }

    /// Apply one transfer event, if a transfer is still on screen to apply
    /// it to.
    fn apply_transfer(&mut self, event: TransferEvent, cx: &mut Context<Self>) {
        let Some(transfer) = self.transfer.as_mut() else {
            return; // Dismissed while the event was in flight.
        };
        match event {
            TransferEvent::Progress { bytes, total } => {
                transfer.bytes = bytes;
                transfer.total = total;
            }
            TransferEvent::UploadSettled(outcome) => {
                use caixonho_core::transfer::UploadOutcome;
                transfer.phase = match outcome {
                    UploadOutcome::Finished {
                        key,
                        stepped_aside,
                        bytes,
                    } => {
                        transfer.bytes = bytes;
                        TransferPhase::Sent { key, stepped_aside }
                    }
                    UploadOutcome::KeyTaken { key } => TransferPhase::KeyTaken { key },
                    UploadOutcome::ConditionUnsupported { key } => {
                        TransferPhase::ConditionUnsupported { key }
                    }
                    UploadOutcome::Cancelled => TransferPhase::Cancelled,
                    UploadOutcome::Failed(error) => TransferPhase::Failed(error),
                };
            }
            TransferEvent::Settled(outcome) => {
                use caixonho_core::transfer::DownloadOutcome;
                match outcome {
                    DownloadOutcome::Finished {
                        name,
                        mapped,
                        bytes,
                    } => {
                        transfer.bytes = bytes;
                        if transfer.then_open {
                            // Handed to the platform's opener. gpui's call
                            // reports nothing back on any platform, so the
                            // finished line below keeps saying where the file
                            // is (with Reveal) — the report the spec asks for
                            // when an opener refuses, shown whether or not it
                            // did.
                            cx.open_with_system(&transfer.directory.join(&name));
                        }
                        transfer.phase = TransferPhase::Finished { name, mapped };
                    }
                    DownloadOutcome::NameTaken { name } => {
                        transfer.phase = TransferPhase::NameTaken { name };
                    }
                    DownloadOutcome::Cancelled => {
                        transfer.phase = TransferPhase::Cancelled;
                    }
                    DownloadOutcome::Failed(error) => {
                        transfer.phase = TransferPhase::Failed(error);
                    }
                }
            }
        }
        cx.notify();
    }

    /// Answer the existing-file question by starting over with the answer.
    fn answer_collision(
        &mut self,
        collision: caixonho_core::transfer::Collision,
        cx: &mut Context<Self>,
    ) {
        let Some(transfer) = self.transfer.take() else {
            return;
        };
        self.start_download(
            transfer.bucket,
            transfer.key,
            transfer.directory,
            collision,
            transfer.then_open,
            cx,
        );
    }

    /// Open the bucket in the table's row `index`.
    fn open_bucket(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.table.read(cx).delegate().name_at(index) else {
            return;
        };
        self.go_to(Location::bucket(name), window, cx);
    }

    /// End the current location, whatever the reason for ending it.
    ///
    /// One method and not two. Leaving a bucket and switching connection both
    /// end a location, and while they were written separately the switch
    /// quietly omitted every part of it — which is the defect `XONHO-0019`
    /// exists for. The read guard on [`Self::location`] makes this
    /// belt-and-braces rather than load-bearing, and that split is deliberate:
    /// the guard keeps the display correct, this keeps the state honest.
    fn end_location(&mut self, cx: &mut Context<Self>) {
        self.position = None;
        // A deletion's confirmation or outcome is about a key at a location;
        // leaving the location takes it along (`XONHO-0021`). The preview is
        // about one too (`XONHO-0008`).
        self.deletion = None;
        self.preview = None;
        self.listing = Listing::Idle;
        self.more = None;
        self.fetching = false;
        self.objects.update(cx, |table, cx| {
            table
                .delegate_mut()
                .show(Prefix::root(), Vec::new(), Vec::new());
            cx.notify();
        });
    }

    /// Leave the bucket entirely, back to the account's listing.
    fn leave_bucket(&mut self, cx: &mut Context<Self>) {
        self.end_location(cx);
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
            Outcome::Loaded(listing) => {
                let buckets = listing.buckets.clone();
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
        self.narrowing.region = self.narrowing.region.clone().retained_for(&rows);
        self.region_options = region_choices(&rows);

        let labels: Vec<SharedString> = self.region_options.iter().map(region_label).collect();
        let selected = self
            .region_options
            .iter()
            .position(|choice| *choice == self.narrowing.region)
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

    /// Put every narrowing back to showing everything.
    ///
    /// The controls are reset with the state, not merely the state: a cleared
    /// filter whose text box still holds a word is a screen disagreeing with
    /// itself about what is in force.
    fn clear_narrowing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.narrowing = Narrowing::default();
        self.kind_select.update(cx, |select, cx| {
            select.set_selected_index(Some(IndexPath::new(0)), window, cx);
        });
        self.filter.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
    }

    /// Apply the region choice to the listing already held.
    ///
    /// No request is made: the service can filter a listing by region, but only
    /// for a request sent to an endpoint in that same region, which would cost
    /// a client per region to narrow a list already in hand.
    fn narrow_rows(&mut self, cx: &mut Context<Self>) {
        let narrowing = self.narrowing.clone();
        self.table.update(cx, |state, cx| {
            state.delegate_mut().narrow(&narrowing);
            cx.notify();
        });
        // Still reported after narrowing, and this line is load-bearing: the
        // viewport is built from the *shown* rows, so a bucket left out of
        // `shown` is never probed. That is exactly why the accessibility
        // narrowing removes observed denials rather than keeping observed
        // opens — the second would hide every unanswered bucket, which would
        // stop it being probed, which would mean its answer never came.
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

    /// Everything that narrows the account listing, and what it is hiding.
    fn region_picker(&self, cx: &Context<Self>) -> impl IntoElement {
        // The badge stays, and stays *derived*: it says every shown row
        // happens to be a directory bucket, which is a different sentence from
        // the kind control's "show me directory buckets" — an account can be
        // all directory buckets without anyone having asked for that. It is
        // suppressed only when the kind control already says the same thing,
        // because two controls saying one thing is where a screen starts to
        // lie about which of them is in force.
        let all_directory = matches!(
            self.table.read(cx).delegate().shown_kind(),
            Some(BucketKind::Directory)
        ) && self.narrowing.kind == KindChoice::Any;
        // The count these narrowings need — "N of M buckets" — was already on
        // screen before this change, in the status bar, and it reads
        // `shown.len()` against the listing's own length so it covers every
        // narrowing including the new ones. A second count up here was written
        // and then deleted: two controls saying almost the same thing is how a
        // screen starts lying about which of them is in force.

        // Filled when it is on, ghosted when it is not — the same way every
        // other toggle in this window says so. A label that changed with the
        // state would be a second answer to a question the fill has answered.
        let accessible_only = Button::new("accessible-only")
            .label("Accessible only")
            .on_click(cx.listener(|app, _, _, cx| {
                app.narrowing.accessible_only = !app.narrowing.accessible_only;
                app.narrow_rows(cx);
            }));
        let accessible_only = if self.narrowing.accessible_only {
            accessible_only.primary()
        } else {
            accessible_only.ghost()
        };

        h_flex()
            .gap_2()
            .pb_2()
            .items_center()
            .child(
                div()
                    .w(px(200.))
                    .child(Select::new(&self.region_select).title_prefix("Region: ")),
            )
            .child(
                div()
                    .w(px(190.))
                    .debug_selector(|| "kind-choice".into())
                    .child(Select::new(&self.kind_select).title_prefix("Kind: ")),
            )
            .child(
                div()
                    .w(px(200.))
                    .child(Input::new(&self.filter).cleanable(true)),
            )
            .child(
                div()
                    .debug_selector(|| "accessible-only".into())
                    .child(accessible_only),
            )
            .children(all_directory.then(|| {
                div()
                    .debug_selector(|| "all-directory".into())
                    .child(status_badge(
                        IconName::LayoutDashboard,
                        "All directory buckets",
                        cx.theme().primary,
                    ))
            }))
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
        // Only while inside one. That is the whole reason this group exists —
        // the main panel gives itself over to the contents, and without it the
        // account disappears. At account level the table already lists every
        // bucket, with room for the full name, so the rail was repeating it in
        // a third of the width.
        self.location()?;
        let names = self.table.read(cx).delegate().shown_names();
        if names.is_empty() {
            return None;
        }
        let here = self.location().map(|at| at.bucket.clone());

        Some(
            SidebarGroup::new("Buckets").child(SidebarMenu::new().children(names.into_iter().map(
                |(name, kind)| {
                    let active = here.as_deref() == Some(name.as_str());
                    // The chosen half alone. The zone is identical on every
                    // bucket in it, so in a 220px rail it is the half that
                    // costs the most width and carries the least — and put in
                    // the item's suffix it took priority over the label, which
                    // shrank to three letters. The full name is on the row in
                    // the table, which has the width for it.
                    let label = match (kind, split_zonal_name(&name)) {
                        (BucketKind::Directory, Some((chosen, _))) => chosen.to_owned(),
                        _ => name.clone(),
                    };
                    SidebarMenuItem::new(label)
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
            session_name: at.label().to_owned().into(),
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
        let offerable = self.offerable_sign_in(error);
        let guidance = guidance_for(error, offerable.is_some());
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
            // The two object verbs live here, beside the location they act
            // in, and only light up when the selection is an object. Open is
            // deliberately a visible button and double-click deliberately
            // stays unbound (owner decision 2026-08-24): a stray double-click
            // must not be enough to write company bytes to disk and hand
            // them to a third-party application.
            let on_object = self.selected_object_key(cx).is_some();
            return h_flex()
                .w_full()
                .items_center()
                .gap(space::TIGHT)
                .child(div().flex_1().child(trail))
                .child(
                    div().debug_selector(|| "preview-action".into()).child(
                        Button::new("preview-action")
                            .label("Preview")
                            .ghost()
                            .disabled(!on_object)
                            .on_click(cx.listener(|app, _, _, cx| app.preview_selected(cx))),
                    ),
                )
                .child(
                    div().debug_selector(|| "open-action".into()).child(
                        Button::new("open-action")
                            .label("Open")
                            .ghost()
                            .disabled(!on_object)
                            .on_click(cx.listener(|app, _, _, cx| app.open_selected(cx))),
                    ),
                )
                .child(
                    div().debug_selector(|| "download-action".into()).child(
                        Button::new("download-action")
                            .label("Download…")
                            .ghost()
                            .disabled(!on_object)
                            .on_click(
                                cx.listener(|app, _, window, cx| app.download_selected(window, cx)),
                            ),
                    ),
                )
                .child(
                    div().debug_selector(|| "upload-action".into()).child(
                        Button::new("upload-action")
                            .label("Upload…")
                            .ghost()
                            // No `disabled`: unlike the other two verbs this
                            // one acts on the location, not on a row, and a
                            // location is what being here means.
                            .on_click(
                                cx.listener(|app, _, window, cx| app.upload_here(window, cx)),
                            ),
                    ),
                )
                .child(
                    div().debug_selector(|| "new-folder-action".into()).child(
                        Button::new("new-folder-action")
                            .label("New folder…")
                            .ghost()
                            // Acts on the location like `Upload…` does, so it
                            // needs no selected row and carries no `disabled`.
                            .on_click(
                                cx.listener(|app, _, window, cx| app.new_folder_here(window, cx)),
                            ),
                    ),
                )
                .child(
                    // Apart from the three benign verbs, and in the danger
                    // colour: this is the one that destroys. It only opens
                    // the confirmation — nothing deletes on this click.
                    div().debug_selector(|| "delete-action".into()).child(
                        Button::new("delete-action")
                            .label("Delete…")
                            .ghost()
                            .danger()
                            .disabled(!on_object)
                            .on_click(cx.listener(|app, _, _, cx| app.delete_selected(cx))),
                    ),
                )
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

    /// The preview, in the listing's place (`XONHO-0008`).
    fn preview_surface(&mut self, location: Location, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(preview) = self.preview.as_ref() else {
            return v_flex().into_any_element();
        };
        let key = preview.key.clone();

        let body = match &preview.phase {
            PreviewPhase::Loading => skeleton_rows(6),
            PreviewPhase::Text {
                content,
                shown,
                total,
            } => {
                let mut page = v_flex().size_full().gap(space::TIGHT);
                // The truncation line exists exactly when the object goes on
                // past what was fetched, and both numbers are the service's.
                if let Some(total) = total.filter(|total| total > shown) {
                    page = page.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "First {} of {} — Open or Download for the rest.",
                                crate::views::objects::readable(*shown),
                                crate::views::objects::readable(total)
                            )),
                    );
                }
                page.child(
                    div()
                        .id("preview-text")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .font_family("monospace")
                        .text_sm()
                        .child(content.clone()),
                )
                .into_any_element()
            }
            PreviewPhase::Image(image) => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(gpui::img(image.clone()).max_w_full().max_h_full())
                .into_any_element(),
            PreviewPhase::Binary => empty_state(
                IconName::File,
                "This is not text.",
                "The name suggested text, the bytes did not. Open it with the \
                 application that owns this kind.",
                cx,
            ),
            PreviewPhase::TooLarge { size } => empty_state(
                IconName::File,
                "Too large to preview.",
                // The size is said, per the spec — clippy flagged this field
                // as unread, which was the requirement going unmet, not a
                // field going spare.
                format!(
                    "This image is {} — over the 20 MiB preview limit. Open or \
                     download it instead.",
                    crate::views::objects::readable(*size)
                ),
                cx,
            ),
            PreviewPhase::NoPreview => empty_state(
                IconName::File,
                "No preview for this kind.",
                "Open it with the application that owns it — the preview shows \
                 text and images only.",
                cx,
            ),
            PreviewPhase::Failed(error) => {
                let rendered = error.to_string();
                let panel = self.failure_panel_from(rendered, error, cx);
                panel.into_any_element()
            }
        };

        v_flex()
            .debug_selector(|| "preview-surface".into())
            .size_full()
            .gap(space::TIGHT)
            // The path bar stays — the preview replaces the listing, not the
            // location (`XONHO-0008` plan, held to).
            .child(self.path_bar(&location, cx))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(space::TIGHT)
                    .child(
                        div().debug_selector(|| "preview-back".into()).child(
                            Button::new("preview-back").label("Back").ghost().on_click(
                                cx.listener(|app, _, window, cx| {
                                    app.preview = None;
                                    // Back lands on a fresh listing, the same
                                    // re-read the deletion strip uses.
                                    if let Some(location) = app.location().cloned() {
                                        app.go_to(location, window, cx);
                                    }
                                }),
                            ),
                        ),
                    )
                    .child(div().text_sm().child(key)),
            )
            .child(v_flex().flex_1().min_h_0().child(body))
            .into_any_element()
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
            .children(self.transfer_line(cx))
            .children(self.deletion_line(cx))
            .children(self.folder_line(cx))
    }

    /// Making a folder, from naming it to what became of it (`XONHO-0024`).
    ///
    /// One strip under the listing, like the transfer's and the deletion's —
    /// and, like the deletion's, its own rather than a shared one, because the
    /// wording is where the meaning is and nothing here is destructive.
    fn folder_line(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let making = self.making_folder.as_ref()?;

        let dismiss = |id: &'static str| {
            Button::new(id)
                .label("Dismiss")
                .ghost()
                .on_click(cx.listener(|app, _, _, cx| {
                    app.making_folder = None;
                    cx.notify();
                }))
        };

        let line = match &making.phase {
            FolderPhase::Naming => h_flex()
                .debug_selector(|| "folder-naming-strip".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(div().text_sm().child("New folder here:"))
                .child(div().w(px(260.)).child(Input::new(&self.folder_name)))
                .child(div().flex_1())
                .child(
                    div().debug_selector(|| "folder-confirm".into()).child(
                        Button::new("folder-confirm")
                            .label("Create")
                            .primary()
                            .on_click(cx.listener(|app, _, _, cx| app.confirm_new_folder(cx))),
                    ),
                )
                .child(dismiss("folder-cancel")),
            FolderPhase::Making => h_flex()
                .debug_selector(|| "folder-making-strip".into())
                .w_full()
                .items_center()
                .child(div().text_sm().child("Making the folder…")),
            FolderPhase::Made { key } => h_flex()
                .debug_selector(|| "folder-made-strip".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(div().text_sm().child(format!("`{key}` is there.")))
                .child(div().flex_1())
                .child(dismiss("folder-made-dismiss")),
            // Nothing failed, and the wording has to carry that or the user
            // goes looking for a broken bucket. It names what a directory
            // bucket does and what makes a folder there instead.
            //
            // A strip and not a card, which took the owner pointing at it. The
            // slot under the listing has one voice — `transfer_line` and
            // `deletion_line` are both flat full-width lines, text left,
            // actions right — and this arrived as a bordered, shadowed card
            // with an icon tile: a heavier treatment than a *failed upload*
            // gets, for a message where nothing is even wrong.
            FolderPhase::NotOnADirectoryBucket => h_flex()
                .debug_selector(|| "folder-not-here-strip".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(div().text_sm().child(
                    "This bucket keeps a folder only while something is in it — upload a \
                     file into the path you want and the folder comes with it.",
                ))
                .child(div().flex_1())
                .child(dismiss("folder-not-here-dismiss")),
            FolderPhase::BadName(bad) => h_flex()
                .debug_selector(|| "folder-bad-name-strip".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(bad.to_string()),
                )
                .child(div().flex_1())
                .child(dismiss("folder-bad-name-dismiss")),
            FolderPhase::Failed(error) => h_flex()
                .debug_selector(|| "folder-failed-strip".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(format!("The folder could not be made — {error}")),
                )
                .child(div().flex_1())
                .child(dismiss("folder-failed-dismiss")),
        };
        Some(line.into_any_element())
    }

    /// The deletion, from its confirmation to its aftermath — one line under
    /// the listing, like the transfer's, and deliberately not the same
    /// widget (`XONHO-0021`).
    fn deletion_line(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let deletion = self.deletion.as_ref()?;
        let key = deletion.key.clone();

        let dismiss = || {
            Button::new("delete-dismiss")
                .label("Dismiss")
                .ghost()
                .on_click(cx.listener(|app, _, _, cx| {
                    app.deletion = None;
                    cx.notify();
                }))
        };

        let line = match &deletion.phase {
            DeletePhase::Confirming => h_flex()
                .debug_selector(|| "delete-confirm-strip".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                // The strong wording, always: no safety net is promised
                // before the service has produced one. The net is announced
                // after, when the response proves it exists.
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(format!("Delete `{key}` from this bucket?")),
                )
                .child(div().flex_1())
                .child(
                    div().debug_selector(|| "delete-confirm".into()).child(
                        Button::new("delete-confirm")
                            .label("Delete")
                            .danger()
                            .on_click(cx.listener(|app, _, _, cx| app.confirm_delete(cx))),
                    ),
                )
                .child(
                    div().debug_selector(|| "delete-cancel".into()).child(
                        Button::new("delete-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.deletion = None;
                                cx.notify();
                            })),
                    ),
                )
                .into_any_element(),
            DeletePhase::Deleting => h_flex()
                .debug_selector(|| "delete-in-flight".into())
                .w_full()
                .items_center()
                .child(div().text_sm().child(format!("Deleting `{key}`…")))
                .into_any_element(),
            DeletePhase::Gone { marker } => {
                let mut line = h_flex()
                    .debug_selector(|| "delete-gone".into())
                    .w_full()
                    .gap(space::TIGHT)
                    .items_center();
                line = match marker {
                    // The undo exists exactly when the response proved it.
                    Some(_) => line
                        .child(div().text_sm().child(format!(
                            "Deleted `{key}` — the bucket versions, so a marker was placed."
                        )))
                        .child(div().flex_1())
                        .child(
                            div().debug_selector(|| "delete-undo".into()).child(
                                Button::new("delete-undo")
                                    .label("Undo")
                                    .ghost()
                                    .on_click(cx.listener(|app, _, _, cx| app.undo_delete(cx))),
                            ),
                        ),
                    None => line
                        .child(div().text_sm().child(format!(
                            "Deleted `{key}`. This bucket keeps no versions — it is gone."
                        )))
                        .child(div().flex_1()),
                };
                line.child(dismiss()).into_any_element()
            }
            DeletePhase::Restoring => h_flex()
                .debug_selector(|| "delete-restoring".into())
                .w_full()
                .items_center()
                .child(div().text_sm().child(format!("Restoring `{key}`…")))
                .into_any_element(),
            DeletePhase::Restored => h_flex()
                .debug_selector(|| "delete-restored".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(div().text_sm().child(format!("`{key}` is back.")))
                .child(div().flex_1())
                .child(dismiss())
                .into_any_element(),
            DeletePhase::Failed { error, during_undo } => {
                let rendered = error.to_string();
                h_flex()
                    .debug_selector(|| "delete-failed".into())
                    .w_full()
                    .gap(space::TIGHT)
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().danger)
                            // A failed undo must not read as a failed
                            // delete: the object is still deleted and the
                            // marker still stands.
                            .child(if *during_undo {
                                format!(
                                    "Could not restore `{key}` — the marker still stands: {rendered}"
                                )
                            } else {
                                format!("Delete failed: {rendered}")
                            }),
                    )
                    .child(div().flex_1())
                    .child(dismiss())
                    .into_any_element()
            }
        };
        Some(
            div()
                .w_full()
                .px(space::TIGHT)
                .py(space::TIGHT)
                .child(line)
                .into_any_element(),
        )
    }

    /// The one transfer, said under the listing it belongs to — a line, not
    /// a panel: the queue gets a panel, one download gets a sentence
    /// (`XONHO-0007` tasks 4.1–4.3).
    fn transfer_line(&self, cx: &Context<Self>) -> Option<AnyElement> {
        use caixonho_core::transfer::MappingOutcome;
        let transfer = self.transfer.as_ref()?;

        let dismiss = || {
            Button::new("transfer-dismiss")
                .label("Dismiss")
                .ghost()
                .on_click(cx.listener(|app, _, _, cx| {
                    app.transfer = None;
                    cx.notify();
                }))
        };

        let line = match &transfer.phase {
            TransferPhase::Running => h_flex()
                .debug_selector(|| "transfer-progress".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(div().text_sm().child(match transfer.direction {
                    // An upload reports no fraction because it has none to
                    // report (`XONHO-0020` design): the size is said, the
                    // progress is not invented.
                    Direction::Up => match transfer.total {
                        Some(total) => {
                            format!("Uploading — {}", crate::views::objects::readable(total))
                        }
                        None => "Uploading…".to_owned(),
                    },
                    Direction::Down => format!(
                        "Downloading — {}",
                        match transfer.total {
                            Some(total) => format!(
                                "{} of {}",
                                crate::views::objects::readable(transfer.bytes),
                                crate::views::objects::readable(total)
                            ),
                            None => crate::views::objects::readable(transfer.bytes),
                        }
                    ),
                }))
                .child(div().flex_1())
                .child(
                    div().debug_selector(|| "transfer-cancel".into()).child(
                        Button::new("transfer-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click({
                                let cancel = transfer.cancel.clone();
                                cx.listener(move |_, _, _, _| cancel.cancel())
                            }),
                    ),
                )
                .into_any_element(),
            TransferPhase::NameTaken { name } => h_flex()
                .debug_selector(|| "transfer-name-taken".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .child(format!("`{name}` is already in that folder.")),
                )
                .child(div().flex_1())
                .child(
                    Button::new("collision-replace")
                        .label("Replace")
                        .ghost()
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.answer_collision(caixonho_core::transfer::Collision::Replace, cx)
                        })),
                )
                .child(
                    Button::new("collision-keep-both")
                        .label("Keep both")
                        .ghost()
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.answer_collision(caixonho_core::transfer::Collision::KeepBoth, cx)
                        })),
                )
                .child(
                    Button::new("collision-abandon")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.transfer = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
            TransferPhase::Finished { name, mapped } => {
                let said = match (transfer.then_open, mapped) {
                    // The opener's own failure is invisible to gpui on every
                    // platform, so the where is always said: this line is the
                    // spec's "the file exists and the report says where it
                    // is", shown whether or not the opener obliged.
                    (true, _) => format!("Downloaded `{name}` and handed it to the system."),
                    (false, MappingOutcome::Unchanged) => format!("Downloaded `{name}`."),
                    // §4.4: every substitution is reported, not silently
                    // absorbed.
                    (false, _) => format!(
                        "Downloaded as `{name}` — the object's name needed changing to be a \
                         filename here."
                    ),
                };
                h_flex()
                    .debug_selector(|| "transfer-finished".into())
                    .w_full()
                    .gap(space::TIGHT)
                    .items_center()
                    .child(div().text_sm().child(said))
                    .child(div().flex_1())
                    .child(
                        Button::new("transfer-reveal")
                            .label("Reveal")
                            .ghost()
                            .on_click({
                                let path = transfer.directory.join(name);
                                cx.listener(move |_, _, _, cx| cx.reveal_path(&path))
                            }),
                    )
                    .child(dismiss())
                    .into_any_element()
            }
            TransferPhase::KeyTaken { key } => {
                h_flex()
                    .debug_selector(|| "transfer-key-taken".into())
                    .w_full()
                    .gap(space::TIGHT)
                    .items_center()
                    // "already has an object", not "already exists": what is in
                    // the way is data someone put there, and the sentence should
                    // read like it.
                    .child(
                        div()
                            .text_sm()
                            .child(format!("`{key}` already has an object in this bucket.")),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("key-replace")
                            .label("Replace")
                            .ghost()
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.answer_key_collision(
                                    caixonho_core::transfer::Collision::Replace,
                                    cx,
                                )
                            })),
                    )
                    .child(
                        Button::new("key-keep-both")
                            .label("Keep both")
                            .ghost()
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.answer_key_collision(
                                    caixonho_core::transfer::Collision::KeepBoth,
                                    cx,
                                )
                            })),
                    )
                    .child(Button::new("key-abandon").label("Cancel").ghost().on_click(
                        cx.listener(|app, _, _, cx| {
                            app.transfer = None;
                            cx.notify();
                        }),
                    ))
                    .into_any_element()
            }
            TransferPhase::ConditionUnsupported { key } => h_flex()
                .debug_selector(|| "transfer-condition-unsupported".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                // The one screen where this application says it cannot keep
                // the promise the rest of the feature is built on.
                .child(div().text_sm().text_color(cx.theme().danger).child(format!(
                    "This endpoint will not promise to leave an existing object \
                             alone. Sending `{key}` may replace one."
                )))
                .child(div().flex_1())
                .child(
                    Button::new("send-anyway")
                        .label("Send anyway")
                        .ghost()
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.answer_key_collision(
                                caixonho_core::transfer::Collision::Replace,
                                cx,
                            )
                        })),
                )
                .child(
                    Button::new("unsupported-abandon")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.transfer = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
            TransferPhase::Sent { key, stepped_aside } => h_flex()
                .debug_selector(|| "transfer-sent".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(div().text_sm().child(if *stepped_aside {
                    // Loudly: the object is not where its file name implies,
                    // and a user who is not told will look for it there.
                    format!("Uploaded as `{key}` — the name you sent was already taken.")
                } else {
                    format!("Uploaded `{key}`.")
                }))
                .child(div().flex_1())
                .child(dismiss())
                .into_any_element(),
            TransferPhase::Cancelled => h_flex()
                .debug_selector(|| "transfer-cancelled".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(div().text_sm().child(match transfer.direction {
                    Direction::Up => "Upload cancelled.",
                    Direction::Down => "Download cancelled — nothing was written.",
                }))
                .child(div().flex_1())
                .child(dismiss())
                .into_any_element(),
            TransferPhase::Failed(error) => {
                // The same vocabulary every other failure uses, sized to a
                // line. The destination path is the window's own knowledge,
                // said here precisely because the error must not carry it.
                let rendered = error.to_string();
                h_flex()
                    .debug_selector(|| "transfer-failed".into())
                    .w_full()
                    .gap(space::TIGHT)
                    .items_center()
                    .child(div().text_sm().text_color(cx.theme().danger).child(
                        match transfer.direction {
                            Direction::Up => format!("Upload failed: {rendered}"),
                            Direction::Down => format!("Download failed: {rendered}"),
                        },
                    ))
                    .child(div().flex_1())
                    .child(dismiss())
                    .into_any_element()
            }
        };
        Some(
            div()
                .w_full()
                .px(space::TIGHT)
                .py(space::TIGHT)
                .child(line)
                .into_any_element(),
        )
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
        // `views/objects.rs` (task 1.1, amended). A preview, when one is
        // open, takes the listing's place — path bar intact, Back to leave
        // (`XONHO-0008`).
        if let Some(location) = self.location().cloned() {
            if self.preview.is_some() {
                return self.preview_surface(location, cx).into_any_element();
            }
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
            // Nothing came back, and the reason matters: an account that is
            // empty and an account whose listing was refused look identical
            // from here, and calling the second one empty is the lie this
            // change exists to stop telling.
            Outcome::Loaded(listing) if listing.buckets.is_empty() => match &listing.refused {
                Some(refused) => empty_state(
                    IconName::TriangleAlert,
                    refusal_headline(refused),
                    refusal_detail(refused),
                    cx,
                ),
                None => empty_state(
                    IconName::Folder,
                    "This account has no buckets.",
                    "The listing succeeded — there is simply nothing in it yet.",
                    cx,
                ),
            },
            Outcome::Loaded(listing) => {
                // An account that holds nothing and an account whose buckets
                // are all narrowed away must not read the same: the only cure
                // for the second is knowing which control emptied it, and the
                // controls stay on screen above this so undoing it is one
                // click rather than a hunt.
                let hidden = self.table.read(cx).delegate().hidden_by_narrowing();
                v_flex()
                    .size_full()
                    .child(self.region_picker(cx))
                    .children(
                        listing
                            .refused
                            .as_ref()
                            .map(|refused| Self::refusal_line(refused, cx)),
                    )
                    .child(if hidden {
                        div()
                            .debug_selector(|| "hidden-by-narrowing".into())
                            .flex_1()
                            .child(empty_state(
                                IconName::Search,
                                "Every bucket is hidden by the filters.",
                                "The account has buckets — these are narrowed out. \
                                 Widen or clear a filter above to see them.",
                                cx,
                            ))
                    } else {
                        div()
                            .relative()
                            .min_h_0()
                            .flex_1()
                            .child(DataTable::new(&self.table))
                            .child(scroll::accelerator(self.table.clone(), self.accel.clone()))
                    })
                    .into_any_element()
            }
        }
    }

    /// What was refused, stated beside what answered.
    ///
    /// Deliberately not the failure panel. The screen is not a failure —
    /// buckets came back and they are real — so a panel in the list's place
    /// would overstate it. This says what is missing without taking the
    /// list's place, which is the difference between partial and broken.
    fn refusal_line(refused: &RefusedListing, cx: &App) -> AnyElement {
        h_flex()
            .w_full()
            .gap(space::TIGHT)
            .items_start()
            .pb_2()
            .debug_selector(|| "listing-refused".into())
            // The badge keeps its width; the sentence takes what is left and
            // wraps inside it. Without `min_w_0` a flex child refuses to
            // shrink below its own text, and the line runs off the window
            // instead of folding — which is what it did.
            .child(status_badge(
                IconName::TriangleAlert,
                refusal_headline(refused),
                cx.theme().warning,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(refusal_detail(refused)),
            )
            .into_any_element()
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
            (Some(_), Outcome::Loaded(_)) => {
                // Says both numbers while anything is narrowing: reporting the
                // account's total beside a narrowed table reads as rows lost.
                //
                // Both numbers from the delegate, which is the only thing that
                // knows the final set. It used to count `shown` here and the
                // total from the listing — two sources for one sentence, and
                // `XONHO-0025` gave them three more chances to disagree.
                let (shown, total) = self.table.read(cx).delegate().shown_of_loaded();
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
                                            guidance_for(error, false),
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

#[cfg(test)]
mod tests {
    //! What the window does with what arrives, driven through the real view
    //! rather than through a delegate lifted out of it — which is the half
    //! `XONHO-0018` could not reach and `XONHO-0015` exists to open.

    use super::*;
    use caixonho_core::store::double::StoreDouble;
    use caixonho_core::{Object, Observation, Region, Scope, types::Prefix as CorePrefix};
    use gpui::TestAppContext;
    use gpui_component::table::TableDelegate as _;
    use std::sync::Arc;

    fn bucket(name: &str) -> Bucket {
        Bucket {
            name: name.to_owned(),
            created: None,
            region: Region::Unknown,
            kind: BucketKind::General,
        }
    }

    /// A window over a session that reads from a script, holding two buckets
    /// and looking inside the first.
    fn looking_at<'a>(
        cx: &'a mut TestAppContext,
        bucket_name: &str,
    ) -> (gpui::Entity<CaixonhoApp>, &'a mut gpui::VisualTestContext) {
        cx.update(gpui_component::init);
        let store: Arc<dyn caixonho_core::ObjectStore> = Arc::new(StoreDouble::allows_listing());
        let (app, cx) = cx.add_window_view(|window, cx| {
            CaixonhoApp::new(
                Diagnostics::without_a_log(),
                World::scripted(store),
                window,
                cx,
            )
        });
        let looking = Location::at(bucket_name.to_owned(), CorePrefix::root());
        app.update(cx, |app, cx| {
            app.table.update(cx, |state, _| {
                let delegate = state.delegate_mut();
                delegate.rows = vec![bucket("reports"), bucket("logs")];
                delegate.shown = vec![0, 1];
            });
            app.position = Some(Position {
                connection: app.outcome.active(),
                at: looking,
            });
            cx.notify();
        });
        (app, cx)
    }

    fn region_of(
        app: &gpui::Entity<CaixonhoApp>,
        cx: &mut gpui::VisualTestContext,
        row: usize,
    ) -> Region {
        app.read_with(cx, |app, cx| {
            app.table.read(cx).delegate().rows[row].region.clone()
        })
    }

    #[gpui::test]
    fn a_page_served_from_elsewhere_corrects_that_bucket_and_no_other(cx: &mut TestAppContext) {
        // `correct_region` has had a test since XONHO-0018. What had not been
        // tested is that `apply_page` calls it, with the bucket the page was
        // for — which is the half a user would have seen go wrong.
        let (app, cx) = looking_at(cx, "reports");

        app.update(cx, |app, cx| {
            app.apply_page(
                Location::at("reports".to_owned(), CorePrefix::root()),
                Ok(Page {
                    served_from: Some(Region::Known("us-west-2".to_owned())),
                    ..Page::default()
                }),
                cx,
            );
        });

        assert_eq!(
            region_of(&app, cx, 0),
            Region::Known("us-west-2".to_owned()),
            "the bucket the page was for now reports the region that served it"
        );
        assert_eq!(
            region_of(&app, cx, 1),
            Region::Unknown,
            "and the bucket it was not for is untouched"
        );
    }

    #[gpui::test]
    fn a_page_for_a_location_already_left_corrects_nothing(cx: &mut TestAppContext) {
        // `apply_page` returns early for a page nobody is looking at. Without
        // a test for it, a version that dropped the early return would pass
        // everything else — and would correct a row from a read the user had
        // already navigated away from.
        let (app, cx) = looking_at(cx, "reports");

        app.update(cx, |app, cx| {
            app.apply_page(
                Location::at("logs".to_owned(), CorePrefix::root()),
                Ok(Page {
                    served_from: Some(Region::Known("eu-west-1".to_owned())),
                    ..Page::default()
                }),
                cx,
            );
        });

        assert_eq!(
            region_of(&app, cx, 1),
            Region::Unknown,
            "a page for a screen nobody is on changes nothing"
        );
    }

    /// A window over two connections, with no profile chosen and no bucket
    /// open. Two `profiles` rather than stored credentials, because that is
    /// the shorter of the two paths into `connections()`.
    fn with_two_connections(
        cx: &mut TestAppContext,
    ) -> (gpui::Entity<CaixonhoApp>, &mut gpui::VisualTestContext) {
        cx.update(gpui_component::init);
        let store: Arc<dyn caixonho_core::ObjectStore> = Arc::new(StoreDouble::allows_listing());
        let (app, cx) = cx.add_window_view(|window, cx| {
            CaixonhoApp::new(
                Diagnostics::without_a_log(),
                World::scripted(store),
                window,
                cx,
            )
        });
        app.update(cx, |app, cx| {
            app.profiles = vec![
                Profile {
                    name: "first".to_owned(),
                    is_default: true,
                },
                Profile {
                    name: "second".to_owned(),
                    is_default: false,
                },
            ];
            cx.notify();
        });
        (app, cx)
    }

    /// Where the window says it is.
    ///
    /// Read in one place on purpose: the change that moves the position behind
    /// an accessor moves this line, and not the tests that ask the question.
    fn position(
        app: &gpui::Entity<CaixonhoApp>,
        cx: &mut gpui::VisualTestContext,
    ) -> Option<Location> {
        app.read_with(cx, |app, _| app.location().cloned())
    }

    #[gpui::test]
    fn switching_connections_ends_the_position(cx: &mut TestAppContext) {
        // The defect the owner found by driving the real application on
        // 2026-08-21: the sidebar follows the switch and the pane does not.
        // `leave_bucket` clears the position; `select_profile` never learned
        // to, and nothing in the types made it.
        let (app, cx) = with_two_connections(cx);

        app.update_in(cx, |app, window, cx| {
            app.select_profile(0, window, cx);
            app.go_to(
                Location::at("reports".to_owned(), CorePrefix::root()),
                window,
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(
            position(&app, cx).map(|at| at.bucket),
            Some("reports".to_owned()),
            "the window should be inside the first connection's bucket before the switch"
        );

        app.update_in(cx, |app, window, cx| app.select_profile(1, window, cx));
        cx.run_until_parked();

        assert_eq!(
            position(&app, cx),
            None,
            "after switching connections the window still reports a position, so the trail, \
             the path bar and the contents of the previous connection's bucket are all still \
             on screen"
        );
    }

    /// How many object rows the contents table is holding.
    fn object_rows(app: &gpui::Entity<CaixonhoApp>, cx: &mut gpui::VisualTestContext) -> usize {
        app.read_with(cx, |app, cx| app.objects.read(cx).delegate().rows_count(cx))
    }

    #[gpui::test]
    fn a_switch_leaves_no_contents_behind_while_the_next_account_loads(cx: &mut TestAppContext) {
        // The window the reproduction actually hit. The sidebar had already
        // moved to the new connection and its listing had not answered yet —
        // and in that gap the pane was still showing the previous account's
        // objects. The read guard alone does not cover this: it stops the
        // stale position being *shown*, and leaves it *held*.
        let (app, cx) = with_two_connections(cx);

        app.update_in(cx, |app, window, cx| {
            app.select_profile(0, window, cx);
            app.go_to(
                Location::at("reports".to_owned(), CorePrefix::root()),
                window,
                cx,
            );
            app.apply_page(
                Location::at("reports".to_owned(), CorePrefix::root()),
                Ok(Page {
                    objects: vec![Object {
                        key: "march.csv".to_owned(),
                        size: 12,
                        last_modified: None,
                        storage_class: None,
                        etag: None,
                    }],
                    ..Page::default()
                }),
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(
            object_rows(&app, cx),
            1,
            "the first connection's bucket should be holding its one object before the switch"
        );

        // The second connection's listing is never answered, so this asserts
        // on the gap rather than on what comes after it.
        app.update_in(cx, |app, window, cx| app.select_profile(1, window, cx));
        cx.run_until_parked();

        assert_eq!(
            object_rows(&app, cx),
            0,
            "the previous connection's objects are still in the contents table while the new \
             connection loads"
        );
        assert!(
            app.read_with(cx, |app, _| matches!(app.listing, Listing::Idle)),
            "the listing still reports the previous connection's read rather than resting"
        );
    }

    #[gpui::test]
    fn re_selecting_the_same_connection_also_ends_the_location(cx: &mut TestAppContext) {
        // Accepted behaviour, not an accident: that click re-lists the
        // account, so landing back on the bucket table is the coherent answer
        // to it. The test exists so nobody later "fixes" it by comparing
        // profile index instead of connection id — which would make the guard
        // depend on a second notion of sameness, the shape of the original
        // defect.
        let (app, cx) = with_two_connections(cx);

        app.update_in(cx, |app, window, cx| {
            app.select_profile(0, window, cx);
            app.go_to(
                Location::at("reports".to_owned(), CorePrefix::root()),
                window,
                cx,
            );
        });
        cx.run_until_parked();

        app.update_in(cx, |app, window, cx| app.select_profile(0, window, cx));
        cx.run_until_parked();

        assert_eq!(
            position(&app, cx),
            None,
            "re-selecting the connection already selected kept the location, so a reconnect \
             lands somewhere the fresh listing has not been read for"
        );
    }

    #[gpui::test]
    fn a_position_is_never_attributed_to_a_bucket_of_the_same_name_elsewhere(
        cx: &mut TestAppContext,
    ) {
        // The motivating case from the proposal: a `-dev` and a `-prod`
        // profile of one project, each holding a bucket of the same name.
        // It passes by construction — the guard compares connection ids and
        // never looks at a name — and is kept anyway, because it is the
        // scenario the spec states and the one where being wrong would be
        // worst: the screen would be confidently naming the wrong account.
        // The test that would catch a guard rewritten to compare *profile
        // index* is the re-selection one below, not this.
        let (app, cx) = with_two_connections(cx);

        app.update_in(cx, |app, window, cx| {
            app.select_profile(0, window, cx);
            app.go_to(
                Location::at("reports".to_owned(), CorePrefix::root()),
                window,
                cx,
            );
        });
        cx.run_until_parked();

        app.update_in(cx, |app, window, cx| app.select_profile(1, window, cx));
        cx.run_until_parked();

        assert_eq!(
            position(&app, cx),
            None,
            "a bucket of the same name on the newly selected connection is a different \
             place, and the trail from the previous one is being shown for it"
        );

        // And the same name, opened deliberately on the new connection, is
        // this connection's position rather than a revival of the old one.
        app.update_in(cx, |app, window, cx| {
            app.go_to(
                Location::at("reports".to_owned(), CorePrefix::root()),
                window,
                cx,
            );
        });
        cx.run_until_parked();

        assert_eq!(
            position(&app, cx).map(|at| at.bucket),
            Some("reports".to_owned()),
            "opening the bucket on the connection now selected should be a position again"
        );
    }

    /// A real view, rendered to an image — `XONHO-0009` task 6.3.
    ///
    /// macOS only, and not by preference. `gpui_platform::current_headless_renderer`
    /// returns `Some` on `target_os = "macos"` and `None` everywhere else
    /// (`gpui_platform.rs:85`), so there is no image to capture on Windows —
    /// which `AGENTS.md` calls this project's primary daily driver. A green
    /// suite is therefore **not** evidence about what Windows draws;
    /// `debug_bounds`, which needs no renderer, is what covers both.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_real_view_renders_to_an_image() {
        use gpui::{HeadlessAppContext, NoopTextSystem, px, size};

        const WIDTH: u32 = 1280;
        const HEIGHT: u32 = 800;

        let store: Arc<dyn caixonho_core::ObjectStore> = Arc::new(StoreDouble::allows_listing());
        let mut cx = HeadlessAppContext::with_platform(
            Arc::new(NoopTextSystem::new()),
            Arc::new(gpui_component_assets::Assets),
            gpui_platform::current_headless_renderer,
        );
        cx.update(gpui_component::init);

        // A world with one profile in it. `World::scripted` deliberately
        // discovers none — a test that wants a connection should say so — and
        // saying so is not optional here: `render` answers
        // `connections().is_empty()` with "No connections yet." and returns
        // **before** it ever consults `outcome`. Without this every state below
        // is staged correctly, drawn faithfully, and invisible; the first
        // twelve images were that screen twelve times, and the state probe
        // said `Loaded(buckets=4)` while the pixels said otherwise.
        let mut world = World::scripted(store);
        world.profiles = vec![caixonho_core::Profile {
            name: "example".to_owned(),
            is_default: true,
        }];

        let window = cx
            .open_window(size(px(WIDTH as f32), px(HEIGHT as f32)), |window, cx| {
                cx.new(|cx| CaixonhoApp::new(Diagnostics::without_a_log(), world, window, cx))
            })
            .expect("a headless window opens");
        cx.run_until_parked();

        let image = cx
            .capture_screenshot(window.into())
            .expect("the renderer produced an image");

        // Device pixels, not logical ones: on a 2x display a 1280x800 window
        // comes back as 2560x1600. Asserted as a whole-number scale of the
        // window rather than as a number, so this says the same thing on a
        // 1x display as on a Retina one.
        let scale = image.width() / WIDTH;
        assert!(scale >= 1, "the image is narrower than the window it is of");
        assert_eq!(
            (image.width(), image.height()),
            (WIDTH * scale, HEIGHT * scale),
            "the image is not a whole-number scale of the window"
        );
        assert!(
            image.pixels().any(|p| p.0[3] != 0),
            "every pixel is transparent, so nothing was drawn at all"
        );
    }

    // ---- The one transfer (XONHO-0007 tasks 4.1–4.3) ----

    fn an_object(key: &str, size: u64) -> Object {
        Object {
            key: key.to_owned(),
            size,
            last_modified: None,
            storage_class: None,
            etag: None,
        }
    }

    /// The verbs gate on the selection being an object: a folder can be
    /// neither downloaded nor opened, and no selection is no object.
    #[gpui::test]
    fn the_object_verbs_light_up_only_on_an_object(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.objects.update(cx, |state, _| {
                state.delegate_mut().show(
                    CorePrefix::root(),
                    vec![caixonho_core::Folder {
                        prefix: CorePrefix::parse("daily/"),
                    }],
                    vec![an_object("summary.csv", 10)],
                );
            });
        });

        app.update(cx, |app, cx| {
            assert_eq!(app.selected_object_key(cx), None, "nothing selected yet");
            app.objects
                .update(cx, |state, cx| state.set_selected_row(0, cx));
            assert_eq!(
                app.selected_object_key(cx),
                None,
                "a folder is selected, and a folder is not an object"
            );
            app.objects
                .update(cx, |state, cx| state.set_selected_row(1, cx));
            assert_eq!(
                app.selected_object_key(cx).as_deref(),
                Some("summary.csv"),
                "the object row is what the verbs act on"
            );
        });
    }

    /// The window's half of a download: progress accumulates, and the
    /// settled outcome becomes the line's phase. The pump and session halves
    /// are core's tests; what is asserted here is that the window applies
    /// what arrives.
    #[gpui::test]
    fn a_download_reports_progress_and_then_that_it_finished(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.transfer = Some(Transfer {
                bucket: "reports".into(),
                key: "daily/summary.csv".into(),
                directory: std::env::temp_dir(),
                then_open: false,
                direction: Direction::Down,
                source: None,
                bytes: 0,
                total: None,
                cancel: caixonho_core::transfer::Cancel::default(),
                phase: TransferPhase::Running,
            });
            app.apply_transfer(
                TransferEvent::Progress {
                    bytes: 512,
                    total: Some(1024),
                },
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            let transfer = app.transfer.as_ref().expect("still on screen");
            assert_eq!((transfer.bytes, transfer.total), (512, Some(1024)));
            assert!(matches!(transfer.phase, TransferPhase::Running));
        });

        app.update(cx, |app, cx| {
            app.apply_transfer(
                TransferEvent::Settled(caixonho_core::transfer::DownloadOutcome::Finished {
                    name: "summary.csv".into(),
                    mapped: caixonho_core::transfer::MappingOutcome::Unchanged,
                    bytes: 1024,
                }),
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            let transfer = app
                .transfer
                .as_ref()
                .expect("the line stays for the report");
            match &transfer.phase {
                TransferPhase::Finished { name, .. } => assert_eq!(name, "summary.csv"),
                other => panic!(
                    "expected Finished, got a different phase: {}",
                    phase_name(other)
                ),
            }
        });
    }

    /// An event landing after the line was dismissed changes nothing — the
    /// same stale-answer discipline every other channel already has.
    #[gpui::test]
    fn a_settlement_after_dismissal_is_dropped(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.transfer = None;
            app.apply_transfer(
                TransferEvent::Settled(caixonho_core::transfer::DownloadOutcome::Cancelled),
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            assert!(app.transfer.is_none(), "nothing came back from the dead");
        });
    }

    /// Answering the existing-file question starts the download over with
    /// the answer — synchronously back into Running, holding the same
    /// object.
    #[gpui::test]
    fn answering_a_collision_reissues_the_download(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.transfer = Some(Transfer {
                bucket: "reports".into(),
                key: "daily/summary.csv".into(),
                directory: std::env::temp_dir(),
                then_open: false,
                direction: Direction::Down,
                source: None,
                bytes: 0,
                total: None,
                cancel: caixonho_core::transfer::Cancel::default(),
                phase: TransferPhase::NameTaken {
                    name: "summary.csv".into(),
                },
            });
            app.answer_collision(caixonho_core::transfer::Collision::KeepBoth, cx);
        });
        app.read_with(cx, |app, _| {
            let transfer = app.transfer.as_ref().expect("reissued");
            assert!(matches!(transfer.phase, TransferPhase::Running));
            assert_eq!(transfer.key, "daily/summary.csv", "the same object");
        });
    }

    // ---- Uploading (XONHO-0020 tasks 4.1–4.2) ----

    fn an_upload(phase: TransferPhase) -> Transfer {
        Transfer {
            bucket: "reports".into(),
            key: "daily/summary.csv".into(),
            directory: std::env::temp_dir(),
            then_open: false,
            direction: Direction::Up,
            source: Some(std::env::temp_dir().join("summary.csv")),
            bytes: 0,
            total: Some(4096),
            cancel: caixonho_core::transfer::Cancel::default(),
            phase,
        }
    }

    /// The taken-key answer re-sends the *same local file* to the *same key*
    /// with the answer attached — losing either would upload the wrong thing
    /// or to the wrong place, and neither is visible on screen.
    #[gpui::test]
    fn answering_a_taken_key_resends_the_same_file_to_the_same_key(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.transfer = Some(an_upload(TransferPhase::KeyTaken {
                key: "daily/summary.csv".into(),
            }));
            app.answer_key_collision(caixonho_core::transfer::Collision::KeepBoth, cx);
        });
        app.read_with(cx, |app, _| {
            let transfer = app.transfer.as_ref().expect("reissued");
            assert!(matches!(transfer.phase, TransferPhase::Running));
            assert_eq!(transfer.key, "daily/summary.csv");
            assert_eq!(transfer.direction, Direction::Up);
            assert!(transfer.source.is_some(), "the file to send is still known");
        });
    }

    /// An upload settling walks the same window path a download does, and
    /// each outcome reaches its own phase.
    #[gpui::test]
    fn each_upload_outcome_becomes_its_own_phase(cx: &mut TestAppContext) {
        use caixonho_core::transfer::UploadOutcome;
        let (app, cx) = looking_at(cx, "reports");

        let cases: Vec<(UploadOutcome, &'static str)> = vec![
            (
                UploadOutcome::Finished {
                    key: "daily/summary (2).csv".into(),
                    stepped_aside: true,
                    bytes: 4096,
                },
                "Sent",
            ),
            (
                UploadOutcome::KeyTaken {
                    key: "daily/summary.csv".into(),
                },
                "KeyTaken",
            ),
            (
                UploadOutcome::ConditionUnsupported {
                    key: "daily/summary.csv".into(),
                },
                "ConditionUnsupported",
            ),
            (UploadOutcome::Cancelled, "Cancelled"),
        ];

        for (outcome, expected) in cases {
            app.update(cx, |app, cx| {
                app.transfer = Some(an_upload(TransferPhase::Running));
                app.apply_transfer(TransferEvent::UploadSettled(outcome), cx);
            });
            app.read_with(cx, |app, _| {
                let phase = &app.transfer.as_ref().expect("held").phase;
                assert_eq!(phase_name(phase), expected);
            });
        }
    }

    /// Keep-both changed the key, and the window has to say so — a user who
    /// is not told will look for the object under the name they sent.
    #[gpui::test]
    fn stepping_aside_is_carried_into_the_phase(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.transfer = Some(an_upload(TransferPhase::Running));
            app.apply_transfer(
                TransferEvent::UploadSettled(caixonho_core::transfer::UploadOutcome::Finished {
                    key: "daily/summary (2).csv".into(),
                    stepped_aside: true,
                    bytes: 4096,
                }),
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            match &app.transfer.as_ref().expect("held").phase {
                TransferPhase::Sent { key, stepped_aside } => {
                    assert_eq!(key, "daily/summary (2).csv");
                    assert!(stepped_aside, "the window must know to say so");
                }
                other => panic!("expected Sent, got {}", phase_name(other)),
            }
        });
    }

    // ---- Deleting (XONHO-0021 tasks 3.1–3.2) ----

    /// The two-act rule at the window: the action confirms, only the
    /// confirmation deletes, and dismissing leaves nothing behind.
    #[gpui::test]
    fn delete_asks_first_and_dismissing_deletes_nothing(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.objects.update(cx, |state, _| {
                state.delegate_mut().show(
                    CorePrefix::root(),
                    Vec::new(),
                    vec![an_object("daily/summary.csv", 10)],
                );
            });
            app.objects
                .update(cx, |state, cx| state.set_selected_row(0, cx));
            app.delete_selected(cx);
        });
        app.read_with(cx, |app, _| {
            let deletion = app.deletion.as_ref().expect("the confirmation is up");
            assert!(matches!(deletion.phase, DeletePhase::Confirming));
            assert_eq!(deletion.key, "daily/summary.csv");
        });

        // Dismiss: the deletion state is gone and — the point of the
        // two-act rule — nothing was ever spawned, because only
        // confirm_delete spawns and it was never called.
        app.update(cx, |app, cx| {
            app.deletion = None;
            cx.notify();
        });
        app.read_with(cx, |app, _| assert!(app.deletion.is_none()));
    }

    /// No selection, or a folder selected: the action does nothing at all.
    #[gpui::test]
    fn delete_gates_on_an_object_selection(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.objects.update(cx, |state, _| {
                state.delegate_mut().show(
                    CorePrefix::root(),
                    vec![caixonho_core::Folder {
                        prefix: CorePrefix::parse("daily/"),
                    }],
                    Vec::new(),
                );
            });
            app.delete_selected(cx);
            assert!(app.deletion.is_none(), "no selection, no confirmation");
            app.objects
                .update(cx, |state, cx| state.set_selected_row(0, cx));
            app.delete_selected(cx);
        });
        app.read_with(cx, |app, _| {
            assert!(
                app.deletion.is_none(),
                "a folder is selected, and a folder is not deletable here"
            );
        });
    }

    /// The undo is offered exactly on proof, and a settled delete re-reads
    /// the listing — observable as the listing going back to Loading.
    #[gpui::test]
    fn a_settled_delete_shows_its_proofed_undo_and_rereads(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update_in(cx, |app, window, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                key: "daily/summary.csv".into(),
                phase: DeletePhase::Deleting,
            });
            app.apply_delete(
                DeleteEvent::Settled(caixonho_core::session::DeleteOutcome::Gone {
                    marker: Some("mk-9".into()),
                }),
                window,
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            match &app.deletion.as_ref().expect("the outcome is up").phase {
                DeletePhase::Gone { marker } => assert_eq!(marker.as_deref(), Some("mk-9")),
                _ => panic!("expected Gone"),
            }
            assert!(
                matches!(app.listing, Listing::Loading),
                "the row leaves because the service says so: a re-read is in flight"
            );
        });
    }

    /// An outcome from a connection the user has left is dropped whole —
    /// above all its Undo, which would otherwise restore into the wrong
    /// account's screen.
    #[gpui::test]
    fn an_outcome_from_a_left_connection_is_dropped(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update_in(cx, |app, window, cx| {
            app.deletion = Some(Deletion {
                connection: ConnectionId(9_999),
                bucket: "reports".into(),
                key: "daily/summary.csv".into(),
                phase: DeletePhase::Deleting,
            });
            app.apply_delete(
                DeleteEvent::Settled(caixonho_core::session::DeleteOutcome::Gone {
                    marker: Some("mk-9".into()),
                }),
                window,
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            assert!(
                app.deletion.is_none(),
                "a stale outcome renders nothing, offers nothing"
            );
        });
    }

    /// A failed undo keeps the truth straight: not restored, and not a
    /// failed delete either.
    #[gpui::test]
    fn a_failed_undo_claims_no_restoration(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update_in(cx, |app, window, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                key: "daily/summary.csv".into(),
                phase: DeletePhase::Restoring,
            });
            app.apply_delete(
                DeleteEvent::UndoSettled(caixonho_core::session::UndoOutcome::Failed(
                    Error::AccessDenied {
                        iam_action: "s3:DeleteObjectVersion",
                    },
                )),
                window,
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            match &app.deletion.as_ref().expect("held").phase {
                DeletePhase::Failed { during_undo, .. } => {
                    assert!(
                        during_undo,
                        "the words must be the undo's, not the delete's"
                    )
                }
                _ => panic!("expected Failed"),
            }
        });
    }

    /// The close-out review's finds: the three deletion paths that were
    /// asserted in prose and driven by nothing.
    #[gpui::test]
    fn confirming_is_what_arms_the_delete(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                key: "daily/summary.csv".into(),
                phase: DeletePhase::Confirming,
            });
            app.confirm_delete(cx);
        });
        app.read_with(cx, |app, _| {
            assert!(
                matches!(
                    app.deletion.as_ref().expect("held").phase,
                    DeletePhase::Deleting
                ),
                "the second act moves it to in-flight"
            );
        });

        // And from any other phase, the same call is a no-op: the button
        // only exists on Confirming, but the state machine must not rely on
        // the render layer for that.
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                key: "daily/summary.csv".into(),
                phase: DeletePhase::Restored,
            });
            app.confirm_delete(cx);
        });
        app.read_with(cx, |app, _| {
            assert!(matches!(
                app.deletion.as_ref().expect("held").phase,
                DeletePhase::Restored
            ));
        });
    }

    #[gpui::test]
    fn undo_without_proof_does_nothing(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                key: "daily/summary.csv".into(),
                phase: DeletePhase::Gone { marker: None },
            });
            app.undo_delete(cx);
        });
        app.read_with(cx, |app, _| {
            assert!(
                matches!(
                    app.deletion.as_ref().expect("held").phase,
                    DeletePhase::Gone { marker: None }
                ),
                "no proof, no restore attempt — the guard holds without the render layer"
            );
        });
    }

    /// The one conditional `go_to` learned for this change: navigating away
    /// takes the strip along, a same-location re-read keeps it.
    #[gpui::test]
    fn the_strip_survives_a_reread_and_not_a_departure(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        let gone = || Deletion {
            connection: ConnectionId(0),
            bucket: "reports".into(),
            key: "daily/summary.csv".into(),
            phase: DeletePhase::Gone { marker: None },
        };

        app.update_in(cx, |app, window, cx| {
            let here = app.location().cloned().expect("looking at a bucket");
            app.deletion = Some(gone());
            app.go_to(here, window, cx);
            assert!(
                app.deletion.is_some(),
                "a re-read of the same location is the strip's own refresh"
            );

            app.deletion = Some(gone());
            app.go_to(Location::bucket("logs"), window, cx);
            assert!(
                app.deletion.is_none(),
                "walking somewhere else takes the strip along"
            );
        });
    }

    // ---- Previewing (XONHO-0008 tasks 4.1–4.2) ----

    fn a_preview(phase: PreviewPhase) -> Preview {
        Preview {
            connection: ConnectionId(0),
            key: "daily/big.log".into(),
            phase,
        }
    }

    /// The verb gates like the others: an object selection or nothing.
    #[gpui::test]
    fn preview_gates_on_an_object_selection(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.objects.update(cx, |state, _| {
                state.delegate_mut().show(
                    CorePrefix::root(),
                    vec![caixonho_core::Folder {
                        prefix: CorePrefix::parse("daily/"),
                    }],
                    vec![an_object("daily/big.log", 100_000)],
                );
            });
            app.preview_selected(cx);
            assert!(app.preview.is_none(), "no selection previews nothing");
            app.objects
                .update(cx, |state, cx| state.set_selected_row(0, cx));
            app.preview_selected(cx);
            assert!(app.preview.is_none(), "a folder previews nothing");
            app.objects
                .update(cx, |state, cx| state.set_selected_row(1, cx));
            app.preview_selected(cx);
        });
        app.read_with(cx, |app, _| {
            let preview = app.preview.as_ref().expect("in flight");
            assert!(matches!(preview.phase, PreviewPhase::Loading));
            assert_eq!(preview.key, "daily/big.log");
        });
    }

    /// A text outcome lands with both numbers; a stale one lands nowhere.
    #[gpui::test]
    fn a_text_outcome_lands_and_a_stale_one_is_dropped(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.preview = Some(Preview {
                connection: app.outcome.active(),
                key: "daily/big.log".into(),
                phase: PreviewPhase::Loading,
            });
            app.apply_preview(
                caixonho_core::preview::PreviewOutcome::Text {
                    content: "first page".into(),
                    shown: 10,
                    total: Some(100_000),
                },
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            match &app.preview.as_ref().expect("on screen").phase {
                PreviewPhase::Text { shown, total, .. } => {
                    assert_eq!((*shown, *total), (10, Some(100_000)));
                }
                _ => panic!("expected Text"),
            }
        });

        // The stale shape: fetched under a connection the user has left.
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.preview = Some(a_preview(PreviewPhase::Loading)); // ConnectionId(0) ≠ active
            app.preview.as_mut().unwrap().connection = ConnectionId(9_999);
            app.apply_preview(caixonho_core::preview::PreviewOutcome::Binary, cx);
        });
        app.read_with(cx, |app, _| {
            assert!(
                app.preview.is_none(),
                "a page fetched under one account never renders under another's name"
            );
        });
    }

    /// The refusals each become their own phase — the window has three
    /// different honest sentences, so the states must stay distinct.
    #[gpui::test]
    fn each_refusal_keeps_its_own_shape(cx: &mut TestAppContext) {
        use caixonho_core::preview::PreviewOutcome;
        let (app, cx) = looking_at(cx, "reports");
        let cases: Vec<(PreviewOutcome, &'static str)> = vec![
            (PreviewOutcome::Binary, "Binary"),
            (PreviewOutcome::ImageTooLarge { size: 999 }, "TooLarge"),
            (PreviewOutcome::NoPreview, "NoPreview"),
        ];
        for (outcome, expected) in cases {
            app.update(cx, |app, cx| {
                app.preview = Some(Preview {
                    connection: app.outcome.active(),
                    key: "k.bin".into(),
                    phase: PreviewPhase::Loading,
                });
                app.apply_preview(outcome, cx);
            });
            app.read_with(cx, |app, _| {
                let name = match &app.preview.as_ref().expect("held").phase {
                    PreviewPhase::Binary => "Binary",
                    PreviewPhase::TooLarge { .. } => "TooLarge",
                    PreviewPhase::NoPreview => "NoPreview",
                    _ => "other",
                };
                assert_eq!(name, expected);
            });
        }
    }

    /// The lifecycle: departure drops it, a same-location re-read keeps it —
    /// the deletion strip's rule, applied to the second location-scoped
    /// surface.
    #[gpui::test]
    fn the_preview_departs_with_the_location(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update_in(cx, |app, window, cx| {
            let here = app.location().cloned().expect("looking at a bucket");
            app.preview = Some(a_preview(PreviewPhase::Binary));
            app.go_to(here, window, cx);
            assert!(app.preview.is_some(), "a re-read is not a departure");

            app.go_to(Location::bucket("logs"), window, cx);
            assert!(app.preview.is_none(), "walking away takes it along");

            app.preview = Some(a_preview(PreviewPhase::Binary));
            app.leave_bucket(cx);
            assert!(app.preview.is_none(), "and so does leaving entirely");
        });
    }

    fn phase_name(phase: &TransferPhase) -> &'static str {
        match phase {
            TransferPhase::Running => "Running",
            TransferPhase::NameTaken { .. } => "NameTaken",
            TransferPhase::KeyTaken { .. } => "KeyTaken",
            TransferPhase::ConditionUnsupported { .. } => "ConditionUnsupported",
            TransferPhase::Sent { .. } => "Sent",
            TransferPhase::Finished { .. } => "Finished",
            TransferPhase::Cancelled => "Cancelled",
            TransferPhase::Failed(_) => "Failed",
        }
    }

    /// Where [`every_state_is_written_for_judgement`] leaves its images.
    ///
    /// Under `target/`, so `.gitignore` already covers it: these are an
    /// instrument's output, regenerated whenever the design moves, and a
    /// screenshot committed beside the code it depicts is stale the first
    /// time anyone edits a token.
    #[cfg(target_os = "macos")]
    fn judgement_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/screenshots")
    }

    /// Open the real window, drive it into one state, and write what it drew.
    ///
    /// A fresh context per state rather than one window walked through all of
    /// them: a screenshot is meant to show what a user arriving at that state
    /// sees, and a window that reached it by way of four others carries
    /// whatever they left behind — a scroll offset, a selection, a region
    /// filter. Cheap enough not to trade for that.
    #[cfg(target_os = "macos")]
    fn shoot(
        name: &str,
        store: Arc<dyn caixonho_core::ObjectStore>,
        drive: impl FnOnce(&mut CaixonhoApp, &mut gpui::Window, &mut gpui::Context<CaixonhoApp>),
    ) -> std::path::PathBuf {
        use gpui::{HeadlessAppContext, px, size};

        const WIDTH: u32 = 1280;
        const HEIGHT: u32 = 800;

        // The platform's own text system, not `NoopTextSystem`. The noop one
        // is what `a_real_view_renders_to_an_image` uses and is right there:
        // that test asks whether a frame comes back at all, and a text system
        // rasterising nothing keeps it from depending on installed fonts. Here
        // it would defeat the whole exercise — most of what
        // `docs/design-language.md` describes is type, and screenshots with no
        // glyphs in them are not evidence about any of it.
        let text_system = gpui_platform::current_platform(true).text_system();

        let mut cx = HeadlessAppContext::with_platform(
            text_system,
            Arc::new(gpui_component_assets::Assets),
            gpui_platform::current_headless_renderer,
        );
        cx.update(gpui_component::init);

        // A world with one profile in it. `World::scripted` deliberately
        // discovers none — a test that wants a connection should say so — and
        // saying so is not optional here: `render` answers
        // `connections().is_empty()` with "No connections yet." and returns
        // **before** it ever consults `outcome`. Without this every state below
        // is staged correctly, drawn faithfully, and invisible; the first
        // twelve images were that screen twelve times, and the state probe
        // said `Loaded(buckets=4)` while the pixels said otherwise.
        let mut world = World::scripted(store);
        world.profiles = vec![caixonho_core::Profile {
            name: "example".to_owned(),
            is_default: true,
        }];

        let window = cx
            .open_window(size(px(WIDTH as f32), px(HEIGHT as f32)), |window, cx| {
                cx.new(|cx| CaixonhoApp::new(Diagnostics::without_a_log(), world, window, cx))
            })
            .expect("a headless window opens");
        cx.run_until_parked();

        // Choose it, through the same call the sidebar makes. A world with a
        // profile in it is still not a window showing an account: `render`
        // answers "Choose a connection." until one has been selected, which is
        // the second gate in front of everything below. Selecting issues a
        // listing, and parking lets the canned one land — so anything `drive`
        // stages afterwards is staged over a settled screen rather than racing
        // one.
        window
            .update(&mut cx, |app, window, cx| app.select_profile(0, window, cx))
            .expect("the window is still open");
        cx.run_until_parked();

        window
            .update(&mut cx, |app, window, cx| {
                drive(app, window, cx);
                cx.notify();
            })
            .expect("the window is still open");

        // Draw, explicitly, and do **not** park again. Two separate traps sit
        // here, and the first one cost this harness its first twelve images.
        //
        // `render_to_image` returns `rendered_frame` — the frame already
        // built. `cx.notify()` only marks the view dirty, and in a headless
        // context opened with `show: false` nothing comes along to redraw it,
        // so a capture after a state change hands back the frame from *before*
        // the change. All twelve came out byte-identical, and the assertion
        // that some pixel was opaque held on every one of them.
        //
        // Parking is the second trap: the world's canned listing resolves
        // through the real path and `accept`s an outcome, which would overwrite
        // whatever this function just staged.
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx)
        })
        .expect("the window is still open");

        let image = cx
            .capture_screenshot(window.into())
            .expect("the renderer produced an image");

        let dir = judgement_dir();
        std::fs::create_dir_all(&dir).expect("the screenshot directory is writable");
        let path = dir.join(format!("{name}.png"));
        image.save(&path).expect("the screenshot is written");
        path
    }

    /// Put an outcome on screen as though it had arrived for the active
    /// connection — which is the only way it is ever allowed on screen.
    #[cfg(target_os = "macos")]
    fn arriving(app: &mut CaixonhoApp, outcome: Outcome) {
        let active = app.outcome.active();
        assert!(
            app.outcome
                .accept(caixonho_core::TaggedOutcome::new(active, outcome)),
            "an outcome tagged with the active connection is never stale"
        );
    }

    #[cfg(target_os = "macos")]
    fn a_few_buckets() -> Vec<Bucket> {
        vec![
            bucket("reports"),
            bucket("logs"),
            bucket("archive"),
            bucket("media-assets"),
        ]
    }

    /// Every state the two screens can be in, written to disk for the owner
    /// to judge against `docs/design-language.md` — `XONHO-0009` task 6.3.
    ///
    /// `#[ignore]`d, and not because it is slow. It asserts almost nothing and
    /// is not trying to: the question it serves — does what is drawn match the
    /// language that document describes — is one no assertion can answer, so
    /// it produces evidence for a person and stays out of the suite that gates
    /// merges. What little it does assert is that each state drew *something*,
    /// because an all-transparent image is the failure that would otherwise be
    /// mistaken for a design opinion.
    ///
    /// Run it with:
    ///
    /// ```text
    /// cargo test -p caixonho-gui -- --ignored --nocapture every_state
    /// ```
    ///
    /// **macOS only**, for the reason `a_real_view_renders_to_an_image`
    /// records: `current_headless_renderer` answers `None` off macOS, so there
    /// is no renderer to capture with. These images are therefore evidence
    /// about what macOS draws and about nothing else — worth saying twice,
    /// because `AGENTS.md` names Windows this project's primary daily driver,
    /// and the states judged here are judged on the platform it is not used on.
    ///
    /// The window background comparison the design deferred is **not** here,
    /// and cannot be: see the note on task 6.3 in the change's `tasks.md`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "writes screenshots for the owner to judge; run explicitly"]
    fn every_state_is_written_for_judgement() {
        use caixonho_core::types::{AccountListing, Folder, RefusedListing};
        use caixonho_core::{BucketKind, Object};

        let refused = || RefusedListing {
            kind: BucketKind::Directory,
            action: "s3:ListAllMyDirectoryBuckets",
        };
        let object = |key: &str, size: u64| Object {
            key: key.to_owned(),
            size,
            last_modified: Some("2026-08-22T09:14:00Z".to_owned()),
            storage_class: Some("STANDARD".to_owned()),
            etag: None,
        };
        // Opening a location is what puts the second screen on screen — but
        // an account that is still listing while one of its buckets is open is
        // not a state a user can reach, and it shows: the rail down the left
        // renders the connection's buckets, so an account left `Loading` draws
        // the second screen beside an empty rail and a status line that reads
        // "Listing buckets…". Both were in the first honest set of images, and
        // both would have been judged as design.
        let settled = |app: &mut CaixonhoApp, cx: &mut gpui::Context<CaixonhoApp>| {
            arriving(
                app,
                Outcome::Loaded(AccountListing::complete(a_few_buckets())),
            );
            app.table.update(cx, |state, _| {
                let delegate = state.delegate_mut();
                delegate.rows = a_few_buckets();
                delegate.shown = (0..a_few_buckets().len()).collect();
            });
        };
        let inside = |app: &mut CaixonhoApp, cx: &mut gpui::Context<CaixonhoApp>| {
            settled(app, cx);
            app.position = Some(Position {
                connection: app.outcome.active(),
                at: Location::at("reports".to_owned(), CorePrefix::root()),
            });
        };

        let mut written = Vec::new();

        // ---- The account screen, in the order the document names them ----

        // Loading is the state the window opens in, so it needs no driving —
        // which is also the only way to be sure this is the real one.
        written.push(shoot(
            "account-01-loading",
            Arc::new(StoreDouble::allows_listing()),
            |_, _, _| {},
        ));

        written.push(shoot(
            "account-02-empty",
            Arc::new(StoreDouble::empty_account()),
            |app, _, _| arriving(app, Outcome::Loaded(AccountListing::complete(Vec::new()))),
        ));

        // Empty and refused are the two the change exists to keep apart: the
        // same blank list, and calling the second one empty is the lie.
        written.push(shoot(
            "account-03-empty-because-refused",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, _| {
                arriving(
                    app,
                    Outcome::Loaded(AccountListing {
                        buckets: Vec::new(),
                        refused: Some(refused()),
                    }),
                )
            },
        ));

        written.push(shoot(
            "account-04-loaded",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| settled(app, cx),
        ));

        // Narrowed, and narrowed to nothing. The second is the one worth
        // judging: it has to read as "your filters did this" and not as "this
        // account is empty", and the controls have to still be there to undo
        // it (`XONHO-0025`).
        written.push(shoot(
            "account-04a-narrowed",
            Arc::new(StoreDouble::allows_listing()),
            |app, window, cx| {
                settled(app, cx);
                // Through the controls, not past them. The first version of
                // this frame set the narrowing directly and photographed a
                // filter in force above a selector still reading "All kinds" —
                // a state no user can reach, which is the exact class of
                // harness bug `XONHO-0009` was written to stop.
                app.filter.update(cx, |state, cx| {
                    state.set_value("re", window, cx);
                });
                app.narrowing.name = "re".to_owned();
                app.narrowing.accessible_only = true;
                app.narrow_rows(cx);
            },
        ));

        written.push(shoot(
            "account-04b-narrowed-to-nothing",
            Arc::new(StoreDouble::allows_listing()),
            |app, window, cx| {
                settled(app, cx);
                // Through the control, not past it. Setting the narrowing
                // alone would draw a filter in force above an empty text box —
                // a state no user can reach, and the harness's job is to
                // photograph states that exist.
                app.filter.update(cx, |state, cx| {
                    state.set_value("nothing-matches-this", window, cx);
                });
                app.narrowing.name = "nothing-matches-this".to_owned();
                app.narrow_rows(cx);
            },
        ));

        // Partial: buckets came back and something else did not. Kept apart
        // from the failure panel on purpose, so the line has to be judged
        // beside a list rather than in place of one.
        written.push(shoot(
            "account-05-loaded-but-partly-refused",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                arriving(
                    app,
                    Outcome::Loaded(AccountListing {
                        buckets: a_few_buckets(),
                        refused: Some(refused()),
                    }),
                );
                app.table.update(cx, |state, _| {
                    let delegate = state.delegate_mut();
                    delegate.rows = a_few_buckets();
                    delegate.shown = (0..a_few_buckets().len()).collect();
                });
            },
        ));

        // Three failures rather than one: the panel is a vocabulary, and the
        // document asks for a cause and at most a single action. Expired
        // carries an action, denied carries an IAM action to read, and the
        // network one carries neither — which is the range to judge.
        for (name, error) in [
            (
                "account-06-error-session-expired",
                caixonho_core::Error::SessionRejected {
                    profile: "example".into(),
                    sso_session: Some("example-sso".into()),
                    problem: caixonho_core::error::SessionProblem::Expired,
                },
            ),
            (
                "account-07-error-access-denied",
                caixonho_core::Error::AccessDenied {
                    iam_action: "s3:ListAllMyBuckets",
                },
            ),
            (
                "account-09-error-credential-store",
                caixonho_core::Error::CredentialStore {
                    connection: "a-connection-with-a-name".into(),
                    problem: caixonho_core::CredentialStoreProblem::Refused,
                },
            ),
            (
                "account-08-error-network",
                caixonho_core::Error::Network {
                    detail: "connection refused".into(),
                },
            ),
        ] {
            written.push(shoot(
                name,
                Arc::new(StoreDouble::allows_listing()),
                move |app, _, _| arriving(app, Outcome::Failed(error)),
            ));
        }

        // ---- Inside a bucket: the denser row, and the same four states ----

        written.push(shoot(
            "bucket-01-loading",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loading;
            },
        ));

        written.push(shoot(
            "bucket-02-empty",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
            },
        ));

        written.push(shoot(
            "bucket-03-loaded",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.objects.update(cx, |state, _| {
                    state.delegate_mut().show(
                        CorePrefix::root(),
                        vec![
                            Folder {
                                prefix: CorePrefix::parse("daily/"),
                            },
                            Folder {
                                prefix: CorePrefix::parse("monthly/"),
                            },
                        ],
                        vec![
                            object("summary.csv", 20_184),
                            object("totals.parquet", 4_919_233),
                            object("readme.txt", 812),
                        ],
                    );
                });
            },
        ));

        // The transfer line, in the two shapes that carry decisions
        // (`XONHO-0007`): a download in flight, and the existing-file
        // question.
        written.push(shoot(
            "bucket-05-downloading",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.transfer = Some(Transfer {
                    bucket: "reports".into(),
                    key: "daily/totals.parquet".into(),
                    directory: std::env::temp_dir(),
                    then_open: false,
                    direction: Direction::Down,
                    source: None,
                    bytes: 2_411_724,
                    total: Some(4_919_233),
                    cancel: caixonho_core::transfer::Cancel::default(),
                    phase: TransferPhase::Running,
                });
            },
        ));

        written.push(shoot(
            "bucket-06-name-taken",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.transfer = Some(Transfer {
                    bucket: "reports".into(),
                    key: "daily/summary.csv".into(),
                    directory: std::env::temp_dir(),
                    then_open: false,
                    direction: Direction::Down,
                    source: None,
                    bytes: 0,
                    total: None,
                    cancel: caixonho_core::transfer::Cancel::default(),
                    phase: TransferPhase::NameTaken {
                        name: "summary.csv".into(),
                    },
                });
            },
        ));

        // The two upload states that carry a decision (`XONHO-0020`): the
        // key that is taken, and the endpoint that will not promise.
        written.push(shoot(
            "bucket-07-key-taken",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.transfer = Some(Transfer {
                    bucket: "reports".into(),
                    key: "daily/summary.csv".into(),
                    directory: std::env::temp_dir(),
                    then_open: false,
                    direction: Direction::Up,
                    source: Some(std::env::temp_dir().join("summary.csv")),
                    bytes: 0,
                    total: Some(20_184),
                    cancel: caixonho_core::transfer::Cancel::default(),
                    phase: TransferPhase::KeyTaken {
                        key: "daily/summary.csv".into(),
                    },
                });
            },
        ));

        written.push(shoot(
            "bucket-08-condition-unsupported",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.transfer = Some(Transfer {
                    bucket: "reports".into(),
                    key: "daily/summary.csv".into(),
                    directory: std::env::temp_dir(),
                    then_open: false,
                    direction: Direction::Up,
                    source: Some(std::env::temp_dir().join("summary.csv")),
                    bytes: 0,
                    total: Some(20_184),
                    cancel: caixonho_core::transfer::Cancel::default(),
                    phase: TransferPhase::ConditionUnsupported {
                        key: "daily/summary.csv".into(),
                    },
                });
            },
        ));

        // Making a folder (`XONHO-0024`). The second of these is the one to
        // judge hardest: nothing has failed, the user did nothing wrong, and
        // the words have to carry that or someone goes looking for a broken
        // bucket. Driven through the real controls, not past them — a lesson
        // from `XONHO-0025`, which photographed two states no user can reach.
        written.push(shoot(
            "bucket-13-new-folder-naming",
            Arc::new(StoreDouble::allows_listing()),
            |app, window, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.new_folder_here(window, cx);
                app.folder_name.update(cx, |state, cx| {
                    state.set_value("august", window, cx);
                });
            },
        ));

        written.push(shoot(
            "bucket-14-new-folder-not-on-a-directory-bucket",
            Arc::new(StoreDouble::allows_listing()),
            |app, window, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.new_folder_here(window, cx);
                app.making_folder.as_mut().expect("the strip is up").phase =
                    FolderPhase::NotOnADirectoryBucket;
                let _ = window;
            },
        ));

        // The deletion's two decision states (`XONHO-0021`): the second act,
        // and the aftermath that proves its undo.
        written.push(shoot(
            "bucket-09-delete-confirm",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.deletion = Some(Deletion {
                    connection: app.outcome.active(),
                    bucket: "reports".into(),
                    key: "daily/summary.csv".into(),
                    phase: DeletePhase::Confirming,
                });
            },
        ));

        written.push(shoot(
            "bucket-10-deleted-with-undo",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.deletion = Some(Deletion {
                    connection: app.outcome.active(),
                    bucket: "reports".into(),
                    key: "daily/summary.csv".into(),
                    phase: DeletePhase::Gone {
                        marker: Some("mk-screenshot".into()),
                    },
                });
            },
        ));

        // The preview's two faces (`XONHO-0008`): a first page with its
        // truncation line, and an honest refusal.
        written.push(shoot(
            "bucket-11-preview-text",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.preview = Some(Preview {
                    connection: app.outcome.active(),
                    key: "daily/build.log".into(),
                    phase: PreviewPhase::Text {
                        content: "2026-08-25T07:12:04 INFO  the run began\n\
                                  2026-08-25T07:12:05 INFO  247 objects listed\n\
                                  2026-08-25T07:12:09 WARN  one page came back slow\n\
                                  2026-08-25T07:12:11 INFO  settled clean\n"
                            .into(),
                        shown: 65_536,
                        total: Some(4_402_133),
                    },
                });
            },
        ));

        written.push(shoot(
            "bucket-12-preview-binary",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.preview = Some(Preview {
                    connection: app.outcome.active(),
                    key: "daily/export.txt".into(),
                    phase: PreviewPhase::Binary,
                });
            },
        ));

        written.push(shoot(
            "bucket-04-error-denied",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Failed(caixonho_core::Error::AccessDenied {
                    iam_action: "s3:ListBucket",
                });
            },
        ));

        // Two assertions, both about the instrument rather than the design.
        //
        // The second one is the one that matters, and it is here because its
        // absence cost a whole sitting. "Some pixel is opaque" passed on twelve
        // byte-identical copies of a screen none of these states was on — the
        // window was rendering `Choose a connection.` every time, because
        // nothing had selected one, while a state probe printed
        // `Loaded(buckets=4)` and agreed with itself. Distinctness is what
        // catches a harness that stages a state faithfully and photographs
        // something else: no two of these screens should look alike, so if two
        // files match, the capture is wrong even when every state is right.
        let mut seen: std::collections::HashMap<Vec<u8>, &std::path::Path> =
            std::collections::HashMap::new();
        for path in &written {
            let decoded = image::open(path)
                .expect("the written file decodes as an image")
                .to_rgba8();
            assert!(
                decoded.pixels().any(|p| p.0[3] != 0),
                "{} is entirely transparent, so nothing was drawn",
                path.display()
            );
            if let Some(other) = seen.insert(decoded.into_raw(), path.as_path()) {
                panic!(
                    "{} and {} are pixel-identical — the states differ, so the capture \
                     is showing something neither of them set",
                    other.display(),
                    path.display()
                );
            }
            println!("{}", path.display());
        }

        println!(
            "\n{} screenshots in {}",
            written.len(),
            judgement_dir().display()
        );
    }

    // ---- Narrowing the bucket list (XONHO-0025) ----

    fn bucket_of(name: &str, kind: BucketKind) -> Bucket {
        Bucket {
            name: name.to_owned(),
            created: None,
            region: Region::Unknown,
            kind,
        }
    }

    /// A window holding `rows`, with nothing observed about any of them.
    ///
    /// Through `set_rows`, the door the real listing comes in by, so the
    /// narrowing is applied the way production applies it.
    fn listing_of(
        cx: &mut TestAppContext,
        rows: Vec<Bucket>,
    ) -> (gpui::Entity<CaixonhoApp>, &mut gpui::VisualTestContext) {
        cx.update(gpui_component::init);
        let store: Arc<dyn caixonho_core::ObjectStore> = Arc::new(StoreDouble::allows_listing());
        let (app, cx) = cx.add_window_view(|window, cx| {
            CaixonhoApp::new(
                Diagnostics::without_a_log(),
                World::scripted(store),
                window,
                cx,
            )
        });
        app.update_in(cx, |app, window, cx| app.set_rows(rows, window, cx));
        (app, cx)
    }

    /// Record what has been observed about entering `bucket`.
    fn observe(app: &CaixonhoApp, bucket: &str, observation: Observation) {
        let session = app
            .session
            .as_ref()
            .expect("a scripted world has a session");
        let credentials = session.credentials().expect("scripted credentials");
        session.observe_list(&credentials, Scope::bucket(bucket), observation);
    }

    /// The bucket names on screen, in order.
    fn shown_names(
        app: &gpui::Entity<CaixonhoApp>,
        cx: &mut gpui::VisualTestContext,
    ) -> Vec<String> {
        app.read_with(cx, |app, cx| {
            app.table
                .read(cx)
                .delegate()
                .shown_names()
                .into_iter()
                .map(|(name, _)| name)
                .collect()
        })
    }

    /// The buckets the window would report for probing — which is what decides
    /// what ever gets observed at all.
    fn probe_targets(
        app: &gpui::Entity<CaixonhoApp>,
        cx: &mut gpui::VisualTestContext,
    ) -> Vec<String> {
        app.read_with(cx, |app, cx| {
            let delegate = app.table.read(cx).delegate();
            delegate
                .targets(0..delegate.shown_of_loaded().0)
                .into_iter()
                .map(|target| target.scope().bucket_name().to_owned())
                .collect()
        })
    }

    #[gpui::test]
    fn accessible_only_removes_the_refused(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("can-open", BucketKind::General),
                bucket_of("refused", BucketKind::General),
            ],
        );
        app.update(cx, |app, _| {
            observe(app, "can-open", Observation::Allowed);
            observe(app, "refused", Observation::Denied);
        });

        app.update(cx, |app, cx| {
            app.narrowing.accessible_only = true;
            app.narrow_rows(cx);
        });

        assert_eq!(shown_names(&app, cx), vec!["can-open".to_owned()]);
    }

    /// The test this change exists for. A bucket nobody has asked about is not
    /// known to be refused, and hiding it would be presenting absence of
    /// evidence as a denial.
    #[gpui::test]
    fn accessible_only_keeps_a_bucket_nothing_is_known_about(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("refused", BucketKind::General),
                bucket_of("unanswered", BucketKind::General),
            ],
        );
        app.update(cx, |app, _| observe(app, "refused", Observation::Denied));

        app.update(cx, |app, cx| {
            app.narrowing.accessible_only = true;
            app.narrow_rows(cx);
        });

        assert_eq!(
            shown_names(&app, cx),
            vec!["unanswered".to_owned()],
            "a bucket with no answer yet was hidden as though it had been refused"
        );
    }

    /// And the half a rendering test cannot see. The viewport is built from the
    /// *shown* rows, so a bucket the narrowing removes is never probed — which
    /// is why `!= Denied` is not interchangeable with `== Open`. Written the
    /// wrong way round, an unanswered bucket would be hidden, so never probed,
    /// so never answered, so hidden for ever, with nothing on screen saying so.
    #[gpui::test]
    fn accessible_only_still_reports_the_unanswered_for_probing(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("refused", BucketKind::General),
                bucket_of("unanswered", BucketKind::General),
            ],
        );
        app.update(cx, |app, _| observe(app, "refused", Observation::Denied));

        app.update(cx, |app, cx| {
            app.narrowing.accessible_only = true;
            app.narrow_rows(cx);
        });

        assert!(
            probe_targets(&app, cx).contains(&"unanswered".to_owned()),
            "the narrowing stopped an unanswered bucket being probed, so its answer can \
             never arrive and it is hidden for ever"
        );
    }

    #[gpui::test]
    fn narrowing_by_kind_and_by_name_compose(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("vault-one--x-s3", BucketKind::Directory),
                bucket_of("vault-two", BucketKind::General),
                bucket_of("other--x-s3", BucketKind::Directory),
            ],
        );

        app.update(cx, |app, cx| {
            app.narrowing.kind = KindChoice::Directory;
            app.narrow_rows(cx);
        });
        assert_eq!(
            shown_names(&app, cx),
            vec!["vault-one--x-s3".to_owned(), "other--x-s3".to_owned()]
        );

        app.update(cx, |app, cx| {
            app.narrowing.name = "vault".to_owned();
            app.narrow_rows(cx);
        });
        assert_eq!(
            shown_names(&app, cx),
            vec!["vault-one--x-s3".to_owned()],
            "the two narrowings should compose, not replace one another"
        );

        // Clearing one leaves the other in force.
        app.update(cx, |app, cx| {
            app.narrowing.kind = KindChoice::Any;
            app.narrow_rows(cx);
        });
        assert_eq!(
            shown_names(&app, cx),
            vec!["vault-one--x-s3".to_owned(), "vault-two".to_owned()]
        );
    }

    #[gpui::test]
    fn a_name_narrowing_ignores_case(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(cx, vec![bucket_of("Reports-2026", BucketKind::General)]);

        app.update(cx, |app, cx| {
            app.narrowing.name = "reports".to_owned();
            app.narrow_rows(cx);
        });

        assert_eq!(shown_names(&app, cx), vec!["Reports-2026".to_owned()]);
    }

    #[gpui::test]
    fn an_account_with_buckets_all_narrowed_away_is_not_an_empty_account(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(cx, vec![bucket_of("reports", BucketKind::General)]);

        app.update(cx, |app, cx| {
            app.narrowing.name = "nothing-matches-this".to_owned();
            app.narrow_rows(cx);
        });

        app.read_with(cx, |app, cx| {
            let delegate = app.table.read(cx).delegate();
            assert!(
                delegate.hidden_by_narrowing(),
                "an account whose buckets are all narrowed away must not read as one that \
                 holds none — the only cure for the second is knowing which control emptied it"
            );
            assert_eq!(delegate.shown_of_loaded(), (0, 1));
        });
    }

    #[gpui::test]
    fn narrowings_do_not_follow_the_user_to_another_connection(cx: &mut TestAppContext) {
        let (app, cx) = with_two_connections(cx);

        app.update_in(cx, |app, window, cx| {
            app.select_profile(0, window, cx);
            app.narrowing.kind = KindChoice::Directory;
            app.narrowing.name = "reports".to_owned();
            app.narrowing.accessible_only = true;
            app.narrow_rows(cx);
        });
        cx.run_until_parked();

        app.update_in(cx, |app, window, cx| app.select_profile(1, window, cx));
        cx.run_until_parked();

        app.read_with(cx, |app, _| {
            assert_eq!(
                app.narrowing,
                Narrowing::default(),
                "a narrowing set on one account followed the user to the next, which is how \
                 someone comes to believe a bucket has gone missing"
            );
        });
    }

    // ---- A folder you can make (XONHO-0024) ----

    #[gpui::test]
    fn asking_for_a_folder_on_a_directory_bucket_never_reaches_the_service(
        cx: &mut TestAppContext,
    ) {
        let (app, cx) = looking_at(cx, "vault");
        // The bucket the window is inside is a directory bucket, as the
        // account listing already says.
        app.update(cx, |app, cx| {
            app.table.update(cx, |state, _| {
                let delegate = state.delegate_mut();
                delegate.rows = vec![bucket_of("vault", BucketKind::Directory)];
                delegate.shown = vec![0];
            });
        });

        app.update_in(cx, |app, window, cx| {
            app.new_folder_here(window, cx);
            app.confirm_new_folder(cx);
        });
        cx.run_until_parked();

        app.read_with(cx, |app, _| {
            assert!(
                matches!(
                    app.making_folder.as_ref().expect("a strip is up").phase,
                    FolderPhase::NotOnADirectoryBucket
                ),
                "a directory bucket must be refused from the kind already in the listing"
            );
        });
    }

    #[gpui::test]
    fn a_folder_is_named_before_anything_is_sent(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");

        app.update_in(cx, |app, window, cx| app.new_folder_here(window, cx));

        app.read_with(cx, |app, _| {
            assert!(
                matches!(
                    app.making_folder.as_ref().expect("a strip is up").phase,
                    FolderPhase::Naming
                ),
                "the button should open a name prompt, not create anything"
            );
        });
    }

    #[gpui::test]
    fn a_name_that_cannot_be_a_folder_is_said_rather_than_sent(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");

        app.update_in(cx, |app, window, cx| {
            app.new_folder_here(window, cx);
            app.folder_name.update(cx, |state, cx| {
                state.set_value("reports/august", window, cx);
            });
            app.confirm_new_folder(cx);
        });
        cx.run_until_parked();

        app.read_with(cx, |app, _| {
            assert!(matches!(
                app.making_folder.as_ref().expect("a strip is up").phase,
                FolderPhase::BadName(_)
            ));
        });
    }

    /// `XONHO-0019`'s discipline, on the newest verb: an answer that lands
    /// after the user has switched accounts must not be announced over the one
    /// they are looking at.
    #[gpui::test]
    fn a_folder_made_into_an_account_the_user_left_is_not_announced(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update_in(cx, |app, window, cx| app.new_folder_here(window, cx));

        app.update(cx, |app, _| {
            let making = app.making_folder.as_mut().expect("a strip is up");
            making.connection = ConnectionId(app.next_connection + 99);
        });
        app.update_in(cx, |app, window, cx| {
            app.folder_settled(
                caixonho_core::session::FolderOutcome::Made {
                    key: "august/".to_owned(),
                },
                window,
                cx,
            );
        });

        app.read_with(cx, |app, _| {
            assert!(
                app.making_folder.is_none(),
                "a folder made into an account the user has left was announced over the one \
                 they are now looking at"
            );
        });
    }
}
