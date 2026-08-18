//! Domain types crossing the core↔frontend boundary.
//!
//! Hard rule (crate invariant): no `aws-sdk-s3` type appears in any public
//! signature — frontends consume these types only, so the UI stays swappable
//! and the core reusable by the future CLI.

/// A connection profile discovered in the AWS shared config files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// The profile's name as written in the config file (`default`, or the
    /// name inside `[profile <name>]`).
    pub name: String,
    /// Whether this is the `default` profile.
    pub is_default: bool,
}

/// Identifies one opened connection.
///
/// Every request outcome is tagged with the id it belongs to, so a late
/// response from a previous profile is dropped instead of rendering as if it
/// belonged to the new one (design: messages, not shared state).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u64);

/// A bucket's region, honest about not knowing.
///
/// `ListBuckets` does not return regions; a later slice resolves them per
/// bucket. Until then a bucket is `Unknown`, never a guessed default —
/// the spec makes "unknown" a first-class display value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Region {
    /// The region is known, e.g. `ap-southeast-1`.
    Known(String),
    /// Not determined yet.
    Unknown,
}

/// One bucket as the domain sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket {
    /// The bucket name.
    pub name: String,
    /// Creation timestamp formatted as RFC 3339, when the service reported
    /// one. A `String` on purpose: display needs no date arithmetic, and a
    /// date dependency in core is not warranted by rendering alone.
    pub created: Option<String>,
    /// Where the bucket lives, when known.
    pub region: Region,
}
