//! The AWS-backed [`SsoSignIn`]: the one module that names an
//! `aws-sdk-ssooidc` type.
//!
//! Sits beside [`crate::adapter`] and follows the same rule — every value it
//! returns is a domain type and every failure has been through the
//! classifier, so nothing above ever sees an `SdkError`.
//!
//! One thing here is unlike the S3 adapter: these three calls carry no
//! credentials. They are how credentials are obtained, so requiring them
//! would be circular. The client is therefore built with
//! [`allow_no_auth`](aws_sdk_ssooidc::config::Builder::allow_no_auth) — the
//! service's own answer to the same problem.

use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use aws_sdk_ssooidc::Client;
use aws_sdk_ssooidc::config::{BehaviorVersion, Region as SdkRegion};
use aws_sdk_ssooidc::error::SdkError;
use aws_sdk_ssooidc::operation::create_token::CreateTokenError;

use crate::classify::{CallContext, SdkFailure, classify};
use crate::error::{Error, Result, SignInProblem};
use crate::sso::{
    ClientRegistration, DeviceAuthorization, SignInLocation, SignInSecret, SsoSignIn, SsoToken,
    TokenAnswer,
};
use crate::tls::HttpStack;

/// How this application introduces itself when registering.
///
/// It reaches the user: the authorization page names the client asking for
/// access, and "unknown application" would be the wrong thing for someone to
/// read while deciding whether to approve.
const CLIENT_NAME: &str = "caixonho";

/// The registration type the device flow requires. The service accepts one
/// value here, and this is it.
const CLIENT_TYPE: &str = "public";

/// The grant the token endpoint is asked for while an authorization is
/// pending. Spelled out by RFC 8628 and not ours to vary.
const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// Signing in against the real Identity Center.
#[derive(Debug, Clone)]
pub struct SsoOidcSignIn {
    /// The application's one HTTP client. Shared rather than built here so
    /// enterprise trust material configured once at startup applies to the
    /// sign-in too — an interception proxy does not stop being there because
    /// the request is going somewhere other than S3.
    http: HttpStack,
}

impl SsoOidcSignIn {
    /// Build one over the shared HTTP stack.
    pub fn new(http: HttpStack) -> Self {
        Self { http }
    }

    /// A client for the region the `sso_session` names.
    ///
    /// Rebuilt per call rather than cached: a sign-in happens once in a while
    /// and against whichever Identity Center instance the profile points at,
    /// so a cache would save nothing and would have to be keyed by region to
    /// stay correct.
    fn client(&self, at: &SignInLocation) -> Client {
        let config = aws_sdk_ssooidc::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(SdkRegion::new(at.region.clone()))
            .http_client(self.http.client())
            // No credentials exist yet — that is what this flow is for.
            .allow_no_auth()
            .build();
        Client::from_conf(config)
    }

    /// The classifier's view of a call to the sign-in endpoint.
    ///
    /// `iam_action` is empty on purpose: no policy grants the right to sign
    /// in, so there is no action to name in a denial. A denial here comes from
    /// the person at the browser, and is told apart before the classifier is
    /// ever reached.
    fn call<'a>(&self, at: &'a SignInLocation) -> CallContext<'a> {
        CallContext {
            profile: at.label(),
            endpoint: &at.start_url,
            iam_action: "",
            sso_session: at.session_name.as_deref(),
            // Signing in is about a session, never about a bucket.
            bucket: None,
        }
    }

    /// Turn any failure that is not one of the flow's own outcomes into a
    /// cause, through the same classifier every other call uses.
    fn failure<E>(
        &self,
        at: &SignInLocation,
        error: &SdkError<E, aws_smithy_runtime_api::http::Response>,
    ) -> Error
    where
        E: std::error::Error + aws_sdk_ssooidc::error::ProvideErrorMetadata + Send + Sync + 'static,
    {
        classify(&SdkFailure::from_sdk(error), &self.call(at))
    }

    /// The attempt ended, and the provider is the one saying so.
    fn ended(at: &SignInLocation, problem: SignInProblem) -> Error {
        Error::SignIn {
            sso_session: at.label().to_owned(),
            problem,
        }
    }

    /// A field the service documents as always present, absent anyway.
    ///
    /// Its own cause rather than an unwrap: a provider that answers without a
    /// device code has not failed in any way this application can describe,
    /// and crashing the window over it would lose the log line that says so.
    fn missing(at: &SignInLocation, field: &str) -> Error {
        Error::Unexpected {
            detail: format!(
                "the sign-in service answered for `{}` without `{field}`",
                at.label()
            ),
        }
    }
}

#[async_trait]
impl SsoSignIn for SsoOidcSignIn {
    async fn register_client(&self, at: &SignInLocation) -> Result<ClientRegistration> {
        let mut call = self
            .client(at)
            .register_client()
            .client_name(CLIENT_NAME)
            .client_type(CLIENT_TYPE);
        for scope in &at.scopes {
            call = call.scopes(scope);
        }

        let answer = call
            .send()
            .await
            .map_err(|error| self.failure(at, &error))?;

        Ok(ClientRegistration {
            client_id: answer
                .client_id
                .ok_or_else(|| Self::missing(at, "clientId"))?,
            client_secret: SignInSecret::new(
                answer
                    .client_secret
                    .ok_or_else(|| Self::missing(at, "clientSecret"))?,
            ),
            // Absolute seconds since the epoch here, unlike everything else in
            // this flow, which is relative. Read from the service's own field
            // rather than computed, so a clock that disagrees with the
            // service's does not shorten or extend the registration.
            registration_expires_at: SystemTime::UNIX_EPOCH
                + Duration::from_secs(answer.client_secret_expires_at.max(0) as u64),
        })
    }

