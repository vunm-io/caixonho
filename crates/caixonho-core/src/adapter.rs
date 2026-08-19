//! The AWS-backed [`ObjectStore`]: the one module that names an
//! `aws-sdk-s3` type.
//!
//! Everything it returns is a domain type, and every failure it produces has
//! already been through the classifier — so the layers above never see an
//! `SdkError`, and never have to parse a string to know what happened.

use async_trait::async_trait;
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::DateTimeFormat;
use aws_sdk_s3::types::Bucket as SdkBucket;

use crate::classify::{CallContext, SdkFailure, classify};
use crate::connection::Connection;
use crate::error::Result;
use crate::store::ObjectStore;
use crate::types::{Bucket, Region};

/// The IAM action a bucket listing needs, named in denial messages.
const LIST_BUCKETS_ACTION: &str = "s3:ListAllMyBuckets";

/// An [`ObjectStore`] backed by a real S3 endpoint.
#[derive(Debug, Clone)]
pub struct S3ObjectStore {
    client: Client,
    profile: String,
    endpoint: String,
    sso_session: Option<String>,
}

impl S3ObjectStore {
    /// Build a store for an opened connection.
    pub fn new(connection: &Connection) -> Self {
        let config = connection.sdk_config();
        Self {
            client: Client::new(config),
            profile: connection.profile().to_owned(),
            // Only used to name the host in a trust failure. The SDK computes
            // the real endpoint per call, so when nothing is configured the
            // regional default is the honest thing to name.
            endpoint: config
                .endpoint_url()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("s3.{}.amazonaws.com", connection.region())),
            sso_session: connection.sso_session().map(ToOwned::to_owned),
        }
    }

    /// The context a failure needs to name what the user must fix.
    fn call(&self, iam_action: &'static str) -> CallContext<'_> {
        CallContext {
            profile: &self.profile,
            endpoint: &self.endpoint,
            iam_action,
            sso_session: self.sso_session.as_deref(),
        }
    }
}

#[async_trait]
impl ObjectStore for S3ObjectStore {
    async fn list_buckets(&self) -> Result<Vec<Bucket>> {
        // Paginated: an account past the service's page size would otherwise
        // be silently truncated, which reads exactly like a smaller account.
        let mut pages = self.client.list_buckets().into_paginator().send();
        let mut buckets = Vec::new();

        while let Some(page) = pages.next().await {
            let page = page.map_err(|error| {
                classify(
                    &SdkFailure::from_sdk(&error),
                    &self.call(LIST_BUCKETS_ACTION),
                )
            })?;
            buckets.extend(page.buckets().iter().map(map_bucket));
        }

        Ok(buckets)
    }
}

/// Map one SDK bucket to the domain type.
///
/// The region is taken only when the service states it: filling in the
/// connection's own region would be a guess that reads as fact, and the spec
/// makes "unknown" a first-class display value instead.
fn map_bucket(bucket: &SdkBucket) -> Bucket {
    Bucket {
        name: bucket.name().unwrap_or_default().to_owned(),
        created: bucket
            .creation_date()
            .and_then(|date| date.fmt(DateTimeFormat::DateTime).ok()),
        region: match bucket.bucket_region() {
            Some(region) if !region.is_empty() => Region::Known(region.to_owned()),
            _ => Region::Unknown,
        },
    }
}

#[cfg(test)]
mod tests {
    //! `bucket-listing` spec — the mapping rules that do not need a network:
    //! what a bucket becomes, and what an absent region becomes.

    use super::*;
    use aws_sdk_s3::primitives::DateTime;

    #[test]
    fn a_bucket_keeps_its_name_and_creation_date() {
        let sdk = SdkBucket::builder()
            .name("logs")
            .creation_date(DateTime::from_secs(1_767_225_600))
            .build();

        let bucket = map_bucket(&sdk);

        assert_eq!(bucket.name, "logs");
        assert_eq!(bucket.created.as_deref(), Some("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn a_region_the_service_states_is_kept_as_stated() {
        let sdk = SdkBucket::builder()
            .name("logs")
            .bucket_region("ap-southeast-1")
            .build();

        assert_eq!(
            map_bucket(&sdk).region,
            Region::Known("ap-southeast-1".to_owned())
        );
    }

    #[test]
    fn an_absent_region_stays_unknown_rather_than_being_guessed() {
        let sdk = SdkBucket::builder().name("logs").build();

        assert_eq!(map_bucket(&sdk).region, Region::Unknown);
    }

    #[test]
    fn a_blank_region_is_unknown_too() {
        let sdk = SdkBucket::builder().name("logs").bucket_region("").build();

        assert_eq!(map_bucket(&sdk).region, Region::Unknown);
    }
}
