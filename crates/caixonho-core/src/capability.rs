//! The observed-capability model.
//!
//! S3 has no API that enumerates a caller's effective permissions, so
//! caixonho never *declares* what the user can do — it records what has been
//! *observed*, one operation at a time, and is honest about everything else.
//!
//! This module is the seed of that model. It will grow probing, caching and
//! invalidation in M1; the invariant that must survive is the three-valued
//! logic below: `Unknown` is the default, and only evidence moves a
//! capability out of it.

/// What we currently know about one operation on one scope
/// (a bucket or a prefix), for one set of credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Observation {
    /// No evidence yet. This is the default and must render as "unknown",
    /// never as "denied".
    #[default]
    Unknown,
    /// A probe or a real operation succeeded.
    Allowed,
    /// A real `AccessDenied` was observed. Only an access-denial error maps
    /// here — expired tokens, wrong regions, network failures and missing
    /// buckets are different states and must never be folded into this one.
    Denied,
}

/// The set of capabilities caixonho tracks per scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capability {
    pub list: Observation,
    pub read: Observation,
    /// Write is special: it is never probed automatically (a write probe
    /// creates an object). It moves out of `Unknown` only through a real
    /// user-initiated operation.
    pub write: Observation,
    pub delete: Observation,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_defaults_to_unknown_everywhere() {
        let cap = Capability::default();
        assert_eq!(cap.list, Observation::Unknown);
        assert_eq!(cap.read, Observation::Unknown);
        assert_eq!(cap.write, Observation::Unknown);
        assert_eq!(cap.delete, Observation::Unknown);
    }
}
