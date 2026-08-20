//! What this application writes down about itself, where it puts it, and what
//! may never appear in it.
//!
//! The application used to write nothing down. Everything it learned about a
//! failure — the cause the classifier settled on, which credential source was
//! used, what the service actually said — existed for as long as the panel was
//! on screen and was then gone, so answering *why* on someone else's machine
//! meant adding a diagnostic by hand, running it, and deleting it again
//! (`diagnostics` spec, "The application keeps a log on this machine").
//!
//! Three rules bind this module:
//!
//! - **A secret is never handed to the logging layer at all.** Not filtered on
//!   the way out — *never given*. Every function below takes a name, a count,
//!   a scope or a structured [`Error`], and there is no signature here that a
//!   [`crate::CredentialSecret`] fits into. That is what makes the rule
//!   checkable by reading six signatures instead of auditing every call site,
//!   and it is why the events name the connection rather than the credential
//!   (design.md, "Redaction is structural, not editorial"). The type itself
//!   holds the other half of the guarantee: it has no [`std::fmt::Display`] at
//!   all and a hand-written [`std::fmt::Debug`] that redacts, so even a
//!   careless `?secret` at some future call site discloses nothing.
//! - **This crate is informative; everything else is quiet.** The AWS SDK
//!   emits through `tracing` too, and at its detailed levels it carries
//!   request and header material nobody asked to have written down. The
//!   default keeps it at warnings; [`LOG_LEVEL_ENV`] raises it for one
//!   investigation, which is a deliberate act.
//! - **Failing to log is not a failure.** A client that refuses to start
//!   because it could not write a diagnostic has mistaken the diagnostic for
//!   the product. A log that cannot be opened is reported once, through
//!   [`Diagnostics::problem`], and the application runs without it.
//!
//! Nothing here reaches a network. There is no telemetry in this project and
//! there never will be (invariant 4); the file is on the user's own machine,
//! and sending it anywhere is something only the user can do.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::Level;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

use crate::capability::{Observation, Scope};
use crate::error::Error;
use crate::types::ConnectionId;

/// The environment variable that raises the level of detail.
///
/// Its own name rather than `RUST_LOG`, deliberately. `RUST_LOG=debug` is a
/// thing people leave set in a shell for some other program, and inheriting it
/// here would turn on the AWS SDK's detailed levels — the ones that carry
/// request and header material — without anybody deciding to. Raising the
/// level is meant to be a deliberate act for one investigation, so it takes a
/// deliberate variable.
///
/// The value is `tracing`'s own target syntax, e.g. `debug`,
/// `caixonho_core=debug`, or `warn,caixonho_core=trace`. A value that cannot
/// be read leaves the default standing and says so in the log.
pub const LOG_LEVEL_ENV: &str = "CAIXONHO_LOG";

/// What this application is called on disk.
const APPLICATION: &str = "caixonho";

/// What every log file's name starts with.
const FILE_PREFIX: &str = "caixonho-";

/// What every log file's name ends with.
const FILE_SUFFIX: &str = ".log";

/// How large one segment may grow before the next one starts.
///
/// Rolling by day alone would not bound anything: the spec's scenario is an
/// operation failing over and over for a long time, and that happens inside
/// one day. Four mebibytes is a great many lines of "this is what I decided"
/// and a size a mail client will still accept.
const MAX_SEGMENT_BYTES: u64 = 4 * 1024 * 1024;

/// How many segments are kept.
///
/// The most recent ones: what explains a failure the user has just had is the
/// end of the log, not its beginning. Five segments bound the whole thing at
/// [`MAX_SEGMENT_BYTES`] times this, whatever happens.
const SEGMENTS_KEPT: usize = 5;

/// What an operation settled as, for the log: what it produced, or the
/// structured cause it failed with.
///
/// Borrowed rather than owned so a call site can report a failure it is about
/// to hand back to the caller, without cloning an error to describe it.
type Settled<'a, T> = std::result::Result<T, &'a Error>;

/// Why there is no log, when there is none.
///
/// Deliberately not a variant of [`Error`]. Every cause in that enum is
/// something that went wrong with what the *user* was doing; this is something
/// that went wrong with our record of it, and the spec is explicit that the
/// absence of a log is not presented as a failure of the thing the user was
/// doing (`diagnostics` spec, "Logging never takes the application down").
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LogProblem {
    /// This machine offers nowhere to keep a log — no home directory, or no
    /// local data directory at all.
    #[error("this machine has nowhere to keep a log")]
    NoLocation,
    /// The log's location could not be written to — no permission, a full
    /// disk, a file where the directory should be.
    #[error("the log could not be written")]
    NotWritable,
    /// Something had already installed a subscriber. Only [`start`] does that,
    /// and only once, so this means it was called twice.
    #[error("the log was already started")]
    AlreadyStarted,
}

/// Where the log is, so a frontend can tell the user without knowing the
/// platform's conventions (`diagnostics` spec, "Finding the file").
///
/// Both a file and a directory, because they answer different questions. The
/// file is what is being written *now* and is what to attach to an issue; the
/// directory is where the file and its predecessors live and never goes stale,
/// which is what to show in an interface that stays open across a roll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostics {
    file: Option<PathBuf>,
    directory: Option<PathBuf>,
    problem: Option<LogProblem>,
}

impl Diagnostics {
    /// The file being written now, when there is one.
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// The directory the log files live in, when this machine has one.
    ///
    /// Still `Some` when [`Self::file`] is `None`: knowing where the log was
    /// meant to go is what makes "the log could not be written" actionable.
    pub fn directory(&self) -> Option<&Path> {
        self.directory.as_deref()
    }

    /// Why there is no log, when there is none.
    ///
    /// Said once, here, rather than at every event. A frontend shows it beside
    /// the log's location and carries on — nothing the user was doing has
    /// failed.
    pub fn problem(&self) -> Option<LogProblem> {
        self.problem
    }
}

/// Start writing the log. Called once, as the application starts.
///
/// Never fails: what it returns says where the log is, or why there is none,
/// and the caller carries on either way.
pub fn start() -> Diagnostics {
    let Some(directory) = log_directory() else {
        return Diagnostics {
            file: None,
            directory: None,
            problem: Some(LogProblem::NoLocation),
        };
    };

    let log = RollingLog::open(&directory);
    let (filter, unreadable) = filter(std::env::var(LOG_LEVEL_ENV).ok().as_deref());
    let mut diagnostics = Diagnostics {
        file: log.path(),
        directory: Some(directory),
        problem: log.problem(),
    };

    if install(log, filter).is_err() {
        // Something is already writing, and to the file this call just opened
        // — so the path is still the right answer, and the handle this one
        // holds is simply dropped. The problem it was opened with, if it had
        // one, is the more useful thing to report.
        diagnostics.problem = diagnostics.problem.or(Some(LogProblem::AlreadyStarted));
        return diagnostics;
    }

    if let Some(value) = unreadable {
        // After installing, so it lands in the log the user is about to read.
        tracing::warn!(
            variable = LOG_LEVEL_ENV,
            value,
            "the requested level of detail could not be read; the default is in use"
        );
    }
    diagnostics
}

