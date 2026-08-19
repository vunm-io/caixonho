//! One place where an SDK failure becomes a cause the user can act on.
//!
//! Nothing above this module ever sees an `SdkError`: the adapter classifies
//! at the boundary, because that is the only place holding both the SDK's
//! error detail and the domain vocabulary.
//!
//! ## Why the order is part of the contract
//!
//! The underlying texts overlap, so precedence decides the answer:
//!
//! - rustls reports an untrusted chain as `invalid peer certificate: Expired`
//!   — which matches an expired-*session* search just as well as a
//!   certificate one. TLS trust is therefore checked first.
//! - AWS answers both a policy denial and an expired token with HTTP 403.
//!   Expired sessions are therefore checked before access denied, so
//!   "sign in again" never arrives dressed as "ask for permissions".
//!
//! Order: TLS trust → network → expired session → access denied → no
//! credentials → missing configuration → unexpected. The spec fixes six of
//! those; "no credentials" is a refinement of the same credential-resolution
//! family as missing configuration and sits directly before it.
//!
//! ## Why the chain is read but never echoed
//!
//! Matching needs the whole source chain, and for one family it is the only
//! evidence there is: when the *credential provider* fails, the S3 call
//! never leaves, so the SDK reports a dispatch failure carrying neither a
//! service error code nor an HTTP status (see `REJECTED_SESSION_MARKERS`).
//! The chain includes text the SDK assembled from a wire response — which
//! is exactly where credential material could hide. Copying it into an
//! `Error` would be the one way a secret reaches a log or the UI, so every
//! message here is authored from a bounded set of extracted facts (failure
//! kind, service error code, HTTP status) and the chain is used for matching
//! only (`connections` spec, "Credentials are never disclosed").

use aws_sdk_s3::config::http::HttpResponse;
use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};

use crate::error::{Error, SessionProblem};

/// What the SDK was doing when it failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureKind {
    /// The client could not even be built — configuration, not transport.
    Construction,
    /// The whole operation timed out.
    Timeout,
    /// The request never reached the service.
    Dispatch,
    /// The service answered something unparseable.
    Response,
    /// The service answered with an error.
    Service,
    /// A variant the SDK added after this was written.
    Other,
}

/// The facts a failure offers, extracted once so classification is a pure
/// function over plain data.
#[derive(Debug, Clone)]
pub(crate) struct SdkFailure {
    kind: FailureKind,
    code: Option<String>,
    status: Option<u16>,
    io: bool,
    timeout: bool,
    /// Lowercased source chain. Private on purpose: matching only, never
    /// copied into an `Error`.
    chain: String,
}

/// What the call was, so a cause can name the thing the user must fix.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CallContext<'a> {
    /// Profile the connection was opened for.
    pub profile: &'a str,
    /// Host we were talking to, for trust failures.
    pub endpoint: &'a str,
    /// IAM action this operation needs, for denials.
    pub iam_action: &'static str,
    /// `sso_session` name from the shared config, when the profile has one.
    pub sso_session: Option<&'a str>,
}

/// Certificate-trust failures, in the spellings rustls, OpenSSL and the
/// platform verifiers actually produce.
const TLS_MARKERS: &[&str] = &[
    "invalid peer certificate",
    "certificate verify failed",
    "unknown issuer",
    "unknownissuer",
    "unable to get local issuer",
    "self-signed certificate",
    "self signed certificate",
    "causedasendentity",
    "notvalidforname",
    "badcertificate",
    "certificate signature",
    "invalid certificate",
];

/// The endpoint could not be reached, or answered too late.
const NETWORK_MARKERS: &[&str] = &[
    "dns error",
    "failed to lookup address",
    "name or service not known",
    "connection refused",
    "connection reset",
    "connection closed",
    "no route to host",
    "network is unreachable",
    "broken pipe",
    "timed out",
];

/// Service error codes that mean "sign in again", not "ask for permission".
const EXPIRED_CODES: &[&str] = &[
    "expiredtoken",
    "expiredtokenexception",
    "requestexpired",
    "tokenrefreshrequired",
];

/// The same condition as it appears in credential-provider text.
const EXPIRED_MARKERS: &[&str] = &[
    "session associated with this profile has expired",
    "security token included in the request is expired",
    "token has expired",
    "session has expired",
    "expired credentials",
];

