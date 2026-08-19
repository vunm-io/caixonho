//! The S3 port: every object-storage operation caixonho performs, as a
//! trait.
//!
//! Core logic depends on this trait; the `aws-sdk-s3` adapter implements it
//! and is the only module that names an SDK type. That split is what makes
//! the specs' scenarios unit-testable — the double below returns canned
//! successes and each error kind, no AWS account or network required — and
//! keeps the door open for S3-compatible services behind the same operations.

use crate::capability::Scope;
use crate::error::Result;
use crate::types::{Bucket, Region};

/// Object-storage operations behind one object-safe async trait.
///
/// Starts with exactly what this slice needs; later slices extend it
/// (`list_objects`, transfers) rather than replacing it.
#[async_trait::async_trait]
pub trait ObjectStore: Send + Sync {
    /// List the buckets visible to this connection.
    ///
    /// An account with no buckets is `Ok(vec![])` — the empty answer is a
    /// truthful result, never an error (`bucket-listing` spec).
    async fn list_buckets(&self) -> Result<Vec<Bucket>>;

    /// Ask whether this scope's contents can be listed, without reading them.
    ///
    /// One request for at most one key: it creates nothing, modifies nothing,
    /// and returns almost nothing, which is what makes it usable as automatic
    /// evidence at all (`capability-awareness`, "Probing is non-destructive").
    /// There is deliberately no write probe here and there will not be one:
    /// write capability moves out of unknown only through an operation the
    /// user asked for.
    ///
    /// `Ok(())` is the evidence. A failure keeps its structured cause for the
    /// caller to report, and [`crate::capability::observation_for`] — not this
    /// port, and not the adapter behind it — decides which causes are evidence
    /// about permission at all.
    ///
    /// `region` is the bucket's own region as the listing reported it, because
    /// object operations are region-scoped: sent elsewhere, the same request
    /// is answered with a redirect, which is no evidence either way.
    /// [`Region::Unknown`] leaves the choice to the implementation — the real
    /// one falls back to the connection's own client.
    async fn probe_list(&self, scope: &Scope, region: &Region) -> Result<()>;
}

#[cfg(test)]
pub(crate) mod double {
    //! Hand-rolled test double: one constructor per canned behaviour, so a
    //! test names the scenario it simulates instead of assembling state.

    use super::ObjectStore;
    use crate::capability::Scope;
    use crate::error::{Error, Result};
    use crate::types::{Bucket, Region};

    /// A canned [`ObjectStore`] for tests.
    pub(crate) struct StoreDouble {
        outcome: Outcome,
    }

    enum Outcome {
        Buckets(Vec<Bucket>),
        Fail(fn() -> Error),
    }

    impl StoreDouble {
        /// Succeeds with the given buckets.
        pub(crate) fn with_buckets(buckets: Vec<Bucket>) -> Self {
            Self {
                outcome: Outcome::Buckets(buckets),
            }
        }

        /// Succeeds with an empty account.
        pub(crate) fn empty_account() -> Self {
            Self::with_buckets(Vec::new())
        }

        /// Fails with `no credentials` for the given profile.
        pub(crate) fn no_credentials() -> Self {
            Self::failing(|| Error::NoCredentials {
                profile: "double".into(),
            })
        }

        /// Fails with an expired session.
        pub(crate) fn expired_session() -> Self {
            Self::failing(|| Error::SessionRejected {
                profile: "double".into(),
                sso_session: Some("corp".into()),
                problem: crate::error::SessionProblem::Expired,
            })
        }

        /// Fails with a TLS trust failure.
        pub(crate) fn tls_trust() -> Self {
            Self::failing(|| Error::TlsTrust {
                endpoint: "s3.example.test".into(),
            })
        }

        /// Fails with an unreachable network.
        pub(crate) fn network() -> Self {
            Self::failing(|| Error::Network {
                detail: "connection refused (double)".into(),
            })
        }

        /// Fails with a service-side denial of the listing.
        pub(crate) fn access_denied() -> Self {
            Self::failing(|| Error::AccessDenied {
                iam_action: "s3:ListAllMyBuckets",
            })
        }

        /// Answers a probe: the credentials may list this bucket, and it
        /// holds nothing.
        pub(crate) fn allows_listing() -> Self {
            Self::with_buckets(Vec::new())
        }

        /// Fails with a service-side denial of listing one bucket's contents
        /// — what a probe meets when the policy withholds `s3:ListBucket`.
        pub(crate) fn bucket_access_denied() -> Self {
            Self::failing(|| Error::AccessDenied {
                iam_action: "s3:ListBucket",
            })
        }