/// Install `writer` as the log, at `filter`.
///
/// `Err` means something had already installed a subscriber, which is the only
/// way this can fail.
fn install<W>(writer: W, filter: Targets) -> std::result::Result<(), LogProblem>
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    tracing::subscriber::set_global_default(subscriber(writer, filter))
        .map_err(|_| LogProblem::AlreadyStarted)
}

/// The subscriber this application writes through.
///
/// Factored out so a test can put a buffer where the file goes and read back
/// exactly what would have been written — which is the only way the redaction
/// rule can be asserted rather than intended.
///
/// No ANSI: the destination is a file somebody opens in an editor or attaches
/// to an issue, and escape codes in one are noise at best.
fn subscriber<W>(writer: W, filter: Targets) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let events = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(writer);
    tracing_subscriber::registry().with(filter).with(events)
}

/// What is recorded, given whatever [`LOG_LEVEL_ENV`] was set to.
///
/// Returns the filter and — when the variable held something that could not be
/// read — the value, so the caller can say so once the log exists. An
/// unreadable value leaves the default standing rather than silencing the log
/// or turning everything on: both would be surprises, and one of them writes
/// request material to disk.
///
/// What is read is laid *on top of* the default rather than replacing it,
/// because the variable is documented as raising the level. Replacing would
/// mean that `CAIXONHO_LOG=aws_config=debug` — the obvious thing to type when
/// the SDK is what you are investigating — silenced this application's own
/// events, which is the opposite of what the person typing it wants. A
/// directive naming a target the default also names replaces that one, so
/// lowering a particular target is still possible for someone who means it.
fn filter(raised: Option<&str>) -> (Targets, Option<String>) {
    let Some(raised) = raised.map(str::trim).filter(|raised| !raised.is_empty()) else {
        return (quiet(), None);
    };
    let Ok(directives) = raised.parse::<Targets>() else {
        return (quiet(), Some(raised.to_owned()));
    };

    let mut effective = quiet();
    if let Some(default) = directives.default_level() {
        effective = effective.with_default(default);
        // A directive naming a target outranks a bare default however verbose
        // the default is — so `CAIXONHO_LOG=debug` would turn on everybody's
        // detail except ours, which is nobody's idea of "more detail". Raised
        // only: a default of `warn` leaves this application informative,
        // because the variable is for raising and lowering our own events
        // takes naming them.
        effective =
            effective.with_targets(OURS.map(|target| (target, default.max(Level::INFO.into()))));
    }
    (effective.with_targets(directives), None)
}

/// The crates this repository ships, whose events are the application's own.
///
/// Underscored because that is what a crate's name looks like in a `tracing`
/// target: the target defaults to the module path, and `caixonho-core`'s
/// modules live under `caixonho_core`.
const OURS: [&str; 2] = ["caixonho_core", "caixonho_gui"];

/// The default: this application's own decisions, and nothing from anybody
/// else short of a warning.
///
/// The AWS SDK emits through `tracing` as well, and its detailed levels carry
/// request and header material that has no business being written down unasked
/// (`diagnostics` spec, "Detail is a choice, and the default is modest"). So
/// the two crates this repository ships are informative and everything else is
/// quiet, which is a rule about *whose* diagnostics rather than about how
/// interesting they are.
fn quiet() -> Targets {
    Targets::new()
        .with_targets(OURS.map(|target| (target, Level::INFO)))
        .with_default(Level::WARN)
}

/// Where the log goes on this machine.
///
/// Each platform's own log location, not a directory of our invention:
///
/// - macOS: `~/Library/Logs/caixonho` — where `Console.app` looks, and what
///   every other application on the machine uses.
/// - Windows: `%LOCALAPPDATA%\caixonho\logs` — local rather than roaming,
///   because a log is machine-specific and has no business being copied
///   between a user's machines.
/// - Linux: `$XDG_STATE_HOME/caixonho/logs`, which is what the state directory
///   is for; the cache directory when the platform offers no state directory.
///
/// `directories` resolves the base, rather than this repository assembling
/// paths out of `HOME` and `LOCALAPPDATA`: that crate is tested on three
/// platforms, which this repository is not yet in a position to be, and
/// Windows is both a first-class target here and the one least likely to be
/// exercised by hand. `ProjectDirs` has no log directory of its own — read out
/// of `directories` 6.0.0's own source rather than recalled — so the base
/// directories are what this composes.
#[cfg(target_os = "macos")]
fn log_directory() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|base| {
        base.home_dir()
            .join("Library")
            .join("Logs")
            .join(APPLICATION)
    })
}

/// Where the log goes on this machine. See the macOS definition above.
#[cfg(target_os = "windows")]
fn log_directory() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|base| base.data_local_dir().join(APPLICATION).join("logs"))
}

/// Where the log goes on this machine. See the macOS definition above.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn log_directory() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", APPLICATION)?;
    Some(
        dirs.state_dir()
            .unwrap_or_else(|| dirs.cache_dir())
            .join("logs"),
    )
}

// ---------------------------------------------------------------------------
// What is worth recording.
//
// Every function below takes names, counts, scopes and structured causes. None
// of them takes a type that holds a secret, and that is the whole of the
// redaction rule: there is no call site that *could* hand one to the logging
// layer.
// ---------------------------------------------------------------------------

/// Where a connection's credentials came from.
///
/// In the log because it is the first thing anyone reading one needs: a
/// failure to sign means something different for a profile the AWS CLI also
/// uses than for a key typed into this application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceKind {
    /// A profile named in the AWS shared configuration files.
    Profile,
    /// A credential this application holds in the OS credential store.
    Stored,
}

impl SourceKind {
    /// What this source is called in the log.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::Stored => "stored credential",
        }
    }
}

/// A connection came up.
pub(crate) fn connection_opened(
    id: ConnectionId,
    connection: &str,
    source: SourceKind,
    region: &str,
) {
    tracing::info!(
        connection,
        id = id.0,
        source = source.label(),
        region,
        "connection opened"
    );
}

/// A connection did not come up, and why.
pub(crate) fn connection_refused(
    id: ConnectionId,
    connection: &str,
    source: SourceKind,
    error: &Error,
) {
    tracing::warn!(
        connection,
        id = id.0,
        source = source.label(),
        // Display, which is exactly the text the user is shown. A log that
        // disagrees with the screen is worse than none.
        cause = %error,
        "connection refused"
    );
}

/// A listing finished: how many buckets, or why none.
pub(crate) fn listing_settled(id: ConnectionId, connection: &str, settled: Settled<'_, usize>) {
    match settled {
        Ok(buckets) => tracing::info!(connection, id = id.0, buckets, "listed the account"),
        Err(error) => tracing::warn!(connection, id = id.0, cause = %error, "listing failed"),
    }
}

