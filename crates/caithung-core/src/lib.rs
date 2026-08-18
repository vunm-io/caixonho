//! # caithung-core
//!
//! The engine behind caithung, a fast, native, cross-platform S3 client.
//! From Vietnamese *"cái thùng"* — the bucket.
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
