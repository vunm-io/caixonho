//! # caixonho-core
//!
//! The engine behind caixonho, a fast, native, cross-platform S3 client.
//! From Vietnamese *"cái xô nhỏ"* — the little pail.
//!
//! This crate owns **all** product logic: credential and profile resolution,
//! the SSO device flow, client construction, listing and pagination, the
//! observed-capability model, the transfer engine, and object operations.
//! The GUI and (later) CLI crates are thin frontends over this one.
//!
//! ## Hard rules
//!
//! - **No UI dependencies.** Nothing in here may depend on `gpui` or any
//!   rendering concern.
//! - **No AWS SDK types in the public API.** Frontends consume domain types
//!   re-exported from here, so the UI stays swappable and the core stays
//!   reusable (see `docs/PROJECT_BRIEF.md` §6).
//! - **Everything long-running is cancellable** and reports progress over a
//!   stream or channel.
//! - **Errors stay structured.** Rich error enums (`thiserror`), never
//!   stringified early — the permission-awareness feature depends on it.
//! - **Secrets reach the OS credential store and nothing else.** Never a
//!   configuration file, never a log, never an error message — and never
//!   `~/.aws/credentials`, which belongs to every other AWS tool on the
//!   machine. Structurally, not by discipline: see `credentials` for the type
//!   that cannot print itself, and `diagnostics` for the recording vocabulary
//!   no secret fits into.
//! - **TDD.** This crate is built test-first; the UI may be exploratory, the
//!   core may not.
//!
//! ## The `test-support` feature
//!
//! Off by default, and not part of what this crate ships. Turning it on
//! exposes exactly two of the doubles this crate already uses for its own
//! tests: [`store::double::StoreDouble`], and [`Session::install_object_store`]
//! to put one where a real connection's S3 adapter would go. It also adds
//! [`Diagnostics::without_a_log`], because a frontend cannot construct its
//! window without a diagnostics handle and a test has no log to name.
//!
//! Deliberately only those two. The keychain and connections-file injectors
//! stay `pub(crate)`: a frontend is handed the connections it should show
//! rather than reading them itself, so it has no use for them — and exposing
//! them would have leaked two `pub(crate)` traits into the public API, which
//! is how a crate acquires a surface nobody chose.
//!
//! It exists so a frontend can build a window over a session that reads from
//! a script rather than from the machine's `~/.aws` and its keychain — a test
//! that reads the developer's own machine answers differently on every machine
//! that runs it. `caixonho-gui` enables it under `[dev-dependencies]`;
//! resolver 2 keeps it out of any build without dev targets, which was
//! measured rather than assumed.
//!
//! A feature rather than plain `pub` on purpose. A test-only API on the
//! shipped surface is one nobody dares remove later, and the close-out review
//! in `AGENTS.md` exists to catch exactly that.

pub mod adapter;
pub mod capability;
pub(crate) mod classify;
pub mod connection;
pub(crate) mod connections;
pub mod credentials;
pub mod diagnostics;
pub mod error;
pub mod folder;
pub(crate) mod listing;
pub mod outcome;
mod preferences;
pub mod preview;
pub mod probe;
pub mod profiles;
pub mod queue;
pub mod session;
pub mod sso;
pub mod sso_adapter;
pub mod store;
pub(crate) mod tls;
pub mod transfer;
pub mod types;

pub use adapter::{LIST_BUCKET_ACTION, S3ObjectStore};
pub use capability::{Capability, CapabilityStore, CredentialsId, Observation, Scope};
pub use connection::{Connection, ConnectionSource};
pub use credentials::{CredentialSecret, StoredCredential};
pub use diagnostics::{Diagnostics, LOG_LEVEL_ENV, LogProblem};
pub use error::{
    ConnectionsProblem, CredentialStoreProblem, Error, Result, SessionProblem, SignInProblem,
};
pub use outcome::{ActiveOutcome, Outcome, TaggedOutcome};
pub use probe::{IN_FLIGHT_BUDGET, ProbeTarget};
pub use profiles::{ConfigPaths, discover, sign_in_location, sso_session};
pub use session::Session;
pub use sso::{
    Abandon, ClientRegistration, DeviceAuthorization, ObtainedSession, RealTime, SignInLocation,
    SignInOutcome, SignInSecret, SsoSignIn, SsoToken, TokenAnswer,
};
pub use sso_adapter::SsoOidcSignIn;
pub use store::ObjectStore;
pub use tls::HttpStack;
pub use types::{
    AccountListing, Bucket, BucketKind, ConnectionId, Cursor, Folder, KeyPage, Location, Object,
    Page, Prefix, Profile, RefusedListing, Region, RegionChoice, region_choices,
};