/// A capability probe settled, and what it turned out to be evidence of.
/// What reading one location came to.
///
/// The location is named by bucket and prefix — never by what it contains, and
/// never by an object's key. A key is the user's own data, and a log they may
/// send to a stranger has no business carrying an inventory of it; counts
/// answer "did it work and how much came back" without disclosing anything.
pub(crate) fn location_settled(
    bucket: &str,
    prefix: &str,
    settled: Settled<'_, (usize, usize, bool)>,
) {
    match settled {
        Ok((folders, objects, more)) => {
            tracing::info!(bucket, prefix, folders, objects, more, "listed a location")
        }
        Err(error) => {
            tracing::warn!(bucket, prefix, cause = %error, "listing a location failed")
        }
    }
}

pub(crate) fn probe_settled(scope: &Scope, observation: Observation) {
    tracing::info!(
        bucket = scope.bucket_name(),
        prefix = scope.key_prefix(),
        observation = ?observation,
        "probe settled"
    );
}

/// A credential was saved under this name, or was not.
///
/// The name and nothing else. What was saved is in the operating system's
/// credential store, which is the only place it exists.
pub(crate) fn credential_saved(connection: &str, settled: Settled<'_, ()>) {
    match settled {
        Ok(()) => tracing::info!(connection, "credential saved"),
        Err(error) => tracing::warn!(connection, cause = %error, "credential not saved"),
    }
}

/// A credential was forgotten, or was not.
pub(crate) fn credential_forgotten(connection: &str, settled: Settled<'_, ()>) {
    match settled {
        Ok(()) => tracing::info!(connection, "credential forgotten"),
        Err(error) => tracing::warn!(connection, cause = %error, "credential not forgotten"),
    }
}

// ---------------------------------------------------------------------------
// The file itself.
// ---------------------------------------------------------------------------

/// The log file, rolled by day and bounded by size.
///
/// Hand-written rather than taken from a crate. `tracing-appender` is the
/// obvious candidate and was refused for one reason: it is neither vendored on
/// this machine nor readable from crates.io from here, so its current version
/// and licence could not be checked before shipping an identifier for it
/// (`AGENTS.md`, "Verify external facts"). What it would have provided is a
/// day-rolled file with a background writer thread; what is here is a
/// day-rolled file with a size cap as well — which the spec asks for and
/// day-rolling alone does not give — and no thread.
///
/// Cheap to clone: every clone is the same file, the same counters and the
/// same lock, which is what lets a test hold one while the subscriber holds
/// another.
#[derive(Debug, Clone)]
pub(crate) struct RollingLog(Arc<Inner>);

/// Everything a [`RollingLog`]'s clones share.
#[derive(Debug)]
struct Inner {
    directory: PathBuf,
    /// The size one segment may reach. A constant in production; a small
    /// number in the tests, so rolling can be watched without writing
    /// megabytes.
    max_segment_bytes: u64,
    current: Mutex<Segment>,
    /// Why there is no file, when there is none. Settled when the log is
    /// opened and reported once, through [`Diagnostics::problem`].
    problem: Option<LogProblem>,
    /// How many writes could not be made. Counted rather than announced —
    /// see [`Inner::announcements`].
    failures: AtomicUsize,
    /// How many times that was said out loud. One, however many failures: a
    /// log that cannot be written must not turn every event into a complaint
    /// on somebody's terminal.
    announcements: AtomicUsize,
}

/// The segment being written now.
#[derive(Debug)]
struct Segment {
    /// `None` when the file could not be opened. Every write then counts as a
    /// failure and none of them says anything.
    file: Option<File>,
    path: Option<PathBuf>,
    day: Day,
    index: u32,
    written: u64,
}

impl RollingLog {
    /// Open the log in `directory`, creating it if it is not there.
    pub(crate) fn open(directory: &Path) -> Self {
        Self::bounded_at(directory, MAX_SEGMENT_BYTES)
    }

    /// The same, with a segment size a test can reach.
    fn bounded_at(directory: &Path, max_segment_bytes: u64) -> Self {
        let day = Day::today();
        let mut problem = None;
        let mut segment = Segment {
            file: None,
            path: None,
            day,
            index: 0,
            written: 0,
        };

        match std::fs::create_dir_all(directory) {
            Ok(()) => {
                // Continue today's last segment rather than starting a new one
                // per launch: a session that opens and closes the application
                // five times is one story, and five files tell it worse.
                segment.index = last_index(directory, day).unwrap_or(0);
                let mut path = segment_path(directory, day, segment.index);
                let mut written = std::fs::metadata(&path).map(|file| file.len()).unwrap_or(0);
                if written >= max_segment_bytes {
                    segment.index += 1;
                    path = segment_path(directory, day, segment.index);
                    written = 0;
                }
                match append_to(&path) {
                    Ok(file) => {
                        segment.file = Some(file);
                        segment.path = Some(path);
                        segment.written = written;
                    }
                    Err(_) => problem = Some(LogProblem::NotWritable),
                }
            }
            Err(_) => problem = Some(LogProblem::NotWritable),
        }

        let log = Self(Arc::new(Inner {
            directory: directory.to_owned(),
            max_segment_bytes,
            current: Mutex::new(segment),
            problem,
            failures: AtomicUsize::new(0),
            announcements: AtomicUsize::new(0),
        }));
        if problem.is_some() {
            // The first of the failures, and the only one that will be said
            // out loud. Everything after it is counted and swallowed.
            log.failed();
        } else {
            prune(directory);
        }
        log
    }

    /// The file being written now, when there is one.
    pub(crate) fn path(&self) -> Option<PathBuf> {
        self.segment().path.clone()
    }

    /// Why there is no file, when there is none.
    pub(crate) fn problem(&self) -> Option<LogProblem> {
        self.0.problem
    }

    /// How many writes could not be made.
    #[cfg(test)]
    fn failures(&self) -> usize {
        self.0.failures.load(Ordering::Relaxed)
    }

    /// How many times that was said out loud.
    #[cfg(test)]
    fn announcements(&self) -> usize {
        self.0.announcements.load(Ordering::Relaxed)
    }

    /// Append one event's line, rolling first if this is a new day or the
    /// segment is full.
    ///
    /// Never propagates a failure. The caller is a logging layer inside
    /// somebody's request path, and an application that fell over because its
    /// diagnostic did is the failure this whole module is written to avoid.
    fn append(&self, chunk: &[u8]) {
        let mut segment = self.segment();
        if segment.file.is_none() {
            self.failed();
            return;
        }

        let today = Day::today();
        if today != segment.day {
            self.roll(&mut segment, today, 0);
        } else if segment.written + chunk.len() as u64 > self.0.max_segment_bytes {
            let next = segment.index + 1;
            self.roll(&mut segment, today, next);
        }

        let Some(file) = segment.file.as_mut() else {
            self.failed();
            return;
        };
        match file.write_all(chunk) {
            Ok(()) => segment.written += chunk.len() as u64,
            Err(_) => {
                self.failed();
            }
        }
    }

