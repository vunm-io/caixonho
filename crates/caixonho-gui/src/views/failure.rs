//! What a failure is called, and what it leaves the user able to do.
//!
//! Both functions here map a cause to words. Neither touches the window, which
//! is why they can be read — and tested — without one.

use caixonho_core::{
    ConnectionsProblem, CredentialStoreProblem, Error, SessionProblem, SignInProblem,
};
use gpui::SharedString;

/// What to do about a failure. The advice belongs to the cause rather than
/// to the panel that happens to show it, now that two surfaces need it.
pub(crate) fn guidance_for(error: &Error) -> SharedString {
    match error {
                Error::Network { .. } => "The endpoint could not be reached. Check the connection and try again.".into(),
                Error::SessionRejected { profile, sso_session, problem } => match (problem, sso_session) {
                    (_, Some(session)) => format!("Sign in again: `aws sso login --sso-session {session}`").into(),
                    (SessionProblem::Expired, None) => format!("Sign in again for profile `{profile}`, then retry.").into(),
                    (SessionProblem::Invalid, None) => format!(
                        "The service does not recognise these credentials. Check the access key and secret for profile `{profile}`."
                    )
                    .into(),
                },
                Error::TlsTrust { endpoint } => format!(
                    "The certificate chain for {endpoint} is not trusted. Add the issuing CA to the \
                     system trust store, or point AWS_CA_BUNDLE at the bundle your network uses."
                )
                .into(),
                Error::AccessDenied { iam_action } => format!(
                    "This profile is not allowed to list buckets. It needs the `{iam_action}` permission."
                )
                .into(),
                Error::NoCredentials { profile } => {
                    format!("No credentials resolved for `{profile}`. Check the profile's keys, role or SSO session.").into()
                }
                Error::MissingConfiguration { .. } => {
                    "Complete the profile's configuration — a region is required — and try again.".into()
                }
                Error::CredentialStore { connection, problem } => match problem {
                    CredentialStoreProblem::Locked => format!(
                        "The system keychain is locked, so the secret for `{connection}` cannot be \
                         read. Unlock it and try again."
                    )
                    .into(),
                    CredentialStoreProblem::Refused => format!(
                        "The system keychain did not hand back the secret for `{connection}`. If a \
                         prompt appeared, it may have been declined."
                    )
                    .into(),
                    CredentialStoreProblem::Absent => {
                        "This system has no credential store for caixonho to use, so credentials \
                         entered here cannot be kept. Use a profile in ~/.aws instead."
                            .into()
                    }
                },
                Error::Connections { problem, path } => {
                    let where_it_is = path
                        .as_ref()
                        .map(|path| format!(" ({})", path.display()))
                        .unwrap_or_default();
                    match problem {
                        ConnectionsProblem::Unreadable => format!(
                            "The file of saved connections could not be read{where_it_is}. The \
                             connections kept in it are not shown; the ones from ~/.aws are \
                             unaffected."
                        )
                        .into(),
                        ConnectionsProblem::Malformed => format!(
                            "The file of saved connections is not in a form caixonho understands\
                             {where_it_is}. It has been left exactly as it is — repair or remove it, \
                             and nothing in it will be overwritten meanwhile."
                        )
                        .into(),
                        ConnectionsProblem::NotWritable => format!(
                            "The file of saved connections could not be written{where_it_is}, so the \
                             change was not kept."
                        )
                        .into(),
                        ConnectionsProblem::NoLocation => {
                            "This machine offers nowhere to keep saved connections, so a credential \
                             entered here cannot be remembered. Use a profile in ~/.aws instead."
                                .into()
                        }
                    }
                }
                Error::NotImplemented { operation, endpoint } => format!(
                    "{endpoint} does not implement {operation}. This is an \
                     S3-compatible service rather than S3 itself, and it does not \
                     offer that request — nothing here is misconfigured."
                )
                .into(),
                Error::SignIn { problem, .. } => match problem {
                    SignInProblem::Declined => {
                        "The sign-in was declined in the browser. Start it again to try once more."
                            .into()
                    }
                    SignInProblem::Expired => {
                        "The sign-in was not completed in time. Start it again — the code is only \
                         good for a few minutes."
                            .into()
                    }
                },
                Error::TokenCacheNotWritable { path, .. } => format!(
                    "Signed in, but the session could not be saved to {path}, so it will be asked \
                     for again. Check that the folder exists and is writable."
                )
                .into(),
                Error::Unexpected { .. } => "The call failed for an unrecognised reason.".into(),
    }
}