/// The same condition once more, in the wording it wears when the thing
/// refused was the *session* rather than a request signed with it.
///
/// IAM Identity Center answers `sso:GetRoleCredentials` with
/// `UnauthorizedException: Session token not found or invalid` once the
/// cached SSO token stops being good — observed against a live account on
/// 2026-08-19. That failure happens while the provider chain is resolving
/// credentials for an S3 call the SDK then never sends, so it arrives as
/// `SdkError::DispatchFailure`: no service error code, no HTTP status, and
/// nothing but the chain to read. Every code-based rule above is blind to
/// it, and before this list the whole family fell through to
/// `Error::Unexpected` — the app telling its owner it had no idea, about
/// the one failure a laptop hits every morning.
///
/// Matched on the service's message, not on `UnauthorizedException`: that
/// exception is also how this operation reports a request it will not serve
/// for other reasons, and "session token not found or invalid" is the part
/// that says a session is the thing to re-establish.
const REJECTED_SESSION_MARKERS: &[&str] = &["session token not found or invalid"];

/// Codes that mean credentials were presented and refused as invalid.
///
/// Verified against the live service on 2026-08-19: S3 answers a bogus access
/// key with `InvalidAccessKeyId` and **HTTP 403** — the same status a policy
/// denial carries, which is why status alone must never decide this.
const INVALID_CREDENTIAL_CODES: &[&str] = &[
    "invalidaccesskeyid",
    "signaturedoesnotmatch",
    "invalidclienttokenid",
    "unrecognizedclientexception",
    "invalidtoken",
];

/// Codes that mean the service refused on authorization grounds.
const DENIED_CODES: &[&str] = &[
    "accessdenied",
    "accessdeniedexception",
    "unauthorizedoperation",
];

/// The provider chain found nothing to sign with.
const NO_CREDENTIALS_MARKERS: &[&str] = &[
    "no credentials",
    "credentials not loaded",
    "credentialsnotloaded",
    "unable to load credentials",
    "couldn't load credentials",
    "no providers in chain provided credentials",
];

/// Configuration that is absent or unusable.
const CONFIGURATION_MARKERS: &[&str] = &[
    "no region",
    "region must be set",
    "region is required",
    "profile not found",
    "no such profile",
];

impl SdkFailure {
    /// Extract the classifiable facts from an SDK error.
    pub(crate) fn from_sdk<E>(error: &SdkError<E, HttpResponse>) -> Self
    where
        E: ProvideErrorMetadata + std::error::Error + 'static,
    {
        let mut failure = match error {
            SdkError::ConstructionFailure(_) => Self::new(FailureKind::Construction),
            SdkError::TimeoutError(_) => Self::new(FailureKind::Timeout).timed_out(),
            SdkError::DispatchFailure(dispatch) => {
                let mut failure = Self::new(FailureKind::Dispatch);
                if dispatch.is_io() {
                    failure = failure.io_error();
                }
                if dispatch.is_timeout() {
                    failure = failure.timed_out();
                }
                failure
            }
            SdkError::ResponseError(_) => Self::new(FailureKind::Response),
            SdkError::ServiceError(context) => {
                let mut failure =
                    Self::new(FailureKind::Service).with_status(context.raw().status().as_u16());
                if let Some(code) = context.err().code() {
                    failure = failure.with_code(code);
                }
                failure
            }
            _ => Self::new(FailureKind::Other),
        };
        failure.chain = source_chain(error);
        failure
    }

    fn new(kind: FailureKind) -> Self {
        Self {
            kind,
            code: None,
            status: None,
            io: false,
            timeout: false,
            chain: String::new(),
        }
    }

    fn with_code(mut self, code: &str) -> Self {
        self.code = Some(code.to_owned());
        self
    }

    fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    /// Only the tests build a chain by hand; `from_sdk` extracts it.
    #[cfg(test)]
    fn with_text(mut self, text: &str) -> Self {
        self.chain = text.to_lowercase();
        self
    }

    fn io_error(mut self) -> Self {
        self.io = true;
        self
    }

    fn timed_out(mut self) -> Self {
        self.timeout = true;
        self
    }

    fn chain_has(&self, markers: &[&str]) -> bool {
        markers.iter().any(|marker| self.chain.contains(marker))
    }

    fn code_is(&self, codes: &[&str]) -> bool {
        self.code
            .as_deref()
            .map(str::to_lowercase)
            .is_some_and(|code| codes.contains(&code.as_str()))
    }
}