    /// Start writing `day`'s segment `index`, and drop whatever is now too old
    /// to keep.
    fn roll(&self, segment: &mut Segment, day: Day, index: u32) {
        // Closed before the next one opens, so a platform that dislikes two
        // handles on one directory entry never sees them.
        segment.file = None;
        segment.path = None;
        segment.day = day;
        segment.index = index;
        segment.written = 0;

        let path = segment_path(&self.0.directory, day, index);
        match append_to(&path) {
            Ok(file) => {
                segment.file = Some(file);
                segment.path = Some(path);
            }
            Err(_) => self.failed(),
        }
        prune(&self.0.directory);
    }

    /// Count a failure, and say so if nothing has said so yet.
    ///
    /// Once, however many events fail. Through `stderr` rather than `tracing`,
    /// which would be a subscriber complaining to itself.
    fn failed(&self) {
        self.0.failures.fetch_add(1, Ordering::Relaxed);
        // Exchanged rather than incremented, so that exactly one thread ever
        // says it however many are logging — and so the counter means "times
        // this was said" rather than "times something went wrong", which is
        // the distinction the test asserts on.
        if self
            .0
            .announcements
            .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            eprintln!(
                "caixonho: no log is being written to {} — carrying on without one",
                self.0.directory.display()
            );
        }
    }

    /// The segment being written, with a poisoned lock recovered rather than
    /// propagated — exactly as [`crate::Session`] does with its own. The worst
    /// that can come of it is a line in the wrong file.
    fn segment(&self) -> MutexGuard<'_, Segment> {
        self.0
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl<'a> MakeWriter<'a> for RollingLog {
    type Writer = &'a RollingLog;

    fn make_writer(&'a self) -> Self::Writer {
        self
    }
}

impl io::Write for &RollingLog {
    /// Always reports success. A write that failed has been counted and, once,
    /// said out loud; reporting it upwards would only give the logging layer
    /// something it cannot do anything useful with.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.append(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The file one day's segment `index` lives in.
///
/// Zero-padded so that the names sort in the order they were written, which is
/// what lets [`prune`] keep the most recent by sorting alone.
fn segment_path(directory: &Path, day: Day, index: u32) -> PathBuf {
    directory.join(format!("{FILE_PREFIX}{day}.{index:03}{FILE_SUFFIX}"))
}

/// Open `path` for appending, creating it if it is not there.
fn append_to(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

/// The highest segment index already written for `day`, if any.
fn last_index(directory: &Path, day: Day) -> Option<u32> {
    let prefix = format!("{FILE_PREFIX}{day}.");
    segments(directory)
        .iter()
        .filter_map(|name| name.strip_prefix(&prefix))
        .filter_map(|rest| rest.strip_suffix(FILE_SUFFIX))
        .filter_map(|index| index.parse().ok())
        .max()
}

/// Every log file in `directory`, sorted oldest first.
///
/// By name, which is by date and then by segment: the names are built by
/// [`segment_path`] precisely so that this is true without reading a single
/// file's modification time.
fn segments(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with(FILE_PREFIX) && name.ends_with(FILE_SUFFIX))
        .collect();
    names.sort_unstable();
    names
}

/// Drop everything but the [`SEGMENTS_KEPT`] most recent files.
///
/// What is kept is the end of the log, because what explains a failure the
/// user has just had is the end of it. A file that cannot be removed is left
/// alone: it is bounded by its own segment size, and refusing to log over it
/// would be a worse answer than keeping one file too many.
fn prune(directory: &Path) {
    let names = segments(directory);
    let Some(stale) = names.len().checked_sub(SEGMENTS_KEPT) else {
        return;
    };
    for name in &names[..stale] {
        let _ = std::fs::remove_file(directory.join(name));
    }
}

/// A civil date, in UTC.
///
/// UTC, and computed here rather than taken from a date crate. Two reasons for
/// each half. UTC because `tracing-subscriber` stamps every line in UTC, so a
/// file named in local time would disagree with its own contents — and someone
/// east of Greenwich would find the evening's lines under tomorrow's name.
/// Computed here because all this is for is naming a file and knowing when the
/// day has turned; a dependency that ships a timezone database to do it would
/// be a poor trade, and this repository does not take one for a date it never
/// shows a user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Day {
    year: i64,
    month: u32,
    day: u32,
}

impl Day {
    /// Today, by this machine's clock.
    ///
    /// A clock before the epoch — which is what an unset CMOS battery looks
    /// like — comes out as a date before it rather than as a panic.
    fn today() -> Self {
        let now = SystemTime::now();
        let seconds = match now.duration_since(UNIX_EPOCH) {
            Ok(since) => since.as_secs() as i64,
            Err(before) => -(before.duration().as_secs() as i64),
        };
        Self::at(seconds)
    }

    /// The civil date `seconds` after the Unix epoch.
    ///
    /// Howard Hinnant's `civil_from_days`, which is the algorithm every date
    /// library uses and is exact for any year this program will see. Its shape
    /// is not obvious; `a_day_is_the_civil_date_of_the_moment_it_names` is what
    /// says it is right.
    fn at(seconds: i64) -> Self {
        let days = seconds.div_euclid(86_400);
        // Shift the epoch to 0000-03-01, which puts the leap day at the end of
        // the year and makes the rest arithmetic instead of special cases.
        let shifted = days + 719_468;
        let era = shifted.div_euclid(146_097);
        let day_of_era = shifted.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let year = year_of_era + era * 400;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        Self {
            year: year + i64::from(month <= 2),
            month: month as u32,
            day,
        }
    }
}

impl std::fmt::Display for Day {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    //! `diagnostics` spec: what is written, where it goes, what may never
    //! appear in it, and what happens when it cannot be written at all.
    //!
    //! Nothing here touches the machine's real log directory — every test that
    //! opens a file opens it under a directory of its own — and nothing here
    //! reaches a network.

    use std::io::Write;
    use std::sync::{Arc, Mutex, PoisonError};

    use super::*;
    use crate::connections::ConnectionFile;
    use crate::connections::double::ConnectionFileDouble;
    use crate::credentials::double::SecretStoreDouble;
    use crate::credentials::{CredentialSecret, SecretStore, StoredCredential};
    use crate::error::CredentialStoreProblem;
    use crate::outcome::Outcome;
    use crate::profiles::ConfigPaths;
    use crate::session::Session;
    use crate::tls::HttpStack;
    use std::time::{Duration, SystemTime};

    /// The secret the tests put through the application. Not a real key — but
    /// it is treated as one everywhere below, which is the point.
    const SECRET: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    const TOKEN: &str = "FQoGZXIvYXdzEExampleSessionToken";

