//! The AWS-backed [`ObjectStore`]: the one module that names an
//! `aws-sdk-s3` type.
//!
//! Everything it returns is a domain type, and every failure it produces has
//! already been through the classifier — so the layers above never see an
//! `SdkError`, and never have to parse a string to know what happened.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use aws_config::SdkConfig;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::Region as SdkRegion;
use aws_sdk_s3::operation::list_buckets::builders::ListBucketsFluentBuilder;
use aws_sdk_s3::operation::list_objects_v2::builders::ListObjectsV2FluentBuilder;
use aws_sdk_s3::primitives::DateTimeFormat;
use aws_sdk_s3::types::Bucket as SdkBucket;

use crate::capability::Scope;
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

/// The IAM action listing one bucket's contents needs, named in denial
/// messages so a denied row says what would lift it.
const LIST_BUCKET_ACTION: &str = "s3:ListBucket";

/// How many keys a list probe asks for.
///
/// One. The probe exists to make the service answer with an authorization
/// decision, and a decision arrives with the first key or without it — asking
/// for more would read data the user has not asked to see and pay for a page
/// nobody reads.
const PROBE_MAX_KEYS: i32 = 1;

/// An [`ObjectStore`] backed by a real S3 endpoint.
#[derive(Debug, Clone)]
pub struct S3ObjectStore {
    /// The connection's own client: what account-wide calls go through, and
    /// what a bucket of unknown region falls back to.
    client: Client,
    /// The connection's resolved configuration, kept so a client for another
    /// region can be derived from it rather than built from scratch.
    config: SdkConfig,
    /// One client per region actually seen, built on first use and kept for
    /// the session. Shared by every clone of this store, so a probe scheduled
    /// on one runtime task reuses what another one built.
    regional: Arc<Mutex<HashMap<String, Client>>>,
    profile: String,
    endpoint: String,
    sso_session: Option<String>,
}

impl S3ObjectStore {
    /// Build a store for an opened connection.
    pub fn new(connection: &Connection) -> Self {
        Self::over(
            connection.sdk_config().clone(),
            connection.profile(),
            connection.region(),
            connection.sso_session(),
        )
    }

