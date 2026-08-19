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
//! - **TDD.** This crate is built test-first; the UI may be exploratory, the
//!   core may not.

pub mod capability;
pub mod connection;
pub mod error;
pub mod profiles;
pub mod store;
pub(crate) mod tls;
pub mod types;

pub use connection::Connection;
pub use error::{Error, Result};
pub use profiles::{ConfigPaths, discover};
pub use store::ObjectStore;
pub use tls::HttpStack;
pub use types::{Bucket, ConnectionId, Profile, Region};
