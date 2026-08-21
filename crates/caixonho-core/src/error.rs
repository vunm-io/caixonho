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

/// Why a sign-in ended without a session.
///
/// Only the two outcomes that belong to the *attempt* live here. A provider
/// that could not be reached is [`Error::Network`] and a profile that does not
/// say where to sign in is [`Error::MissingConfiguration`] — both already mean
/// exactly that, and inventing sign-in-shaped copies of them would give the
/// same condition two spellings for no one's benefit (`xonho-0011` task 2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInProblem {
    /// The user was asked and said no.
    Declined,
    /// The attempt's own window closed before the user finished. Not a
    /// credential problem and not a refusal — the offer simply went stale,
    /// and the answer is another attempt.
    Expired,
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

/// Why the list of remembered connections could not be used.
///
/// A cause of its own, for the same reason a credential store failure is one:
/// a configuration file the user can repair is not a locked keychain and not
/// an IAM policy, and each is fixed by a different person in a different
/// place. None of them is ever reported as another.
///
/// Nothing here says anything about the credential store. Losing this file
/// loses no secret — it loses this application's knowledge that one exists,
/// which is why the connection it names may not simply be dropped in silence
/// (design.md, "A stored connection is remembered, or it should not be offered
/// at all").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionsProblem {
    /// The file is there and could not be read — permissions, a device
    /// failure, a directory where a file should be.
    Unreadable,
    /// The file was read and is not what this application writes. It is left
    /// exactly as it was: replacing a file we failed to parse would discard
    /// every connection in it to save the one being written.
    Malformed,
    /// The file could not be written, so the change was not remembered.
    NotWritable,
    /// This machine offers nowhere to keep it — no home directory, or no
    /// configuration directory at all.
    NoLocation,
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

    /// A sign-in attempt ended without producing a session.
    ///
    /// Distinct from [`Error::SessionRejected`], which is what a *credential*
    /// gets when it is refused: nothing was rejected here, the attempt just
    /// did not complete. Reporting one as the other would send someone to
    /// check an access key over a browser tab they closed.
    #[error("signing in to `{sso_session}` {}", match problem { SignInProblem::Declined => "was declined", SignInProblem::Expired => "expired before it was completed — try again" })]
    SignIn {
        /// The `sso_session` the attempt was for.
        sso_session: String,
        /// How the attempt ended.
        problem: SignInProblem,
    },

    /// The session was obtained and then could not be saved.
    ///
    /// Its own cause because it is its own situation: the sign-in worked, the
    /// user did nothing wrong, and nothing is broken except that none of it
    /// will survive. Reporting it as a sign-in failure would send someone
    /// back through a browser that is going to work again and change nothing.
    #[error("signed in, but the session could not be saved to `{path}` — {detail}")]
    TokenCacheNotWritable {
        /// The file that could not be written.
        path: String,
        /// What the filesystem said, in the crate's own words.
        detail: String,
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
    #[error("access denied — this operation requires {iam_action}")]
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

    /// The list of remembered connections could not be used.
    ///
    /// Distinct from [`Error::CredentialStore`] on purpose: that is the
    /// keychain, this is an ordinary configuration file, and the two fail for
    /// different reasons and are repaired by different means. Carries no
    /// contents from the file — there is no secret in it to disclose, and
    /// echoing a file back at the user is not what tells them what to do.
    ///
    /// The file is never replaced on the strength of a failed read. The
    /// connections in it are the user's, and discarding them to save the one
    /// being written is the failure this cause exists to make impossible.
    #[error("the list of remembered connections{}{}", path.as_ref().map(|path| format!(" at `{}`", path.display())).unwrap_or_default(), match problem { ConnectionsProblem::Unreadable => " could not be read", ConnectionsProblem::Malformed => " is not in a form caixonho can read — repair it or remove it; no secret is kept in it", ConnectionsProblem::NotWritable => " could not be written, so this change was not remembered", ConnectionsProblem::NoLocation => " has nowhere to live on this machine" })]
    Connections {
        /// What went wrong with it.
        problem: ConnectionsProblem,
        /// Where the file is, when this machine has a place for one — so the
        /// user asked to repair it is told which file to open.
        path: Option<std::path::PathBuf>,
    },

    /// The endpoint does not implement what was asked of it.
    ///
    /// Its own cause, and not [`Self::Unexpected`], because it has a precise
    /// meaning and a precise remedy: this service is not that service. S3 is a
    /// protocol several vendors implement to differing extents, so a client
    /// that reports "something unexpected happened" when told plainly "I do
    /// not implement that" is failing at the one thing this project promises —
    /// telling causes apart.
    #[error("{endpoint} does not implement {operation}")]
    NotImplemented {
        /// What was asked for, as the service named it.
        operation: String,
        /// Which endpoint said so.
        endpoint: String,
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