/// Turn an extracted failure into the cause the user must act on.
///
/// The order of these checks is the contract — see the module docs.
pub(crate) fn classify(failure: &SdkFailure, call: &CallContext<'_>) -> Error {
    // 1. Trust first: its text overlaps with everything below it.
    if failure.chain_has(TLS_MARKERS) {
        return Error::TlsTrust {
            endpoint: call.endpoint.to_owned(),
        };
    }

    // 2. Unreachable or too slow — never a credential problem.
    if failure.timeout || failure.io || failure.chain_has(NETWORK_MARKERS) {
        return Error::Network {
            detail: if failure.timeout {
                "the endpoint did not answer in time".to_owned()
            } else {
                "the endpoint could not be reached".to_owned()
            },
        };
    }

    // 3. Credentials the service refused, before any denial check: AWS
    //    answers 403 to an expired token, an invalid key and a policy denial
    //    alike, so status cannot be the thing that decides.
    //
    //    `REJECTED_SESSION_MARKERS` is read here rather than in a step of
    //    its own, and reported as `Expired` rather than `Invalid`: a session
    //    the issuer no longer accepts — aged out, revoked, or gone from the
    //    cache — is a session the user must establish again, which is what
    //    `Expired` says and what the GUI turns into `aws sso login`.
    //    `Invalid` would tell a profile that has no access key to go and
    //    check its access key.
    let problem = if failure.code_is(EXPIRED_CODES)
        || failure.chain_has(EXPIRED_MARKERS)
        || failure.chain_has(REJECTED_SESSION_MARKERS)
    {
        Some(SessionProblem::Expired)
    } else if failure.code_is(INVALID_CREDENTIAL_CODES) {
        Some(SessionProblem::Invalid)
    } else {
        None
    };
    if let Some(problem) = problem {
        return Error::SessionRejected {
            profile: call.profile.to_owned(),
            sso_session: call.sso_session.map(ToOwned::to_owned),
            problem,
        };
    }

    // 4. A real authorization refusal, and only that. The code has to say so:
    //    a bare 403 is not evidence of an authorization decision, and
    //    guessing one sends the user to edit IAM policy over a wrong key.
    if failure.code_is(DENIED_CODES) {
        return Error::AccessDenied {
            iam_action: call.iam_action,
        };
    }

    // 5. Nothing to sign with.
    if failure.chain_has(NO_CREDENTIALS_MARKERS) {
        return Error::NoCredentials {
            profile: call.profile.to_owned(),
        };
    }

    // 6. Present but incomplete configuration.
    if failure.chain_has(CONFIGURATION_MARKERS) || failure.kind == FailureKind::Construction {
        return Error::MissingConfiguration {
            profile: Some(call.profile.to_owned()),
            detail: "the profile or environment is incomplete for this call".to_owned(),
        };
    }

    // 7. Unattributed. Growth here means the classifier needs work, so the
    //    detail carries the two facts that make the next case diagnosable.
    Error::Unexpected {
        detail: match (failure.code.as_deref(), failure.status) {
            (Some(code), Some(status)) => format!("the service reported `{code}` (HTTP {status})"),
            (Some(code), None) => format!("the service reported `{code}`"),
            (None, Some(status)) => format!("the service answered HTTP {status}"),
            (None, None) => "the call failed without a reportable cause".to_owned(),
        },
    }
}

/// Every `Display` in the error's source chain, lowercased for matching.
fn source_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut chain = String::new();
    let mut current = Some(error);
    while let Some(error) = current {
        chain.push_str(&error.to_string());
        chain.push('\n');
        current = error.source();
    }
    chain.to_lowercase()
}

#[cfg(test)]
mod tests {
    //! `connections` spec, "Failure causes are distinguished" and
    //! "Credentials are never disclosed". Every ordering hazard the spec
    //! names has its own test, because the order is the part a future
    //! refactor can break silently.

    use super::*;