    async fn start_device_authorization(
        &self,
        at: &SignInLocation,
        client: &ClientRegistration,
    ) -> Result<DeviceAuthorization> {
        let answer = self
            .client(at)
            .start_device_authorization()
            .client_id(&client.client_id)
            .client_secret(client.client_secret.expose())
            .start_url(&at.start_url)
            .send()
            .await
            .map_err(|error| self.failure(at, &error))?;

        // Relative, so it is anchored now. The alternative — trusting a
        // service-supplied absolute time — would put this machine's clock and
        // the service's in disagreement about when to stop polling.
        let expires_at = SystemTime::now() + Duration::from_secs(answer.expires_in.max(0) as u64);

        Ok(DeviceAuthorization {
            device_code: SignInSecret::new(
                answer
                    .device_code
                    .ok_or_else(|| Self::missing(at, "deviceCode"))?,
            ),
            user_code: answer
                .user_code
                .ok_or_else(|| Self::missing(at, "userCode"))?,
            verification_uri: answer
                .verification_uri
                .ok_or_else(|| Self::missing(at, "verificationUri"))?,
            verification_uri_complete: answer
                .verification_uri_complete
                .ok_or_else(|| Self::missing(at, "verificationUriComplete"))?,
            expires_at,
            // A provider that names no interval is not asking for a fast one;
            // five seconds is what RFC 8628 §3.5 tells a client to assume.
            interval: Duration::from_secs(answer.interval.max(0) as u64).max(DEFAULT_INTERVAL),
        })
    }

    async fn create_token(
        &self,
        at: &SignInLocation,
        client: &ClientRegistration,
        authorization: &DeviceAuthorization,
    ) -> Result<TokenAnswer> {
        let answer = self
            .client(at)
            .create_token()
            .client_id(&client.client_id)
            .client_secret(client.client_secret.expose())
            .grant_type(DEVICE_CODE_GRANT)
            .device_code(authorization.device_code.expose())
            .send()
            .await;

        let answer = match answer {
            Ok(answer) => answer,
            // The four the protocol defines, told apart here rather than
            // flattened into "sign-in failed". Two of them are not failures at
            // all: the loop above turns them back into waiting.
            Err(SdkError::ServiceError(service)) => {
                return match service.err() {
                    CreateTokenError::AuthorizationPendingException(_) => Ok(TokenAnswer::Pending),
                    CreateTokenError::SlowDownException(_) => Ok(TokenAnswer::SlowDown),
                    CreateTokenError::AccessDeniedException(_) => {
                        Err(Self::ended(at, SignInProblem::Declined))
                    }
                    CreateTokenError::ExpiredTokenException(_) => {
                        Err(Self::ended(at, SignInProblem::Expired))
                    }
                    // Everything else — an invalid client, a bad request, the
                    // service itself failing — is not about this attempt, and
                    // goes through the classifier like any other call.
                    _ => Err(self.failure(at, &SdkError::ServiceError(service))),
                };
            }
            Err(error) => return Err(self.failure(at, &error)),
        };

        Ok(TokenAnswer::Issued(SsoToken {
            access_token: SignInSecret::new(
                answer
                    .access_token
                    .ok_or_else(|| Self::missing(at, "accessToken"))?,
            ),
            refresh_token: answer.refresh_token.map(SignInSecret::new),
            expires_at: SystemTime::now() + Duration::from_secs(answer.expires_in.max(0) as u64),
        }))
    }
}

/// What RFC 8628 §3.5 says a client assumes when the provider names no
/// polling interval.
const DEFAULT_INTERVAL: Duration = Duration::from_secs(5);

#[cfg(test)]
mod tests {
    use super::*;

    fn somewhere() -> SignInLocation {
        SignInLocation {
            session_name: Some("corp".into()),
            start_url: "https://corp.awsapps.com/start".into(),
            region: "ap-southeast-1".into(),
            scopes: Vec::new(),
        }
    }

    fn stack() -> HttpStack {
        HttpStack::with_ca_bundle(None).expect("the OS trust store alone builds a client")
    }

    #[test]
    fn the_sign_in_client_uses_the_applications_one_http_stack() {
        // The property the whole struct exists for. A client of its own would
        // work on a developer's laptop and fail on a machine behind an
        // interception proxy, which is the machine this feature is for.
        let client = SsoOidcSignIn::new(stack()).client(&somewhere());

        assert!(
            client.config().http_client().is_some(),
            "the shared stack has to reach the sign-in calls"
        );
    }

    #[test]
    fn the_region_is_the_one_the_sso_session_names() {
        // Not the bucket's region and not the connection's: an Identity Center
        // instance lives where it lives, and asking the wrong region answers
        // with a redirect that reads like a broken sign-in.
        let client = SsoOidcSignIn::new(stack()).client(&somewhere());

        assert_eq!(
            client.config().region().map(|region| region.to_string()),
            Some("ap-southeast-1".to_owned())
        );
    }
}
