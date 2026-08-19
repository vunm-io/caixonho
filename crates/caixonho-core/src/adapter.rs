//! The AWS-backed [`ObjectStore`]: the one module that names an
//! `aws-sdk-s3` type.
//!
//! Everything it returns is a domain type, and every failure it produces has
//! already been through the classifier — so the layers above never see an
//! `SdkError`, and never have to parse a string to know what happened.

use async_trait::async_trait;
use aws_sdk_s3::Client;
use aws_sdk_s3::operation::list_buckets::builders::ListBucketsFluentBuilder;
use aws_sdk_s3::primitives::DateTimeFormat;
use aws_sdk_s3::types::Bucket as SdkBucket;

use crate::classify::{CallContext, SdkFailure, classify};
use crate::connection::Connection;
use crate::error::Result;
use crate::store::ObjectStore;
use crate::types::{Bucket, Region};

/// The IAM action a bucket listing needs, named in denial messages.
const LIST_BUCKETS_ACTION: &str = "s3:ListAllMyBuckets";

/// How many buckets to ask for per page.
///
/// Its presence matters more than its value. `ListBuckets` reports each
/// bucket's `BucketRegion` only "if the request contains at least one valid
/// parameter" — a request with no parameters at all, which is what the
/// paginator sends on its own, comes back with names and creation dates and no
/// regions whatsoever. Confirmed live against the development account
/// (`XONHO-0005`, task 1.1): the same account answered without regions
/// parameterless and with a region for every bucket once a page size was sent.
/// So this is what buys the account's regions inside the calls already made,
/// instead of a location lookup per bucket at open time.
///
/// The service accepts 1..=10000 per page. A thousand covers most accounts in
/// one round trip without asking for a page no account will fill; anything
/// larger is still paginated below.
const LIST_BUCKETS_PAGE_SIZE: i32 = 1000;

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
        // The page size travels with every page the paginator asks for, so
        // regions come back on all of them, not just the first.
        let mut pages = list_buckets_request(&self.client).into_paginator().send();
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

/// The listing request, shaped so the service reports regions.
///
/// Kept apart from sending it so the shape can be asserted without a network:
/// the page size goes on the request rather than on the paginator (which
/// offers the equivalent `page_size`) because a fluent builder can be read
/// back in a test, and a paginator cannot.
fn list_buckets_request(client: &Client) -> ListBucketsFluentBuilder {
    client.list_buckets().max_buckets(LIST_BUCKETS_PAGE_SIZE)
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
    //! `bucket-listing` spec — everything about the listing that does not need
    //! a network: the shape of the request that makes regions come back, what
    //! a bucket becomes, and what an absent region becomes.

    use super::*;
    use aws_sdk_s3::config::{BehaviorVersion, Region as SdkRegion};
    use aws_sdk_s3::primitives::DateTime;

    /// A client good enough to shape a request with. It is never sent, so no
    /// credentials are involved; the region only satisfies the SDK's own
    /// validation and stands in for "the connection's region" below.
    const CONNECTION_REGION: &str = "us-east-1";

    fn client() -> Client {
        Client::from_conf(
            aws_sdk_s3::Config::builder()
                .behavior_version(BehaviorVersion::latest())
                .region(SdkRegion::new(CONNECTION_REGION))
                .build(),
        )
    }

    #[test]
    fn the_listing_request_carries_a_page_size_so_the_service_reports_regions() {
        // The whole point of the parameter: `ListBuckets` reports
        // `BucketRegion` only when the request carries at least one valid
        // parameter, so a parameterless listing leaves every row unknown.
        let request = list_buckets_request(&client());

        assert_eq!(request.get_max_buckets(), &Some(LIST_BUCKETS_PAGE_SIZE));
        assert!(
            (1..=10_000).contains(&LIST_BUCKETS_PAGE_SIZE),
            "the service accepts 1..=10000 buckets per page, not {LIST_BUCKETS_PAGE_SIZE}"
        );
    }

    #[test]
    fn a_listing_reporting_some_regions_leaves_the_rest_unknown_not_local() {
        let page = [
            SdkBucket::builder()
                .name("logs")
                .bucket_region("ap-southeast-1")
                .build(),
            SdkBucket::builder().name("backups").build(),
        ];

        let mapped: Vec<Bucket> = page.iter().map(map_bucket).collect();

        assert_eq!(mapped[0].region, Region::Known("ap-southeast-1".to_owned()));
        assert_eq!(mapped[1].region, Region::Unknown);
        assert_ne!(
            mapped[1].region,
            Region::Known(CONNECTION_REGION.to_owned()),
            "a bucket the service said nothing about must not borrow the connection's region"
        );
    }

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