    fn call() -> CallContext<'static> {
        CallContext {
            profile: "work",
            endpoint: "s3.ap-southeast-1.amazonaws.com",
            iam_action: "s3:ListAllMyBuckets",
            sso_session: Some("corp"),
        }
    }

    #[test]
    fn an_untrusted_chain_is_a_trust_failure_even_though_it_says_expired() {
        // rustls' own wording for an untrusted chain. The word "expired"
        // here is about the certificate, not the session.
        let failure = SdkFailure::new(FailureKind::Dispatch)
            .with_text("error trying to connect: invalid peer certificate: Expired");

        match classify(&failure, &call()) {
            Error::TlsTrust { endpoint } => {
                assert_eq!(endpoint, "s3.ap-southeast-1.amazonaws.com");
            }
            other => panic!("expected TlsTrust, got {other:?}"),
        }
    }

    #[test]
    fn an_interception_proxy_is_a_trust_failure_not_a_credential_problem() {
        let failure = SdkFailure::new(FailureKind::Dispatch)
            .io_error()
            .with_text("invalid peer certificate: UnknownIssuer");

        assert!(matches!(
            classify(&failure, &call()),
            Error::TlsTrust { .. }
        ));
    }

    #[test]
    fn an_invalid_key_is_a_credential_problem_not_a_policy_denial() {
        // The live service's actual answer to a bogus key (verified
        // 2026-08-19): same 403 a denial carries.
        let failure = SdkFailure::new(FailureKind::Service)
            .with_code("InvalidAccessKeyId")
            .with_status(403)
            .with_text("the aws access key id you provided does not exist in our records");

        match classify(&failure, &call()) {
            Error::SessionRejected { problem, .. } => {
                assert_eq!(problem, SessionProblem::Invalid);
            }
            other => panic!("expected SessionRejected/Invalid, got {other:?}"),
        }
    }

    #[test]
    fn a_bare_403_without_a_denial_code_is_not_called_a_denial() {
        let failure = SdkFailure::new(FailureKind::Service).with_status(403);

        assert!(
            !matches!(classify(&failure, &call()), Error::AccessDenied { .. }),
            "a status alone must not be read as an authorization decision"
        );
    }

    #[test]
    fn an_expired_session_is_never_reported_as_access_denied() {
        // AWS answers 403 to an expired token and to a policy denial alike.
        let failure = SdkFailure::new(FailureKind::Service)
            .with_code("ExpiredToken")
            .with_status(403)
            .with_text("the security token included in the request is expired");

        match classify(&failure, &call()) {
            Error::SessionRejected {
                profile,
                sso_session,
                problem,
            } => {
                assert_eq!(profile, "work");
                assert_eq!(sso_session.as_deref(), Some("corp"));
                assert_eq!(problem, SessionProblem::Expired);
            }
            other => panic!("expected SessionRejected, got {other:?}"),
        }
    }

    /// The chain a rejected SSO session actually produced, captured from a
    /// live account on 2026-08-19.
    ///
    /// One `Display` per link, newline-joined — the shape [`source_chain`]
    /// builds. (A report rendered for a human prefixes each link with
    /// "caused by:"; those words are the reporter's, not the errors'.) It
    /// arrives as a *dispatch* failure because the S3 request never left:
    /// the provider chain failed while resolving credentials for it. So
    /// there is no service error code and no HTTP status here — the chain is
    /// the only evidence there is.
    const REJECTED_SSO_CHAIN: &str = "dispatch failure\n\
         other\n\
         an error occurred while loading credentials\n\
         an error occurred while loading credentials\n\
         service error\n\
         UnauthorizedException: Session token not found or invalid\n\
         UnauthorizedException: Session token not found or invalid";

    #[test]
    fn a_session_the_service_no_longer_accepts_is_a_session_problem_not_an_unexplained_failure() {
        // The defect this test exists for: the app told the owner
        // "unexpected error: the call failed without a reportable cause"
        // for a profile whose SSO session had simply gone stale. Nothing
        // was unexplained about it — the cause was in the chain, and the
        // classifier was only looking at a code that this path never
        // carries (`connections` spec, "Credential resolution", scenario
        // "SSO profile with an expired cached token").
        let failure = SdkFailure::new(FailureKind::Dispatch).with_text(REJECTED_SSO_CHAIN);

        match classify(&failure, &call()) {
            Error::SessionRejected {
                profile,
                sso_session,
                problem,
            } => {
                assert_eq!(profile, "work");
                assert_eq!(
                    sso_session.as_deref(),
                    Some("corp"),
                    "the message has to name the session `aws sso login --sso-session` needs"
                );
                // `Expired`, not `Invalid`, for a token reported as "not
                // found or invalid": both mean the user must establish the
                // session again, and only `Expired` says that. `Invalid`
                // asks a profile without an `sso_session` to check its
                // access key and secret — advice an SSO profile has no key
                // to act on.
                assert_eq!(problem, SessionProblem::Expired);
            }
            other => panic!("expected SessionRejected, got {other:?}"),
        }
    }

    #[test]
    fn a_credential_failure_that_names_no_session_is_not_reported_as_a_session_problem() {
        // The other half of the rule above: the branch reads the session
        // out of the chain, so it must key on the session and not on the
        // credential-loading wrapper that carries it. Every credential
        // failure in existence goes through that wrapper — an unreadable
        // `credential_process`, a broken role chain, a malformed cache
        // file — and none of those is fixed by signing in again.
        let failure = SdkFailure::new(FailureKind::Dispatch).with_text(
            "dispatch failure\n\
             other\n\
             an error occurred while loading credentials\n\
             an error occurred while loading credentials\n\
             error running the credential_process command",
        );

        assert!(
            !matches!(classify(&failure, &call()), Error::SessionRejected { .. }),
            "a credential failure saying nothing about a session must not send the \
             user to sign in again"
        );
    }

    #[test]
    fn a_rejected_session_never_outranks_a_trust_or_network_failure() {
        // Placement guard for the branch above. It sits inside the
        // expired-session step, which the spec puts *after* trust and
        // network ("A TLS trust failure SHALL be classified before the
        // expired-session case"; a network failure "does not report a
        // credential problem"). Both fixtures carry the session wording as
        // well, so a branch that moved earlier would show up here as a
        // trust or network failure re-labelled "sign in again" — which is
        // the user rebuilding their trust store or their wifi via the AWS
        // CLI.
        let untrusted = SdkFailure::new(FailureKind::Dispatch).with_text(&format!(
            "invalid peer certificate: UnknownIssuer\n{REJECTED_SSO_CHAIN}"
        ));
        let unreachable = SdkFailure::new(FailureKind::Dispatch)
            .io_error()
            .with_text(&format!(
                "error trying to connect: dns error: failed to lookup address information\n\
                 {REJECTED_SSO_CHAIN}"
            ));

        assert!(
            matches!(classify(&untrusted, &call()), Error::TlsTrust { .. }),
            "trust is checked first and stays first"
        );
        assert!(
            matches!(classify(&unreachable, &call()), Error::Network { .. }),
            "an endpoint that cannot be reached is never a credential problem"
        );
    }

    #[test]
    fn a_real_denial_names_the_iam_action_it_needed() {
        let failure = SdkFailure::new(FailureKind::Service)
            .with_code("AccessDenied")
            .with_status(403)
            .with_text("user is not authorized to perform this operation");

        match classify(&failure, &call()) {
            Error::AccessDenied { iam_action } => assert_eq!(iam_action, "s3:ListAllMyBuckets"),
            other => panic!("expected AccessDenied, got {other:?}"),
        }
    }

    #[test]
    fn an_unreachable_endpoint_is_a_network_failure_not_a_credential_one() {
        let failure = SdkFailure::new(FailureKind::Dispatch)
            .io_error()
            .with_text("error trying to connect: tcp connect error: connection refused");

        assert!(matches!(classify(&failure, &call()), Error::Network { .. }));
    }

    #[test]
    fn a_timeout_says_so_rather_than_blaming_the_endpoint_for_being_absent() {
        let failure = SdkFailure::new(FailureKind::Timeout).timed_out();

        match classify(&failure, &call()) {
            Error::Network { detail } => assert!(detail.contains("in time"), "got: {detail}"),
            other => panic!("expected Network, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_provider_chain_is_reported_as_no_credentials() {
        let failure = SdkFailure::new(FailureKind::Construction)
            .with_text("no providers in chain provided credentials");

        match classify(&failure, &call()) {
            Error::NoCredentials { profile } => assert_eq!(profile, "work"),
            other => panic!("expected NoCredentials, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_region_is_configuration_not_authentication() {
        let failure =
            SdkFailure::new(FailureKind::Construction).with_text("no region was configured");

        assert!(matches!(
            classify(&failure, &call()),
            Error::MissingConfiguration { .. }
        ));
    }

    #[test]
    fn throttling_is_never_reported_as_a_denial() {
        // A busy account answers `SlowDown`, and the throttling codes wear
        // every status from 400 to 503. None of them is an authorization
        // decision, and the capability model reads a denial as one — so a
        // throttled account must not render as a wall of locks
        // (`capability-awareness`, "Only a denial may be presented as a
        // denial"). Nothing here needs a rule of its own: the denial branch
        // asks for a denial code and these are not it. This test is what
        // keeps that true if the branch is ever loosened.
        let throttles = [
            ("SlowDown", 503),
            ("Throttling", 400),
            ("ThrottlingException", 400),
            ("RequestLimitExceeded", 400),
            ("TooManyRequestsException", 429),
        ];

        for (code, status) in throttles {
            let failure = SdkFailure::new(FailureKind::Service)
                .with_code(code)
                .with_status(status)
                .with_text("please reduce your request rate");

            assert!(
                !matches!(classify(&failure, &call()), Error::AccessDenied { .. }),
                "`{code}` (HTTP {status}) is a rate limit, not an authorization decision"
            );
        }
    }

    #[test]
    fn an_unattributed_service_error_keeps_the_code_and_status_for_diagnosis() {
        let failure = SdkFailure::new(FailureKind::Service)
            .with_code("SlowDown")
            .with_status(503)
            .with_text("please reduce your request rate");

        match classify(&failure, &call()) {
            Error::Unexpected { detail } => {
                assert!(detail.contains("SlowDown"), "got: {detail}");
                assert!(detail.contains("503"), "got: {detail}");
            }
            other => panic!("expected Unexpected, got {other:?}"),
        }
    }

    #[test]
    fn no_classification_ever_leaks_credential_material() {
        // Everything a real chain could plausibly carry: keys, a session
        // token, and a signed Authorization header.
        //
        // A stored credential goes through here too, and it is the case with
        // the most to lose: it was typed into this application, handed to the
        // SDK as static credentials, and the first thing a wrong one produces
        // is a signing failure whose chain quotes what was signed with. The
        // secret in the list below is the one `credentials.rs` stores, on
        // purpose — the two redaction tests guard the same string arriving
        // from opposite directions.
        const SECRETS: &[&str] = &[
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "FQoGZXIvYXdzEExampleSessionToken",
            "AWS4-HMAC-SHA256",
        ];
        let leaky = format!(
            "aws_access_key_id={} aws_secret_access_key={} \
             x-amz-security-token: {} authorization: {} Credential=...",
            SECRETS[0], SECRETS[1], SECRETS[2], SECRETS[3]
        );

        // One case per branch of the classifier, each carrying the secrets.
        let cases = [
            SdkFailure::new(FailureKind::Dispatch)
                .with_text(&format!("invalid peer certificate: UnknownIssuer {leaky}")),
            SdkFailure::new(FailureKind::Dispatch)
                .io_error()
                .with_text(&format!("connection refused {leaky}")),
            SdkFailure::new(FailureKind::Service)
                .with_code("ExpiredToken")
                .with_status(403)
                .with_text(&format!("token has expired {leaky}")),
            // The same cause reached through the credential provider, where
            // the chain is all there is to match on — and therefore the case
            // most at risk of being copied into the message.
            SdkFailure::new(FailureKind::Dispatch)
                .with_text(&format!("{REJECTED_SSO_CHAIN} {leaky}")),
            SdkFailure::new(FailureKind::Service)
                .with_code("AccessDenied")
                .with_status(403)
                .with_text(&leaky),
            SdkFailure::new(FailureKind::Construction)
                .with_text(&format!("no credentials {leaky}")),
            SdkFailure::new(FailureKind::Construction).with_text(&format!("no region {leaky}")),
            SdkFailure::new(FailureKind::Service)
                .with_code("SlowDown")
                .with_status(503)
                .with_text(&leaky),
            // What a mistyped stored credential actually produces: the app's
            // own static credentials signed a request and the service did not
            // agree with the signature. The chain names the key that signed
            // and the string that was signed.
            SdkFailure::new(FailureKind::Service)
                .with_code("SignatureDoesNotMatch")
                .with_status(403)
                .with_text(&format!(
                    "the request signature we calculated does not match the signature you \
                     provided; check your key and signing method {leaky}"
                )),
        ];

        for failure in &cases {
            let error = classify(failure, &call());
            let rendered = format!("{error} {error:?}");
            for secret in SECRETS {
                assert!(
                    !rendered.contains(secret),
                    "`{secret}` reached a user-visible message: {rendered}"
                );
            }
        }
    }
}