    /// The same, made of characters the log's own format has to escape.
    ///
    /// Its readable spelling and its spelling *in the log* differ, which is
    /// what makes the third check in `no_secret_reaches_the_log_in_any_spelling`
    /// bite: a secret written out as a field value would arrive escaped and
    /// sail straight past a search for the readable one.
    const AWKWARD_SECRET: &str = "wJalr\\XUtn\"FEMI\tEXAMPLEKEY";
    const AWKWARD_TOKEN: &str = "FQoGZXIv\\YXdz\"ExampleSessionToken";

    /// A directory of this test's own, removed when the test ends.
    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("caixonho-diagnostics-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("fixture dir");
            Self { dir }
        }

        /// A path inside the fixture that is not a directory and cannot become
        /// one, because a file is sitting where it would have to go.
        fn blocked(&self) -> PathBuf {
            let file = self.dir.join("not-a-directory");
            std::fs::write(&file, b"in the way").expect("fixture file");
            file.join("logs")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// The log, in memory, so a test can read exactly what would have been
    /// written to the file.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Captured {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner))
                .into_owned()
        }
    }

    impl Write for &Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Captured {
        type Writer = &'a Captured;

        fn make_writer(&'a self) -> Self::Writer {
            self
        }
    }

    /// Run `work` with the log captured, and hand back everything it wrote.
    ///
    /// Thread-local rather than global: `set_global_default` can be called
    /// once per process, and these tests share one. Everything asserted below
    /// therefore runs on this thread — which is why the asynchronous cases use
    /// a current-thread runtime, where a spawned task is polled on the thread
    /// that is driving it.
    fn recording<T>(filter: Targets, work: impl FnOnce() -> T) -> (String, T) {
        let captured = Captured::default();
        let produced =
            tracing::subscriber::with_default(subscriber(captured.clone(), filter), work);
        (captured.text(), produced)
    }

    /// Everything, from everybody: the spec's "most detailed setting", where
    /// the redaction rule still has to hold.
    fn everything() -> Targets {
        Targets::new().with_default(Level::TRACE)
    }

    /// A runtime a test drives on its own thread, so the thread-local
    /// subscriber above sees what the spawned work logs.
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
    }

    /// A session whose credential store and connections file are doubles, and
    /// which is told about no AWS shared configuration at all.
    fn session(secrets: Arc<dyn SecretStore>) -> Session {
        Session::new(
            tokio::runtime::Handle::current(),
            HttpStack::with_ca_bundle(None).expect("client builds"),
            ConfigPaths {
                config: None,
                credentials: None,
            },
        )
        .with_secret_store(secrets)
        .with_connection_file(Arc::new(ConnectionFileDouble::empty()) as Arc<dyn ConnectionFile>)
    }

    /// The stored credential the tests connect with.
    fn stored() -> StoredCredential {
        StoredCredential::new("typed-in", "ap-southeast-1", "AKIAIOSFODNN7EXAMPLE")
    }

    /// A credential store already holding [`stored`]'s secret.
    fn store_holding(secret: &str, token: &str) -> Arc<SecretStoreDouble> {
        let store = Arc::new(SecretStoreDouble::open());
        crate::credentials::save(
            store.as_ref(),
            &stored(),
            &CredentialSecret::new(secret, Some(token.to_owned())),
        )
        .expect("an open store accepts it");
        store
    }

    /// Every way a secret can reach a file looking like something other than
    /// itself.
    ///
    /// The same three the credential store's
    /// `no_credential_store_failure_ever_discloses_the_secret` documents, for
    /// the same reason: a naive check passed a verbatim disclosure once
    /// already. Readable is the obvious route; the byte array is what `{:?}`
    /// on a payload prints (`[119, 74, 97, ...]`); and the escaped one is this
    /// destination's own — `tracing`'s formatter writes a string field through
    /// `{:?}`, so a secret arriving as a field value would be quoted and
    /// backslash-escaped, and a search for the readable string would sail
    /// straight past it.
    fn disclosures(secret: &str) -> [String; 3] {
        [
            secret.to_owned(),
            format!("{:?}", secret.as_bytes()),
            format!("{secret:?}"),
        ]
    }

    /// Fail if `log` holds `secret` in any spelling.
    fn assert_undisclosed(log: &str, secret: &str, what: &str) {
        for disclosure in disclosures(secret) {
            assert!(
                !log.contains(&disclosure),
                "{what} reached the log as `{disclosure}`:\n{log}"
            );
        }
    }

    #[test]
    fn no_secret_reaches_the_log_in_any_spelling() {
        // Invariant 5's second half, which has been half-written since the
        // beginning because there were no logs to check. There are now.
        //
        // Everything is turned up to its most detailed setting, because the
        // spec says the rule holds there too: more detail never means more
        // secret.
        for (secret, token) in [(SECRET, TOKEN), (AWKWARD_SECRET, AWKWARD_TOKEN)] {
            let (log, ()) = recording(everything(), || {
                runtime().block_on(async {
                    // A connection that opens, from a credential whose secret
                    // is in the store — the path that has a secret in hand.
                    let opened = session(store_holding(secret, token));
                    let _ = opened.open(ConnectionId(1), stored()).await;

                    // A connection that does not, because the store will not
                    // open: the failure path, where an error carrying store
                    // detail would be the disclosure.
                    let refused = session(Arc::new(SecretStoreDouble::refusing(
                        CredentialStoreProblem::Locked,
                    )));
                    let _ = refused.open(ConnectionId(2), stored()).await;

                    // Saving and forgetting: the two operations a secret is
                    // actually handed to.
                    let saving = session(Arc::new(SecretStoreDouble::open()));
                    let (tell, told) = tokio::sync::oneshot::channel();
                    saving.spawn_save_credential(
                        stored(),
                        CredentialSecret::new(secret, Some(token.to_owned())),
                        move |saved| {
                            let _ = tell.send(saved.map(|_| ()));
                        },
                    );
                    let _ = told.await.expect("the callback runs exactly once");

                    let (tell, told) = tokio::sync::oneshot::channel();
                    saving.spawn_forget_credential("typed-in".to_owned(), move |forgotten| {
                        let _ = tell.send(forgotten);
                    });
                    let _ = told.await.expect("the callback runs exactly once");

                    // A listing, which fails at the credential store and is
                    // reported through the outcome the frontend renders.
                    let (tell, told) = tokio::sync::oneshot::channel();
                    refused.spawn_listing(ConnectionId(3), stored(), move |tagged| {
                        let _ = tell.send(tagged);
                    });
                    let _ = told.await.expect("the callback runs exactly once");

                    // And the careless call site: a secret handed straight to
                    // the logging layer at the most detailed level, which is
                    // what the structural rule exists to survive. Nothing in
                    // the crate does this; that it discloses nothing anyway is
                    // the property being asserted.
                    let secret = CredentialSecret::new(secret, Some(token.to_owned()));
                    tracing::error!(secret = ?secret, "a call site that should not exist");
                });
            });

            // Before the assertions that nothing is in it: a log that captured
            // nothing would pass every check below and prove nothing at all.
            assert!(
                log.contains("connection opened") && log.contains("credential saved"),
                "the log has to hold the events these assertions are about:\n{log}"
            );
            assert!(
                log.contains("typed-in"),
                "the connection is what the log may name:\n{log}"
            );
            assert_undisclosed(&log, secret, "the secret access key");
            assert_undisclosed(&log, token, "the session token");
        }
    }

    #[test]
    fn no_sign_in_secret_reaches_the_log_in_any_spelling() {
        // `XONHO-0011` gives secrets a second home. An access token, a refresh
        // token and a client secret are bearer material exactly like an access
        // key, and the rule that covers one has to cover all of them — the
        // three spellings included, since the awkward variants are the ones a
        // readable-text search sails straight past.
        for (access, refresh) in [(SECRET, TOKEN), (AWKWARD_SECRET, AWKWARD_TOKEN)] {
            let fixture = Fixture::new("sign-in-secrets");
            let (log, ()) = recording(everything(), || {
                let at = crate::sso::SignInLocation {
                    session_name: "corp".into(),
                    start_url: "https://corp.awsapps.com/start".into(),
                    region: "ap-southeast-1".into(),
                    scopes: Vec::new(),
                };
                let obtained = crate::sso::ObtainedSession {
                    token: crate::sso::SsoToken {
                        access_token: crate::sso::SignInSecret::new(access),
                        refresh_token: Some(crate::sso::SignInSecret::new(refresh)),
                        expires_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
                    },
                    registration: crate::sso::ClientRegistration {
                        client_id: "client-id-is-not-secret".into(),
                        client_secret: crate::sso::SignInSecret::new(access),
                        registration_expires_at: SystemTime::UNIX_EPOCH
                            + Duration::from_secs(1_800_000_000),
                    },
                };

                // The path that actually holds these secrets: writing them to
                // the token cache. The file is allowed to carry them; the log
                // is not.
                let written = crate::sso::write_session(&fixture.dir, &at, &obtained)
                    .expect("the session is written");
                // In *some* spelling: the JSON writer escapes what the
                // format requires, which is exactly why `disclosures` exists
                // and why a readable-text search is not a check.
                let body = std::fs::read_to_string(&written).expect("the file is there");
                assert!(
                    disclosures(access)
                        .iter()
                        .any(|spelling| body.contains(spelling)),
                    "the cache file is the one place these may land, and it holds none of them"
                );

                // And the careless call sites: the whole session, and one
                // secret on its own, handed straight to the logging layer at
                // its most detailed level. Nothing in the crate does this. That
                // it discloses nothing anyway is the property being asserted,
                // and the hand-written Debug on `SignInSecret` is what holds it.
                tracing::error!(session = ?obtained, "a call site that should not exist");
                tracing::error!(
                    secret = ?obtained.token.access_token,
                    "another call site that should not exist"
                );
            });

            assert!(
                log.contains("a call site that should not exist"),
                "the log has to hold the events these assertions are about:\n{log}"
            );
            assert!(
                log.contains("client-id-is-not-secret"),
                "the client id is public material and may be named:\n{log}"
            );
            assert_undisclosed(&log, access, "the access token, and the client secret");
            assert_undisclosed(&log, refresh, "the refresh token");
        }
    }

    #[test]
    fn a_failure_the_log_records_is_the_failure_the_user_is_shown() {
        // A log that disagrees with the screen is worse than none: the person
        // reading it is trying to explain what somebody else saw.
        let (log, outcome) = recording(quiet(), || {
            runtime().block_on(async {
                let session = session(Arc::new(SecretStoreDouble::refusing(
                    CredentialStoreProblem::Locked,
                )));
                let (tell, told) = tokio::sync::oneshot::channel();
                session.spawn_listing(ConnectionId(7), stored(), move |tagged| {
                    let _ = tell.send(tagged);
                });
                told.await.expect("the callback runs exactly once").outcome
            })
        });

        let Outcome::Failed(error) = outcome else {
            panic!("expected a failure, got {outcome:?}");
        };
        assert!(
            matches!(error, Error::CredentialStore { .. }),
            "got {error:?}"
        );
        assert!(
            log.contains(&error.to_string()),
            "the log has to carry the same cause the user is shown \
             (`{error}`):\n{log}"
        );
        assert!(
            log.contains("listing failed"),
            "and say what was being attempted:\n{log}"
        );
    }

    #[test]
    fn the_log_records_the_decisions_a_failure_is_explained_from() {
        // The spec's scenario: an operation failed, the user reports it later,
        // and the log has to name the connection, what was attempted and the
        // cause. Every event this crate emits is exercised here, including the
        // two — a listing that succeeded, and a probe — whose live paths need
        // a network.
        let listed = Error::Network {
            detail: "connection reset".to_owned(),
        };
        let (log, ()) = recording(quiet(), || {
            connection_opened(ConnectionId(1), "work", SourceKind::Profile, "eu-west-1");
            connection_opened(
                ConnectionId(2),
                "typed-in",
                SourceKind::Stored,
                "ap-southeast-1",
            );
            connection_refused(
                ConnectionId(3),
                "broken",
                SourceKind::Profile,
                &Error::NoCredentials {
                    profile: "broken".to_owned(),
                },
            );
            listing_settled(ConnectionId(1), "work", Ok(12));
            listing_settled(ConnectionId(3), "broken", Err(&listed));
            probe_settled(&Scope::bucket("logs"), Observation::Denied);
            probe_settled(&Scope::prefix("logs", "2026/"), Observation::Allowed);
            credential_saved("typed-in", Ok(()));
            credential_forgotten("typed-in", Ok(()));
        });

        for expected in [
            // A connection, and which kind of place its credentials came from.
            "connection opened",
            "source=\"profile\"",
            "source=\"stored credential\"",
            "region=\"eu-west-1\"",
            // A listing's outcome, and its cause when it has one.
            "listed the account",
            "buckets=12",
            "listing failed",
            &listed.to_string(),
            // A probe's result.
            "probe settled",
            "bucket=\"logs\"",
            "prefix=\"2026/\"",
            "observation=Denied",
            "observation=Allowed",
            // A credential, by name.
            "credential saved",
            "credential forgotten",
            "connection=\"typed-in\"",
        ] {
            assert!(
                log.contains(expected),
                "`{expected}` is missing from:\n{log}"
            );
        }
        assert_undisclosed(&log, SECRET, "the secret access key");
    }

    #[test]
    fn by_default_this_application_is_informative_and_everybody_else_is_quiet() {
        // The AWS SDK's detailed levels carry request and header material. It
        // is not that they are uninteresting — it is that writing them to a
        // file unasked is exactly the quiet accumulation this project refuses
        // everywhere else.
        let quiet = quiet();

        for ours in ["caixonho_core", "caixonho_core::session", "caixonho_gui"] {
            assert!(
                quiet.would_enable(ours, &Level::INFO),
                "{ours} records what it decided"
            );
            assert!(
                !quiet.would_enable(ours, &Level::DEBUG),
                "{ours} records decisions, not every step"
            );
        }
        for theirs in ["aws_smithy_runtime", "aws_config", "hyper_util", "rustls"] {
            assert!(
                !quiet.would_enable(theirs, &Level::INFO),
                "{theirs} is quiet unless asked for"
            );
            assert!(
                quiet.would_enable(theirs, &Level::WARN),
                "{theirs} still says when something went wrong"
            );
        }
    }

    #[test]
    fn one_environment_variable_raises_the_level_for_an_investigation() {
        let (raised, unreadable) = filter(Some("debug"));

        assert_eq!(unreadable, None);
        assert!(
            raised.would_enable("aws_smithy_runtime", &Level::DEBUG),
            "an investigation is exactly when the underlying libraries are worth hearing"
        );
        assert!(raised.would_enable("caixonho_core", &Level::DEBUG));
    }

    #[test]
    fn raising_one_thing_does_not_silence_everything_else() {
        // `CAIXONHO_LOG=aws_config=debug` is the obvious thing to type when
        // the SDK is what you are investigating. Read as a replacement it
        // would turn this application's own events off, and the log would come
        // back holding somebody else's story and not ours.
        let (raised, unreadable) = filter(Some("aws_config=debug"));

        assert_eq!(unreadable, None);
        assert!(raised.would_enable("aws_config", &Level::DEBUG));
        assert!(
            raised.would_enable("caixonho_core", &Level::INFO),
            "raising one target must not silence the default"
        );
        assert!(
            !raised.would_enable("hyper_util", &Level::INFO),
            "and must not raise anything that was not asked for"
        );
    }

    #[test]
    fn a_target_named_outright_is_what_that_target_records() {
        // The other direction: someone who means to quieten one noisy target
        // can, because a directive naming it replaces the default's.
        let (raised, _) = filter(Some("caixonho_core=warn"));

        assert!(!raised.would_enable("caixonho_core", &Level::INFO));
        assert!(raised.would_enable("caixonho_core", &Level::WARN));
    }

    #[test]
    fn a_level_that_cannot_be_read_leaves_the_default_standing_and_says_so() {
        // Neither silence nor everything: one would hide the log the user
        // came for, the other would write request material to disk over a
        // typo.
        for value in ["caixonho_core=shout", "caixonho_core=9"] {
            let (fallback, unreadable) = filter(Some(value));

            assert_eq!(unreadable.as_deref(), Some(value), "`{value}`");
            assert!(fallback.would_enable("caixonho_core", &Level::INFO));
            assert!(!fallback.would_enable("aws_config", &Level::INFO));
        }
    }

    #[test]
    fn a_bare_word_that_is_not_a_level_records_no_less_than_the_default() {
        // `CAIXONHO_LOG=shout` is a typo for a level, and `tracing` reads a
        // bare word as a *target* name — so on its own it would be a filter
        // that turns everything off. Laid over the default it cannot be: the
        // worst a typo can do is name a target nothing emits from.
        let (raised, _) = filter(Some("shout"));

        assert!(raised.would_enable("caixonho_core", &Level::INFO));
        assert!(!raised.would_enable("aws_config", &Level::INFO));
    }

    #[test]
    fn an_unset_or_empty_variable_is_the_default() {
        for unset in [None, Some(""), Some("   ")] {
            let (default, unreadable) = filter(unset);

            assert_eq!(unreadable, None, "{unset:?}");
            assert!(default.would_enable("caixonho_core", &Level::INFO));
            assert!(!default.would_enable("aws_config", &Level::INFO));
        }
    }

    #[test]
    fn a_log_that_cannot_be_opened_leaves_the_application_running_and_says_so_once() {
        // "The log's location cannot be written to": the application starts
        // and works, and the absence of a log is not a failure of anything the
        // user was doing. What it must also not be is a complaint per event —
        // the failure repeats for every line the application would have
        // written, and a diagnostic that shouts at every event is worse than
        // one that is missing.
        let fixture = Fixture::new("unwritable");

        let log = RollingLog::open(&fixture.blocked());

        assert_eq!(log.problem(), Some(LogProblem::NotWritable));
        assert_eq!(log.path(), None);

        let (captured, ()) = recording(quiet(), || {
            // Through the real subscriber, so this is the path a running
            // application takes.
            tracing::subscriber::with_default(subscriber(log.clone(), quiet()), || {
                for attempt in 0..100 {
                    connection_opened(
                        ConnectionId(attempt),
                        "work",
                        SourceKind::Profile,
                        "eu-west-1",
                    );
                }
            });
        });

        assert!(
            captured.is_empty(),
            "the inner subscriber is the one under test"
        );
        assert!(
            log.failures() > 100,
            "every event, plus the open that started it, failed: {}",
            log.failures()
        );
        assert_eq!(
            log.announcements(),
            1,
            "said once, not once per event: {} times",
            log.announcements()
        );
    }

    /// The real thing, in this machine's own log location — run it with
    /// `cargo test -p caixonho-core -- --ignored`.
    ///
    /// Ignored because it takes the process-wide subscriber and appends to the
    /// machine's own log, which is not something an ordinary test run should
    /// do behind someone's back. It exists because every other test here
    /// proves one piece — where the directory is, that a file opens in it,
    /// what the filter admits, that the subscriber installs — and none of them
    /// proves that the four assembled produce a file with this application's
    /// events in it.
    #[test]
    #[ignore = "appends to this machine's own log directory"]
    fn the_log_this_machine_writes_holds_this_applications_events() {
        let diagnostics = start();

        assert_eq!(diagnostics.problem(), None);
        let file = diagnostics.file().expect("a log file").to_owned();
        assert_eq!(diagnostics.directory(), file.parent());
        connection_opened(
            ConnectionId(0),
            "caixonho-test-please-ignore",
            SourceKind::Profile,
            "ap-southeast-1",
        );

        let written = std::fs::read_to_string(&file).expect("what was written is readable");
        assert!(
            written.contains("connection opened")
                && written.contains("caixonho-test-please-ignore"),
            "the log at {} does not hold what was just logged:\n{written}",
            file.display()
        );
    }

    #[test]
    fn starting_the_log_twice_is_reported_rather_than_a_crash() {
        // A subscriber takes the process, once. A second `start` is a mistake
        // rather than a condition — but it happens at startup, where a panic
        // would take the application down over its diagnostic, which is the
        // one thing this module may never do.
        //
        // This is the only test here that touches the process-wide
        // subscriber; every other one overrides it on its own thread, which is
        // what `with_default` is for.
        let _ = install(Captured::default(), quiet());

        assert_eq!(
            install(Captured::default(), quiet()),
            Err(LogProblem::AlreadyStarted)
        );
    }

    #[test]
    fn where_the_log_is_is_something_a_frontend_can_be_told() {
        let fixture = Fixture::new("location");

        let log = RollingLog::open(&fixture.dir);

        let path = log.path().expect("a writable directory holds a log");
        assert_eq!(log.problem(), None);
        assert_eq!(path.parent(), Some(fixture.dir.as_path()));
        assert!(path.exists(), "{} was not created", path.display());
        assert!(
            path.file_name()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|name| name.starts_with(FILE_PREFIX) && name.ends_with(FILE_SUFFIX)),
            "a name a user asked to attach it can recognise: {}",
            path.display()
        );
    }

    #[test]
    fn the_log_lands_in_the_platforms_own_log_location() {
        // Resolved, not assembled: `directories` knows the Known Folder API on
        // Windows and this repository does not yet test on three platforms.
        let directory = log_directory().expect("this machine has a home directory");

        assert!(
            directory.ancestors().any(|ancestor| ancestor
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                == Some(APPLICATION)),
            "the application's own directory, not somebody else's: {}",
            directory.display()
        );
        if cfg!(target_os = "macos") {
            assert!(
                directory.ends_with(Path::new("Library/Logs").join(APPLICATION)),
                "macOS keeps logs where Console.app looks: {}",
                directory.display()
            );
        }
        if cfg!(target_os = "windows") {
            assert!(
                directory.ends_with(Path::new(APPLICATION).join("logs")),
                "{}",
                directory.display()
            );
        }
    }

    /// Push `lines` events through `log`, each big enough to matter against a
    /// small segment size.
    fn fill(log: &RollingLog, lines: usize) {
        let mut writer = log;
        for line in 0..lines {
            writeln!(
                writer,
                "{line:04}: a line of about fifty bytes, give or take"
            )
            .expect("this writer never fails");
        }
    }

    #[test]
    fn the_log_cannot_grow_without_limit_and_what_is_kept_is_the_most_recent() {
        // A session that fails over and over for a long time: the file rolls,
        // the old ones go, and what survives is the end of the story rather
        // than its beginning.
        let fixture = Fixture::new("bounded");
        let log = RollingLog::bounded_at(&fixture.dir, 200);

        fill(&log, 200);

        let kept = segments(&fixture.dir);
        assert_eq!(
            kept.len(),
            SEGMENTS_KEPT,
            "the directory holds {} files: {kept:?}",
            kept.len()
        );
        let current = log.path().expect("something is being written");
        assert_eq!(
            current.file_name().and_then(std::ffi::OsStr::to_str),
            kept.last().map(String::as_str),
            "the file being written is the newest one kept"
        );
        assert!(
            std::fs::read_to_string(&current)
                .expect("readable")
                .contains("0199:"),
            "and the most recent line is in it"
        );
        assert_eq!(log.failures(), 0, "none of that is a failure");
    }

    #[test]
    fn a_new_day_starts_a_new_file() {
        // Rolling by day is what makes a log navigable: "it broke on Tuesday"
        // is how the failure will be reported.
        let fixture = Fixture::new("new-day");
        let log = RollingLog::open(&fixture.dir);
        let today = log.path().expect("something is being written");

        // The machine's clock cannot be moved, so the log is put into the
        // state a session left open overnight is in: writing a file named for
        // a day that is no longer today.
        log.roll(&mut log.segment(), Day::at(-86_400), 0);
        let yesterday = log.path().expect("something is being written");
        {
            let mut segment = log.segment();
            let open = segment.file.as_mut().expect("yesterday's file is open");
            writeln!(open, "0000: written before midnight").expect("writable");
        }

        // The next event, after midnight.
        fill(&log, 1);

        assert_ne!(yesterday, today, "yesterday's file is named for yesterday");
        assert_eq!(
            log.path(),
            Some(today.clone()),
            "the day turned and the file did not"
        );
        assert_eq!(
            segments(&fixture.dir).len(),
            2,
            "and yesterday's is still there to be read"
        );
        assert!(
            std::fs::read_to_string(&yesterday)
                .expect("readable")
                .contains("0000:"),
            "what was written before midnight stays where it was written"
        );
    }

    #[test]
    fn a_relaunch_the_same_day_continues_the_same_file() {
        // Five launches in an afternoon are one story, and five files tell it
        // worse.
        let fixture = Fixture::new("relaunch");
        let first = RollingLog::open(&fixture.dir);
        fill(&first, 1);
        let path = first.path().expect("something is being written");
        drop(first);

        let second = RollingLog::open(&fixture.dir);
        fill(&second, 1);

        assert_eq!(second.path(), Some(path.clone()));
        let written = std::fs::read_to_string(&path).expect("readable");
        assert_eq!(
            written.lines().count(),
            2,
            "the second launch appended rather than replacing: {written}"
        );
    }

    #[test]
    fn a_day_is_the_civil_date_of_the_moment_it_names() {
        // The one piece of arithmetic in this module, and the one whose being
        // wrong would be invisible — a misnamed file still holds the right
        // lines. Dates that break the naive versions: an epoch boundary, a
        // leap day in a leap century, the day before one, and a moment before
        // the epoch, which is what an unset clock looks like.
        for (seconds, expected) in [
            (0, "1970-01-01"),
            (86_399, "1970-01-01"),
            (86_400, "1970-01-02"),
            (-1, "1969-12-31"),
            (951_782_400, "2000-02-29"),
            (951_868_800, "2000-03-01"),
            (1_709_164_800, "2024-02-29"),
            (1_767_225_600, "2026-01-01"),
            (1_772_323_200, "2026-03-01"),
            (4_102_444_800, "2100-01-01"),
        ] {
            assert_eq!(Day::at(seconds).to_string(), expected, "at {seconds}");
        }
    }

    #[test]
    fn a_segments_name_sorts_it_into_the_order_it_was_written() {
        // What `prune` relies on to keep the most recent without reading a
        // single modification time.
        let directory = Path::new("/nowhere");
        let mut names: Vec<String> = [
            (Day::at(86_400), 2),
            (Day::at(0), 10),
            (Day::at(0), 2),
            (Day::at(86_400), 0),
        ]
        .into_iter()
        .map(|(day, index)| {
            segment_path(directory, day, index)
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .expect("a name")
                .to_owned()
        })
        .collect();
        names.sort_unstable();

        assert_eq!(
            names,
            [
                "caixonho-1970-01-01.002.log",
                "caixonho-1970-01-01.010.log",
                "caixonho-1970-01-02.000.log",
                "caixonho-1970-01-02.002.log",
            ],
            "segment 10 sorting before segment 2 would prune the wrong file"
        );
    }
}