        /// Fails because the credentials themselves were refused as invalid
        /// — a different cause from an expired session, and a different fix.
        pub(crate) fn rejected_credentials() -> Self {
            Self::failing(|| Error::SessionRejected {
                profile: "double".into(),
                sso_session: None,
                problem: crate::error::SessionProblem::Invalid,
            })
        }

        /// Fails the way a bucket in another region answers: a redirect to
        /// the endpoint that owns it. The classifier attributes it to no
        /// specific cause today, so it arrives as `Unexpected` carrying the
        /// service's own code — and, being no kind of denial, it is no
        /// evidence about permission either.
        pub(crate) fn wrong_region() -> Self {
            Self::failing(|| Error::Unexpected {
                detail: "the service reported `PermanentRedirect` (HTTP 301)".into(),
            })
        }

        /// Fails the way a busy account answers: slow down. Not a denial,
        /// and it must never be recorded as one — a throttled account would
        /// otherwise render as a wall of locks.
        pub(crate) fn throttled() -> Self {
            Self::failing(|| Error::Unexpected {
                detail: "the service reported `SlowDown` (HTTP 503)".into(),
            })
        }

        /// Fails with a missing-configuration error.
        pub(crate) fn missing_configuration() -> Self {
            Self::failing(|| Error::MissingConfiguration {
                profile: Some("double".into()),
                detail: "no region configured (double)".into(),
            })
        }

        /// Fails with an unclassifiable error.
        pub(crate) fn unexpected() -> Self {
            Self::failing(|| Error::Unexpected {
                detail: "internal service error (double)".into(),
            })
        }

        fn failing(make: fn() -> Error) -> Self {
            Self {
                outcome: Outcome::Fail(make),
            }
        }
    }

    #[async_trait::async_trait]
    impl ObjectStore for StoreDouble {
        async fn list_buckets(&self) -> Result<Vec<Bucket>> {
            match &self.outcome {
                Outcome::Buckets(buckets) => Ok(buckets.clone()),
                Outcome::Fail(make) => Err(make()),
            }
        }

        /// The canned behaviour applies to the probe too: a double that can
        /// list answers it, and a double that fails fails it the same way.
        async fn probe_list(&self, _scope: &Scope, _region: &Region) -> Result<()> {
            match &self.outcome {
                Outcome::Buckets(_) => Ok(()),
                Outcome::Fail(make) => Err(make()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! `bucket-listing` spec, scenarios "Account with buckets" and "Account
    //! with no buckets", and `capability-awareness`, "Probing is
    //! non-destructive" and "Only a denial may be presented as a denial" —
    //! all exercised through the port as the GUI will use it: a
    //! `dyn ObjectStore`, no SDK, no network.

    use super::ObjectStore;
    use super::double::StoreDouble;
    use crate::capability::{CapabilityStore, Observation, Scope, observation_for};
    use crate::error::Error;
    use crate::types::{Bucket, Region};

    fn bucket(name: &str, created: Option<&str>) -> Bucket {
        Bucket {
            name: name.into(),
            created: created.map(Into::into),
            region: Region::Unknown,
        }
    }

    /// A store with one profile open and nothing observed yet.
    fn capabilities() -> (CapabilityStore, crate::capability::CredentialsId) {
        let mut store = CapabilityStore::new();
        let credentials = store.credentials_changed("work");
        (store, credentials)
    }

    #[tokio::test]
    async fn a_probe_the_service_answers_settles_the_scope_as_listable() {
        let (mut capabilities, credentials) = capabilities();
        let logs = Scope::bucket("logs");
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::allows_listing());

        let probe = store
            .probe_list(&logs, &Region::Known("ap-southeast-1".into()))
            .await;

        let observation = observation_for(&probe);
        assert_eq!(observation, Observation::Allowed);
        assert!(capabilities.observe_list(&credentials, logs.clone(), observation));
        assert_eq!(
            capabilities.capability(&credentials, &logs).list,
            Observation::Allowed
        );
        assert!(
            !capabilities.needs_list_probe(&credentials, &logs),
            "the evidence is in hand; nothing is left to probe for"
        );
    }

    #[tokio::test]
    async fn a_probe_refused_on_authorization_grounds_is_recorded_as_denied() {
        let (mut capabilities, credentials) = capabilities();
        let logs = Scope::bucket("logs");
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::bucket_access_denied());

        let probe = store.probe_list(&logs, &Region::Unknown).await;

        match &probe {
            Err(Error::AccessDenied { iam_action }) => assert_eq!(
                *iam_action, "s3:ListBucket",
                "a denial has to name the IAM action that would lift it"
            ),
            other => panic!("expected AccessDenied, got {other:?}"),
        }
        let observation = observation_for(&probe);
        assert_eq!(observation, Observation::Denied);
        assert!(capabilities.observe_list(&credentials, logs.clone(), observation));
        assert_eq!(
            capabilities.capability(&credentials, &logs).list,
            Observation::Denied
        );
    }

    #[tokio::test]
    async fn a_probe_that_fails_for_any_other_reason_leaves_the_capability_untouched() {
        // Every failure the spec names as not-a-denial, end to end: through
        // the port, through the mapping, into the store. None of them may
        // record anything, and each must leave the scope worth probing again.
        let cases = [
            ("an expired session", StoreDouble::expired_session()),
            ("rejected credentials", StoreDouble::rejected_credentials()),
            ("a wrong-region redirect", StoreDouble::wrong_region()),
            ("an unreachable network", StoreDouble::network()),
            ("throttling", StoreDouble::throttled()),
        ];

        for (case, double) in cases {
            let (mut capabilities, credentials) = capabilities();
            let logs = Scope::bucket("logs");
            let store: Box<dyn ObjectStore> = Box::new(double);

            let probe = store.probe_list(&logs, &Region::Unknown).await;

            assert!(probe.is_err(), "{case} is a failure");
            let observation = observation_for(&probe);
            assert_eq!(
                observation,
                Observation::Unknown,
                "{case} is no evidence about permission"
            );
            assert!(
                !capabilities.observe_list(&credentials, logs.clone(), observation),
                "{case} must not record anything"
            );
            assert_eq!(
                capabilities.capability(&credentials, &logs).list,
                Observation::Unknown,
                "{case} must never read as denied"
            );
            assert!(
                capabilities.needs_list_probe(&credentials, &logs),
                "{case} leaves the scope unobserved, so still worth probing"
            );
        }
    }

    #[tokio::test]
    async fn every_bucket_comes_back_with_name_and_creation_date() {
        let canned = vec![
            bucket("logs", Some("2026-01-03T05:47:00Z")),
            bucket("backups", None),
        ];
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::with_buckets(canned.clone()));

        let listed = store.list_buckets().await.expect("listing must succeed");

        assert_eq!(listed, canned);
        assert_eq!(listed[0].created.as_deref(), Some("2026-01-03T05:47:00Z"));
        assert_eq!(listed[1].region, Region::Unknown);
    }

