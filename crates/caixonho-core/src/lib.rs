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

pub mod adapter;
pub mod capability;
pub(crate) mod classify;
pub mod connection;
pub(crate) mod connections;
pub mod credentials;
pub mod diagnostics;
pub mod error;
pub(crate) mod listing;
pub mod outcome;
pub mod probe;
pub mod profiles;
pub mod session;
pub mod store;
pub(crate) mod tls;
pub mod types;

pub use adapter::{LIST_BUCKET_ACTION, S3ObjectStore};
pub use capability::{Capability, CapabilityStore, CredentialsId, Observation, Scope};
pub use connection::{Connection, ConnectionSource};
pub use credentials::{CredentialSecret, StoredCredential};
pub use diagnostics::{Diagnostics, LOG_LEVEL_ENV, LogProblem};
pub use error::{ConnectionsProblem, CredentialStoreProblem, Error, Result, SessionProblem};
pub use outcome::{ActiveOutcome, Outcome, TaggedOutcome};
pub use probe::{IN_FLIGHT_BUDGET, ProbeTarget};
pub use profiles::{ConfigPaths, discover, sso_session};
pub use session::Session;
pub use store::ObjectStore;
pub use tls::HttpStack;
pub use types::{
    Bucket, ConnectionId, Cursor, Folder, Location, Object, Page, Prefix, Profile, Region,
    RegionChoice, region_choices,
};
