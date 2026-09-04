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
    ActiveTheme, Icon, IconName, IndexPath, Side, TitleBar,
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
use crate::theme::{Clay, space, tile};
use crate::views::buckets::{BucketsDelegate, KindChoice, Narrowing, RegionSelect, region_label};
use crate::views::credential_form::CredentialForm;
use crate::views::failure::{guidance_for, refusal_detail, refusal_headline, unavailable_reason};
use crate::views::format::split_zonal_name;
use crate::views::objects::ObjectsDelegate;
use caixonho_core::queue::{Queue, Standing, TransferId};
use caixonho_core::transfer::Cancel;

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
    /// Every transfer this run has taken on (`XONHO-0028`).
    ///
    /// Was `Option<Transfer>` — one, and not by policy but by type. That one
    /// line is where every "one file at a time" came from, and three unbuilt
    /// `[M]` rows were waiting on it rather than on themselves.
    ///
    /// The queue decides *which* run; this window starts what it hands back
    /// and reports what became of them. Nothing about a single transfer
    /// changed: `Transfer` and its five phases describe one transfer's end,
    /// which was right for one and is right for each of many.
    queue: Queue<Transfer>,
    /// Where a download's progress and outcome come back.
    transfers: flume::Sender<Tagged>,
    /// The one deletion being confirmed, in flight, or just settled
    /// (`XONHO-0021`).
    deletion: Option<Deletion>,
    /// The deletes a confirmation armed, bounded the way transfers are.
    ///
    /// Its own queue rather than an arm of the transfer one: a deletion moves
    /// nothing, and "Downloading…" vocabulary does not belong one enum away
    /// from a destructive verb (`XONHO-0021`'s reason, unchanged). What is
    /// shared is the *structure* — `Queue` is generic precisely so a second
    /// kind of work can be bounded without a second scheduler.
    deletes: Queue<String>,
    /// Each refusal a bulk delete met, as `key: cause`, gathered while the
    /// queue drains and taken when the last one settles.
    delete_failures: Vec<String>,
    /// The buckets ticked in the chooser, while it is open (`XONHO-0027`).
    ///
    /// `None` when the chooser is closed. Held apart from
    /// `narrowing.chosen` so that abandoning the chooser changes nothing —
    /// a picker that edited the live choice would apply half a decision.
    choosing_buckets: Option<Vec<String>>,
    /// Why the last drop was refused, when it was (`XONHO-0029`).
    ///
    /// A drop that vanishes is indistinguishable from a broken application,
    /// so a refusal is always said rather than merely not-done.
    dropped_refusal: Option<SharedString>,
    /// The upload waiting on a destination, if one is (`XONHO-0026`).
    choosing_destination: Option<ChoosingDestination>,
    /// Where the object will land. Its own input rather than a `String` on the
    /// state, for `folder_name`'s reason: the control is what the user types
    /// into, and two places holding one answer is how they come to disagree.
    destination: Entity<InputState>,
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
/// One event, and which transfer it belongs to (`XONHO-0028`).
///
/// The id is not decoration. A window holding one transfer could assume any
/// event was *the* transfer's; with a queue that assumption becomes a defect
/// showing as one file's progress moving for another file's bytes — silently
/// wrong, which is the worst kind available.
struct Tagged {
    id: TransferId,
    event: TransferEvent,
}

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
    /// What the user pointed at, kept for the **wording**.
    ///
    /// The work is always the same shape — a list of keys — but the sentence
    /// is not: a count of one is weaker than a name, and someone who asked to
    /// delete a folder is owed the folder's name rather than a number.
    asked: Asked,
    phase: DeletePhase,
}

/// What the user pointed at when they asked for a delete (`XONHO-0030`).
enum Asked {
    /// One object's row.
    Object(String),
    /// One folder's row, by the name shown on it.
    Folder(String),
    /// The ticked rows, however many were ticked. Not the same number as the
    /// keys: ticking one folder can come to fifty objects, and saying both is
    /// how that stops being a surprise.
    Rows(usize),
}

enum DeletePhase {
    /// Walking prefixes to find out how many there are.
    ///
    /// **Cannot be confirmed.** A dialog offering a number it may still be
    /// about to change is asking for a yes to the wrong question. The
    /// `cancels` stop the walks; they are held rather than dropped because a
    /// walk nobody can stop keeps listing after the user has moved on.
    Counting {
        cancels: Vec<Cancel>,
        /// How many walks have not answered yet.
        left: usize,
        /// What the answered ones found.
        gathered: Vec<String>,
    },
    /// Counted. `keys` is exactly what confirming would remove — the same
    /// list the count was taken from, so the number and the work cannot
    /// disagree.
    Confirming { keys: Vec<String> },
    /// The walk found nothing under the prefix. Told rather than shown as a
    /// confirmation for zero.
    Empty,
    /// More objects than one walk will gather. Refused with the reason.
    TooMany { at_least: usize },
    /// Deletes are in flight. `total` of one is the single-object case, which
    /// takes the same path so that there is only one.
    Deleting { done: usize, total: usize },
    /// One object was deleted. `marker` is the undo's proof and token —
    /// reached only when `total` was one, because a bulk delete is not undone
    /// here (`XONHO-0030`).
    Gone { marker: Option<String> },
    /// Several were deleted. `failures` keeps each refusal's own cause, so
    /// one denial does not become "the delete failed".
    Went { gone: usize, failures: Vec<String> },
    /// The undo is in flight. The marker travelled with the spawn; nothing
    /// here needs it again, and a field nobody reads is how retry-shaped
    /// ideas sneak in unreviewed.
    Restoring,
    /// The marker is gone; the object is back.
    Restored,
    /// A count, a delete or an undo failed. `during_undo` picks the words.
    Failed { error: Error, during_undo: bool },
}

/// A file has been chosen and is waiting on where to land (`XONHO-0026`).
///
/// Its own state rather than a `TransferPhase`, and not for tidiness: a
/// `Transfer` holds a `Cancel` for an upload that is already running, and
/// nothing is running yet. Making one here would mean inventing a cancel for
/// a request that does not exist.
struct ChoosingDestination {
    // No `connection`, unlike `Deletion` and `MakingFolder`. Those carry one
    // because an *answer* arrives later and must not be reported over an
    // account the user has left (`XONHO-0019`). Nothing has been sent here, so
    // there is no late answer to guard — and `end_location` already drops this
    // on any switch. A field nobody reads is how a guarantee comes to look
    // enforced when it is not.
    bucket: String,
    /// The local files, held until there is somewhere to send them.
    ///
    /// One or many, and the count is what the destination field *means*
    /// (`XONHO-0029`): with one it is the whole key, editable down to the
    /// file's name; with several it is the folder they share, each keeping
    /// its own. Two meanings for one control is a real hazard, and the
    /// mitigation is words on the strip rather than cleverness here.
    sources: Vec<std::path::PathBuf>,
    /// Why the last attempt was refused, when it was. Nothing was sent.
    refused: Option<caixonho_core::folder::BadObjectKey>,
}

impl ChoosingDestination {
    /// Whether the destination being asked for is a folder rather than a key.
    fn wants_a_folder(&self) -> bool {
        self.sources.len() > 1
    }
}

impl TransferPhase {
    /// What this phase means to the queue, when it means anything.
    ///
    /// `None` for `Running`, and that is the whole reason the two are separate
    /// axes rather than one: a phase says what the *service* last said, and a
    /// standing says what the *queue* thinks — and only the queue can tell
    /// `Waiting` from `Running`, because neither has reached the service yet.
    ///
    /// Everything else is derivable, and derived here so it is decided once.
    /// The screenshot harness proved they could drift by photographing a
    /// panel whose header read "0 of 4 transferred" above a row that said
    /// `Uploaded`. That was a harness artefact — production set both together
    /// — but nothing *made* it so, and a fact with two sources eventually
    /// disagrees somewhere it matters.
    fn standing(&self) -> Option<Standing> {
        match self {
            Self::Running => None,
            Self::Sent { .. } | Self::Finished { .. } => Some(Standing::Finished),
            Self::NameTaken { .. } | Self::KeyTaken { .. } | Self::ConditionUnsupported { .. } => {
                Some(Standing::Asking)
            }
            Self::Cancelled => Some(Standing::Cancelled),
            Self::Failed(_) => Some(Standing::Failed),
        }
    }
}

impl Transfer {
    /// A download about to start: everything known before a request is made.
    fn down(bucket: String, key: String, directory: std::path::PathBuf, then_open: bool) -> Self {
        Self {
            bucket,
            key,
            directory,
            then_open,
            direction: Direction::Down,
            source: None,
            bytes: 0,
            total: None,
            cancel: caixonho_core::transfer::Cancel::default(),
            phase: TransferPhase::Running,
        }
    }
}

/// Where a finished download should be handed over, if it should be.
///
/// Its own function because the handing itself cannot be tested: gpui's test
/// platform answers `open_with_system` with `not implemented`
/// (`platform/test/platform.rs:582`), so any window test that reaches that
/// line panics. The panic is at least honest — it proves the call is real —
/// but it means **no test can cover the last step**, so the step before it is
/// made small, pure and covered instead.
///
/// The name comes from the outcome rather than the key: `mapped` may have
/// changed it to avoid a collision, and joining the key would name a file
/// What the confirmation asks, given what was pointed at and how many keys it
/// came to (`XONHO-0030`).
///
/// A free function so it can be read without a window: this sentence is the
/// last thing between a person and losing data, and it is worth being able to
/// test every branch of it directly.
///
/// The rule is that **a name beats a count**. One object is named, however it
/// was reached — through its own row, or as the only thing ticked — because
/// "1 object" tells the reader nothing they can check. Past one, the count is
/// what a person can actually verify against what they meant to tick; a list
/// of twenty keys is a list the eye skims.
fn confirmation_sentence(asked: &Asked, keys: usize) -> String {
    match asked {
        Asked::Folder(name) => format!(
            "Delete `{name}/` and everything under it — {keys} \
             {} — from this bucket?",
            plural_objects(keys)
        ),
        Asked::Object(key) => format!("Delete `{key}` from this bucket?"),
        Asked::Rows(rows) => {
            if keys == 1 {
                // One thing ticked, and it turned out to be one object: it
                // still gets its name.
                return "Delete this object from this bucket?".to_owned();
            }
            if *rows == keys {
                format!("Delete {keys} {}?", plural_objects(keys))
            } else {
                // The surprise worth saying out loud: three ticks can be
                // forty-seven objects, and the number that matters is the
                // one that gets deleted.
                format!(
                    "Delete {keys} {} from the {rows} rows you ticked?",
                    plural_objects(keys)
                )
            }
        }
    }
}

/// "object" or "objects", so no sentence above has to say "1 objects".
fn plural_objects(count: usize) -> &'static str {
    if count == 1 { "object" } else { "objects" }
}

/// that is not there.
fn opens_at(transfer: &Transfer, name: &str) -> Option<std::path::PathBuf> {
    transfer.then_open.then(|| transfer.directory.join(name))
}

/// Whether this dragged thing is something the window can take.
///
/// Used by `can_drop`, by the drag-over styling and by the drop handler, so
/// all three answer the same question. Anything that is not a set of external
/// paths is not ours.
fn droppable(dragged: &dyn std::any::Any) -> Result<(), SharedString> {
    match dragged.downcast_ref::<gpui::ExternalPaths>() {
        Some(paths) => droppable_paths(paths.paths()),
        None => Err("Only files can be dropped here.".into()),
    }
}

/// Whether these paths are files this window will upload.
///
/// A **folder** is refused rather than partly honoured. Three options were
/// weighed in `design.md` and the tempting one — upload the files at its top
/// level — is the worst available: it does part of a job, and the user cannot
/// see which part without comparing by hand. Walking the tree is the separate
/// `[M]` about preserving prefix structure.
fn droppable_paths(paths: &[std::path::PathBuf]) -> Result<(), SharedString> {
    if paths.is_empty() {
        return Err("Nothing was dropped.".into());
    }
    if paths.iter().any(|path| path.is_dir()) {
        return Err("Folders cannot be uploaded yet — drop the files inside them instead.".into());
    }
    Ok(())
}

/// How many transfers run at once (`XONHO-0028`).
///
/// Four, and it is a placeholder with its successor already named: adaptive
/// concurrency is its own `[M]`, and it exists because no fixed number can be
/// right for every network and every account.
const TRANSFERS_AT_ONCE: usize = 4;