    #[tokio::test]
    async fn each_bucket_crosses_the_port_with_the_region_the_service_reported() {
        // What an account looks like when the service reports a region for
        // some buckets and none for others: each row keeps its own answer.
        let canned = vec![
            Bucket {
                name: "logs".into(),
                created: None,
                region: Region::Known("ap-southeast-1".into()),
            },
            Bucket {
                name: "backups".into(),
                created: None,
                region: Region::Unknown,
            },
        ];
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::with_buckets(canned));

        let listed = store.list_buckets().await.expect("listing must succeed");

        assert_eq!(listed[0].region, Region::Known("ap-southeast-1".into()));
        assert_eq!(
            listed[1].region,
            Region::Unknown,
            "an unreported region stays unknown across the port — the frontend \
             must never receive a stand-in, least of all the connection's own region"
        );
    }

    #[tokio::test]
    async fn empty_account_is_a_truthful_result_not_an_error() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::empty_account());

        let listed = store.list_buckets().await.expect("empty is Ok");

        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn each_failure_constructor_produces_its_matching_variant() {
        assert!(matches!(
            StoreDouble::no_credentials().list_buckets().await,
            Err(Error::NoCredentials { .. })
        ));
        assert!(matches!(
            StoreDouble::expired_session().list_buckets().await,
            Err(Error::SessionRejected { .. })
        ));
        assert!(matches!(
            StoreDouble::tls_trust().list_buckets().await,
            Err(Error::TlsTrust { .. })
        ));
        assert!(matches!(
            StoreDouble::network().list_buckets().await,
            Err(Error::Network { .. })
        ));
        assert!(matches!(
            StoreDouble::missing_configuration().list_buckets().await,
            Err(Error::MissingConfiguration { .. })
        ));
        assert!(matches!(
            StoreDouble::unexpected().list_buckets().await,
            Err(Error::Unexpected { .. })
        ));
    }

    #[tokio::test]
    async fn a_denied_listing_is_an_error_never_an_empty_list() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::access_denied());

        match store.list_buckets().await {
            Err(Error::AccessDenied { iam_action }) => {
                assert_eq!(iam_action, "s3:ListAllMyBuckets");
            }
            other => panic!("expected AccessDenied, got {other:?}"),
        }
    }
}
