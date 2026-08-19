//! The crate's error type: one variant per cause a user actually needs to
//! tell apart (`openspec` change `thung-0002`, `connections` spec).
//!
//! Two rules bind everything here:
//!
//! - **Causes users confuse stay separate variants.** An expired SSO session,
//!   a TLS interception proxy, a dead network and a real policy denial all
//!   surface as "can't list" — the whole point of this enum is that the UI can
//!   offer the matching next action for each without parsing strings.
//! - **No credential material, ever.** Variants carry profile names, endpoint
//!   hosts and classifier-authored detail strings — never keys, tokens or raw
//!   wire payloads. A test in the classifier module enforces this.

/// Why a connection or a call failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No credentials could be found for the selected profile.
    #[error("no credentials found for profile `{profile}`")]
    NoCredentials {
        /// The profile that failed to resolve.
        profile: String,
    },

    /// A cached session or SSO token exists but has expired or is invalid.
    /// Distinct from [`Error::AccessDenied`]: the fix is signing in again,
    /// not changing IAM policy.
    #[error("session for profile `{profile}` has expired — sign in again{}", sso_session.as_deref().map(|s| format!(" (SSO session `{s}`)")).unwrap_or_default())]
    ExpiredSession {
        /// The profile whose session expired.
        profile: String,
        /// The `sso_session` name from the shared config, when the profile
        /// has one — the thing `aws sso login --sso-session <name>` needs.
        sso_session: Option<String>,
    },

    /// The presented certificate chain is not trusted. Classified before the
    /// credential cases because the underlying messages overlap.
    #[error(
        "certificate chain for `{endpoint}` is not trusted — check the OS trust store or `AWS_CA_BUNDLE`"
    )]
    TlsTrust {
        /// Host we were talking to when trust verification failed.
        endpoint: String,
    },

    /// The endpoint could not be reached at all. The matching next action is
    /// a retry, never a credential fix.
    #[error("network failure: {detail}")]
    Network {
        /// Classifier-authored description (DNS, connect, timeout, ...).
        detail: String,
    },

    /// The service itself denied the request on authorization grounds.
    /// Only the adapter's classifier may construct this, and only for a real
    /// service-side denial.
    #[error("access denied — this operation requires `{iam_action}`")]
    AccessDenied {
        /// The IAM action the caller would need, e.g. `s3:ListAllMyBuckets`.
        iam_action: &'static str,
    },

    /// The profile or environment is incomplete — e.g. no region configured.
    /// A configuration fix, not an authentication problem.
    #[error("configuration problem{}: {detail}", profile.as_deref().map(|p| format!(" with profile `{p}`")).unwrap_or_default())]
    MissingConfiguration {
        /// The profile whose configuration is incomplete, when the
        /// problem belongs to one — trust material comes from the
        /// environment and belongs to no profile.
        profile: Option<String>,
        /// What exactly is missing or malformed.
        detail: String,
    },

    /// Anything the classifier could not attribute to a specific cause.
    /// Deliberately last: growth here is a signal the classifier needs work.
    #[error("unexpected error: {detail}")]
    Unexpected {
        /// Classifier-authored description of what the service reported.
        detail: String,
    },
}

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;