/// How many deletes run at once.
///
/// The same number as transfers and for the same reason: `PROJECT_BRIEF.md`
/// §4.4 names `503 SlowDown` as what a burst into one prefix meets, and a
/// bulk delete is exactly that burst with a different verb.
const DELETES_AT_ONCE: usize = 4;

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
    /// One walk under a prefix finished.
    Counted(caixonho_core::session::Tally),
    /// One delete settled. `id` names which — every delete goes through the
    /// queue, including a single one, so that there is one path and not two.
    Settled {
        id: TransferId,
        outcome: caixonho_core::session::DeleteOutcome,
    },
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
        // A row's own menu acts on the window, so the table is told which
        // window it belongs to. Weak, because the window owns the table.
        let reachable = cx.weak_entity();
        objects.update(cx, |state, _| {
            state.delegate_mut().reachable_from(reachable);
        });
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
        // "Search", not "Filter", and the word is chosen rather than casual.
        // The brief makes the UI say which of the two is happening, because a
        // filter covers *loaded* rows and a search covers everything — and on
        // an object listing, which is paginated and lazy, that gap is real.
        // The account listing has no gap: `list_buckets` drains its paginator
        // before returning, so every bucket is already in hand. Calling this a
        // filter would imply something unfetched that does not exist.
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search buckets"));
        let folder_name = cx.new(|cx| InputState::new(window, cx).placeholder("Folder name"));
        let destination = cx.new(|cx| InputState::new(window, cx).placeholder("bucket/path/name"));
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
                    if this.update(cx, |app, cx| app.probe_settled(cx)).is_err() {
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
        let (transfers, transferring) = flume::unbounded::<Tagged>();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = transferring.recv_async().await {
                let applied = this.update_in(cx, |app, window, cx| {
                    // An upload nobody can see is an upload nobody believes
                    // in — the sentence `XONHO-0024` wrote for folders, which
                    // applies word for word here and was missing because the
                    // upload path predates it. `XONHO-0026` made the gap
                    // plain: send a file to `a/b/c.txt` and the folders it
                    // implies were nowhere until the user navigated away and
                    // came back.
                    //
                    // Through `re_read_location`, so the strip saying where it
                    // went survives the refresh that proves it.
                    if app.apply_transfer(event.id, event.event, cx)
                        && let Some(location) = app.location().cloned()
                    {
                        app.re_read_location(location, window, cx);
                    }
                });
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
            // Small on purpose. `PROJECT_BRIEF.md` §4.4 names the failure a
            // bound prevents — "many small files into one prefix" is the
            // classic trigger for `503 SlowDown`.
            queue: Queue::new(TRANSFERS_AT_ONCE),
            deletes: Queue::new(DELETES_AT_ONCE),
            delete_failures: Vec::new(),
            transfers,
            deletion: None,
            dropped_refusal: None,
            choosing_buckets: None,
            choosing_destination: None,
            destination,
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
        // The remembered choice is the exception, and the only one: the other
        // four narrowings are *reset* here and this one is *loaded*
        // (`XONHO-0027`).
        self.narrowing.chosen = self
            .session
            .as_ref()
            .and_then(|session| session.chosen_buckets(source.name()));
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
    /// Go where the user asked to go.
    ///
    /// Distinct from [`Self::re_read_location`], and the distinction is the
    /// fix for a defect the owner found on 2026-08-26: previewing an object at
    /// a bucket's root, clicking the bucket in the breadcrumb did **nothing at
    /// all**. One function was serving two meanings. A re-read keeps a preview
    /// — that is deliberate and `XONHO-0008` tested it — and the bucket crumb
    /// walks to the location you are already at, so it took the re-read branch
    /// and left the preview standing over the listing it had just refreshed.
    ///
    /// So the two meanings get two names. Asking for a location, even the one
    /// you are already at, is asking to *see what is in it*: a preview stands
    /// in the listing's place rather than beside it, so it ends here.
    fn go_to(&mut self, location: Location, window: &mut Window, cx: &mut Context<Self>) {
        self.preview = None;
        self.re_read_location(location, window, cx);
    }

    /// Read this location again without treating it as somewhere new.
    ///
    /// What a deletion's outcome and a made folder both need: the listing is
    /// stale and has to be refetched, but the strip reporting *why* belongs to
    /// this location and must survive the refetch it triggered.
    fn re_read_location(
        &mut self,
        location: Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Walking somewhere else takes the deletion strip along — its key
        // belongs to the location it was deleted at. A re-read of the *same*
        // location keeps it, because that re-read is how the strip's own
        // outcome refreshes the listing (`XONHO-0021`).
        if self.location() != Some(&location) {
            self.forget_deletion();
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
    pub(crate) fn enter(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
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

    /// The key of the object at `index`, when that row is an object.
    ///
    /// Takes the row rather than reading the table's own selection: the verbs
    /// this feeds are reached from a row's menu now, and the row under the
    /// pointer is the one being acted on (`XONHO-0030`).
    fn object_key_at(&self, index: usize, cx: &Context<Self>) -> Option<String> {
        match self.objects.read(cx).delegate().row(index)? {
            crate::views::objects::Entry::Object(object) => Some(object.key.clone()),
            crate::views::objects::Entry::Folder(_) => None,
        }
    }

    /// Download the object at `index` to a directory the user chooses
    /// (`XONHO-0007` task 4.1).
    pub(crate) fn download_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(location), Some(key)) = (self.location().cloned(), self.object_key_at(index, cx))
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
                    None,
                    Transfer::down(location.bucket.clone(), key, directory, false),
                    caixonho_core::transfer::Collision::Ask,
                    cx,
                );
            });
        })
        .detach();
    }

    /// Open the object at `index` with the system's own application for it
    /// (`XONHO-0007` task 4.3): download to the open-cache, then hand over.
    pub(crate) fn open_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(location), Some(key)) = (self.location().cloned(), self.object_key_at(index, cx))
        else {
            return;
        };
        let Some(cache) = caixonho_core::transfer::open_cache_dir() else {
            // A machine with no resolvable cache directory: say so as a
            // failed transfer, because nothing was transferred.
            self.enqueue_settled(Transfer {
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
            self.enqueue_settled(Transfer {
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
            None,
            Transfer::down(location.bucket.clone(), key, cache, true),
            caixonho_core::transfer::Collision::Replace,
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
            self.re_read_location(location, window, cx);
        }
        cx.notify();
    }

    fn upload_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.location().is_none() {
            return;
        }
        let ask = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            // Was `false`, and that one word was why `XONHO-0028`'s queue had
            // nothing to fill it: six transfers meant six presses of this
            // button.
            multiple: true,
            prompt: Some("Upload".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(chosen))) = ask.await else {
                return; // Cancelled dialog, or the platform refused it.
            };
            // Through the same door a drop comes through.
            let _ = this.update_in(cx, |app, window, cx| {
                app.offer_upload_of(chosen, window, cx);
            });
        })
        .detach();
    }

    /// Put the chosen file's destination on screen, filled in and editable.
    fn offer_destination(
        &mut self,
        bucket: String,
        sources: Vec<std::path::PathBuf>,
        proposed: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if sources.is_empty() {
            return;
        }
        // The placeholder carries the meaning too, and is set here rather
        // than at render because it belongs to the state. A label saying
        // "folder" over a box hinting `bucket/path/name` is a small
        // contradiction, and small contradictions are what a reader resolves
        // by guessing.
        let hint = if sources.len() > 1 {
            "path/to/folder — blank means this one"
        } else {
            "bucket/path/name"
        };
        self.destination.update(cx, |state, cx| {
            state.set_placeholder(hint, window, cx);
            state.set_value(proposed, window, cx);
        });
        self.choosing_destination = Some(ChoosingDestination {
            bucket,
            sources,
            refused: None,
        });
        cx.notify();
    }

    /// What a drop does, or why it cannot.
    ///
    /// One place, asked three times — before the drop by `can_drop`, during
    /// it by the styling, and after it by the handler. Three predicates that
    /// were meant to agree is how a window comes to promise a landing it then
    /// refuses.
    fn take_dropped(
        &mut self,
        paths: &[std::path::PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.location().is_none() {
            // Silence here would be indistinguishable from a broken
            // application: the user did something and nothing happened.
            self.dropped_refusal =
                Some("Open a bucket first — there is nowhere to put these yet.".into());
            cx.notify();
            return;
        }
        let files: Vec<std::path::PathBuf> = paths.to_vec();
        match droppable_paths(&files) {
            Ok(()) => {
                self.dropped_refusal = None;
                self.offer_upload_of(files, window, cx);
            }
            Err(reason) => {
                self.dropped_refusal = Some(reason);
                cx.notify();
            }
        }
    }

    /// Take on these local files, asking where they go.
    ///
    /// The one door both `Upload…` and a drop come through. Two doors would
    /// become two behaviours, and a drop is meant to be the same act reached
    /// with the hand instead of the button (`XONHO-0029`).
    fn offer_upload_of(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(location) = self.location().cloned() else {
            return;
        };
        // With one file the whole key is offered, exactly as `XONHO-0026`
        // left it. With several the folder is, because typing ten keys is
        // not a thing anyone would do.
        let proposed = match paths.split_first() {
            Some((only, [])) => format!(
                "{}{}",
                location.prefix.as_str(),
                only.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
            ),
            _ => location.prefix.as_str().to_owned(),
        };
        self.offer_destination(location.bucket.clone(), paths, proposed, window, cx);
    }

    /// Send them, to whatever the field says.
    ///
    /// One file: the field is the whole key. Several: it is the folder they
    /// share, and each keeps its own name — which is why the two go through
    /// different rules in `core::folder` rather than through one that tries
    /// to be both.
    fn confirm_destination(&mut self, cx: &mut Context<Self>) {
        let Some(choosing) = self.choosing_destination.as_ref() else {
            return;
        };
        // Read from the control, never recomposed from the location and the
        // file name — that recomposition is the thing `XONHO-0026` removed,
        // and doing it here would put it back where no reader would look.
        let typed = self.destination.read(cx).value().to_string();
        let bucket = choosing.bucket.clone();
        let sources = choosing.sources.clone();

        let keyed: Result<Vec<(String, std::path::PathBuf)>, _> = if choosing.wants_a_folder() {
            caixonho_core::folder::folder_prefix(&typed).and_then(|prefix| {
                sources
                    .into_iter()
                    .map(|path| {
                        let name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or_default();
                        caixonho_core::folder::object_key(&format!("{prefix}{name}"))
                            .map(|key| (key, path))
                    })
                    .collect()
            })
        } else {
            caixonho_core::folder::object_key(&typed).map(|key| {
                sources
                    .into_iter()
                    .map(|path| (key.clone(), path))
                    .collect()
            })
        };

        let keyed = match keyed {
            Ok(keyed) => keyed,
            Err(bad) => {
                // Nothing is sent. A mistake costs a sentence, not a request
                // and an unexplained failure — and with several files it must
                // cost nothing *partly* sent either.
                if let Some(choosing) = self.choosing_destination.as_mut() {
                    choosing.refused = Some(bad);
                }
                cx.notify();
                return;
            }
        };

        self.choosing_destination = None;
        for (key, path) in keyed {
            self.start_upload(
                None,
                bucket.clone(),
                key,
                path,
                caixonho_core::transfer::Collision::Ask,
                cx,
            );
        }
    }

    /// Start one upload and hold it as the window's transfer.
    /// Take on a transfer that has already ended.
    ///
    /// Two places report a failure without ever making a request — a machine
    /// with no cache directory, and one whose cache will not be created. They
    /// are transfers in every way the user cares about, so they go in the
    /// queue and are settled in the same breath.
    fn enqueue_settled(&mut self, transfer: Transfer) {
        let standing = transfer.phase.standing().unwrap_or(Standing::Finished);
        let id = self.queue.accept(transfer);
        self.queue.settled(id, standing);
    }

    fn start_upload(
        &mut self,
        // `Some` when a collision answer re-issues a transfer already in the
        // queue. Re-using the id keeps one file to one row: the user answered
        // a question about *that* transfer, and it carrying on is not a
        // second one.
        existing: Option<TransferId>,
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
        // Accepted before it is started, because the id has to exist before
        // anything can be tagged with it.
        let id = existing.unwrap_or_else(|| {
            self.queue.accept(Transfer {
                bucket: bucket.clone(),
                key: key.clone(),
                directory: path.parent().map(ToOwned::to_owned).unwrap_or_default(),
                then_open: false,
                direction: Direction::Up,
                source: Some(path.clone()),
                bytes: 0,
                // Known up front and shown as a total, with no fraction: an
                // upload reports no progress in this slice, and the size is
                // still worth saying.
                total,
                cancel: caixonho_core::transfer::Cancel::default(),
                phase: TransferPhase::Running,
            })
        });
        let cancel = session.spawn_upload(bucket, key, path, collision, move |outcome| {
            let _ = inbox.send(Tagged {
                id,
                event: TransferEvent::UploadSettled(outcome),
            });
        });
        if let Some(transfer) = self.queue.payload_mut(id) {
            transfer.cancel = cancel;
            transfer.phase = TransferPhase::Running;
        }
        self.queue.settled(id, Standing::Running);
        cx.notify();
    }

    /// Answer the taken-*key* question by sending again with the answer.
    fn answer_key_collision(
        &mut self,
        id: TransferId,
        collision: caixonho_core::transfer::Collision,
        cx: &mut Context<Self>,
    ) {
        // The answer belongs to *this* transfer. `XONHO-0028`'s spec requires
        // it: two transfers can each be waiting on a question, and one
        // "replace" must not silently overwrite the other's destination.
        let Some(transfer) = self.queue.payload_mut(id) else {
            return;
        };
        let (bucket, key) = (transfer.bucket.clone(), transfer.key.clone());
        let Some(source) = transfer.source.clone() else {
            return;
        };
        self.queue.answered(id);
        self.start_upload(Some(id), bucket, key, source, collision, cx);
    }

    /// Preview the object at `index` (`XONHO-0008` task 4.1).
    pub(crate) fn preview_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(location) = self.location().cloned() else {
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

    /// Ask about deleting the row at `index` (`XONHO-0030`).
    ///
    /// An object is one key and is ready to confirm at once. A folder is
    /// every key under a prefix, and nobody knows how many that is yet — so
    /// it counts first, and the confirmation cannot be confirmed until the
    /// counting stops.
    pub(crate) fn delete_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.objects.read(cx).delegate().row(index).cloned() else {
            return;
        };
        match entry {
            crate::views::objects::Entry::Object(object) => {
                self.ask_about_deleting(Asked::Object(object.key.clone()), vec![object.key], cx);
            }
            crate::views::objects::Entry::Folder(folder) => {
                let name = folder.name().to_owned();
                self.count_then_ask(Asked::Folder(name), Vec::new(), vec![folder.prefix], cx);
            }
        }
    }

    /// Ask about deleting every ticked row.
    ///
    /// The toolbar's own verb, and the only one there that still acts on rows
    /// — because a tick is not a place and there is nowhere else it could
    /// live. Folders among the ticks are walked; objects go straight in.
    fn delete_ticked(&mut self, cx: &mut Context<Self>) {
        let ticked = self.objects.read(cx).delegate().chosen_rows();
        if ticked.is_empty() {
            return;
        }
        let asked = Asked::Rows(ticked.len());
        let mut keys = Vec::new();
        let mut prefixes = Vec::new();
        for row in ticked {
            match row {
                crate::views::objects::Entry::Object(object) => keys.push(object.key),
                crate::views::objects::Entry::Folder(folder) => prefixes.push(folder.prefix),
            }
        }
        if prefixes.is_empty() {
            self.ask_about_deleting(asked, keys, cx);
        } else {
            self.count_then_ask(asked, keys, prefixes, cx);
        }
    }

    /// Put a confirmation up for a list of keys already known.
    fn ask_about_deleting(&mut self, asked: Asked, keys: Vec<String>, cx: &mut Context<Self>) {
        let Some(location) = self.location().cloned() else {
            return;
        };
        if keys.is_empty() {
            return;
        }
        self.deletion = Some(Deletion {
            connection: self.outcome.active(),
            bucket: location.bucket,
            asked,
            phase: DeletePhase::Confirming { keys },
        });
        cx.notify();
    }

    /// Walk `prefixes`, then put the confirmation up with the total.
    ///
    /// `already` are keys that need no walking — the plain objects among a
    /// ticked set. They join the walked ones, so the count is of everything
    /// that would go and not only of the folders.
    fn count_then_ask(
        &mut self,
        asked: Asked,
        already: Vec<String>,
        prefixes: Vec<Prefix>,
        cx: &mut Context<Self>,
    ) {
        let (Some(location), Some(session)) = (self.location().cloned(), self.session.clone())
        else {
            return;
        };
        let mut cancels = Vec::new();
        for prefix in &prefixes {
            let inbox = self.deletions.clone();
            let cancel = session.spawn_walk_under(
                Location::at(location.bucket.clone(), prefix.clone()),
                move |tally| {
                    let _ = inbox.send(DeleteEvent::Counted(tally));
                },
            );
            cancels.push(cancel);
        }
        self.deletion = Some(Deletion {
            connection: self.outcome.active(),
            bucket: location.bucket,
            asked,
            phase: DeletePhase::Counting {
                left: prefixes.len(),
                cancels,
                gathered: already,
            },
        });
        cx.notify();
    }

    /// Drop the deletion, stopping any walk it started.
    ///
    /// Dismissal has to reach the walks: a listing nobody is waiting for is
    /// still requests going out, and the user has moved on.
    fn dismiss_deletion(&mut self, cx: &mut Context<Self>) {
        self.forget_deletion();
        cx.notify();
    }

    /// Forget the deletion and everything it armed.
    ///
    /// **The queue goes with it, and that is the point.** `start_ready_deletes`
    /// reads the bucket from `self.deletion`, so a queue left holding waiting
    /// items after the deletion is dropped would hand them to whatever the
    /// *next* confirmation is about — deleting keys from one bucket against
    /// another. Found in this change's own close-out review: the queue was
    /// only ever pruned of *finished* items, which is not the same thing.
    ///
    /// Replaced rather than drained, so the ids go too: a late `Settled` for
    /// an abandoned item then finds no payload, which `apply_delete` already
    /// treats as the answer to a question nobody is asking.
    fn forget_deletion(&mut self) {
        if let Some(Deletion {
            phase: DeletePhase::Counting { cancels, .. },
            ..
        }) = &self.deletion
        {
            for cancel in cancels {
                cancel.cancel();
            }
        }
        self.deletion = None;
        self.deletes = Queue::new(DELETES_AT_ONCE);
        self.delete_failures.clear();
    }

    /// The second act: the confirmation's own button.
    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(deletion) = self.deletion.as_mut() else {
            return;
        };
        // Counting refuses here, and that is the requirement: a number still
        // being worked out is not a number anybody can agree to.
        let DeletePhase::Confirming { keys } = &deletion.phase else {
            return;
        };
        let keys = keys.clone();
        if self.session.is_none() {
            return;
        }
        deletion.phase = DeletePhase::Deleting {
            done: 0,
            total: keys.len(),
        };
        for key in keys {
            self.deletes.accept(key);
        }
        self.start_ready_deletes(cx);
        cx.notify();
    }

    /// Spawn whatever the delete queue says may start now.
    fn start_ready_deletes(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let Some(bucket) = self.deletion.as_ref().map(|d| d.bucket.clone()) else {
            return;
        };
        for id in self.deletes.ready() {
            let Some(key) = self.deletes.payload_mut(id).map(|key| key.clone()) else {
                continue;
            };
            let inbox = self.deletions.clone();
            session.spawn_delete(bucket.clone(), key, move |outcome| {
                let _ = inbox.send(DeleteEvent::Settled { id, outcome });
            });
        }
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
        // Only ever one key here: `Gone` is reached only when the delete was
        // of a single object, because a bulk delete does not offer an undo.
        let Asked::Object(key) = &deletion.asked else {
            return;
        };
        let Some(session) = self.session.clone() else {
            return;
        };
        let (marker, key) = (marker.clone(), key.clone());
        deletion.phase = DeletePhase::Restoring;
        let inbox = self.deletions.clone();
        session.spawn_undo_delete(deletion.bucket.clone(), key, marker, move |outcome| {
            let _ = inbox.send(DeleteEvent::UndoSettled(outcome));
        });
        cx.notify();
    }

    /// Apply a count, a delete or an undo outcome, unless the deletion it
    /// belongs to has left the screen — dismissed, or the connection switched.
    fn apply_delete(&mut self, event: DeleteEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(deletion) = self.deletion.as_mut() else {
            return; // Dismissed while in flight; nothing to apply to.
        };
        if deletion.connection != self.outcome.active() {
            // A switch happened. An outcome — and above all an Undo — from
            // the account the user left must not render under the new one's
            // name (`XONHO-0019`). The queue goes too: keys queued against
            // that account must never be sent to this one.
            self.forget_deletion();
            cx.notify();
            return;
        }
        use caixonho_core::session::{DeleteOutcome, Tally, UndoOutcome};
        match event {
            DeleteEvent::Counted(tally) => {
                let DeletePhase::Counting { left, gathered, .. } = &mut deletion.phase else {
                    return; // A late count for a question already answered.
                };
                match tally {
                    Tally::All(keys) => {
                        gathered.extend(keys);
                        *left = left.saturating_sub(1);
                        if *left == 0 {
                            let keys = std::mem::take(gathered);
                            // Nothing under it is *told*, not shown as a
                            // confirmation for zero — which would ask
                            // somebody to agree to no work.
                            deletion.phase = if keys.is_empty() {
                                DeletePhase::Empty
                            } else {
                                DeletePhase::Confirming { keys }
                            };
                        }
                    }
                    // One prefix too large refuses the whole ask. Deleting
                    // the rest and silently skipping that one would be a
                    // partial answer to a question nobody asked.
                    Tally::TooMany { at_least } => {
                        deletion.phase = DeletePhase::TooMany { at_least };
                    }
                    Tally::Cancelled => {
                        self.forget_deletion();
                    }
                    Tally::Failed(error) => {
                        deletion.phase = DeletePhase::Failed {
                            error,
                            during_undo: false,
                        };
                    }
                }
            }
            DeleteEvent::Settled { id, outcome } => {
                let Some(key) = self.deletes.payload_mut(id).map(|key| key.clone()) else {
                    return; // A late answer about an item nobody holds.
                };
                let marker = match &outcome {
                    DeleteOutcome::Gone { marker } => marker.clone(),
                    DeleteOutcome::Failed(_) => None,
                };
                self.deletes.settled(
                    id,
                    match &outcome {
                        DeleteOutcome::Gone { .. } => Standing::Finished,
                        DeleteOutcome::Failed(_) => Standing::Failed,
                    },
                );
                let Some(deletion) = self.deletion.as_mut() else {
                    return;
                };
                let DeletePhase::Deleting { done, total } = &mut deletion.phase else {
                    return;
                };
                *done += 1;
                let (done, total) = (*done, *total);

                if let DeleteOutcome::Failed(error) = &outcome {
                    // Each refusal keeps its own cause and its own key: one
                    // denial in the middle of twenty is not "the delete
                    // failed", and a user owed a permission to ask for needs
                    // to know which object wanted it.
                    self.delete_failures.push(format!("{key}: {error}"));
                }

                if done < total {
                    self.start_ready_deletes(cx);
                    cx.notify();
                    return;
                }

                let failures = std::mem::take(&mut self.delete_failures);
                let Some(deletion) = self.deletion.as_mut() else {
                    return;
                };
                deletion.phase = if total == 1 {
                    match outcome {
                        DeleteOutcome::Gone { .. } => DeletePhase::Gone { marker },
                        DeleteOutcome::Failed(error) => DeletePhase::Failed {
                            error,
                            during_undo: false,
                        },
                    }
                } else {
                    DeletePhase::Went {
                        gone: total - failures.len(),
                        failures,
                    }
                };
                self.deletes.clear_finished();
                // The rows leave because the service says so: re-read.
                if let Some(location) = self.location().cloned() {
                    self.re_read_location(location, window, cx);
                }
            }
            DeleteEvent::UndoSettled(UndoOutcome::Restored) => {
                deletion.phase = DeletePhase::Restored;
                if let Some(location) = self.location().cloned() {
                    self.re_read_location(location, window, cx);
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

    /// Start one download, or re-issue one already queued.
    ///
    /// Takes the whole `Transfer` rather than five loose arguments. Clippy
    /// noticed the argument count first, but the real smell was next door:
    /// `start_ready` was unpacking a payload into arguments so this could pack
    /// them back into a payload. One shape, passed along.
    fn start_download(
        &mut self,
        // `Some` when a collision answer re-issues one already queued.
        // Re-using the id keeps one file to one row: the user answered a
        // question about *that* transfer, and it carrying on is not a second.
        existing: Option<TransferId>,
        transfer: Transfer,
        collision: caixonho_core::transfer::Collision,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let progress_inbox = self.transfers.clone();
        let settled_inbox = self.transfers.clone();
        let (bucket, key, directory) = (
            transfer.bucket.clone(),
            transfer.key.clone(),
            transfer.directory.clone(),
        );
        // Accepted before it is started: the id has to exist before anything
        // can be tagged with it.
        let id = existing.unwrap_or_else(|| self.queue.accept(transfer));
        let cancel = session.spawn_download(
            bucket,
            key,
            directory,
            collision,
            move |bytes, total| {
                let _ = progress_inbox.send(Tagged {
                    id,
                    event: TransferEvent::Progress { bytes, total },
                });
            },
            move |outcome| {
                let _ = settled_inbox.send(Tagged {
                    id,
                    event: TransferEvent::Settled(outcome),
                });
            },
        );
        if let Some(held) = self.queue.payload_mut(id) {
            held.cancel = cancel;
            held.phase = TransferPhase::Running;
        }
        self.queue.settled(id, Standing::Running);
        cx.notify();
    }

    /// Apply one event to the transfer it names.
    ///
    /// Returns whether an upload **landed**, so the caller can re-read the
    /// location. The caller rather than here because re-reading needs a
    /// window, and threading one through every call site — including tests
    /// with no business knowing about listings — would be noise for one `if`.
    ///
    /// An event for an item the queue no longer holds falls through the
    /// lookup and changes nothing. That is what the id is for: a late answer
    /// about something the user cleared must not land on whatever now sits
    /// where it was.
    fn apply_transfer(
        &mut self,
        id: TransferId,
        event: TransferEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut sent = false;
        // Set by a finished download that was asked to open, and acted on
        // after the borrow ends.
        let mut opened: Option<std::path::PathBuf> = None;
        let Some(transfer) = self.queue.payload_mut(id) else {
            return false;
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
                        sent = true;
                        TransferPhase::Sent { key, stepped_aside }
                    }
                    // A question only a person can answer, and it gives up its
                    // slot while it waits: two of these must not stall twenty.
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
                transfer.phase = match outcome {
                    DownloadOutcome::Finished {
                        name,
                        mapped,
                        bytes,
                    } => {
                        transfer.bytes = bytes;
                        // Hand it over, which is what `then_open` has always
                        // claimed and never did. The path is the directory it
                        // landed in plus the name it landed under — `mapped`
                        // may have changed that name, so it is read from the
                        // outcome rather than from the key.
                        opened = opens_at(transfer, &name);
                        TransferPhase::Finished { name, mapped }
                    }
                    DownloadOutcome::NameTaken { name } => TransferPhase::NameTaken { name },
                    DownloadOutcome::Cancelled => TransferPhase::Cancelled,
                    DownloadOutcome::Failed(error) => TransferPhase::Failed(error),
                };
            }
        }
        // One source, asked after the phase is set: `TransferPhase::standing`
        // decides, here and in the harness alike.
        if let Some(standing) = self
            .queue
            .items()
            .iter()
            .find(|item| item.id == id)
            .and_then(|item| item.payload.phase.standing())
        {
            self.queue.settled(id, standing);
        }
        if let Some(path) = opened {
            cx.open_with_system(&path);
        }
        cx.notify();
        sent
    }

    /// Answer the existing-file question by starting over with the answer.
    fn answer_collision(
        &mut self,
        id: TransferId,
        collision: caixonho_core::transfer::Collision,
        cx: &mut Context<Self>,
    ) {
        let Some(transfer) = self.queue.payload_mut(id) else {
            return;
        };
        let (bucket, key, directory, then_open) = (
            transfer.bucket.clone(),
            transfer.key.clone(),
            transfer.directory.clone(),
            transfer.then_open,
        );
        self.queue.answered(id);
        self.start_download(
            Some(id),
            Transfer::down(bucket, key, directory, then_open),
            collision,
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
        // leaving the location takes it along (`XONHO-0021`) — and takes the
        // deletes it armed, which is the half `XONHO-0030` added.
        self.forget_deletion();
        self.preview = None;
        // A destination is a key at a location; leaving takes it along
        // (`XONHO-0026`), exactly as the deletion strip goes.
        self.choosing_destination = None;
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
                        .clay()
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.confirming = None;
                            app.forget(for_remove.clone(), window, cx);
                        })),
                )
                .child(
                    Button::new("cancel-remove")
                        .label("Cancel")
                        .custom(crate::theme::quiet(cx))
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
        // The remembered choice is not a narrowing the user set this session,
        // so it is not one this clears. `select_profile` loads the next
        // connection's straight after.
        let chosen = self.narrowing.chosen.take();
        self.narrowing = Narrowing {
            chosen,
            ..Narrowing::default()
        };
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
    /// A probe answered, so what the listing admits may have changed.
    ///
    /// Redrawing is not enough. Four of the five narrowings read data that
    /// cannot change while the user sits still; the accessibility one reads an
    /// *observation*, and observations arrive later than the click that turned
    /// it on. Without re-narrowing here, switching to an account and turning
    /// "Accessible only" on straight away leaves every refused bucket listed
    /// for ever, wearing its own **No access** badge — the filter frozen at the
    /// moment nothing had been answered yet.
    ///
    /// Found by the owner on 2026-08-26, on the first screen they tried it on.
    /// `XONHO-0025` planned a test for exactly this and shipped without one.
    ///
    /// Only when that narrowing is on: it is the only one an answer can move,
    /// and re-narrowing on every probe otherwise would be work for nothing.
    fn probe_settled(&mut self, cx: &mut Context<Self>) {
        if self.narrowing.accessible_only {
            self.narrow_rows(cx);
        } else {
            cx.notify();
        }
    }

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
        //
        // The chosen-subset line below is not that count. It answers a
        // different question — *why* is this list short — which the status bar
        // does not and cannot.
        let (_, loaded) = self.table.read(cx).delegate().shown_of_loaded();

        // Filled when it is on, ghosted when it is not — the same way every
        // other toggle in this window says so. A label that changed with the
        // state would be a second answer to a question the fill has answered.
        let accessible_only = Button::new("accessible-only")
            .label("Accessible only")
            .on_click(cx.listener(|app, _, _, cx| {
                app.narrowing.accessible_only = !app.narrowing.accessible_only;
                app.narrow_rows(cx);
            }));
        // Filled *is* clay — the two are the same statement, and splitting
        // them is how the on-state ended up flat while every other filled
        // button in the window was moulded.
        let accessible_only = if self.narrowing.accessible_only {
            accessible_only.primary().clay()
        } else {
            accessible_only.custom(crate::theme::quiet(cx))
        };

        let controls = h_flex()
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
                    // Or the row's other children squeeze it as controls are
                    // added — which is what `XONHO-0027` did to it before this.
                    .flex_shrink_0()
                    .child(Input::new(&self.filter).cleanable(true)),
            )
            .child(
                div()
                    .debug_selector(|| "accessible-only".into())
                    .child(accessible_only),
            )
            .child(
                div().debug_selector(|| "choose-buckets".into()).child(
                    Button::new("choose-buckets")
                        .label("Choose buckets…")
                        .custom(crate::theme::quiet(cx))
                        .on_click(cx.listener(|app, _, _, cx| app.start_choosing_buckets(cx))),
                ),
            )
            // A choice made in another session is indistinguishable from a
            // bug unless the screen says so: the account has eleven buckets
            // and two are listed, with nothing on screen explaining why
            // (`XONHO-0027`). Showing all does **not** discard the choice.
            .children(all_directory.then(|| {
                div()
                    .debug_selector(|| "all-directory".into())
                    .child(status_badge(
                        IconName::LayoutDashboard,
                        "All directory buckets",
                        cx.theme().primary,
                    ))
            }))
            .into_any_element();

        // A choice made in another session is indistinguishable from a bug
        // unless the screen says so: the account has eleven buckets and two
        // are listed, with nothing on screen explaining why (`XONHO-0027`).
        //
        // Its own row, under the controls, because it is a *statement about
        // the list* rather than a control — and because a seventh thing on
        // that row squeezed the search field down to a few characters.
        let in_force = self.narrowing.chosen.as_ref().map(|chosen| {
            h_flex()
                .w_full()
                .pb_2()
                .debug_selector(|| "chosen-subset".into())
                .gap(space::TIGHT)
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(if self.narrowing.showing_all {
                            format!("Showing all {loaded} — {} chosen", chosen.len())
                        } else {
                            format!(
                                "Showing {} chosen of {loaded} in this account",
                                chosen.len()
                            )
                        }),
                )
                .child(if self.narrowing.showing_all {
                    Button::new("back-to-chosen")
                        .label("Back to my buckets")
                        .custom(crate::theme::quiet(cx))
                        .on_click(cx.listener(|app, _, _, cx| app.back_to_chosen_buckets(cx)))
                } else {
                    Button::new("show-all-buckets")
                        .label("Show all")
                        .custom(crate::theme::quiet(cx))
                        .on_click(cx.listener(|app, _, _, cx| app.show_all_buckets(cx)))
                })
        });

        v_flex().w_full().child(controls).children(in_force)
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
                                .custom(crate::theme::quiet(cx))
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
                .clay()
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
                .custom(crate::theme::quiet(cx))
                .on_click(cx.listener(|app, _, _, cx| app.leave_bucket(cx))),
        );

        trail = trail.child(div().text_color(cx.theme().muted_foreground).child("/"));
        trail = trail.child(
            Button::new("bucket-root")
                .label(location.bucket.clone())
                .custom(crate::theme::quiet(cx))
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
                        .custom(crate::theme::quiet(cx))
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
            // **Only verbs that act on a place.** Preview, Open, Download and
            // Delete-this-one act on a *row*, and they moved to the row —
            // reached by right-clicking it (`XONHO-0030`). What is left acts
            // on the location the user is standing in, which is what being
            // here means, so none of it needs a selection.
            //
            // That rule is also the fix for this row overflowing, which it has
            // done twice. Both times it was patched with flex properties, and
            // flex only ever decides who loses: seven verbs in a row do not
            // fit beside a sixty-character directory-bucket name at any
            // window width, and the answer was never a better squeeze.
            //
            // The one exception is Delete, and it earns it: it acts on the
            // *ticked* rows, and a tick is not a place — there is nowhere else
            // it could live. It appears only when something is ticked, and it
            // carries the count, so nobody presses it wondering what it means.
            let ticked = self.objects.read(cx).delegate().chosen_count();
            return h_flex()
                .w_full()
                .items_center()
                .gap(space::TIGHT)
                // The trail is the variable-width half — a directory bucket's
                // name is sixty characters — so it is the half that shrinks.
                // The verbs refuse to (`flex_shrink_0` below) and the trail
                // absorbs all of it, clipped. Letting one child shrink is only
                // half an instruction; the other half is naming who must not.
                //
                // The `mr` is not decoration, and it is a margin rather than
                // padding on purpose. At 900px with a Local Zone bucket name
                // the clip lands *exactly* at the next button's edge, and a
                // name cut mid-word touching a red `Delete` reads as one
                // broken control rather than as two. Padding does not fix it:
                // overflow clips at the padding box, so right padding is
                // *inside* the clip and the text runs straight through it.
                // Found by looking at `bucket-09e`, which is what that frame
                // is for — and the first attempt was the padding one.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .mr(space::ROW)
                        .child(trail),
                )
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(space::TIGHT)
                        .children((ticked > 0).then(|| {
                            // Apart from the benign verbs and in the danger
                            // colour: this is the one that destroys. It only
                            // opens the confirmation — nothing deletes here.
                            div()
                                .debug_selector(|| "delete-ticked-action".into())
                                .child(
                                    // Ghost, so **flat** — and deliberately:
                                    // this one opens the question, and the
                                    // `Delete` inside the confirmation is the
                                    // one that does the deed and is moulded.
                                    // Two filled danger buttons a strip apart
                                    // would make neither of them mean much.
                                    Button::new("delete-ticked-action")
                                        .label(format!("Delete {ticked}…"))
                                        .ghost()
                                        .danger()
                                        .on_click(
                                            cx.listener(|app, _, _, cx| app.delete_ticked(cx)),
                                        ),
                                )
                        }))
                        .child(
                            div().debug_selector(|| "upload-action".into()).child(
                                Button::new("upload-action")
                                    .label("Upload…")
                                    .custom(crate::theme::quiet(cx))
                                    // No `disabled`: this acts on the
                                    // location, and a location is what being
                                    // here means.
                                    .on_click(cx.listener(|app, _, window, cx| {
                                        app.upload_here(window, cx)
                                    })),
                            ),
                        )
                        .child(
                            div().debug_selector(|| "new-folder-action".into()).child(
                                Button::new("new-folder-action")
                                    .label("New folder…")
                                    .custom(crate::theme::quiet(cx))
                                    .on_click(cx.listener(|app, _, window, cx| {
                                        app.new_folder_here(window, cx)
                                    })),
                            ),
                        )
                        .child(
                            Button::new("edit-path")
                                .label("Type a location")
                                .custom(crate::theme::quiet(cx))
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.editing_path = true;
                                    cx.notify();
                                })),
                        ),
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
                    .clay()
                    .on_click(cx.listener(|app, _, window, cx| app.go_typed(window, cx))),
            )
            .child(
                Button::new("cancel-path")
                    .label("Cancel")
                    .custom(crate::theme::quiet(cx))
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
                            Button::new("preview-back")
                                .label("Back")
                                .custom(crate::theme::quiet(cx))
                                .on_click(cx.listener(|app, _, window, cx| {
                                    app.preview = None;
                                    // Back lands on a fresh listing, the same
                                    // re-read the deletion strip uses.
                                    if let Some(location) = app.location().cloned() {
                                        app.go_to(location, window, cx);
                                    }
                                })),
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
                    // **Task 3.2, decided rather than omitted.** Nothing on
                    // screen announces a context menu, and right-click is a
                    // convention people either already know or never discover
                    // — which for the *only* place Preview, Open, Download and
                    // Delete now live would be a set of verbs nobody finds.
                    //
                    // So: one muted line under the table, in the voice the
                    // "More to come." strip beside it already uses. A tooltip
                    // would need the pointer to already be where the reader
                    // does not know to put it; a toolbar hint would put back
                    // the row this change just emptied.
                    .child(
                        h_flex().w_full().items_center().px(space::TIGHT).child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "Right-click a row to preview, open, download \
                                         or delete it. Tick rows to delete several.",
                                ),
                        ),
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
                                    .custom(crate::theme::quiet(cx))
                                    .on_click(cx.listener(|app, _, _, cx| app.read_more(cx))),
                            )
                    }))
                    .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .gap(space::TIGHT)
            // Files dropped anywhere over a bucket's contents are uploaded to
            // the location on screen (`XONHO-0029`). The target is the
            // location rather than a row: dropping onto `reports/` to land
            // inside it needs a hit-tested destination and an answer for
            // dropping onto a *file*, which is a different and much larger
            // feature.
            //
            // `can_drop` and the drag-over styling are decided by the *same*
            // predicate below, because a window that shows "will land" and
            // then refuses is worse than one that shows nothing.
            .id("contents-drop-target")
            .can_drop(|dragged, _, _| droppable(dragged).is_ok())
            .drag_over::<gpui::ExternalPaths>(|style, dragged, _, cx| {
                if droppable(dragged).is_ok() {
                    style.bg(cx.theme().primary.opacity(0.08))
                } else {
                    style
                }
            })
            .on_drop::<gpui::ExternalPaths>(cx.listener(
                |app, dropped: &gpui::ExternalPaths, window, cx| {
                    app.take_dropped(dropped.paths(), window, cx);
                },
            ))
            .child(self.path_bar(&location, cx))
            // `v_flex`, not a bare `div`: the states below size themselves
            // with `size_full`, which resolves against a parent that is a
            // flex container with a height. A plain div here left the empty
            // state with nowhere to be and drew nothing at all — the same
            // family of bug as the `h_flex` one in `design-language.md`.
            .child(v_flex().flex_1().min_h_0().child(body))
            .children(self.destination_line(cx))
            .children(self.dropped_refusal.clone().map(|reason| {
                h_flex()
                    .debug_selector(|| "drop-refused".into())
                    .w_full()
                    .gap(space::TIGHT)
                    .items_center()
                    .child(div().text_sm().child(reason))
                    .child(div().flex_1())
                    .child(
                        Button::new("drop-refused-dismiss")
                            .label("Dismiss")
                            .custom(crate::theme::quiet(cx))
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.dropped_refusal = None;
                                cx.notify();
                            })),
                    )
            }))
            .children(self.queue_panel(cx))
            .children(self.deletion_line(cx))
            .children(self.folder_line(cx))
    }

    /// Which buckets this connection shows, chosen from its own listing.
    ///
    /// A strip, in the voice its neighbours use — one row, text left, actions
    /// right. The buckets themselves are ticked in a wrapping row rather than
    /// a modal: the account listing is behind it, and a dialog over the list
    /// you are choosing from hides the thing you are choosing.
    fn bucket_chooser(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let choosing = self.choosing_buckets.as_ref()?;
        let all = self.table.read(cx).delegate().all_names();

        Some(
            v_flex()
                .debug_selector(|| "bucket-chooser".into())
                .w_full()
                .gap(space::TIGHT)
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap(space::TIGHT)
                        .child(div().text_sm().child(format!(
                            "Showing {} of {} buckets on this connection",
                            choosing.len(),
                            all.len()
                        )))
                        // Beside the count, because they act on what is
                        // *ticked* — the right-hand group acts on the saved
                        // choice, and mixing the two would put "select
                        // everything" next to "forget everything" wearing
                        // similar words.
                        .child(
                            div().debug_selector(|| "chooser-none".into()).child(
                                Button::new("chooser-none")
                                    .label("None")
                                    .custom(crate::theme::quiet(cx))
                                    .on_click(cx.listener(|app, _, _, cx| {
                                        app.tick_every_bucket(false, cx)
                                    })),
                            ),
                        )
                        .child(
                            Button::new("chooser-all")
                                .label("All")
                                .custom(crate::theme::quiet(cx))
                                .on_click(
                                    cx.listener(|app, _, _, cx| app.tick_every_bucket(true, cx)),
                                ),
                        )
                        .child(div().flex_1())
                        .child(
                            div().debug_selector(|| "chooser-keep".into()).child(
                                Button::new("chooser-keep")
                                    .label("Keep these")
                                    .primary()
                                    .clay()
                                    .on_click(
                                        cx.listener(|app, _, _, cx| app.confirm_chosen_buckets(cx)),
                                    ),
                            ),
                        )
                        .child(
                            Button::new("chooser-forget")
                                // Named for the act, not the outcome. "Show
                                // every bucket" is what *All* then *Keep
                                // these* also does — and the two differ the
                                // day the account gains a bucket: a forgotten
                                // choice shows it, a chosen-everything hides
                                // it.
                                .label("Forget my choice")
                                .custom(crate::theme::quiet(cx))
                                .on_click(
                                    cx.listener(|app, _, _, cx| app.forget_bucket_choice(cx)),
                                ),
                        )
                        .child(
                            Button::new("chooser-cancel")
                                .label("Cancel")
                                .custom(crate::theme::quiet(cx))
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.choosing_buckets = None;
                                    cx.notify();
                                })),
                        ),
                )
                .child(h_flex().w_full().flex_wrap().gap(space::TIGHT).children(
                    all.into_iter().enumerate().map(|(index, name)| {
                        let ticked = choosing.contains(&name);
                        let button = Button::new(("chosen-bucket", index))
                            .label(name.clone())
                            .on_click(
                                cx.listener(move |app, _, _, cx| app.toggle_chosen(&name, cx)),
                            );
                        if ticked {
                            button.primary().clay()
                        } else {
                            button.outline()
                        }
                    }),
                ))
                .into_any_element(),
        )
    }

    /// Open the chooser, ticked to whatever is in force now (`XONHO-0027`).
    fn start_choosing_buckets(&mut self, cx: &mut Context<Self>) {
        let all = self.table.read(cx).delegate().all_names();
        // No choice recorded means everything is showing, so everything starts
        // ticked. Opening the picker must not look like a fresh empty choice.
        self.choosing_buckets = Some(self.narrowing.chosen.clone().unwrap_or(all));
        cx.notify();
    }

    /// Tick every bucket, or none of them.
    ///
    /// Asked for by the owner on 2026-08-26, and the reason is the shape of
    /// the accounts this feature exists for: theirs lists ten and they want
    /// two, so the picker opened ticked-to-everything meant eight clicks of
    /// *un*ticking before the first useful one.
    fn tick_every_bucket(&mut self, ticked: bool, cx: &mut Context<Self>) {
        let all = self.table.read(cx).delegate().all_names();
        self.choosing_buckets = Some(if ticked { all } else { Vec::new() });
        cx.notify();
    }

    /// Tick or untick one bucket in the open chooser.
    fn toggle_chosen(&mut self, bucket: &str, cx: &mut Context<Self>) {
        let Some(choosing) = self.choosing_buckets.as_mut() else {
            return;
        };
        match choosing.iter().position(|name| name == bucket) {
            Some(at) => {
                choosing.remove(at);
            }
            None => choosing.push(bucket.to_owned()),
        }
        cx.notify();
    }

    /// Keep what is ticked, and remember it for this connection.
    fn confirm_chosen_buckets(&mut self, cx: &mut Context<Self>) {
        let Some(chosen) = self.choosing_buckets.take() else {
            return;
        };
        if let (Some(session), Some(name)) = (self.session.as_ref(), self.active_connection_name())
        {
            // A choice that could not be written is still applied for this
            // run: refusing to narrow because a display preference would not
            // persist would be the file's problem becoming the user's.
            let _ = session.choose_buckets(&name, chosen.clone());
        }
        self.narrowing.chosen = Some(chosen);
        self.narrowing.showing_all = false;
        self.narrow_rows(cx);
    }

    /// Show every bucket again, **without** discarding the choice.
    ///
    /// Two different acts, and keeping them apart is a requirement rather than
    /// a nicety: someone checking whether a bucket still exists has not
    /// decided to stop using their choice.
    fn show_all_buckets(&mut self, cx: &mut Context<Self>) {
        self.narrowing.showing_all = true;
        self.narrow_rows(cx);
    }

    /// Put the choice back in force.
    fn back_to_chosen_buckets(&mut self, cx: &mut Context<Self>) {
        self.narrowing.showing_all = false;
        self.narrow_rows(cx);
    }

    /// Forget the choice for good.
    fn forget_bucket_choice(&mut self, cx: &mut Context<Self>) {
        if let (Some(session), Some(name)) = (self.session.as_ref(), self.active_connection_name())
        {
            let _ = session.clear_bucket_choice(&name);
        }
        self.narrowing.chosen = None;
        self.narrowing.showing_all = false;
        self.choosing_buckets = None;
        self.narrow_rows(cx);
    }

    /// What the active connection is called — the key a choice is filed under.
    fn active_connection_name(&self) -> Option<String> {
        let index = self.active_profile?;
        self.connections()
            .into_iter()
            .nth(index)
            .map(|(_, source)| source.name().to_owned())
    }

    /// Where the chosen file will land (`XONHO-0026`).
    ///
    /// A strip in the same voice as its neighbours — text left, actions right,
    /// full width. It is a phase rather than a permanent control: it is here
    /// only while an upload is waiting on a destination.
    fn destination_line(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let choosing = self.choosing_destination.as_ref()?;

        Some(
            h_flex()
                .debug_selector(|| "destination-strip".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                // Two meanings for one field, so the label carries the
                // difference rather than leaving it to be inferred: a key
                // when there is one file, a folder when there are several.
                .child(div().text_sm().child(if choosing.wants_a_folder() {
                    format!("Upload {} files into folder:", choosing.sources.len())
                } else {
                    "Upload to:".to_owned()
                }))
                .child(div().w(px(380.)).child(Input::new(&self.destination)))
                // The refusal sits beside the field it is about, so the fix is
                // where the mistake is.
                .children(choosing.refused.map(|bad| {
                    div()
                        .debug_selector(|| "destination-refused".into())
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(bad.to_string())
                }))
                .child(div().flex_1())
                .child(
                    div().debug_selector(|| "destination-send".into()).child(
                        Button::new("destination-send")
                            .label("Send")
                            .primary()
                            .clay()
                            .on_click(cx.listener(|app, _, _, cx| app.confirm_destination(cx))),
                    ),
                )
                .child(
                    Button::new("destination-cancel")
                        .label("Cancel")
                        .custom(crate::theme::quiet(cx))
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.choosing_destination = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
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
                .custom(crate::theme::quiet(cx))
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
                            .clay()
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
    /// The deletion's own strip: the question, then what came of it.
    ///
    /// One strip for one object and for twenty, because they are one act with
    /// one confirmation. What changes is the sentence, and the sentence is
    /// decided here rather than by the caller — so that "1 object" can never
    /// be rendered where a key would say more.
    fn deletion_line(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let deletion = self.deletion.as_ref()?;

        let dismiss = || {
            Button::new("delete-dismiss")
                .label("Dismiss")
                .custom(crate::theme::quiet(cx))
                .on_click(cx.listener(|app, _, _, cx| app.dismiss_deletion(cx)))
        };
        let cancel = |id: &'static str| {
            Button::new(id)
                .label("Cancel")
                .custom(crate::theme::quiet(cx))
                .on_click(cx.listener(|app, _, _, cx| app.dismiss_deletion(cx)))
        };

        let line = match &deletion.phase {
            DeletePhase::Counting { left, .. } => h_flex()
                .debug_selector(|| "delete-counting".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(div().text_sm().child(match &deletion.asked {
                    Asked::Folder(name) => {
                        format!("Counting what is under `{name}/`…")
                    }
                    _ => format!("Counting what is under {left} folders…"),
                }))
                .child(div().flex_1())
                // Present, and it stops the walks rather than only hiding
                // them: a listing nobody is waiting for is still requests
                // going out.
                .child(
                    div()
                        .debug_selector(|| "delete-counting-cancel".into())
                        .child(cancel("delete-counting-cancel")),
                )
                .into_any_element(),
            DeletePhase::Empty => h_flex()
                .debug_selector(|| "delete-empty".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                // Told, not offered as a confirmation for zero: nobody should
                // be asked to agree to no work.
                .child(div().text_sm().child(match &deletion.asked {
                    Asked::Folder(name) => format!(
                        "`{name}/` holds nothing, so there is nothing to delete. \
                         On a directory bucket a folder disappears with its last object."
                    ),
                    _ => "There is nothing under what you ticked.".to_owned(),
                }))
                .child(div().flex_1())
                .child(dismiss())
                .into_any_element(),
            DeletePhase::TooMany { at_least } => h_flex()
                .debug_selector(|| "delete-too-many".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        // `at_least`, never a total: the walk stopped, so the
                        // real number is unknown and saying one would be an
                        // invention.
                        .child(format!(
                            "More than {at_least} objects — too many to delete in one go. \
                             Nothing was deleted. Go into it and delete in smaller parts."
                        )),
                )
                .child(div().flex_1())
                .child(dismiss())
                .into_any_element(),
            DeletePhase::Confirming { keys } => h_flex()
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
                        .child(confirmation_sentence(&deletion.asked, keys.len())),
                )
                .child(div().flex_1())
                .child(
                    div().debug_selector(|| "delete-confirm".into()).child(
                        Button::new("delete-confirm")
                            .label("Delete")
                            .danger()
                            .clay()
                            .on_click(cx.listener(|app, _, _, cx| app.confirm_delete(cx))),
                    ),
                )
                .child(
                    div()
                        .debug_selector(|| "delete-cancel".into())
                        .child(cancel("delete-cancel")),
                )
                .into_any_element(),
            DeletePhase::Deleting { done, total } => h_flex()
                .debug_selector(|| "delete-in-flight".into())
                .w_full()
                .items_center()
                .child(div().text_sm().child(if *total == 1 {
                    "Deleting…".to_owned()
                } else {
                    format!("Deleting… {done} of {total}")
                }))
                .into_any_element(),
            DeletePhase::Gone { marker } => {
                let key = match &deletion.asked {
                    Asked::Object(key) => key.clone(),
                    _ => String::new(),
                };
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
                                    .custom(crate::theme::quiet(cx))
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
            DeletePhase::Went { gone, failures } => {
                let mut line = h_flex()
                    .debug_selector(|| "delete-went".into())
                    .w_full()
                    .gap(space::TIGHT)
                    .items_center()
                    .child(
                        v_flex()
                            .gap(space::TIGHT)
                            .child(div().text_sm().child(if failures.is_empty() {
                                format!("Deleted {gone} objects.")
                            } else {
                                format!(
                                    "Deleted {gone} objects; {} could not be deleted.",
                                    failures.len()
                                )
                            }))
                            // Said, never merely omitted. The user has seen
                            // Undo appear after a single delete, so its
                            // silent absence would read as a bug rather than
                            // as a decision — which is exactly what happened
                            // the one time it correctly did not appear.
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        "Undo is not offered for a bulk delete: it would be \
                                         many restores that could half-succeed.",
                                    ),
                            )
                            // Each refusal keeps its own key and its own
                            // cause. "Some failed" sends nobody anywhere.
                            .children(failures.iter().take(5).map(|failure| {
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().danger)
                                    .child(failure.clone())
                            }))
                            .children((failures.len() > 5).then(|| {
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("…and {} more, in the log.", failures.len() - 5))
                            })),
                    );
                line = line.child(div().flex_1());
                line.child(dismiss()).into_any_element()
            }
            DeletePhase::Restoring => h_flex()
                .debug_selector(|| "delete-restoring".into())
                .w_full()
                .items_center()
                .child(div().text_sm().child("Restoring…"))
                .into_any_element(),
            DeletePhase::Restored => h_flex()
                .debug_selector(|| "delete-restored".into())
                .w_full()
                .gap(space::TIGHT)
                .items_center()
                .child(div().text_sm().child(match &deletion.asked {
                    Asked::Object(key) => format!("`{key}` is back."),
                    _ => "It is back.".to_owned(),
                }))
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
                                    "Could not restore it — the marker still stands: {rendered}"
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

    /// The queue, in the slot the single transfer's strip used to hold.
    ///
    /// **Where it lives, which `design.md` deliberately left open.** Under the
    /// listing, in the strip's own place, with its height capped and its rows
    /// scrolling inside it. The constraint the design set was that a queue of
    /// twenty must not hide what the user is browsing — and a capped box costs
    /// the same screen for twenty as for two, which is the only shape that
    /// satisfies it without inventing an expand-and-collapse the rest of this
    /// window has no precedent for.
    ///
    /// Absent entirely when there is nothing in it: an empty frame is a
    /// permanent grey box nobody asked for.
    fn queue_panel(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if self.queue.is_empty() {
            return None;
        }
        let (finished, total) = self.queue.progress();
        let rows: Vec<AnyElement> = self
            .queue
            .items()
            .iter()
            .filter_map(|item| self.transfer_row(item.id, &item.payload, cx))
            .collect();

        Some(
            v_flex()
                .debug_selector(|| "queue-panel".into())
                .w_full()
                .gap(space::TIGHT)
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap(space::TIGHT)
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{finished} of {total} transferred")),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("queue-retry")
                                .label("Retry failed")
                                .custom(crate::theme::quiet(cx))
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.queue.retry_failed();
                                    app.start_ready(cx);
                                })),
                        )
                        .child(
                            Button::new("queue-clear")
                                .label("Clear finished")
                                .custom(crate::theme::quiet(cx))
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.queue.clear_finished();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("queue-cancel")
                                .label("Cancel all")
                                .custom(crate::theme::quiet(cx))
                                .on_click(cx.listener(|app, _, _, cx| app.cancel_queue(cx))),
                        ),
                )
                .child(
                    v_flex()
                        .w_full()
                        .max_h(px(180.))
                        .overflow_hidden()
                        .gap(space::TIGHT)
                        .children(rows),
                )
                .into_any_element(),
        )
    }

    /// Stop everything not yet finished.
    ///
    /// Three things, and the third is the one that was missing.
    ///
    /// A **running** transfer is stopped by its `Cancel`, and its own
    /// settlement arrives shortly after saying `Cancelled` — that closes the
    /// loop by itself. A transfer that is **waiting for a slot** or **waiting
    /// for a person to answer a collision** has no request in flight, so no
    /// settlement is ever coming, and marking it in the queue changes nothing
    /// the screen draws: rows render by *phase*, and its phase still says
    /// `Running` or `KeyTaken`.
    ///
    /// That was the defect the owner found by pressing Cancel during a
    /// collision question and watching the question stay. `XONHO-0028` made
    /// `Standing` derive from `TransferPhase` and never closed the other
    /// direction — when the queue decides something the phase also describes,
    /// it has to say so.
    fn cancel_queue(&mut self, cx: &mut Context<Self>) {
        let stranded: Vec<TransferId> = self
            .queue
            .items()
            .iter()
            .filter(|item| {
                !matches!(
                    item.standing,
                    Standing::Finished | Standing::Failed | Standing::Cancelled
                )
            })
            .map(|item| {
                item.payload.cancel.cancel();
                (item.id, item.standing)
            })
            // Only these two have nothing in flight to answer for them.
            .filter(|(_, standing)| matches!(standing, Standing::Waiting | Standing::Asking))
            .map(|(id, _)| id)
            .collect();

        self.queue.cancel_all();
        for id in stranded {
            if let Some(transfer) = self.queue.payload_mut(id) {
                transfer.phase = TransferPhase::Cancelled;
            }
        }
        cx.notify();
    }

    /// Start whatever the queue says may begin.
    fn start_ready(&mut self, cx: &mut Context<Self>) {
        for id in self.queue.ready() {
            let Some(transfer) = self.queue.payload_mut(id) else {
                continue;
            };
            let (bucket, key, directory, then_open, source) = (
                transfer.bucket.clone(),
                transfer.key.clone(),
                transfer.directory.clone(),
                transfer.then_open,
                transfer.source.clone(),
            );
            match source {
                Some(path) => self.start_upload(
                    Some(id),
                    bucket,
                    key,
                    path,
                    caixonho_core::transfer::Collision::Ask,
                    cx,
                ),
                None => self.start_download(
                    Some(id),
                    Transfer::down(bucket, key, directory, then_open),
                    caixonho_core::transfer::Collision::Ask,
                    cx,
                ),
            }
        }
    }

    /// One transfer's own row: what it is doing, and what can be done to it.
    ///
    /// Was `transfer_line`, rendering *the* transfer. It renders *a* transfer
    /// now and takes which one, because with a queue the phrase "the transfer"
    /// has no referent (`XONHO-0028`).
    fn transfer_row(
        &self,
        id: TransferId,
        transfer: &Transfer,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        use caixonho_core::transfer::MappingOutcome;

        let dismiss = || {
            Button::new(("transfer-dismiss", id.0 as usize))
                .label("Dismiss")
                .custom(crate::theme::quiet(cx))
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.queue.forget(id);
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
                        Button::new(("transfer-cancel", id.0 as usize))
                            .label("Cancel")
                            .custom(crate::theme::quiet(cx))
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
                    Button::new(("collision-replace", id.0 as usize))
                        .label("Replace")
                        .custom(crate::theme::quiet(cx))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.answer_collision(
                                id,
                                caixonho_core::transfer::Collision::Replace,
                                cx,
                            )
                        })),
                )
                .child(
                    Button::new(("collision-keep-both", id.0 as usize))
                        .label("Keep both")
                        .custom(crate::theme::quiet(cx))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.answer_collision(
                                id,
                                caixonho_core::transfer::Collision::KeepBoth,
                                cx,
                            )
                        })),
                )
                .child(
                    Button::new(("collision-abandon", id.0 as usize))
                        .label("Cancel")
                        .custom(crate::theme::quiet(cx))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.queue.forget(id);
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
                    //
                    // That reasoning was sound and the sentence was still a
                    // lie, because until 2026-08-27 **nothing called the
                    // opener at all**. `then_open` was stored, passed around,
                    // and read only here, to choose these words. A
                    // justification for behaviour that did not exist, and it
                    // read plausibly enough that nobody went looking. The
                    // owner found it by pressing Open.
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
                            .custom(crate::theme::quiet(cx))
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
                        Button::new(("key-replace", id.0 as usize))
                            .label("Replace")
                            .custom(crate::theme::quiet(cx))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.answer_key_collision(
                                    id,
                                    caixonho_core::transfer::Collision::Replace,
                                    cx,
                                )
                            })),
                    )
                    .child(
                        Button::new(("key-keep-both", id.0 as usize))
                            .label("Keep both")
                            .custom(crate::theme::quiet(cx))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.answer_key_collision(
                                    id,
                                    caixonho_core::transfer::Collision::KeepBoth,
                                    cx,
                                )
                            })),
                    )
                    .child(
                        Button::new(("key-abandon", id.0 as usize))
                            .label("Cancel")
                            .custom(crate::theme::quiet(cx))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.queue.forget(id);
                                cx.notify();
                            })),
                    )
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
                        .custom(crate::theme::quiet(cx))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.answer_key_collision(
                                id,
                                caixonho_core::transfer::Collision::Replace,
                                cx,
                            )
                        })),
                )
                .child(
                    Button::new("unsupported-abandon")
                        .label("Cancel")
                        .custom(crate::theme::quiet(cx))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.queue.forget(id);
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
                    // Under the controls it belongs to, and on the *account*
                    // screen — it was first wired into the strip row beneath a
                    // bucket's contents, where it could never appear, and the
                    // harness's distinctness assertion is what said so.
                    .children(self.bucket_chooser(cx))
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
            // Body type for the window as a whole, and display type named only
            // where the design system names it: titles, labels, and the text
            // on a button. The system forbids mixing the two inside one block,
            // so the default has to be the one used for reading.
            .font_family(crate::theme::FONT_BODY)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child(
                    div()
                        .font_family(crate::theme::FONT_DISPLAY)
                        .font_weight(gpui::FontWeight::EXTRA_BOLD)
                        .text_color(cx.theme().sidebar_primary)
                        .child("caixonho"),
                ),
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
        looking_through(cx, bucket_name, Arc::new(StoreDouble::allows_listing()))
    }

    /// [`looking_at`], through a double the test keeps its own handle on — so
    /// it can ask afterwards what actually crossed the port. "Nothing was
    /// sent" is not assertable any other way.
    fn looking_through<'a>(
        cx: &'a mut TestAppContext,
        bucket_name: &str,
        double: Arc<StoreDouble>,
    ) -> (gpui::Entity<CaixonhoApp>, &'a mut gpui::VisualTestContext) {
        cx.update(gpui_component::init);
        cx.update(crate::theme::install);
        let store: Arc<dyn caixonho_core::ObjectStore> = double;
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

    #[gpui::test]
    fn leaving_the_location_takes_the_ticked_rows_with_it(cx: &mut TestAppContext) {
        // A tick is about a row at a location, exactly as a deletion strip and
        // a preview are, and the three leave together. What this guards
        // against is the shape of the bug rather than its likelihood: ticks
        // surviving a departure would be invisible — no rows are on screen to
        // show them — and the next Delete would act on rows in a bucket the
        // user is no longer standing in.
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.objects.update(cx, |table, _| {
                let delegate = table.delegate_mut();
                delegate.show(
                    CorePrefix::root(),
                    Vec::new(),
                    vec![an_object("one.txt", 1), an_object("two.txt", 2)],
                );
                delegate.toggle(0);
                delegate.toggle(1);
            });
            assert_eq!(app.objects.read(cx).delegate().chosen_count(), 2);

            app.leave_bucket(cx);
        });

        assert_eq!(
            app.read_with(cx, |app, cx| app.objects.read(cx).delegate().chosen_count()),
            0,
            "the ticks belonged to a location nobody is standing in any more"
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
        // **The application's own theme and fonts, which this harness went
        // without until `XONHO-0032`.** It called `gpui_component::init` and
        // stopped, so every frame it has ever written photographed the
        // *toolkit's* default styling — and the brand ramp in `theme.json` had
        // never once appeared in a judgement image. A harness for judging how
        // the window looks, blind to how the window is dressed.

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

    /// The verbs gate on the row being an object: a folder can be neither
    /// downloaded nor opened, and a row that is not there is not one either.
    ///
    /// Was about the *selection* until `XONHO-0030`. The verbs are reached
    /// from a row's own menu now, so the question the gate answers changed
    /// from "what is selected" to "what is this row" — and this test is the
    /// same guard asked the new way.
    #[gpui::test]
    fn the_object_verbs_light_up_only_on_an_object_row(cx: &mut TestAppContext) {
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
            assert_eq!(
                app.object_key_at(0, cx),
                None,
                "row 0 is a folder, and a folder is not an object"
            );
            assert_eq!(
                app.object_key_at(1, cx).as_deref(),
                Some("summary.csv"),
                "the object row is what the verbs act on"
            );
            assert_eq!(
                app.object_key_at(9, cx),
                None,
                "and a row that is not there yields nothing"
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
            let id = queued(
                app,
                Transfer {
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
                },
            );
            app.apply_transfer(
                id,
                TransferEvent::Progress {
                    bytes: 512,
                    total: Some(1024),
                },
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            let transfer = only(app);
            assert_eq!((transfer.bytes, transfer.total), (512, Some(1024)));
            assert!(matches!(transfer.phase, TransferPhase::Running));
        });

        app.update(cx, |app, cx| {
            let id = only_id(app);
            app.apply_transfer(
                id,
                TransferEvent::Settled(caixonho_core::transfer::DownloadOutcome::Finished {
                    name: "summary.csv".into(),
                    mapped: caixonho_core::transfer::MappingOutcome::Unchanged,
                    bytes: 1024,
                }),
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            let transfer = only(app);
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
            let id = queued(app, an_upload(TransferPhase::Running));
            // Dismissed, and *then* the answer arrives.
            app.queue.forget(id);
            app.apply_transfer(
                id,
                TransferEvent::Settled(caixonho_core::transfer::DownloadOutcome::Cancelled),
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            assert!(app.queue.is_empty(), "nothing came back from the dead");
        });
    }

    /// Answering the existing-file question starts the download over with
    /// the answer — synchronously back into Running, holding the same
    /// object.
    #[gpui::test]
    fn answering_a_collision_reissues_the_download(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            let id = queued(
                app,
                Transfer {
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
                },
            );
            app.answer_collision(id, caixonho_core::transfer::Collision::KeepBoth, cx);
        });
        app.read_with(cx, |app, _| {
            let transfer = only(app);
            assert!(matches!(transfer.phase, TransferPhase::Running));
            assert_eq!(transfer.key, "daily/summary.csv", "the same object");
        });
    }

    // ---- Uploading (XONHO-0020 tasks 4.1–4.2) ----

    /// Put one transfer in the queue, running, and return its id.
    ///
    /// Every test below used to assign `app.transfer`; with a queue the same
    /// intent is "the window has taken this on", and the id is what the
    /// assertions and the collision answers now need.
    fn queued(app: &mut CaixonhoApp, transfer: Transfer) -> TransferId {
        let id = app.queue.accept(transfer);
        app.queue.settled(id, Standing::Running);
        id
    }

    /// The id of the one transfer a single-transfer test put in the queue.
    fn only_id(app: &CaixonhoApp) -> TransferId {
        app.queue.items().first().expect("one transfer queued").id
    }

    /// The one transfer a single-transfer test put in the queue.
    fn only(app: &CaixonhoApp) -> &Transfer {
        &app.queue
            .items()
            .first()
            .expect("one transfer queued")
            .payload
    }

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
            let id = queued(
                app,
                an_upload(TransferPhase::KeyTaken {
                    key: "daily/summary.csv".into(),
                }),
            );
            app.answer_key_collision(id, caixonho_core::transfer::Collision::KeepBoth, cx);
        });
        app.read_with(cx, |app, _| {
            let transfer = only(app);
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
                app.queue = Queue::new(TRANSFERS_AT_ONCE);
                let id = queued(app, an_upload(TransferPhase::Running));
                app.apply_transfer(id, TransferEvent::UploadSettled(outcome), cx);
            });
            app.read_with(cx, |app, _| {
                let phase = &only(app).phase;
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
            let id = queued(app, an_upload(TransferPhase::Running));
            app.apply_transfer(
                id,
                TransferEvent::UploadSettled(caixonho_core::transfer::UploadOutcome::Finished {
                    key: "daily/summary (2).csv".into(),
                    stepped_aside: true,
                    bytes: 4096,
                }),
                cx,
            );
        });
        app.read_with(cx, |app, _| match &only(app).phase {
            TransferPhase::Sent { key, stepped_aside } => {
                assert_eq!(key, "daily/summary (2).csv");
                assert!(stepped_aside, "the window must know to say so");
            }
            other => panic!("expected Sent, got {}", phase_name(other)),
        });
    }

    // ---- Deleting (XONHO-0021 tasks 3.1–3.2, XONHO-0030 section 4) ----

    /// A deletion of one named object, in `phase`.
    fn deleting_one(connection: ConnectionId, phase: DeletePhase) -> Deletion {
        Deletion {
            connection,
            bucket: "reports".into(),
            asked: Asked::Object("daily/summary.csv".into()),
            phase,
        }
    }

    /// The queue item id a single hand-built delete settles under. Every
    /// delete goes through the queue, including one, so a test that settles
    /// one has to put it there first.
    fn one_queued_delete(app: &mut CaixonhoApp, key: &str) -> TransferId {
        app.deletes.accept(key.to_owned())
    }

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
            app.delete_row(0, cx);
        });
        app.read_with(cx, |app, _| {
            let deletion = app.deletion.as_ref().expect("the confirmation is up");
            match &deletion.phase {
                DeletePhase::Confirming { keys } => {
                    assert_eq!(keys, &["daily/summary.csv".to_owned()]);
                }
                _ => panic!("expected Confirming"),
            }
            assert!(matches!(&deletion.asked, Asked::Object(key) if key == "daily/summary.csv"));
        });

        // Dismiss: the deletion state is gone and — the point of the
        // two-act rule — nothing was ever spawned, because only
        // confirm_delete spawns and it was never called.
        app.update(cx, |app, cx| app.dismiss_deletion(cx));
        app.read_with(cx, |app, _| assert!(app.deletion.is_none()));
    }

    /// A row that is not there: the action does nothing at all.
    ///
    /// Was "a folder is not deletable here" until `XONHO-0030`. A folder now
    /// *is* deletable — it counts what is under it first — so what is left to
    /// guard is the case that still means nothing: no such row.
    #[gpui::test]
    fn deleting_a_row_that_is_not_there_does_nothing(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.objects.update(cx, |state, _| {
                state
                    .delegate_mut()
                    .show(CorePrefix::root(), Vec::new(), Vec::new());
            });
            app.delete_row(0, cx);
        });
        app.read_with(cx, |app, _| {
            assert!(app.deletion.is_none(), "no row, no confirmation");
        });
    }

    /// Deleting with nothing ticked asks nothing. The button is not rendered
    /// then, but the state machine must not depend on the render layer for
    /// that — the same discipline `confirming_is_what_arms_the_delete` keeps.
    #[gpui::test]
    fn deleting_nothing_ticked_asks_nothing(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.objects.update(cx, |state, _| {
                state.delegate_mut().show(
                    CorePrefix::root(),
                    Vec::new(),
                    vec![an_object("one.txt", 1)],
                );
            });
            app.delete_ticked(cx);
        });
        app.read_with(cx, |app, _| assert!(app.deletion.is_none()));
    }

    // ---- Deleting more than one (XONHO-0030 section 4) ----

    /// The sentence, branch by branch. It is the last thing between a person
    /// and losing data, so every form of it is asserted rather than inferred
    /// from one.
    #[test]
    fn the_confirmation_names_one_object_and_counts_more_than_one() {
        // A name, whenever it is one object — however that one was reached.
        assert_eq!(
            confirmation_sentence(&Asked::Object("daily/summary.csv".into()), 1),
            "Delete `daily/summary.csv` from this bucket?"
        );

        // Past one, the count: a list of twenty keys is a list the eye skims,
        // and the number is the thing a person can check against what they
        // meant to tick.
        assert_eq!(
            confirmation_sentence(&Asked::Rows(3), 3),
            "Delete 3 objects?"
        );

        // The surprise worth saying out loud: three ticks, forty-seven
        // objects, because one of the ticks was a folder.
        assert_eq!(
            confirmation_sentence(&Asked::Rows(3), 47),
            "Delete 47 objects from the 3 rows you ticked?"
        );

        // A folder is named even though the work is its keys, because the
        // folder is what the user pointed at.
        assert_eq!(
            confirmation_sentence(&Asked::Folder("daily".into()), 12),
            "Delete `daily/` and everything under it — 12 objects — from this bucket?"
        );

        // And nothing says "1 objects".
        assert_eq!(
            confirmation_sentence(&Asked::Folder("daily".into()), 1),
            "Delete `daily/` and everything under it — 1 object — from this bucket?"
        );
    }

    /// Ticking several rows and pressing Delete asks about all of them, and
    /// asks **before** anything is sent.
    #[gpui::test]
    fn ticked_rows_are_confirmed_together_and_nothing_is_sent_first(cx: &mut TestAppContext) {
        let deleted = Arc::new(StoreDouble::allows_listing());
        let (app, cx) = looking_through(cx, "reports", deleted.clone());
        app.update(cx, |app, cx| {
            app.objects.update(cx, |state, _| {
                let delegate = state.delegate_mut();
                delegate.show(
                    CorePrefix::root(),
                    Vec::new(),
                    vec![
                        an_object("one.txt", 1),
                        an_object("two.txt", 2),
                        an_object("three.txt", 3),
                    ],
                );
                delegate.toggle(0);
                delegate.toggle(2);
            });
            app.delete_ticked(cx);
        });

        app.read_with(cx, |app, _| {
            match &app.deletion.as_ref().expect("the confirmation is up").phase {
                DeletePhase::Confirming { keys } => {
                    assert_eq!(keys, &["one.txt".to_owned(), "three.txt".to_owned()]);
                }
                other => panic!("expected Confirming, got {}", delete_phase_name(other)),
            }
            assert!(matches!(
                app.deletion.as_ref().expect("held").asked,
                Asked::Rows(2)
            ));
        });
        assert!(
            deleted.deleted_keys().is_empty(),
            "the confirmation is a question, not a delete with a receipt"
        );
    }

    /// A folder counts first, and **cannot be confirmed while counting**.
    #[gpui::test]
    fn a_folder_counts_before_it_asks_and_refuses_to_be_confirmed_meanwhile(
        cx: &mut TestAppContext,
    ) {
        let deleted = Arc::new(StoreDouble::allows_listing());
        let (app, cx) = looking_through(cx, "reports", deleted.clone());
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
            app.delete_row(0, cx);
        });

        app.read_with(cx, |app, _| {
            assert!(
                matches!(
                    app.deletion.as_ref().expect("held").phase,
                    DeletePhase::Counting { left: 1, .. }
                ),
                "a folder's size is not known until something goes and looks"
            );
        });

        // The requirement: a dialog offering a number it may still be about
        // to change must not be confirmable. Yes to a number that then moves
        // is a yes to a different question.
        app.update(cx, |app, cx| app.confirm_delete(cx));
        app.read_with(cx, |app, _| {
            assert!(
                matches!(
                    app.deletion.as_ref().expect("held").phase,
                    DeletePhase::Counting { .. }
                ),
                "confirming mid-count did something"
            );
        });
        assert!(deleted.deleted_keys().is_empty(), "and sent nothing");

        // The count lands, and only then is there something to agree to.
        app.update_in(cx, |app, window, cx| {
            app.apply_delete(
                DeleteEvent::Counted(caixonho_core::session::Tally::All(vec![
                    "daily/a.txt".into(),
                    "daily/b.txt".into(),
                ])),
                window,
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            match &app.deletion.as_ref().expect("held").phase {
                DeletePhase::Confirming { keys } => assert_eq!(keys.len(), 2),
                other => panic!("expected Confirming, got {}", delete_phase_name(other)),
            }
        });
    }

    /// A prefix that holds nothing is *told*, not shown as a confirmation for
    /// zero: nobody should be asked to agree to no work.
    #[gpui::test]
    fn a_folder_that_holds_nothing_says_so(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update_in(cx, |app, window, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                asked: Asked::Folder("daily".into()),
                phase: DeletePhase::Counting {
                    cancels: Vec::new(),
                    left: 1,
                    gathered: Vec::new(),
                },
            });
            app.apply_delete(
                DeleteEvent::Counted(caixonho_core::session::Tally::All(Vec::new())),
                window,
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            assert!(matches!(
                app.deletion.as_ref().expect("held").phase,
                DeletePhase::Empty
            ));
        });
    }

    /// A prefix past the walk's bound refuses the whole ask, and says a
    /// floor rather than a total.
    #[gpui::test]
    fn a_folder_too_large_to_walk_refuses_the_whole_ask(cx: &mut TestAppContext) {
        let deleted = Arc::new(StoreDouble::allows_listing());
        let (app, cx) = looking_through(cx, "reports", deleted.clone());
        app.update_in(cx, |app, window, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                asked: Asked::Rows(2),
                phase: DeletePhase::Counting {
                    cancels: Vec::new(),
                    left: 2,
                    // Objects already ticked. They go too — the ask is one
                    // ask, and half of it is not an answer.
                    gathered: vec!["one.txt".into()],
                },
            });
            app.apply_delete(
                DeleteEvent::Counted(caixonho_core::session::Tally::TooMany { at_least: 5_001 }),
                window,
                cx,
            );
        });
        app.read_with(cx, |app, _| {
            assert!(matches!(
                app.deletion.as_ref().expect("held").phase,
                DeletePhase::TooMany { at_least: 5_001 }
            ));
        });
        assert!(
            deleted.deleted_keys().is_empty(),
            "refusing the ask means refusing all of it"
        );
    }

    /// Confirming several arms the queue, and a refusal in the middle leaves
    /// the rest deleted with its own cause kept.
    #[gpui::test]
    fn one_refusal_among_many_does_not_stop_the_others(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        let ids = app.update(cx, |app, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                asked: Asked::Rows(3),
                phase: DeletePhase::Confirming {
                    keys: vec!["a.txt".into(), "b.txt".into(), "c.txt".into()],
                },
            });
            app.confirm_delete(cx);
            app.deletes
                .items()
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>()
        });
        assert_eq!(ids.len(), 3, "three keys, three queue items");

        app.update_in(cx, |app, window, cx| {
            use caixonho_core::session::DeleteOutcome;
            app.apply_delete(
                DeleteEvent::Settled {
                    id: ids[0],
                    outcome: DeleteOutcome::Gone { marker: None },
                },
                window,
                cx,
            );
            app.apply_delete(
                DeleteEvent::Settled {
                    id: ids[1],
                    outcome: DeleteOutcome::Failed(Error::AccessDenied {
                        iam_action: "s3:DeleteObject",
                    }),
                },
                window,
                cx,
            );
            assert!(
                matches!(
                    app.deletion.as_ref().expect("held").phase,
                    DeletePhase::Deleting { done: 2, total: 3 }
                ),
                "a refusal in the middle is not the end of the run"
            );
            app.apply_delete(
                DeleteEvent::Settled {
                    id: ids[2],
                    outcome: DeleteOutcome::Gone { marker: None },
                },
                window,
                cx,
            );
        });

        app.read_with(cx, |app, _| {
            match &app.deletion.as_ref().expect("held").phase {
                DeletePhase::Went { gone, failures } => {
                    assert_eq!(*gone, 2, "the two that went, went");
                    assert_eq!(failures.len(), 1);
                    assert!(
                        failures[0].starts_with("b.txt: "),
                        "each refusal keeps its own key and its own cause, got {:?}",
                        failures[0]
                    );
                }
                other => panic!("expected Went, got {}", delete_phase_name(other)),
            }
        });
    }

    /// A bulk delete offers no Undo — and the *marker* is not the reason: one
    /// of these deletes reported one, and it still does not.
    #[gpui::test]
    fn a_bulk_delete_offers_no_undo_even_when_a_marker_came_back(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        let ids = app.update(cx, |app, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                asked: Asked::Rows(2),
                phase: DeletePhase::Confirming {
                    keys: vec!["a.txt".into(), "b.txt".into()],
                },
            });
            app.confirm_delete(cx);
            app.deletes
                .items()
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>()
        });

        app.update_in(cx, |app, window, cx| {
            use caixonho_core::session::DeleteOutcome;
            for id in &ids {
                app.apply_delete(
                    DeleteEvent::Settled {
                        id: *id,
                        outcome: DeleteOutcome::Gone {
                            marker: Some("mk-1".into()),
                        },
                    },
                    window,
                    cx,
                );
            }
        });

        app.read_with(cx, |app, _| {
            assert!(
                matches!(
                    app.deletion.as_ref().expect("held").phase,
                    DeletePhase::Went { .. }
                ),
                "a bulk delete never reaches Gone, which is where the Undo button lives"
            );
        });

        // And pressing it anyway — as a stale click might — restores nothing.
        app.update(cx, |app, cx| app.undo_delete(cx));
        app.read_with(cx, |app, _| {
            assert!(matches!(
                app.deletion.as_ref().expect("held").phase,
                DeletePhase::Went { .. }
            ));
        });
    }

    /// Dismissing a confirmation that is still counting stops the walks.
    #[gpui::test]
    fn dismissing_mid_count_stops_the_walk(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        let cancel = Cancel::default();
        app.update(cx, |app, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                asked: Asked::Folder("daily".into()),
                phase: DeletePhase::Counting {
                    cancels: vec![cancel.clone()],
                    left: 1,
                    gathered: Vec::new(),
                },
            });
            app.dismiss_deletion(cx);
        });

        app.read_with(cx, |app, _| assert!(app.deletion.is_none()));
        // The walk has to hear about it: listings nobody is waiting for are
        // still requests going out.
        app.update_in(cx, |app, window, cx| {
            app.apply_delete(
                DeleteEvent::Counted(caixonho_core::session::Tally::Cancelled),
                window,
                cx,
            );
        });
        app.read_with(cx, |app, _| assert!(app.deletion.is_none()));
    }

    /// The close-out review's find: an abandoned bulk delete must not leave
    /// keys in the queue for the *next* confirmation to send.
    ///
    /// `start_ready_deletes` reads the bucket from `self.deletion`, so waiting
    /// items surviving a dismissal would be spawned against whatever the next
    /// deletion is about — keys from one bucket deleted from another. Pruning
    /// finished items, which is all the queue did, is not the same thing.
    #[gpui::test]
    fn an_abandoned_bulk_delete_leaves_nothing_for_the_next_one(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "reports".into(),
                asked: Asked::Rows(6),
                phase: DeletePhase::Confirming {
                    keys: (0..6).map(|n| format!("reports-only/{n}.txt")).collect(),
                },
            });
            app.confirm_delete(cx);
            // Four run at once, so two are left waiting — which is exactly
            // the state that used to survive.
            assert_eq!(app.deletes.items().len(), 6);
            assert_eq!(app.deletes.running(), DELETES_AT_ONCE);

            app.dismiss_deletion(cx);
        });

        app.read_with(cx, |app, _| {
            assert!(app.deletion.is_none());
            assert!(
                app.deletes.is_empty(),
                "six keys for `reports` are still queued with nothing to send them to"
            );
        });

        // And the next confirmation, about somewhere else, sends only its own.
        app.update(cx, |app, cx| {
            app.deletion = Some(Deletion {
                connection: app.outcome.active(),
                bucket: "logs".into(),
                asked: Asked::Object("logs/one.txt".into()),
                phase: DeletePhase::Confirming {
                    keys: vec!["logs/one.txt".into()],
                },
            });
            app.confirm_delete(cx);
        });
        app.read_with(cx, |app, _| {
            let queued: Vec<&String> = app.deletes.items().iter().map(|i| &i.payload).collect();
            assert_eq!(
                queued,
                vec![&"logs/one.txt".to_owned()],
                "the second confirmation sent the first one's keys to another bucket"
            );
        });
    }

    /// Naming a phase, for a panic message that says which one it was.
    fn delete_phase_name(phase: &DeletePhase) -> &'static str {
        match phase {
            DeletePhase::Counting { .. } => "Counting",
            DeletePhase::Confirming { .. } => "Confirming",
            DeletePhase::Empty => "Empty",
            DeletePhase::TooMany { .. } => "TooMany",
            DeletePhase::Deleting { .. } => "Deleting",
            DeletePhase::Gone { .. } => "Gone",
            DeletePhase::Went { .. } => "Went",
            DeletePhase::Restoring => "Restoring",
            DeletePhase::Restored => "Restored",
            DeletePhase::Failed { .. } => "Failed",
        }
    }

    /// The undo is offered exactly on proof, and a settled delete re-reads
    /// the listing — observable as the listing going back to Loading.
    #[gpui::test]
    fn a_settled_delete_shows_its_proofed_undo_and_rereads(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update_in(cx, |app, window, cx| {
            app.deletion = Some(deleting_one(
                app.outcome.active(),
                DeletePhase::Deleting { done: 0, total: 1 },
            ));
            let id = one_queued_delete(app, "daily/summary.csv");
            app.apply_delete(
                DeleteEvent::Settled {
                    id,
                    outcome: caixonho_core::session::DeleteOutcome::Gone {
                        marker: Some("mk-9".into()),
                    },
                },
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
            app.deletion = Some(deleting_one(
                ConnectionId(9_999),
                DeletePhase::Deleting { done: 0, total: 1 },
            ));
            let id = one_queued_delete(app, "daily/summary.csv");
            app.apply_delete(
                DeleteEvent::Settled {
                    id,
                    outcome: caixonho_core::session::DeleteOutcome::Gone {
                        marker: Some("mk-9".into()),
                    },
                },
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
            app.deletion = Some(deleting_one(app.outcome.active(), DeletePhase::Restoring));
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
            app.deletion = Some(deleting_one(
                app.outcome.active(),
                DeletePhase::Confirming {
                    keys: vec!["daily/summary.csv".into()],
                },
            ));
            app.confirm_delete(cx);
        });
        app.read_with(cx, |app, _| {
            assert!(
                matches!(
                    app.deletion.as_ref().expect("held").phase,
                    DeletePhase::Deleting { done: 0, total: 1 }
                ),
                "the second act moves it to in-flight"
            );
        });

        // And from any other phase, the same call is a no-op: the button
        // only exists on Confirming, but the state machine must not rely on
        // the render layer for that.
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, cx| {
            app.deletion = Some(deleting_one(app.outcome.active(), DeletePhase::Restored));
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
            app.deletion = Some(deleting_one(
                app.outcome.active(),
                DeletePhase::Gone { marker: None },
            ));
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
        let gone = || deleting_one(ConnectionId(0), DeletePhase::Gone { marker: None });

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

    /// The verb gates like the others: an object row or nothing.
    #[gpui::test]
    fn preview_gates_on_an_object_row(cx: &mut TestAppContext) {
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
            app.preview_row(9, cx);
            assert!(
                app.preview.is_none(),
                "a row that is not there previews nothing"
            );
            app.preview_row(0, cx);
            assert!(app.preview.is_none(), "a folder previews nothing");
            app.preview_row(1, cx);
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
            // Through `re_read_location`, which is what this assertion was
            // always about: a deletion's outcome refetching its own listing
            // must not yank the screen away. It read `go_to` because the two
            // meanings shared one function until 2026-08-26 — and that
            // sharing is what made the bucket crumb do nothing.
            app.re_read_location(here, window, cx);
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
    /// A filled button is clay, everywhere, without anyone remembering.
    ///
    /// `XONHO-0032`. The `Clay` trait was supposed to make this true by
    /// construction and did not: the trait says what clay *is*, and nothing
    /// said where it *goes*, so applying it came down to matching text — which
    /// missed every filled button written as `if on { b.primary() }` rather
    /// than as one chain. The owner found two of them on screen, next to
    /// moulded ones, on the first pass.
    ///
    /// So the rule is checked instead of trusted. Reading the source from a
    /// test is unusual and it is the honest tool here: the alternative is a
    /// screenshot per button, and the failure has no behaviour to assert —
    /// it is a thing that looks slightly wrong.
    ///
    /// `ghost` is exempt, and that is the system's own rule: a ghost carries
    /// its tone's **ink**, not a moulded surface.
    #[test]
    fn every_filled_button_is_made_of_clay() {
        const TONES: [&str; 6] = [
            ".primary()",
            ".danger()",
            ".success()",
            ".warning()",
            ".info()",
            ".secondary()",
        ];

        let mut flat = Vec::new();
        for (name, source) in [
            ("app.rs", include_str!("app.rs")),
            (
                "views/credential_form.rs",
                include_str!("views/credential_form.rs"),
            ),
            ("views/failure.rs", include_str!("views/failure.rs")),
            ("views/buckets.rs", include_str!("views/buckets.rs")),
        ] {
            let lines: Vec<&str> = source.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                // A comment mentioning a tone is not a button wearing one,
                // and neither is a quoted one — the array above names all six,
                // and a lint that trips over its own definition is a lint
                // nobody keeps.
                //
                // This replaces cutting the file at its first `#[cfg(test)]`,
                // which was the second flaw in this lint and the worse one:
                // that attribute first appears at line 734 of seven thousand,
                // so the check was reading a tenth of the file and reporting
                // the rest as clean.
                if line.trim_start().starts_with("//")
                    || !TONES
                        .iter()
                        .any(|tone| line.contains(tone) && !line.contains(&format!("\"{tone}\"")))
                {
                    continue;
                }

                // The chain this call belongs to, and **only** that chain: a
                // fixed window of nearby lines is what made the first draft of
                // this lint useless. `accessible_only.primary()` sits two
                // lines above an `else { … .custom(crate::theme::quiet(cx)) }`, so a window saw a
                // ghost and exempted the filled branch — the lint passed with
                // the exact button the owner had found still flat.
                //
                // A continuation line begins with `.`; anything else ends the
                // chain.
                let mut chain = vec![*line];
                for above in lines[..n].iter().rev() {
                    let t = above.trim_start();
                    if t.starts_with("//") {
                        continue;
                    }
                    chain.push(*above);
                    if !t.starts_with('.') {
                        break;
                    }
                }
                for below in &lines[n + 1..] {
                    let t = below.trim_start();
                    if !t.starts_with('.') {
                        break;
                    }
                    chain.push(*below);
                }
                let chain = chain.join(" ");

                // A button that wears **ink** rather than a surface is flat
                // by the system's own rule, not by omission: `quiet`, the
                // toolkit's `ghost`, and `outline`.
                //
                // Spelled in pieces on purpose. A blunt `.ghost()` →
                // `.custom(…)` substitution across this crate rewrote this
                // very condition once, so the lint stopped exempting ghosts
                // and flagged one — which is at least the tool catching the
                // damage done to it.
                const INK_NOT_SURFACE: [&str; 3] = [".ghos", "t()", ".outline()"];
                let ghost = format!("{}{}", INK_NOT_SURFACE[0], INK_NOT_SURFACE[1]);
                if chain.contains(&ghost)
                    || chain.contains(INK_NOT_SURFACE[2])
                    || chain.contains("quiet(cx)")
                {
                    continue;
                }
                if !chain.contains(".clay()") {
                    flat.push(format!("{name}:{} — {}", n + 1, line.trim()));
                }
            }
        }

        assert!(
            flat.is_empty(),
            "filled buttons with no clay under them, which will sit flat beside \
             moulded ones:\n  {}",
            flat.join("\n  ")
        );
    }

    /// The theme this application ships is the one the window actually gets.
    ///
    /// `XONHO-0032`. Written because the screenshot harness had been
    /// photographing the toolkit's default styling all along, and nothing
    /// noticed — a frame is drawn either way, and "it looks fine" is not a
    /// comparison anyone was making. This asserts the value rather than the
    /// pixel, which is the part a test can actually hold.
    #[gpui::test]
    fn the_window_is_dressed_in_the_theme_this_app_ships(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(crate::theme::install);

        cx.update(|cx| {
            use gpui_component::ActiveTheme as _;
            let background = cx.theme().background;
            let sidebar = cx.theme().sidebar;
            let danger = cx.theme().danger;
            eprintln!(
                "background={:?}\n  sidebar={:?}\n  danger={:?}",
                background.to_rgb(),
                sidebar.to_rgb(),
                danger.to_rgb()
            );
            // Đất Nặn's app surface, #F2F4F0 — cool, not white.
            // Within one unit per channel, not equal: a colour goes through
            // HSLA on the way in and comes back a hair off. `#F5A81C` returns
            // as `#F5A81B`, which is the round trip and not a wrong colour.
            let near = |got: gpui::Hsla, want: u32| {
                let got = got.to_rgb();
                let (r, g, b) = (
                    (got.r * 255.).round() as i32,
                    (got.g * 255.).round() as i32,
                    (got.b * 255.).round() as i32,
                );
                let want = (
                    ((want >> 16) & 0xff) as i32,
                    ((want >> 8) & 0xff) as i32,
                    (want & 0xff) as i32,
                );
                ((r - want.0).abs() <= 1 && (g - want.1).abs() <= 1 && (b - want.2).abs() <= 1)
                    .then_some(())
                    .ok_or(format!(
                        "#{r:02x}{g:02x}{b:02x} vs #{:06x}",
                        want.0 << 16 | want.1 << 8 | want.2
                    ))
            };

            // Named one by one, and this is why: **a key the schema does not
            // know is dropped without a word.** The first draft of this theme
            // spelled the button tones `button.danger` where the schema wants
            // `button.danger.background`, and every one of them silently fell
            // back to the toolkit's own — which rendered the delete button pale
            // pink with white text on it, unreadable, and looking for all the
            // world like a deliberate choice.
            for (what, got, want) in [
                ("background", background, 0xf2f4f0),
                ("sidebar", sidebar, 0xeaeee7),
                ("danger", danger, 0xd4552f),
                ("button danger", cx.theme().button_danger, 0xd4552f),
                (
                    "button danger ink",
                    cx.theme().button_danger_foreground,
                    0xfff1ee,
                ),
                ("button primary", cx.theme().button_primary, 0xf5a81c),
                (
                    "button primary ink",
                    cx.theme().button_primary_foreground,
                    0x5c3a05,
                ),
                ("sidebar current", cx.theme().sidebar_primary, 0xf5a81c),
                ("table head", cx.theme().table_head, 0xeaeee7),
                (
                    "selected row border",
                    cx.theme().table_active_border,
                    0x3aafc9,
                ),
            ] {
                if let Err(seen) = near(got, want) {
                    panic!(
                        "`{what}` is {seen} — the toolkit's, not this app's. Most \
                         likely a theme key the schema does not recognise, which \
                         it drops in silence."
                    );
                }
            }
        });
    }

    /// Does the variable font's weight axis actually reach the renderer?
    ///
    /// `XONHO-0032` task 1.2, and the one thing about that change that could
    /// not be settled by reading source. Baloo 2 ships from `google/fonts` as
    /// a single variable file; gpui picks a weight through font-kit, and
    /// whether that selects along `wght` or just hands back the default
    /// instance is not visible in either codebase.
    ///
    /// So it is measured. The same string is laid out at 400 and at 800, and
    /// their widths compared: a heavier cut of the same face is wider. If the
    /// axis is not reached, both come back identical and the fallback is
    /// static instances — a download, not a redesign.
    /// Are the families this window draws with actually registered?
    ///
    /// Asked separately, and first, because the two weight tests cannot ask
    /// it. `TextSystem::resolve_font` falls back silently — an unresolvable
    /// family walks down `fallback_font_stack` and returns a system face
    /// (`gpui/src/text_system.rs:148`) — so a width is always > 0 and the
    /// `regular > px(0.)` guard those tests carried never fired.
    ///
    /// What it hid, measured on Windows CI: Baloo 2 and Be Vietnam Pro both
    /// reported a 'B' of **25.088px at every weight** — the same number for
    /// two unrelated typefaces, which is one fallback face answering for all
    /// of them. `add_fonts` returned `Ok`, so nothing said so.
    ///
    /// A weight test on a family that never loaded measures the fallback's
    /// weights, not the family's. This has to pass before those mean anything.

    /// Why the three font tests above do not run here.
    ///
    /// Not a skip and not a comment: the reason is asserted, so the day this
    /// platform's headless mode gains a real text system, this fails and says
    /// to ungate them. A comment becomes folklore; a test does not.
    ///
    /// Written after those tests were taken at face value on Windows CI and
    /// read as a product defect. They reported Baloo 2 and Be Vietnam Pro
    /// rendering identically at every weight — which was true, and meant
    /// nothing, because both were being answered by a constant.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn this_platforms_headless_text_system_is_a_noop() {
        use gpui::{Font, FontFeatures, FontStyle, FontWeight, HeadlessAppContext, px};

        let text_system = gpui_platform::current_platform(true).text_system();
        let mut cx = HeadlessAppContext::with_platform(
            text_system,
            Arc::new(gpui_component_assets::Assets),
            gpui_platform::current_headless_renderer,
        );

        cx.update(|cx| {
            crate::theme::load_fonts(cx);

            assert!(
                cx.text_system().all_font_names().is_empty(),
                "this platform's headless text system now names fonts, so it \
                 is no longer a noop — ungate the font tests above and delete \
                 this one"
            );

            let width = |family: &str, weight: FontWeight| {
                let id = cx.text_system().resolve_font(&Font {
                    family: family.into(),
                    features: FontFeatures::default(),
                    fallbacks: None,
                    weight,
                    style: FontStyle::Normal,
                });
                cx.text_system()
                    .typographic_bounds(id, px(64.), 'B')
                    .expect("the noop answers everything")
                    .size
                    .width
            };

            // 392/1000 em at 64px. The same number for two unrelated
            // typefaces at unrelated weights is the signature of the noop,
            // and is what a reader of a red Windows log needs to recognise.
            let constant = px(25.088);
            for (family, weight) in [
                (crate::theme::FONT_DISPLAY, FontWeight::NORMAL),
                (crate::theme::FONT_DISPLAY, FontWeight::EXTRA_BOLD),
                (crate::theme::FONT_BODY, FontWeight::NORMAL),
                (crate::theme::FONT_BODY, FontWeight::BOLD),
            ] {
                assert_eq!(
                    width(family, weight),
                    constant,
                    "{family} at {weight:?} no longer measures the noop's \
                     constant, so this platform is measuring something real"
                );
            }
        });
    }

    /// The typeface this window declares is the one it draws with.
    ///
    /// **One test, and one headless platform, on purpose.** Two of these in
    /// one process abort with SIGABRT — reproduced by splitting this into
    /// separate `#[test]` functions, where any two of them running in
    /// parallel killed the whole binary while each passed alone. The screenshot
    /// harness below shares the constraint and is `#[ignore]`d, which is why it
    /// never collided with anything. So the three questions this asks are asked
    /// together rather than made into three tests.
    ///
    /// **macOS only, and the reason is the platform not the assertion.**
    /// `gpui_windows::WindowsPlatform::new(headless)` installs
    /// `NoopTextSystem` rather than DirectWrite (`platform.rs:113`), so a
    /// headless Windows run has no text system to ask.
    /// `this_platforms_headless_text_system_is_a_noop` holds that reason and
    /// fails when it stops being true.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_typeface_this_window_declares_is_the_one_it_draws_with() {
        use gpui::{Font, FontFeatures, FontStyle, FontWeight, HeadlessAppContext, px};

        let text_system = gpui_platform::current_platform(true).text_system();
        let mut cx = HeadlessAppContext::with_platform(
            text_system,
            Arc::new(gpui_component_assets::Assets),
            gpui_platform::current_headless_renderer,
        );

        cx.update(|cx| {
            crate::theme::load_fonts(cx);

            // Registration first. `TextSystem::resolve_font` walks
            // `fallback_font_stack` for a family it cannot resolve and returns
            // a system face (`gpui/src/text_system.rs:148`), so a width is
            // always > 0 and a weight comparison on a family that never
            // loaded is measuring the fallback's weights, not this one's.
            let known = cx.text_system().all_font_names();
            for family in [crate::theme::FONT_DISPLAY, crate::theme::FONT_BODY] {
                assert!(
                    known.iter().any(|name| name == family),
                    "`{family}` is not among the families the text system \
                     knows, so every use of it resolves to a fallback face and \
                     the window is not drawn in the typeface it declares. \
                     `add_fonts` reports no error when this happens. Families \
                     whose name mentions one of ours: {:?}",
                    known
                        .iter()
                        .filter(|name| name.contains("Baloo") || name.contains("Vietnam"))
                        .collect::<Vec<_>>()
                );
            }

            // The ink box of one glyph, at a weight. A heavier cut of the same
            // face draws a fatter `B`, so if the weight is reached these
            // differ and if it is not they are the same number.
            let width_of_b = |family: &str, weight: FontWeight| {
                let id = cx.text_system().resolve_font(&Font {
                    family: family.into(),
                    features: FontFeatures::default(),
                    fallbacks: None,
                    weight,
                    style: FontStyle::Normal,
                });
                cx.text_system()
                    .typographic_bounds(id, px(64.), 'B')
                    .expect("the family has a B")
                    .size
                    .width
            };

            let display_regular = width_of_b(crate::theme::FONT_DISPLAY, FontWeight::NORMAL);
            let display_black = width_of_b(crate::theme::FONT_DISPLAY, FontWeight::EXTRA_BOLD);
            let body_regular = width_of_b(crate::theme::FONT_BODY, FontWeight::NORMAL);
            let body_semibold = width_of_b(crate::theme::FONT_BODY, FontWeight::SEMIBOLD);
            let body_bold = width_of_b(crate::theme::FONT_BODY, FontWeight::BOLD);

            eprintln!(
                "'B' at 64px — Baloo 2: regular={display_regular:?} \
                 extra-bold={display_black:?}; Be Vietnam Pro: \
                 regular={body_regular:?} semibold={body_semibold:?} \
                 bold={body_bold:?}"
            );

            assert_ne!(
                display_regular, display_black,
                "Baloo 2 renders identically at 400 and 800, so the weight is \
                 not being reached (XONHO-0032 task 1.2)"
            );
            assert_ne!(
                body_regular, body_bold,
                "Be Vietnam Pro renders identically at 400 and 700, and those \
                 two share one legacy family — so this is not a naming limit \
                 and something more basic is wrong with loading it"
            );
            // SEMIBOLD is what the sidebar, the components and the credential
            // form are set in, so this is the wide one: a family that cannot
            // reach 600 is a window whose type hierarchy has flattened.
            assert_ne!(
                body_regular, body_semibold,
                "Be Vietnam Pro renders identically at 400 and 600. One \
                 hypothesis, never yet observed to cause anything: \
                 `BeVietnamPro-SemiBold.ttf` declares the legacy family \
                 `Be Vietnam Pro SemiBold`, and only its typographic name says \
                 `Be Vietnam Pro`, so a platform matching legacy names would \
                 not find 600. Check that before redesigning anything — the \
                 last time this failed, the cause was the harness."
            );
        });
    }

    #[cfg(target_os = "macos")]
    fn shoot(
        name: &str,
        store: Arc<dyn caixonho_core::ObjectStore>,
        drive: impl FnOnce(&mut CaixonhoApp, &mut gpui::Window, &mut gpui::Context<CaixonhoApp>),
    ) -> std::path::PathBuf {
        shoot_at(name, 1280, store, drive)
    }

    /// [`shoot`], at a window width the caller chooses.
    ///
    /// Width is a parameter because the two overflow defects this project has
    /// shipped were **both** invisible at 1280 and obvious at 900: a row of
    /// verbs beside a sixty-character directory-bucket name has plenty of
    /// space until it does not, and a harness that only ever renders wide
    /// cannot photograph the failure it is meant to catch.
    #[cfg(target_os = "macos")]
    fn shoot_at(
        name: &str,
        width: u32,
        store: Arc<dyn caixonho_core::ObjectStore>,
        drive: impl FnOnce(&mut CaixonhoApp, &mut gpui::Window, &mut gpui::Context<CaixonhoApp>),
    ) -> std::path::PathBuf {
        use gpui::{HeadlessAppContext, px, size};

        let height = 800u32;

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
        // **The application's own theme and fonts, which this harness went
        // without until `XONHO-0032`.** It called `gpui_component::init` and
        // stopped, so every frame it ever wrote photographed the *toolkit's*
        // default styling — the brand ramp in `theme.json` had never once
        // appeared in a judgement image. A harness for judging how the window
        // looks, blind to how the window is dressed.
        cx.update(crate::theme::load_fonts);
        cx.update(crate::theme::install);

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
            .open_window(size(px(width as f32), px(height as f32)), |window, cx| {
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

        // The chosen subset, and the chooser. The first has to read as
        // "someone chose this" and not as "your account is small"
        // (`XONHO-0027`), which is why it is shot next to 04b.
        written.push(shoot(
            "account-04c-chosen-subset",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                settled(app, cx);
                app.narrowing.chosen = Some(vec!["reports".to_owned()]);
                app.narrow_rows(cx);
            },
        ));

        written.push(shoot(
            "account-04d-chosen-subset-showing-all",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                settled(app, cx);
                app.narrowing.chosen = Some(vec!["reports".to_owned()]);
                app.show_all_buckets(cx);
            },
        ));

        written.push(shoot(
            "account-04e-choosing-buckets",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                settled(app, cx);
                app.start_choosing_buckets(cx);
                // Through None, which is how someone picking two out of ten
                // will actually use it (`XONHO-0027` 2.6).
                app.tick_every_bucket(false, cx);
                app.toggle_chosen("reports", cx);
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
                queued(
                    app,
                    Transfer {
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
                    },
                );
            },
        ));

        written.push(shoot(
            "bucket-06-name-taken",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                queued(
                    app,
                    Transfer {
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
                    },
                );
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
                queued(
                    app,
                    Transfer {
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
                    },
                );
            },
        ));

        written.push(shoot(
            "bucket-08-condition-unsupported",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                queued(
                    app,
                    Transfer {
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
                    },
                );
            },
        ));

        // The queue (`XONHO-0028`). Three frames, and the third is the one
        // to judge: a question asked of one transfer while others carry on is
        // the state the whole design turns on.
        let queued_run = |app: &mut CaixonhoApp, cx: &mut gpui::Context<CaixonhoApp>| {
            inside(app, cx);
            app.listing = Listing::Loaded;
            for (key, phase) in [
                ("daily/summary.csv", TransferPhase::Running),
                (
                    "daily/ledger.csv",
                    TransferPhase::Sent {
                        key: "daily/ledger.csv".to_owned(),
                        stepped_aside: false,
                    },
                ),
                (
                    "daily/broken.csv",
                    TransferPhase::Failed(Error::Network {
                        detail: "the name did not resolve".into(),
                    }),
                ),
            ] {
                let id = app.queue.accept(Transfer {
                    bucket: "reports".to_owned(),
                    key: key.to_owned(),
                    directory: std::path::PathBuf::from("/tmp"),
                    then_open: false,
                    direction: Direction::Up,
                    source: Some(std::path::PathBuf::from("/tmp").join(key)),
                    bytes: 4096,
                    total: Some(16384),
                    cancel: caixonho_core::transfer::Cancel::default(),
                    phase,
                });
                if let Some(standing) = app.queue.payload_mut(id).and_then(|t| t.phase.standing()) {
                    app.queue.settled(id, standing);
                }
            }
        };

        written.push(shoot(
            "bucket-17-queue-running",
            Arc::new(StoreDouble::allows_listing()),
            move |app, _, cx| queued_run(app, cx),
        ));

        written.push(shoot(
            "bucket-18-queue-asking-while-others-run",
            Arc::new(StoreDouble::allows_listing()),
            move |app, _, cx| {
                queued_run(app, cx);
                // One transfer stopped on a question, holding no slot, while
                // the rest carry on — the state `XONHO-0028` exists for.
                let id = app.queue.accept(Transfer {
                    bucket: "reports".to_owned(),
                    key: "daily/taken.csv".to_owned(),
                    directory: std::path::PathBuf::from("/tmp"),
                    then_open: false,
                    direction: Direction::Up,
                    source: Some(std::path::PathBuf::from("/tmp/taken.csv")),
                    bytes: 0,
                    total: Some(2048),
                    cancel: caixonho_core::transfer::Cancel::default(),
                    phase: TransferPhase::KeyTaken {
                        key: "daily/taken.csv".to_owned(),
                    },
                });
                app.queue.asking(id);
            },
        ));

        // Choosing where an upload lands (`XONHO-0026`). Driven through the
        // control, and the refused frame is the one to judge: the reason has
        // to sit beside the field it is about.
        written.push(shoot(
            "bucket-15-upload-destination",
            Arc::new(StoreDouble::allows_listing()),
            |app, window, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.offer_destination(
                    "reports".to_owned(),
                    vec![std::path::PathBuf::from("/tmp/summary.csv")],
                    "daily/summary.csv".to_owned(),
                    window,
                    cx,
                );
            },
        ));

        // Several files sharing one folder, and a refused drop. The first is
        // the frame to judge: "Upload 6 files into folder:" has to be
        // distinguishable at a glance from "Upload to:", because the field
        // means different things and only the words say which (`XONHO-0029`).
        written.push(shoot(
            "bucket-19-upload-many-into-a-folder",
            Arc::new(StoreDouble::allows_listing()),
            |app, window, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.offer_upload_of(
                    (1..=6)
                        .map(|n| std::path::PathBuf::from(format!("/tmp/report-{n}.csv")))
                        .collect(),
                    window,
                    cx,
                );
            },
        ));

        written.push(shoot(
            "bucket-20-drop-refused",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.dropped_refusal = Some(
                    "Folders cannot be uploaded yet — drop the files inside them instead.".into(),
                );
            },
        ));

        written.push(shoot(
            "bucket-16-upload-destination-refused",
            Arc::new(StoreDouble::allows_listing()),
            |app, window, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.offer_destination(
                    "reports".to_owned(),
                    vec![std::path::PathBuf::from("/tmp/summary.csv")],
                    "daily/summary.csv".to_owned(),
                    window,
                    cx,
                );
                app.destination.update(cx, |state, cx| {
                    state.set_value("uploads/", window, cx);
                });
                app.confirm_destination(cx);
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
                app.deletion = Some(deleting_one(
                    app.outcome.active(),
                    DeletePhase::Confirming {
                        keys: vec!["daily/summary.csv".into()],
                    },
                ));
            },
        ));

        written.push(shoot(
            "bucket-10-deleted-with-undo",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                inside(app, cx);
                app.listing = Listing::Loaded;
                app.deletion = Some(deleting_one(
                    app.outcome.active(),
                    DeletePhase::Gone {
                        marker: Some("mk-screenshot".into()),
                    },
                ));
            },
        ));

        // Adding a connection. **No frame existed for this until
        // `XONHO-0032`**, which is why nobody had seen that its description
        // ran out past the card's edge — a state with no image is a state
        // nobody looks at, and this one is the first screen a new user meets.
        written.push(shoot(
            "account-10-add-a-connection",
            Arc::new(StoreDouble::allows_listing()),
            |app, window, cx| {
                settled(app, cx);
                app.form = Some(crate::views::credential_form::CredentialForm::new(
                    window, cx,
                ));
            },
        ));

        // ---- Acting on rows (`XONHO-0030`) ----
        //
        // Staged with real rows rather than the empty state the two frames
        // above use: the whole subject here is what a *row* looks like when
        // it is ticked, and an empty folder cannot show that.

        let with_rows = |app: &mut CaixonhoApp, cx: &mut gpui::Context<CaixonhoApp>| {
            inside(app, cx);
            app.listing = Listing::Loaded;
            app.objects.update(cx, |state, _| {
                state.delegate_mut().show(
                    CorePrefix::root(),
                    vec![caixonho_core::Folder {
                        prefix: CorePrefix::parse("archive/"),
                    }],
                    // Root-level keys, because the location staged is the
                    // root: a key with a separator in it would have been
                    // *grouped* into a folder by the service and could never
                    // be a row here. The harness photographs states that
                    // exist.
                    vec![
                        an_object("summary.csv", 4_096),
                        an_object("build.log", 91_204),
                        an_object("notes.md", 812),
                    ],
                );
            });
        };

        written.push(shoot(
            "bucket-09b-rows-ticked",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                with_rows(app, cx);
                app.objects.update(cx, |state, _| {
                    let delegate = state.delegate_mut();
                    delegate.toggle(1);
                    delegate.toggle(3);
                });
            },
        ));

        // Counting: the number is not known yet, and the button that would
        // agree to it is deliberately not on screen.
        written.push(shoot(
            "bucket-09c-folder-counting",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                with_rows(app, cx);
                app.deletion = Some(Deletion {
                    connection: app.outcome.active(),
                    bucket: "reports".into(),
                    asked: Asked::Folder("archive".into()),
                    phase: DeletePhase::Counting {
                        cancels: Vec::new(),
                        left: 1,
                        gathered: Vec::new(),
                    },
                });
            },
        ));

        // Counted: the same strip, now with a number on it and a button.
        written.push(shoot(
            "bucket-09d-folder-counted",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                with_rows(app, cx);
                app.deletion = Some(Deletion {
                    connection: app.outcome.active(),
                    bucket: "reports".into(),
                    asked: Asked::Folder("archive".into()),
                    phase: DeletePhase::Confirming {
                        keys: (0..37).map(|n| format!("archive/{n}.csv")).collect(),
                    },
                });
            },
        ));

        // The aftermath of a bulk delete, with one refusal in it — the state
        // where "some failed" would be useless and each cause has to survive,
        // and where the absence of Undo has to be *said*.
        written.push(shoot(
            "bucket-10b-bulk-outcome-with-a-refusal",
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                with_rows(app, cx);
                app.deletion = Some(Deletion {
                    connection: app.outcome.active(),
                    bucket: "reports".into(),
                    asked: Asked::Rows(3),
                    phase: DeletePhase::Went {
                        gone: 2,
                        failures: vec![
                            "build.log: access denied — this needs `s3:DeleteObject`".to_owned(),
                        ],
                    },
                });
            },
        ));

        // The width the two overflow defects actually happened at, with the
        // longest bucket name this application has to render — a directory
        // bucket in a Local Zone — and the ticks that add a verb to the row.
        // Both previous overflows were invisible at 1280 and obvious here.
        written.push(shoot_at(
            "bucket-09e-narrow-window-with-a-long-bucket-name",
            900,
            Arc::new(StoreDouble::allows_listing()),
            |app, _, cx| {
                settled(app, cx);
                app.position = Some(Position {
                    connection: app.outcome.active(),
                    at: Location::at(
                        "vunm-production-archive--apse1-han1-az1--x-s3".to_owned(),
                        CorePrefix::parse("2026/august/daily-exports/"),
                    ),
                });
                app.listing = Listing::Loaded;
                app.objects.update(cx, |state, _| {
                    let delegate = state.delegate_mut();
                    delegate.show(
                        CorePrefix::parse("2026/august/daily-exports/"),
                        Vec::new(),
                        vec![
                            an_object("2026/august/daily-exports/summary.csv", 4_096),
                            an_object("2026/august/daily-exports/build.log", 91_204),
                        ],
                    );
                    delegate.toggle(0);
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

    /// The test `XONHO-0025` planned and did not write, which is why the
    /// defect it describes reached the owner's screen.
    ///
    /// Turning the narrowing on before anything has been answered must not
    /// freeze it there: the answers arrive afterwards, and the list has to
    /// follow them.
    #[gpui::test]
    fn a_denial_that_settles_while_the_narrowing_is_on_removes_its_row(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("can-open", BucketKind::General),
                bucket_of("refused", BucketKind::General),
            ],
        );

        // On before anything is known — the owner's exact sequence.
        app.update(cx, |app, cx| {
            app.narrowing.accessible_only = true;
            app.narrow_rows(cx);
        });
        assert_eq!(
            shown_names(&app, cx),
            vec!["can-open".to_owned(), "refused".to_owned()],
            "nothing is answered yet, so nothing may be hidden"
        );

        // The answers arrive.
        app.update(cx, |app, _| {
            observe(app, "can-open", Observation::Allowed);
            observe(app, "refused", Observation::Denied);
        });
        app.update(cx, |app, cx| app.probe_settled(cx));

        assert_eq!(
            shown_names(&app, cx),
            vec!["can-open".to_owned()],
            "the denial settled and the row stayed, so the filter is frozen at the moment \
             nothing had been answered"
        );
    }

    /// And the other half: with the narrowing off, a settling probe must not
    /// quietly re-run every narrowing behind the user's back.
    #[gpui::test]
    fn a_settling_probe_changes_nothing_when_the_narrowing_is_off(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("can-open", BucketKind::General),
                bucket_of("refused", BucketKind::General),
            ],
        );
        app.update(cx, |app, _| observe(app, "refused", Observation::Denied));

        app.update(cx, |app, cx| app.probe_settled(cx));

        assert_eq!(
            shown_names(&app, cx),
            vec!["can-open".to_owned(), "refused".to_owned()],
            "a refused bucket is still a bucket, and nobody asked for it to be hidden"
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

    // ---- Leaving a preview (XONHO-0008, found live 2026-08-26) ----

    /// The owner's report: previewing an object at a bucket's root, clicking
    /// the bucket in the breadcrumb did nothing at all.
    ///
    /// `go_to` cleared the preview only when the location *changed*, and the
    /// bucket crumb goes to the location you are already at — so the preview
    /// survived, `render` kept choosing `preview_surface`, and the click had
    /// no visible effect. The carve-out was written for the deletion strip
    /// (`XONHO-0021`), which does need to survive a re-read of the same
    /// location; the preview was swept in with it and never needed it.
    #[gpui::test]
    fn walking_to_the_location_you_are_already_at_still_leaves_the_preview(
        cx: &mut TestAppContext,
    ) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, _| {
            app.preview = Some(Preview {
                connection: app.outcome.active(),
                key: "images.jpeg".to_owned(),
                phase: PreviewPhase::Loading,
            });
        });

        // Exactly what the bucket crumb does.
        app.update_in(cx, |app, window, cx| {
            app.go_to(Location::bucket("reports".to_owned()), window, cx);
        });

        app.read_with(cx, |app, _| {
            assert!(
                app.preview.is_none(),
                "clicking the bucket you are already inside left the preview up, so the \
                 click did nothing a user could see"
            );
        });
    }

    /// And the guarantee it must not break: a re-read of the same location is
    /// how the deletion strip refreshes its own listing, so that strip stays.
    #[gpui::test]
    fn a_re_read_of_the_same_location_keeps_the_deletion_strip(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, _| {
            app.deletion = Some(deleting_one(
                app.outcome.active(),
                DeletePhase::Gone { marker: None },
            ));
        });

        app.update_in(cx, |app, window, cx| {
            app.go_to(Location::bucket("reports".to_owned()), window, cx);
        });

        app.read_with(cx, |app, _| {
            assert!(
                app.deletion.is_some(),
                "the deletion strip is refreshed by re-reading its own location, and losing \
                 it there would take the Undo with it"
            );
        });
    }

    // ---- A destination you choose (XONHO-0026) ----

    /// A window inside `bucket`, over a store a test can read back.
    fn uploading_from<'a>(
        cx: &'a mut TestAppContext,
        bucket: &str,
    ) -> (gpui::Entity<CaixonhoApp>, &'a mut gpui::VisualTestContext) {
        cx.update(gpui_component::init);
        let store: Arc<dyn caixonho_core::ObjectStore> = Arc::new(StoreDouble::allows_listing());
        let looking = Location::at(bucket.to_owned(), CorePrefix::parse("daily/"));
        let (app, cx) = cx.add_window_view(|window, cx| {
            CaixonhoApp::new(
                Diagnostics::without_a_log(),
                World::scripted(store),
                window,
                cx,
            )
        });
        app.update(cx, |app, cx| {
            app.position = Some(Position {
                connection: app.outcome.active(),
                at: looking,
            });
            cx.notify();
        });
        (app, cx)
    }

    /// A real local file, so the upload has something to read.
    fn a_local_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("caixonho-destination");
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let path = dir.join(name);
        std::fs::write(&path, b"contents").expect("a temp file");
        path
    }

    #[gpui::test]
    fn the_destination_offered_is_where_the_user_is_standing(cx: &mut TestAppContext) {
        let (app, cx) = uploading_from(cx, "reports");
        let file = a_local_file("summary.csv");

        app.update_in(cx, |app, window, cx| {
            app.offer_destination(
                "reports".to_owned(),
                vec![file],
                "daily/summary.csv".to_owned(),
                window,
                cx,
            );
        });

        app.read_with(cx, |app, cx| {
            assert!(app.choosing_destination.is_some());
            assert_eq!(
                app.destination.read(cx).value().to_string(),
                "daily/summary.csv",
                "the default should be exactly what was sent without asking before this change"
            );
        });
    }

    /// The claim of this change: the key handed to the session is the one the
    /// field showed, with no part of it recomposed.
    ///
    /// Asserted at the window's own seam and **not** on the store, and that is
    /// a limit worth naming rather than hiding. The session runs uploads on
    /// its own tokio runtime, which a window test never drives — so a
    /// store-side assertion here would be empty whatever happened, and an
    /// assertion that cannot fail is worse than none because it reads as
    /// proof. `start_upload` hands the same string to `spawn_upload` and to
    /// the transfer it records, synchronously; core's own tests carry it from
    /// there to the service.
    #[gpui::test]
    fn what_is_shown_is_what_is_sent(cx: &mut TestAppContext) {
        let (app, cx) = uploading_from(cx, "reports");
        let file = a_local_file("summary.csv");

        app.update_in(cx, |app, window, cx| {
            app.offer_destination(
                "reports".to_owned(),
                vec![file],
                "daily/summary.csv".to_owned(),
                window,
                cx,
            );
            // Shares no part with the default — different prefix *and*
            // different file name. A test that only changed the prefix would
            // pass a version still re-deriving the name from the local file.
            app.destination.update(cx, |state, cx| {
                state.set_value("uploads/2026/renamed.txt", window, cx);
            });
            app.confirm_destination(cx);
        });

        app.read_with(cx, |app, _| {
            let transfer = only(app);
            assert_eq!(
                (transfer.bucket.as_str(), transfer.key.as_str()),
                ("reports", "uploads/2026/renamed.txt"),
                "the key sent was not the key on screen"
            );
        });
    }

    #[gpui::test]
    fn a_destination_that_cannot_be_a_key_costs_a_sentence_not_a_request(cx: &mut TestAppContext) {
        // `transfer.is_none()` is the assertion that bites, and it is not a
        // phase check: `start_upload` records the transfer in the same breath
        // as it spawns the request, so no transfer means nothing was asked
        // for. A store-side count would be vacuous here — see
        // `what_is_shown_is_what_is_sent`.
        let (app, cx) = uploading_from(cx, "reports");
        let file = a_local_file("summary.csv");

        for bad in ["", "   ", "uploads/", "/uploads/summary.csv"] {
            app.update_in(cx, |app, window, cx| {
                app.offer_destination(
                    "reports".to_owned(),
                    vec![file.clone()],
                    "daily/summary.csv".to_owned(),
                    window,
                    cx,
                );
                app.destination.update(cx, |state, cx| {
                    state.set_value(bad, window, cx);
                });
                app.confirm_destination(cx);
            });
            cx.run_until_parked();

            app.read_with(cx, |app, _| {
                assert!(
                    app.choosing_destination
                        .as_ref()
                        .is_some_and(|choosing| choosing.refused.is_some()),
                    "`{bad}` should have been refused with a reason"
                );
                assert!(app.queue.is_empty(), "`{bad}` started an upload");
            });
        }
    }

    #[gpui::test]
    fn leaving_the_location_drops_a_destination_nobody_confirmed(cx: &mut TestAppContext) {
        let (app, cx) = uploading_from(cx, "reports");
        let file = a_local_file("summary.csv");

        app.update_in(cx, |app, window, cx| {
            app.offer_destination(
                "reports".to_owned(),
                vec![file],
                "daily/summary.csv".to_owned(),
                window,
                cx,
            );
        });
        app.update(cx, |app, cx| app.leave_bucket(cx));

        app.read_with(cx, |app, _| {
            assert!(
                app.choosing_destination.is_none(),
                "a destination is a key at a location, and the location is gone"
            );
        });
    }

    // ---- A bucket list you choose once (XONHO-0027) ----

    #[gpui::test]
    fn a_chosen_subset_is_what_gets_listed(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("reports", BucketKind::General),
                bucket_of("logs", BucketKind::General),
                bucket_of("archive", BucketKind::General),
            ],
        );

        app.update(cx, |app, cx| {
            app.narrowing.chosen = Some(vec!["reports".into(), "archive".into()]);
            app.narrow_rows(cx);
        });

        assert_eq!(
            shown_names(&app, cx),
            vec!["reports".to_owned(), "archive".to_owned()]
        );
    }

    #[gpui::test]
    fn a_connection_nobody_chose_for_shows_everything(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("reports", BucketKind::General),
                bucket_of("logs", BucketKind::General),
            ],
        );

        app.read_with(cx, |app, _| assert!(app.narrowing.chosen.is_none()));
        assert_eq!(
            shown_names(&app, cx),
            vec!["reports".to_owned(), "logs".to_owned()]
        );
    }

    /// "I chose nothing" and "I have not chosen" are different statements, and
    /// collapsing them would make an empty choice silently show every bucket —
    /// the one outcome a person reads as the feature being broken.
    #[gpui::test]
    fn choosing_nothing_is_not_the_same_as_not_choosing(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(cx, vec![bucket_of("reports", BucketKind::General)]);

        app.update(cx, |app, cx| {
            app.narrowing.chosen = Some(Vec::new());
            app.narrow_rows(cx);
        });

        assert!(shown_names(&app, cx).is_empty());
    }

    /// A wish about names. The account is the authority on which of them
    /// exist, and a bucket absent for one session must not be forgotten.
    #[gpui::test]
    fn a_chosen_bucket_the_account_no_longer_has_is_passed_over(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(cx, vec![bucket_of("reports", BucketKind::General)]);

        app.update(cx, |app, cx| {
            app.narrowing.chosen = Some(vec!["reports".into(), "deleted-elsewhere".into()]);
            app.narrow_rows(cx);
        });

        assert_eq!(shown_names(&app, cx), vec!["reports".to_owned()]);
        app.read_with(cx, |app, _| {
            assert_eq!(
                app.narrowing.chosen.as_deref(),
                Some(["reports".to_owned(), "deleted-elsewhere".to_owned()].as_slice()),
                "the choice was pruned to what the account happens to hold today"
            );
        });
    }

    /// Showing all is not forgetting. Someone checking whether a bucket still
    /// exists has not decided to stop using their choice.
    #[gpui::test]
    fn showing_every_bucket_does_not_discard_the_choice(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("reports", BucketKind::General),
                bucket_of("logs", BucketKind::General),
            ],
        );

        app.update(cx, |app, cx| {
            app.narrowing.chosen = Some(vec!["reports".into()]);
            app.narrow_rows(cx);
            app.show_all_buckets(cx);
        });

        assert_eq!(
            shown_names(&app, cx),
            vec!["reports".to_owned(), "logs".to_owned()],
            "showing all should list everything"
        );
        app.read_with(cx, |app, _| {
            assert_eq!(
                app.narrowing.chosen.as_deref(),
                Some(["reports".to_owned()].as_slice()),
                "the choice was given up rather than set aside, so there is nothing to return to"
            );
        });

        // And the way back exists, which is the half the first version of this
        // test did not check and the implementation therefore did not have.
        app.update(cx, |app, cx| app.back_to_chosen_buckets(cx));
        assert_eq!(shown_names(&app, cx), vec!["reports".to_owned()]);
    }

    /// Opening the chooser with no choice recorded must tick everything —
    /// otherwise the picker opens looking like a fresh empty choice.
    #[gpui::test]
    fn the_chooser_opens_ticked_to_what_is_showing(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("reports", BucketKind::General),
                bucket_of("logs", BucketKind::General),
            ],
        );

        app.update(cx, |app, cx| app.start_choosing_buckets(cx));

        app.read_with(cx, |app, _| {
            assert_eq!(
                app.choosing_buckets.as_deref(),
                Some(["reports".to_owned(), "logs".to_owned()].as_slice())
            );
        });
    }

    /// Ten buckets and you want two: the picker must not cost eight clicks of
    /// unticking before the first useful one.
    #[gpui::test]
    fn the_chooser_can_tick_none_and_all_at_once(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("reports", BucketKind::General),
                bucket_of("logs", BucketKind::General),
                bucket_of("archive", BucketKind::General),
            ],
        );

        app.update(cx, |app, cx| {
            app.start_choosing_buckets(cx);
            app.tick_every_bucket(false, cx);
        });
        app.read_with(cx, |app, _| {
            assert_eq!(app.choosing_buckets.as_deref(), Some([].as_slice()));
            assert!(
                app.narrowing.chosen.is_none(),
                "ticking in the picker must not touch the live choice"
            );
        });

        app.update(cx, |app, cx| {
            app.tick_every_bucket(true, cx);
            app.toggle_chosen("logs", cx);
        });
        app.read_with(cx, |app, _| {
            assert_eq!(
                app.choosing_buckets.as_deref(),
                Some(["reports".to_owned(), "archive".to_owned()].as_slice()),
                "All then one untick should leave everything but that one"
            );
        });
    }

    /// Abandoning the chooser changes nothing. A picker that edited the live
    /// choice as you ticked would apply half a decision.
    #[gpui::test]
    fn cancelling_the_chooser_leaves_the_choice_alone(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(
            cx,
            vec![
                bucket_of("reports", BucketKind::General),
                bucket_of("logs", BucketKind::General),
            ],
        );

        app.update(cx, |app, cx| {
            app.narrowing.chosen = Some(vec!["reports".into()]);
            app.narrow_rows(cx);
            app.start_choosing_buckets(cx);
            app.toggle_chosen("logs", cx);
            app.choosing_buckets = None;
        });

        app.read_with(cx, |app, _| {
            assert_eq!(
                app.narrowing.chosen.as_deref(),
                Some(["reports".to_owned()].as_slice()),
                "ticking in an abandoned chooser changed the live choice"
            );
        });
    }

    /// The three short lists must read as three different things. This is the
    /// requirement `XONHO-0027`'s design calls the one most likely to do harm:
    /// a choice made weeks ago is otherwise indistinguishable from a bug.
    #[gpui::test]
    fn an_empty_account_a_narrowed_one_and_a_chosen_one_are_three_states(cx: &mut TestAppContext) {
        // 1. An account that holds nothing.
        let (empty, cx) = listing_of(cx, Vec::new());
        empty.read_with(cx, |app, cx| {
            let delegate = app.table.read(cx).delegate();
            assert_eq!(delegate.shown_of_loaded(), (0, 0));
            assert!(
                !delegate.hidden_by_narrowing(),
                "an account holding nothing is not an account narrowed to nothing"
            );
            assert!(app.narrowing.chosen.is_none());
        });

        // 2. An account narrowed to nothing.
        let (narrowed, cx) = listing_of(cx, vec![bucket_of("reports", BucketKind::General)]);
        narrowed.update(cx, |app, cx| {
            app.narrowing.name = "matches-nothing".to_owned();
            app.narrow_rows(cx);
        });
        narrowed.read_with(cx, |app, cx| {
            assert!(app.table.read(cx).delegate().hidden_by_narrowing());
            assert!(
                app.narrowing.chosen.is_none(),
                "a narrowed account must not claim a remembered choice is in force"
            );
        });

        // 3. An account reduced by a remembered choice — short, but not empty,
        //    and it says so.
        let (chosen, cx) = listing_of(
            cx,
            vec![
                bucket_of("reports", BucketKind::General),
                bucket_of("logs", BucketKind::General),
            ],
        );
        chosen.update(cx, |app, cx| {
            app.narrowing.chosen = Some(vec!["reports".into()]);
            app.narrow_rows(cx);
        });
        chosen.read_with(cx, |app, cx| {
            assert!(
                !app.table.read(cx).delegate().hidden_by_narrowing(),
                "a chosen subset is not 'everything is hidden'"
            );
            assert!(
                app.narrowing.chosen.is_some(),
                "the screen has nothing to say the list is a chosen subset with"
            );
        });
    }

    /// An upload nobody can see is an upload nobody believes in.
    ///
    /// `XONHO-0024` wrote that sentence for folders and `XONHO-0020`'s upload
    /// path predated it, so a sent object stayed invisible until the user
    /// navigated away and back. `XONHO-0026` made it plain: send a file to a
    /// path that does not exist yet and the folders it implies were nowhere.
    #[gpui::test]
    fn a_finished_upload_re_reads_the_location(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, _| {
            queued(app, an_upload(TransferPhase::Running));
            app.listing = Listing::Loaded;
        });

        let refreshed = app.update_in(cx, |app, window, cx| {
            let sent = app.apply_transfer(
                only_id(app),
                TransferEvent::UploadSettled(caixonho_core::transfer::UploadOutcome::Finished {
                    key: "test-folder/images.jpeg".into(),
                    stepped_aside: false,
                    bytes: 16,
                }),
                cx,
            );
            // The caller re-reads on this answer; asserting the answer is
            // asserting the wiring, and the `if` that acts on it is one line
            // beside the loop that produces it.
            if sent && let Some(location) = app.location().cloned() {
                app.re_read_location(location, window, cx);
            }
            sent
        });

        assert!(
            refreshed,
            "a finished upload has to ask for the listing again"
        );
        app.read_with(cx, |app, _| {
            assert!(
                matches!(app.listing, Listing::Loading),
                "the location was not re-read, so the object stays invisible until the user \
                 navigates away and comes back"
            );
        });
    }

    /// And the other direction: a *download* finishing changes nothing about
    /// the bucket, so it must not cost a listing.
    #[gpui::test]
    fn a_finished_download_does_not_re_read_the_location(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");
        app.update(cx, |app, _| {
            queued(
                app,
                Transfer {
                    direction: Direction::Down,
                    ..an_upload(TransferPhase::Running)
                },
            );
        });

        let refreshed = app.update(cx, |app, cx| {
            let id = only_id(app);
            app.apply_transfer(
                id,
                TransferEvent::Settled(caixonho_core::transfer::DownloadOutcome::Finished {
                    name: "summary.csv".into(),
                    mapped: caixonho_core::transfer::MappingOutcome::Unchanged,
                    bytes: 1024,
                }),
                cx,
            )
        });

        assert!(!refreshed, "a download wrote nothing to the bucket");
    }

    // ---- Files dropped onto the window (XONHO-0029) ----

    fn a_local(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("caixonho-dropped");
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let path = dir.join(name);
        std::fs::write(&path, b"x").expect("a temp file");
        path
    }

    #[gpui::test]
    fn several_files_share_a_folder_and_keep_their_names(cx: &mut TestAppContext) {
        let (app, cx) = uploading_from(cx, "reports");
        let files = vec![a_local("one.csv"), a_local("two.csv"), a_local("three.csv")];

        app.update_in(cx, |app, window, cx| {
            app.offer_upload_of(files, window, cx);
            app.destination.update(cx, |state, cx| {
                state.set_value("uploads/2026", window, cx);
            });
            app.confirm_destination(cx);
        });

        app.read_with(cx, |app, _| {
            let keys: Vec<&str> = app
                .queue
                .items()
                .iter()
                .map(|item| item.payload.key.as_str())
                .collect();
            assert_eq!(
                keys,
                vec![
                    "uploads/2026/one.csv",
                    "uploads/2026/two.csv",
                    "uploads/2026/three.csv"
                ],
                "each file must keep its own name under the shared folder"
            );
        });
    }

    /// One file is unchanged from `XONHO-0026`: the whole key, editable down
    /// to the name.
    #[gpui::test]
    fn one_file_still_gets_a_whole_editable_key(cx: &mut TestAppContext) {
        let (app, cx) = uploading_from(cx, "reports");

        app.update_in(cx, |app, window, cx| {
            app.offer_upload_of(vec![a_local("one.csv")], window, cx);
            app.destination.update(cx, |state, cx| {
                state.set_value("somewhere/renamed.txt", window, cx);
            });
            app.confirm_destination(cx);
        });

        app.read_with(cx, |app, _| {
            assert_eq!(
                app.queue.items()[0].payload.key,
                "somewhere/renamed.txt",
                "one file must still be renameable, which is what XONHO-0026 bought"
            );
        });
    }

    /// A refusal must cost nothing *partly* sent: with several files, one bad
    /// folder cannot leave some of them uploaded.
    #[gpui::test]
    fn a_refused_folder_sends_none_of_them(cx: &mut TestAppContext) {
        let (app, cx) = uploading_from(cx, "reports");
        let files = vec![a_local("one.csv"), a_local("two.csv")];

        app.update_in(cx, |app, window, cx| {
            app.offer_upload_of(files, window, cx);
            app.destination.update(cx, |state, cx| {
                state.set_value("/rooted", window, cx);
            });
            app.confirm_destination(cx);
        });

        app.read_with(cx, |app, _| {
            assert!(app.queue.is_empty(), "a refused folder sent something");
            assert!(
                app.choosing_destination
                    .as_ref()
                    .is_some_and(|c| c.refused.is_some()),
                "and it must say why"
            );
        });
    }

    #[gpui::test]
    fn a_drop_with_nowhere_to_go_says_so(cx: &mut TestAppContext) {
        let (app, cx) = listing_of(cx, vec![bucket_of("reports", BucketKind::General)]);

        app.update_in(cx, |app, window, cx| {
            app.take_dropped(&[a_local("one.csv")], window, cx);
        });

        app.read_with(cx, |app, _| {
            assert!(
                app.queue.is_empty(),
                "nothing may be uploaded with no location"
            );
            assert!(
                app.dropped_refusal.is_some(),
                "a drop that vanishes is indistinguishable from a broken application"
            );
        });
    }

    /// Partly honouring a dropped folder is the tempting option and the worst.
    #[gpui::test]
    fn a_dropped_folder_is_refused_rather_than_partly_honoured(cx: &mut TestAppContext) {
        let (app, cx) = uploading_from(cx, "reports");
        let folder = std::env::temp_dir().join("caixonho-dropped");
        std::fs::create_dir_all(&folder).expect("a temp dir");

        app.update_in(cx, |app, window, cx| {
            app.take_dropped(&[folder], window, cx);
        });

        app.read_with(cx, |app, _| {
            assert!(app.queue.is_empty());
            assert!(app.choosing_destination.is_none(), "nor may it ask where");
            assert!(app.dropped_refusal.is_some());
        });
    }

    /// The three predicates that decide a drop must agree, or the window
    /// promises a landing it then refuses.
    #[gpui::test]
    fn what_can_drop_admits_is_what_the_handler_takes(_cx: &mut TestAppContext) {
        let folder = std::env::temp_dir().join("caixonho-dropped");
        std::fs::create_dir_all(&folder).expect("a temp dir");

        assert!(droppable_paths(&[a_local("one.csv")]).is_ok());
        assert!(
            droppable_paths(&[]).is_err(),
            "nothing dropped is not a drop"
        );
        assert!(droppable_paths(&[folder]).is_err());
    }

    /// The bug the owner found by pressing Open: the file downloaded, the
    /// strip said it had been handed to the system, and nothing opened.
    ///
    /// `then_open` was stored, passed around, and read only to choose that
    /// sentence — a justification for behaviour that did not exist.
    ///
    /// This covers everything up to the handing over. The handing itself is
    /// `cx.open_with_system`, and gpui's test platform answers it with
    /// `not implemented`, so no window test can reach it without panicking.
    /// Named rather than papered over.
    #[gpui::test]
    fn a_download_asked_to_open_names_where_to_open(_cx: &mut TestAppContext) {
        let cache = std::env::temp_dir().join("caixonho-open-check");
        let asked = Transfer {
            bucket: "reports".into(),
            key: "daily/summary.csv".into(),
            directory: cache.clone(),
            then_open: true,
            direction: Direction::Down,
            source: None,
            bytes: 0,
            total: None,
            cancel: caixonho_core::transfer::Cancel::default(),
            phase: TransferPhase::Running,
        };

        assert_eq!(
            opens_at(&asked, "summary.csv"),
            Some(cache.join("summary.csv")),
            "the path handed over is where it landed under the name it landed as"
        );

        // The name comes from the outcome, not the key: a collision may have
        // renamed it, and joining the key would name a file that is not there.
        assert_eq!(
            opens_at(&asked, "summary (2).csv"),
            Some(cache.join("summary (2).csv"))
        );

        let plain = Transfer {
            then_open: false,
            ..asked
        };
        assert_eq!(
            opens_at(&plain, "summary.csv"),
            None,
            "a plain download opens nothing"
        );
    }

    /// The owner pressed Cancel during a collision question and the question
    /// stayed on screen.
    ///
    /// Rows render by *phase*. A transfer waiting on a person has no request
    /// in flight, so no settlement will ever arrive to change its phase, and
    /// marking it cancelled in the queue alone changes nothing anyone can
    /// see. Same for one still waiting for a slot.
    #[gpui::test]
    fn cancelling_reaches_the_ones_with_nothing_in_flight(cx: &mut TestAppContext) {
        let (app, cx) = looking_at(cx, "reports");

        let (asking, waiting, running) = app.update(cx, |app, _| {
            let asking = app.queue.accept(Transfer {
                phase: TransferPhase::KeyTaken {
                    key: "daily/taken.csv".into(),
                },
                ..an_upload(TransferPhase::Running)
            });
            app.queue.asking(asking);
            let waiting = app.queue.accept(an_upload(TransferPhase::Running));
            let running = app.queue.accept(an_upload(TransferPhase::Running));
            app.queue.settled(running, Standing::Running);
            (asking, waiting, running)
        });

        app.update(cx, |app, cx| app.cancel_queue(cx));

        app.read_with(cx, |app, _| {
            for (id, what) in [(asking, "asking"), (waiting, "waiting")] {
                assert!(
                    matches!(
                        app.queue
                            .items()
                            .iter()
                            .find(|i| i.id == id)
                            .map(|i| &i.payload.phase),
                        Some(TransferPhase::Cancelled)
                    ),
                    "the {what} transfer still draws its old row, so Cancel did nothing \
                     anyone could see"
                );
            }
            // The running one is stopped by its `Cancel`, and its own
            // settlement says so — this window must not pre-empt that.
            assert!(
                matches!(
                    app.queue
                        .items()
                        .iter()
                        .find(|i| i.id == running)
                        .map(|i| &i.payload.phase),
                    Some(TransferPhase::Running)
                ),
                "a running transfer's own settlement reports the cancellation"
            );
        });
    }
}
