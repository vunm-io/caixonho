//! What a failure is called, and what it leaves the user able to do.
//!
//! Both functions here map a cause to words. Neither touches the window, which
//! is why they can be read — and tested — without one.

use caixonho_core::{
    BucketKind, ConnectionsProblem, CredentialStoreProblem, Error, RefusedListing, SessionProblem,
    SignInProblem,
};
use gpui::SharedString;

/// What to do about a failure. The advice belongs to the cause rather than
/// to the panel that happens to show it, now that two surfaces need it.
///
/// `sign_in_offered` is whether the surface showing this is also showing a
/// sign-in button. It changes the advice for exactly the causes a sign-in
/// fixes, and it has to be passed in because the same cause deserves
/// different words depending on whether the fix is one click away or somewhere
/// else entirely. Telling someone to run `aws sso login` beside a button that
/// does it for them is how the CLI stays a dependency in practice after it has
/// stopped being one in code.
pub(crate) fn guidance_for(error: &Error, sign_in_offered: bool) -> SharedString {
    if sign_in_offered
        && matches!(
            error,
            Error::SessionRejected { .. } | Error::NoCredentials { .. }
        )
    {
        return "Sign in to continue. The listing runs again by itself once you have.".into();
    }
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
                // Not "list buckets": this cause covers the account listing,
                // one bucket's contents, and the session a directory bucket
                // needs before it can be read at all. Naming the wrong one
                // sends the user to ask for a permission that would change
                // nothing — and the action already says which it was.
                Error::AccessDenied { iam_action } => format!(
                    "This profile does not have {iam_action}. Ask for that permission, or use a \
                     profile that has it."
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

/// What to call the listing that was refused, when the other one answered.
///
/// A heading about the *kind that is missing*, not about the account: the
/// account is fine, and what is on screen is real. What is absent is one half
/// of the question, and the user cannot know that from a list that simply
/// ends.
pub(crate) fn refusal_headline(refused: &RefusedListing) -> SharedString {
    match refused.kind {
        BucketKind::General => "General purpose buckets were not listed".into(),
        BucketKind::Directory => "Directory buckets were not listed".into(),
    }
}

/// Why, and what would change it.
///
/// Names the permission plainly, without backticks: nothing here renders
/// markdown, so a code span arrives as punctuation the reader has to look
/// past.
pub(crate) fn refusal_detail(refused: &RefusedListing) -> SharedString {
    format!(
        "This profile may not list them — that needs {}.",
        refused.action
    )
    .into()
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
    fn a_refusal_names_the_kind_that_is_missing_and_its_own_permission() {
        let directory = RefusedListing {
            kind: BucketKind::Directory,
            action: "s3express:ListAllMyDirectoryBuckets",
        };
        let general = RefusedListing {
            kind: BucketKind::General,
            action: "s3:ListAllMyBuckets",
        };

        assert_eq!(
            refusal_headline(&directory),
            "Directory buckets were not listed"
        );
        assert_eq!(
            refusal_headline(&general),
            "General purpose buckets were not listed"
        );
        assert!(
            refusal_detail(&directory).contains("s3express:ListAllMyDirectoryBuckets"),
            "the action named must be the one that was refused"
        );
        assert!(
            !refusal_detail(&directory).contains('`'),
            "nothing renders markdown here, so a backtick arrives as punctuation"
        );
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

    #[test]
    fn a_failed_sign_in_or_unwritable_cache_does_not_make_the_connection_unusable() {
        // An attempt that was declined or timed out does not change what the
        // connection is, and a session that was obtained but could not be saved
        // works right now — marking either would overwrite the real cause with
        // a symptom.
        let declined = Error::SignIn {
            sso_session: "corp".into(),
            problem: SignInProblem::Declined,
        };
        let expired = Error::SignIn {
            sso_session: "corp".into(),
            problem: SignInProblem::Expired,
        };
        let cache = Error::TokenCacheNotWritable {
            path: "~/.aws/sso/cache/abc.json".into(),
            detail: "permission denied".into(),
        };

        assert_eq!(unavailable_reason(&declined), None);
        assert_eq!(unavailable_reason(&expired), None);
        assert_eq!(unavailable_reason(&cache), None);
    }

    #[test]
    fn a_sign_in_failure_or_unwritable_cache_offers_guidance() {
        // A browser sign-in that was declined, one that timed out, and a
        // session that could not be saved to disk have different next actions.
        // Giving them the same message would leave the user repeating an
        // action that cannot fix the problem.
        let path = "~/.aws/sso/cache/abc.json";
        let declined = Error::SignIn {
            sso_session: "corp".into(),
            problem: SignInProblem::Declined,
        };
        let expired = Error::SignIn {
            sso_session: "corp".into(),
            problem: SignInProblem::Expired,
        };
        let cache = Error::TokenCacheNotWritable {
            path: path.into(),
            detail: "permission denied".into(),
        };

        let declined_guidance = guidance_for(&declined, false);
        let expired_guidance = guidance_for(&expired, false);
        let cache_guidance = guidance_for(&cache, false);

        assert_ne!(declined_guidance, expired_guidance);
        assert_ne!(declined_guidance, cache_guidance);
        assert_ne!(expired_guidance, cache_guidance);
        assert!(cache_guidance.contains(path));
    }

    #[test]
    fn when_signing_in_is_one_click_away_the_advice_stops_naming_a_command() {
        // The regression this guards is subtle and was shipped: the button
        // arrived and the sentence beside it still told the user to go and run
        // `aws sso login`, which is the dependency this whole change exists to
        // remove. A CLI named next to a button that does the same thing keeps
        // the CLI required in practice.
        let expired = Error::SessionRejected {
            profile: "work".into(),
            sso_session: Some("corp".into()),
            problem: SessionProblem::Expired,
        };

        let offered = guidance_for(&expired, true);
        let alone = guidance_for(&expired, false);

        assert!(!offered.contains("aws sso"), "{offered}");
        assert!(!offered.contains("retry"), "{offered}");
        // And the other way: a surface with no button to offer still tells the
        // user where to go, because there the command is the only way.
        assert!(alone.contains("aws sso login"), "{alone}");
    }

    #[test]
    fn a_profile_with_no_credentials_gets_the_same_treatment() {
        let none = Error::NoCredentials {
            profile: "work".into(),
        };

        assert_ne!(guidance_for(&none, true), guidance_for(&none, false));
    }

    #[test]
    fn causes_a_sign_in_would_not_fix_read_the_same_either_way() {
        // The flag changes the advice for the two causes signing in fixes, and
        // for nothing else. A denial is a denial whether or not a button
        // happens to be on screen.
        for error in [
            Error::AccessDenied {
                iam_action: "s3:ListAllMyBuckets",
            },
            Error::Network {
                detail: "connect timed out".into(),
            },
        ] {
            assert_eq!(guidance_for(&error, true), guidance_for(&error, false));
        }
    }

    #[test]
    fn a_rejected_session_offers_guidance_distinguishing_expired_invalid_and_sso() {
        // An expired token, an invalid static key, and an SSO session require
        // different remedies: signing in again, checking secret keys, or
        // running the AWS SSO login command.
        let expired = Error::SessionRejected {
            profile: "work".into(),
            sso_session: None,
            problem: SessionProblem::Expired,
        };
        let invalid = Error::SessionRejected {
            profile: "work".into(),
            sso_session: None,
            problem: SessionProblem::Invalid,
        };
        let sso = Error::SessionRejected {
            profile: "work".into(),
            sso_session: Some("corp".into()),
            problem: SessionProblem::Expired,
        };

        let expired_guidance = guidance_for(&expired, false);
        let invalid_guidance = guidance_for(&invalid, false);
        let sso_guidance = guidance_for(&sso, false);

        assert_ne!(expired_guidance, invalid_guidance);
        assert_ne!(expired_guidance, sso_guidance);
        assert_ne!(invalid_guidance, sso_guidance);
        assert!(sso_guidance.contains("aws sso login --sso-session corp"));
        assert!(expired_guidance.contains("work"));
        assert!(invalid_guidance.contains("work"));
    }

    #[test]
    fn a_credential_store_failure_distinguishes_locked_refused_and_absent_stores() {
        // A locked keychain needs unlocking, a refused prompt was declined,
        // and a missing keychain means using ~/.aws — confusing them sends the
        // user to fix the wrong layer of the system.
        let connection = "prod-db";
        let locked = Error::CredentialStore {
            connection: connection.into(),
            problem: CredentialStoreProblem::Locked,
        };
        let refused = Error::CredentialStore {
            connection: connection.into(),
            problem: CredentialStoreProblem::Refused,
        };
        let absent = Error::CredentialStore {
            connection: connection.into(),
            problem: CredentialStoreProblem::Absent,
        };

        let locked_guidance = guidance_for(&locked, false);
        let refused_guidance = guidance_for(&refused, false);
        let absent_guidance = guidance_for(&absent, false);

        assert_ne!(locked_guidance, refused_guidance);
        assert_ne!(locked_guidance, absent_guidance);
        assert_ne!(refused_guidance, absent_guidance);
        assert!(locked_guidance.contains(connection));
        assert!(refused_guidance.contains(connection));
    }

    #[test]
    fn a_connections_file_failure_distinguishes_unreadable_malformed_and_unwritable() {
        // Malformed files tell the user to repair or remove without overwriting,
        // unwritable files explain a change was lost, and unreadable files
        // note that ~/.aws remains intact.
        let path = std::path::PathBuf::from("/home/user/.config/caixonho/connections.json");
        let unreadable = Error::Connections {
            problem: ConnectionsProblem::Unreadable,
            path: Some(path.clone()),
        };
        let malformed = Error::Connections {
            problem: ConnectionsProblem::Malformed,
            path: Some(path.clone()),
        };
        let not_writable = Error::Connections {
            problem: ConnectionsProblem::NotWritable,
            path: Some(path.clone()),
        };
        let no_location = Error::Connections {
            problem: ConnectionsProblem::NoLocation,
            path: None,
        };

        let unreadable_guidance = guidance_for(&unreadable, false);
        let malformed_guidance = guidance_for(&malformed, false);
        let not_writable_guidance = guidance_for(&not_writable, false);
        let no_location_guidance = guidance_for(&no_location, false);

        assert_ne!(unreadable_guidance, malformed_guidance);
        assert_ne!(unreadable_guidance, not_writable_guidance);
        assert_ne!(unreadable_guidance, no_location_guidance);
        assert_ne!(malformed_guidance, not_writable_guidance);
        assert_ne!(malformed_guidance, no_location_guidance);
        assert_ne!(not_writable_guidance, no_location_guidance);
    }
}
