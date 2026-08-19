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
//! Matching needs the whole source chain, including text the SDK assembled
//! from a wire response — which is exactly where credential material could
//! hide. Copying it into an `Error` would be the one way a secret reaches a
//! log or the UI, so every message here is authored from a bounded set of
//! extracted facts (failure kind, service error code, HTTP status) and the
//! chain is used for matching only (`connections` spec, "Credentials are
//! never disclosed").

use aws_sdk_s3::config::http::HttpResponse;
use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};

use crate::error::Error;

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

    // 3. Expired before denied: AWS answers 403 to both.
    if failure.code_is(EXPIRED_CODES) || failure.chain_has(EXPIRED_MARKERS) {
        return Error::ExpiredSession {
            profile: call.profile.to_owned(),
            sso_session: call.sso_session.map(ToOwned::to_owned),
        };
    }

    // 4. A real authorization refusal, and only that.
    if failure.code_is(DENIED_CODES) || failure.status == Some(403) {
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
    fn an_expired_session_is_never_reported_as_access_denied() {
        // AWS answers 403 to an expired token and to a policy denial alike.
        let failure = SdkFailure::new(FailureKind::Service)
            .with_code("ExpiredToken")
            .with_status(403)
            .with_text("the security token included in the request is expired");

        match classify(&failure, &call()) {
            Error::ExpiredSession {
                profile,
                sso_session,
            } => {
                assert_eq!(profile, "work");
                assert_eq!(sso_session.as_deref(), Some("corp"));
            }
            other => panic!("expected ExpiredSession, got {other:?}"),
        }
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