    /// The store over an already-resolved configuration.
    ///
    /// Everything a per-region client needs is in that configuration, which
    /// is the whole point — see [`Self::client_for`].
    fn over(config: SdkConfig, profile: &str, region: &str, sso_session: Option<&str>) -> Self {
        // Only used to name the host in a trust failure. The SDK computes
        // the real endpoint per call, so when nothing is configured the
        // regional default is the honest thing to name.
        let endpoint = config
            .endpoint_url()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| default_endpoint(region));
        Self {
            client: Client::new(&config),
            config,
            regional: Arc::default(),
            profile: profile.to_owned(),
            endpoint,
            sso_session: sso_session.map(ToOwned::to_owned),
        }
    }

    /// The client a request about `region` has to go through.
    ///
    /// Object operations are region-scoped: a request for a bucket in another
    /// region is answered with a redirect rather than with data, and a
    /// redirect is no evidence about permission. So a bucket whose region the
    /// listing reported gets a client for that region, built on first use and
    /// kept for the session; a bucket whose region nobody stated goes through
    /// the connection's own client, and whatever comes back is read as no
    /// evidence rather than as a denial.
    ///
    /// The derived clients are cheap because they are *derived*: the
    /// configuration they come from already holds the resolved credentials
    /// provider, the identity cache that keeps one resolution shared between
    /// them, and the HTTP client from `tls.rs` with its trust material.
    /// Building a provider chain per region instead would re-run
    /// `credential_process` once per region — seconds each, on every machine
    /// that signs in that way.
    ///
    /// The lock is held for a map lookup and a client construction, both
    /// synchronous, and never across an await.
    fn client_for(&self, region: &Region) -> Client {
        let Some(region) = known_region(region) else {
            return self.client.clone();
        };
        let mut regional = self.regional.lock().unwrap_or_else(PoisonError::into_inner);
        regional
            .entry(region.to_owned())
            .or_insert_with(|| regional_client(&self.config, region))
            .clone()
    }

    /// The host a failure about `region` should name.
    ///
    /// A probe travels to the bucket's own region, so a trust failure there
    /// is about that host — not about the one the connection was opened
    /// against. An explicitly configured endpoint overrides every region.
    fn endpoint_for(&self, region: &Region) -> String {
        match known_region(region) {
            Some(region) if self.config.endpoint_url().is_none() => default_endpoint(region),
            _ => self.endpoint.clone(),
        }
    }

    /// The context a failure needs to name what the user must fix.
    fn call<'a>(&'a self, iam_action: &'static str, endpoint: &'a str) -> CallContext<'a> {
        CallContext {
            profile: &self.profile,
            endpoint,
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
                    &self.call(LIST_BUCKETS_ACTION, &self.endpoint),
                )
            })?;
            buckets.extend(page.buckets().iter().map(map_bucket));
        }

        Ok(buckets)
    }

    async fn probe_list(&self, scope: &Scope, region: &Region) -> Result<()> {
        let client = self.client_for(region);

        probe_request(&client, scope)
            .send()
            .await
            .map_err(|error| {
                // Through the same classifier as every other call, and no
                // second look at the SDK error: what a denial is, and what is
                // merely a failure, is decided in exactly one place. The
                // adapter's job ends at handing back the structured cause —
                // `capability::observation_for` decides what it is evidence of.
                classify(
                    &SdkFailure::from_sdk(&error),
                    &self.call(LIST_BUCKET_ACTION, &self.endpoint_for(region)),
                )
            })?;

        Ok(())
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

/// The probe request: at most one key, and nothing that writes.
///
/// Kept apart from sending it so the shape can be asserted without a network,
/// exactly like the listing above. A scope naming a prefix asks about that
/// prefix: a policy can grant a prefix while withholding the bucket root, so
/// answering a prefix scope with the root's evidence would be a wrong answer
/// dressed as a real one.
fn probe_request(client: &Client, scope: &Scope) -> ListObjectsV2FluentBuilder {
    let request = client
        .list_objects_v2()
        .bucket(scope.bucket_name())
        .max_keys(PROBE_MAX_KEYS);
    match scope.key_prefix() {
        Some(prefix) => request.prefix(prefix),
        None => request,
    }
}

/// A client for one region, derived from the connection's configuration.
///
/// `Builder::from(&SdkConfig)` is what makes this affordable: it carries over
/// the same credentials provider, the same identity cache and the same HTTP
/// client, and changes only the region. Assembling a configuration by hand
/// here would drop whichever of those was forgotten.
fn regional_client(config: &SdkConfig, region: &str) -> Client {
    Client::from_conf(
        aws_sdk_s3::config::Builder::from(config)
            .region(SdkRegion::new(region.to_owned()))
            .build(),
    )
}

/// The region a request can actually be routed to, if any. A region the
/// service never stated — or stated blank — is not one to build a client for.
fn known_region(region: &Region) -> Option<&str> {
    match region {
        Region::Known(region) if !region.trim().is_empty() => Some(region),
        _ => None,
    }
}

/// The regional endpoint host, for naming in a trust failure.
fn default_endpoint(region: &str) -> String {
    format!("s3.{region}.amazonaws.com")
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
    //! a bucket becomes, and what an absent region becomes — and
    //! `capability-awareness`, "Probing is non-destructive": the shape of the
    //! probe, and which client carries it.

    use super::*;
    use crate::capability::Scope;
    use crate::tls::HttpStack;
    use aws_config::SdkConfig;
    use aws_sdk_s3::config::{
        AppName, BehaviorVersion, ConfigBag, Credentials, IdentityCache, Region as SdkRegion,
        RuntimeComponents, SharedCredentialsProvider,
    };
    use aws_sdk_s3::primitives::DateTime;
    use aws_smithy_runtime_api::client::http::{
        HttpClient, HttpConnector, HttpConnectorFuture, HttpConnectorSettings, SharedHttpConnector,
    };
    use aws_smithy_runtime_api::client::identity::{
        Identity, IdentityCachePartition, IdentityFuture, ResolveCachedIdentity,
        SharedIdentityResolver,
    };
    use aws_smithy_runtime_api::client::orchestrator::HttpRequest;
    use aws_smithy_runtime_api::client::result::ConnectorError;
    use std::collections::HashSet;

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

    /// What an opened connection hands the adapter: one resolved
    /// configuration carrying the credentials provider, the identity cache
    /// that keeps it from being resolved twice, and the shared HTTP client
    /// from `tls.rs`. This is the shape
    /// `aws_config::defaults(BehaviorVersion::latest()).load()` produces —
    /// assembled by hand here so the test needs neither a network nor the
    /// developer's `~/.aws`.
    fn connection_config() -> SdkConfig {
        SdkConfig::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(SdkRegion::new(CONNECTION_REGION))
            .app_name(AppName::new("caixonho-tests").expect("a valid app name"))
            .credentials_provider(SharedCredentialsProvider::new(test_credentials()))
            .identity_cache(IdentityCache::lazy().build())
            .http_client(
                HttpStack::with_ca_bundle(None)
                    .expect("the OS trust store alone builds a client")
                    .client(),
            )
            .build()
    }

    /// The same configuration, with the identity cache and the HTTP client
    /// swapped for ones a test can watch.
    fn probing_config(cache: &RecordingIdentityCache) -> SdkConfig {
        SdkConfig::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(SdkRegion::new(CONNECTION_REGION))
            .credentials_provider(SharedCredentialsProvider::new(test_credentials()))
            .identity_cache(cache.clone())
            .http_client(RefusingHttpClient)
            .build()
    }

    fn store() -> S3ObjectStore {
        S3ObjectStore::over(connection_config(), "work", CONNECTION_REGION, Some("corp"))
    }

    fn region_of(client: &Client) -> String {
        client
            .config()
            .region()
            .expect("every client is built for a region")
            .as_ref()
            .to_owned()
    }

    /// The credentials the fixtures sign with. Not a secret, and never sent:
    /// no test here reaches a socket.
    fn test_credentials() -> Credentials {
        Credentials::new(
            "AKIAEXAMPLE",
            "wJalrEXAMPLEKEY",
            None,
            None,
            "caixonho-tests",
        )
    }

    /// An identity cache that answers every resolution itself and records
    /// which partition it was asked about.
    ///
    /// It stands in for the lazy cache a real connection carries, and it is
    /// what makes "resolved once" observable: a client that resolved
    /// credentials through a cache of its own would never appear here at all.
    #[derive(Debug, Clone, Default)]
    struct RecordingIdentityCache {
        resolutions: Arc<Mutex<Vec<IdentityCachePartition>>>,
    }

    impl RecordingIdentityCache {
        fn resolutions(&self) -> Vec<IdentityCachePartition> {
            self.resolutions
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl ResolveCachedIdentity for RecordingIdentityCache {
        fn resolve_cached_identity<'a>(
            &'a self,
            resolver: SharedIdentityResolver,
            _runtime_components: &'a RuntimeComponents,
            _config_bag: &'a ConfigBag,
        ) -> IdentityFuture<'a> {
            self.resolutions
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(resolver.cache_partition());
            IdentityFuture::ready(Ok(Identity::from(test_credentials())))
        }
    }

    /// An HTTP client that refuses every request without touching a socket.
    ///
    /// The probe is signed and then dropped: what these tests are about is
    /// everything the SDK did before it tried to send. `user` rather than
    /// `io` so the failure is not retried and each probe counts once.
    #[derive(Debug, Clone)]
    struct RefusingHttpClient;

    impl HttpClient for RefusingHttpClient {
        fn http_connector(
            &self,
            _settings: &HttpConnectorSettings,
            _components: &RuntimeComponents,
        ) -> SharedHttpConnector {
            SharedHttpConnector::new(self.clone())
        }
    }

    impl HttpConnector for RefusingHttpClient {
        fn call(&self, _request: HttpRequest) -> HttpConnectorFuture {
            HttpConnectorFuture::ready(Err(ConnectorError::user(
                "no probe leaves this test".into(),
            )))
        }
    }

    #[test]
    fn a_probe_asks_for_at_most_one_key_from_the_bucket_it_names() {
        // Non-destructive by construction: it lists, it cannot create, and
        // it brings back one key at most — enough for the service to answer
        // with an authorization decision and nothing more.
        let request = probe_request(&client(), &Scope::bucket("logs"));

        assert_eq!(request.get_bucket().as_deref(), Some("logs"));
        assert_eq!(request.get_max_keys(), &Some(1));
        assert_eq!(
            request.get_prefix(),
            &None,
            "a bucket scope asks about the whole bucket"
        );
    }

    #[test]
    fn a_probe_of_a_prefix_asks_about_that_prefix_not_the_bucket_root() {
        // Nothing probes prefixes until `XONHO-0006`. The port already takes
        // the `Scope` the capability store is keyed by, though, so a prefix
        // scope must never come back with evidence about the bucket root:
        // a policy can grant one and withhold the other.
        let request = probe_request(&client(), &Scope::prefix("logs", "2026/"));

        assert_eq!(request.get_bucket().as_deref(), Some("logs"));
        assert_eq!(request.get_prefix().as_deref(), Some("2026/"));
    }

    #[test]
    fn a_bucket_elsewhere_is_probed_through_a_client_for_its_own_region() {
        let store = store();

        let client = store.client_for(&Region::Known("eu-west-1".to_owned()));

        assert_eq!(
            region_of(&client),
            "eu-west-1",
            "sent to the connection's region, the same request is answered with a \
             redirect — which is no evidence about permission either way"
        );
        assert!(
            client.config().http_client().is_some(),
            "the region changes; the HTTP client from `tls.rs` does not, so trust \
             material stays consistent across every probe"
        );
        assert_eq!(
            client.config().app_name(),
            connection_config().app_name(),
            "the client is derived from the connection's own configuration, not \
             assembled from scratch"
        );
    }

    #[test]
    fn a_bucket_with_no_known_region_is_probed_through_the_connections_own_client() {
        let store = store();

        for region in [Region::Unknown, Region::Known(String::new())] {
            let client = store.client_for(&region);

            assert_eq!(region_of(&client), CONNECTION_REGION, "{region:?}");
        }
        assert!(
            store.regional.lock().expect("not poisoned").is_empty(),
            "a region nobody named is not a region to build a client for"
        );
    }

    #[test]
    fn the_client_for_a_region_is_built_once_and_kept_for_the_session() {
        let store = store();
        let elsewhere = Region::Known("eu-west-1".to_owned());

        for _ in 0..5 {
            let _ = store.client_for(&elsewhere);
        }
        let _ = store.client_for(&Region::Known("us-east-2".to_owned()));

        assert_eq!(
            store.regional.lock().expect("not poisoned").len(),
            2,
            "one client per region actually seen, however many probes ask for it"
        );
    }

    #[tokio::test]
    async fn resolving_credentials_happens_once_however_many_regions_are_probed() {
        // The cost this guards: credentials on the development machine come
        // from `credential_process`, which takes seconds per resolution. A
        // regional client built on a provider chain of its own would pay that
        // again for every region on screen, turning a probe budget into a
        // login storm.
        //
        // So this probes four regions and watches the connection's identity
        // cache. Every probe has to arrive there — a client caching for
        // itself would resolve out of sight, and show up as a probe missing
        // from the record — and every one of them has to name the same cache
        // partition, because the partition is minted once, when the
        // credentials provider is built, and travels with every clone of it.
        // One partition in one cache is one resolution of the chain, however
        // many regions ask for credentials.
        let cache = RecordingIdentityCache::default();
        let store = S3ObjectStore::over(
            probing_config(&cache),
            "work",
            CONNECTION_REGION,
            Some("corp"),
        );
        let regions = ["eu-west-1", "us-east-2", "ap-northeast-1", "sa-east-1"];

        for region in regions {
            let probe = store
                .probe_list(&Scope::bucket("logs"), &Region::Known(region.to_owned()))
                .await;

            assert!(probe.is_err(), "{region}: nothing is actually sent");
        }

        let resolutions = cache.resolutions();
        assert!(
            resolutions.len() >= regions.len(),
            "every probe must resolve through the connection's identity cache; \
             {} of {} did",
            resolutions.len(),
            regions.len()
        );
        assert_eq!(
            resolutions.iter().collect::<HashSet<_>>().len(),
            1,
            "{} regions resolved credentials in {} partitions — one is the whole \
             point: the provider chain is shared, never rebuilt",
            regions.len(),
            resolutions.iter().collect::<HashSet<_>>().len()
        );
    }

    #[test]
    fn a_probe_failure_names_the_host_the_request_was_sent_to() {
        let store = store();

        assert_eq!(
            store.endpoint_for(&Region::Known("eu-west-1".to_owned())),
            "s3.eu-west-1.amazonaws.com"
        );
        assert_eq!(
            store.endpoint_for(&Region::Unknown),
            format!("s3.{CONNECTION_REGION}.amazonaws.com"),
            "an unknown region goes through the connection's own client, so its \
             host is the honest one to name"
        );
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
