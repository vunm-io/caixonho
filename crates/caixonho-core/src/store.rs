//! The S3 port: every object-storage operation caixonho performs, as a
//! trait.
//!
//! Core logic depends on this trait; the `aws-sdk-s3` adapter implements it
//! and is the only module that names an SDK type. That split is what makes
//! the specs' scenarios unit-testable — the double below returns canned
//! successes and each error kind, no AWS account or network required — and
//! keeps the door open for S3-compatible services behind the same operations.

use crate::error::Result;
use crate::types::Bucket;

/// Object-storage operations behind one object-safe async trait.
///
/// Starts with exactly what this slice needs; later slices extend it
/// (`list_objects`, capability probes, transfers) rather than replacing it.
#[async_trait::async_trait]
pub trait ObjectStore: Send + Sync {
    /// List the buckets visible to this connection.
    ///
    /// An account with no buckets is `Ok(vec![])` — the empty answer is a
    /// truthful result, never an error (`bucket-listing` spec).
    async fn list_buckets(&self) -> Result<Vec<Bucket>>;
}

#[cfg(test)]
pub(crate) mod double {
    //! Hand-rolled test double: one constructor per canned behaviour, so a
    //! test names the scenario it simulates instead of assembling state.

    use super::ObjectStore;
    use crate::error::{Error, Result};
    use crate::types::Bucket;

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
    }
}

#[cfg(test)]
mod tests {
    //! `bucket-listing` spec, scenarios "Account with buckets" and "Account
    //! with no buckets", exercised through the port as the GUI will use it —
    //! a `dyn ObjectStore`, no SDK, no network.

    use super::ObjectStore;
    use super::double::StoreDouble;
    use crate::error::Error;
    use crate::types::{Bucket, Region};

    fn bucket(name: &str, created: Option<&str>) -> Bucket {
        Bucket {
            name: name.into(),
            created: created.map(Into::into),
            region: Region::Unknown,
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
