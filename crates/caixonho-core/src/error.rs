//! The crate's error type: one variant per cause a user actually needs to
//! tell apart (`openspec` change `xonho-0003`, `connections` spec).
//!
//! Two rules bind everything here:
//!
//! - **Causes users confuse stay separate variants.** An expired SSO session,
//!   a TLS interception proxy, a dead network and a real policy denial all
//!   surface as "can't list" — the whole point of this enum is that the UI can
//!   offer the matching next action for each without parsing strings.
//! - **No credential material, ever.** Variants carry profile and connection
//!   names, endpoint hosts and authored detail strings — never keys, tokens
//!   or raw payloads, whether those came off the wire or out of the operating
//!   system's credential store. A test in the classifier module and one in
//!   the credentials module enforce this from both ends.

/// Why credentials were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionProblem {
    /// They were valid once and their lifetime ran out.
    Expired,
    /// The service does not recognise them, or the signature did not match.
    Invalid,
}

/// Why the operating system's credential store could not be used.
///
/// A cause of its own, never folded into a generic failure and never into an
/// access denial: a locked keychain and an IAM policy are fixed by different
/// people in different places, and a store that will not open says nothing at
/// all about what the credentials inside it may do (`stored-credentials`
/// spec, "The credential store may be unavailable").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStoreProblem {
    /// The store exists and will not open until the user unlocks it.
    Locked,
    /// The store was reached and did not carry out the request — a prompt the
    /// user declined, a platform failure, or an entry it will not hand back.
    Refused,
    /// There is no credential store on this machine to use at all.
    Absent,
}

/// Why a connection or a call failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No credentials could be found for the selected profile.
    #[error("no credentials found for profile `{profile}`")]
    NoCredentials {
        /// The profile that failed to resolve.
        profile: String,
    },

    /// Credentials were presented and the service refused them. Distinct
    /// from [`Error::AccessDenied`] on purpose: the fix is new credentials,
    /// not a new IAM policy, and confusing the two sends people to edit
    /// policy documents over a mistyped access key.
    #[error("credentials for profile `{profile}` {}{}", match problem { SessionProblem::Expired => "have expired — sign in again", SessionProblem::Invalid => "were rejected as invalid — check the access key, or sign in again" }, sso_session.as_deref().map(|s| format!(" (SSO session `{s}`)")).unwrap_or_default())]
    SessionRejected {
        /// The profile whose credentials were refused.
        profile: String,
        /// The `sso_session` name from the shared config, when the profile
        /// has one — the thing `aws sso login --sso-session <name>` needs.
        sso_session: Option<String>,
        /// Why they were refused.
        problem: SessionProblem,
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

    /// The operating system's credential store could not be used.
    ///
    /// Carries no detail from the store's own error on purpose. `keyring`
    /// reports a payload it could not decode by attaching the payload, and
    /// here that payload *is* the secret — so only the kind of failure
    /// crosses this boundary, exactly as the classifier reads an SDK chain
    /// without echoing it.
    ///
    /// Nothing is written anywhere else when this happens. There is no second
    /// place for a secret to go, least of all the AWS shared credentials file
    /// (`stored-credentials` spec).
    #[error("the credential store {} (connection `{connection}`)", match problem { CredentialStoreProblem::Locked => "is locked — unlock it and try again", CredentialStoreProblem::Refused => "refused the request — allow caixonho to use it and try again", CredentialStoreProblem::Absent => "is not available on this machine" })]
    CredentialStore {
        /// The connection whose secret was being saved, read or removed.
        connection: String,
        /// What the store did.
        problem: CredentialStoreProblem,
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