/// Why a connection cannot be used at all, if that is what happened.
///
/// Only a failure to authenticate makes a *connection* unusable. A network
/// failure belongs to the network and will pass; a denial means the connection
/// worked perfectly and the permission did not. Marking either would say
/// something untrue about the connection.
pub(crate) fn unavailable_reason(error: &Error) -> Option<SharedString> {
    match error {
        Error::SessionRejected { problem, .. } => Some(match problem {
            SessionProblem::Expired => "sign-in expired".into(),
            SessionProblem::Invalid => "credentials refused".into(),
        }),
        Error::NoCredentials { .. } => Some("no credentials".into()),
        // The secret exists somewhere the app cannot reach, which for the
        // purpose of connecting is the same as not having one.
        Error::CredentialStore { problem, .. } => Some(match problem {
            CredentialStoreProblem::Locked => "keychain locked".into(),
            CredentialStoreProblem::Refused => "keychain refused".into(),
            CredentialStoreProblem::Absent => "no keychain".into(),
        }),
        // A configuration file that will not parse says nothing about whether
        // any particular credential works, so it marks no connection.
        // Signing in worked; the endpoint simply does not offer that
        // request. Marking the connection would send the user to fix a
        // credential that is fine.
        // An attempt that was declined or ran out of time leaves the
        // connection exactly as it was: still unusable, still for whatever
        // reason it was unusable before. Marking it with the *attempt's*
        // outcome would overwrite the cause with the symptom.
        //
        // A session that was obtained but could not be saved is the same
        // story from the other side: the connection works right now, and will
        // stop when the token lapses, which is not something to mark it with
        // today.
        Error::SignIn { .. }
        | Error::TokenCacheNotWritable { .. }
        | Error::Connections { .. }
        | Error::Network { .. }
        | Error::TlsTrust { .. }
        | Error::AccessDenied { .. }
        | Error::MissingConfiguration { .. }
        | Error::NotImplemented { .. }
        | Error::Unexpected { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    //! `connections` spec — "A connection that cannot authenticate is not
    //! offered as usable". Which failures mean that, and which do not.

    use super::*;

    #[test]
    fn a_session_the_service_will_not_accept_makes_the_connection_unusable() {
        let expired = Error::SessionRejected {
            profile: "work".into(),
            sso_session: Some("corp".into()),
            problem: SessionProblem::Expired,
        };
        let refused = Error::SessionRejected {
            profile: "work".into(),
            sso_session: None,
            problem: SessionProblem::Invalid,
        };

        assert_eq!(unavailable_reason(&expired), Some("sign-in expired".into()));
        assert_eq!(
            unavailable_reason(&refused),
            Some("credentials refused".into())
        );
    }

    #[test]
    fn a_connection_with_nothing_to_sign_in_with_is_unusable() {
        let error = Error::NoCredentials {
            profile: "work".into(),
        };

        assert_eq!(unavailable_reason(&error), Some("no credentials".into()));
    }

    #[test]
    fn a_denial_does_not_make_the_connection_unusable() {
        // The whole point: the connection worked and the permission did not.
        // Marking it would say something untrue about the connection, and send
        // the user to fix a sign-in that is fine.
        let error = Error::AccessDenied {
            iam_action: "s3:ListAllMyBuckets",
        };

        assert_eq!(unavailable_reason(&error), None);
    }

    #[test]
    fn a_secret_the_app_cannot_reach_makes_the_connection_unusable() {
        // Whether the store is locked, refusing or missing, the effect on
        // connecting is the same: there is nothing to sign in with.
        for problem in [
            CredentialStoreProblem::Locked,
            CredentialStoreProblem::Refused,
            CredentialStoreProblem::Absent,
        ] {
            let error = Error::CredentialStore {
                connection: "my-key".into(),
                problem,
            };

            assert!(
                unavailable_reason(&error).is_some(),
                "a secret that cannot be read leaves nothing to connect with: {problem:?}"
            );
        }
    }

    #[test]
    fn a_failure_of_the_environment_is_not_the_connection_s_fault() {
        let network = Error::Network {
            detail: "connection reset".into(),
        };
        let trust = Error::TlsTrust {
            endpoint: "s3.example.com".into(),
        };

        assert_eq!(unavailable_reason(&network), None);
        assert_eq!(unavailable_reason(&trust), None);
    }
}
