//! The AWS-backed [`ObjectStore`]: the one module that names an
//! `aws-sdk-s3` type.
//!
//! Everything it returns is a domain type, and every failure it produces has
//! already been through the classifier — so the layers above never see an
//! `SdkError`, and never have to parse a string to know what happened.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use aws_config::SdkConfig;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::Region as SdkRegion;
use aws_sdk_s3::config::http::HttpResponse;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::{GetObjectError, GetObjectOutput};
use aws_sdk_s3::operation::list_buckets::builders::ListBucketsFluentBuilder;
use aws_sdk_s3::operation::list_objects_v2::builders::ListObjectsV2FluentBuilder;
use aws_sdk_s3::operation::list_objects_v2::{ListObjectsV2Error, ListObjectsV2Output};
use aws_sdk_s3::primitives::DateTimeFormat;
use aws_sdk_s3::types::Bucket as SdkBucket;
use aws_sdk_s3::types::Object as SdkObject;

use crate::capability::Scope;
use crate::classify::{CallContext, SdkFailure, classify};
use crate::connection::Connection;
use crate::error::{Error, Result};
use crate::listing;
use crate::store::ObjectStore;
use crate::store::{IfAbsent, ObjectContent, ObjectRead, PutOutcome};
use crate::types::{
    AccountListing, Bucket, BucketKind, Cursor, Location, Object, Page, RefusedListing, Region,
};

/// The IAM action a bucket listing needs, named in denial messages.
const LIST_BUCKETS_ACTION: &str = "s3:ListAllMyBuckets";

/// What the directory listing requires, which is not the same permission.
///
/// Quoted from `aws-sdk-s3`'s own documentation for the operation rather than
/// recalled: directory buckets are governed by `s3express:*`, and telling
/// someone to obtain `s3:ListAllMyBuckets` when this is what was refused costs
/// them a request to whoever grants permissions, and the wait for it.
const LIST_DIRECTORY_BUCKETS_ACTION: &str = "s3express:ListAllMyDirectoryBuckets";

/// What reading a directory bucket requires, whatever the read is.
///
/// Objects in a directory bucket are reached with a session, not with the
/// caller's own credentials, so a refusal almost always lands on obtaining the
/// session rather than on the listing that wanted it. Reporting `s3:ListBucket`
/// there sends the user to ask for a permission that would change nothing.
const SESSION_ACTION: &str = "s3express:CreateSession";

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
pub const LIST_BUCKET_ACTION: &str = "s3:ListBucket";

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
    /// Which of this connection's buckets the directory listing returned.
    ///
    /// Remembered, not inferred: the operation that returned a bucket is what
    /// knows its kind, and the alternative — reading the `--x-s3` suffix off
    /// the name — is a guess about a string the account holder chose part of.
    /// A bucket opened by name without a listing is simply absent from here,
    /// and then this connection genuinely does not know.
    directory: Arc<Mutex<HashSet<String>>>,
    /// Which of this connection's buckets turned out to live in a region
    /// other than the one the connection is pointed at, and where.
    ///
    /// Learned from a redirect the service sent, never guessed: beside the
    /// directory set for the same reason it is, because the operation that
    /// answered is what knows, and what it learned belongs to the connection
    /// that learned it. It cannot go stale the way a cache can — a bucket
    /// does not change region — and it dies with the connection regardless.
    elsewhere: Arc<Mutex<HashMap<String, String>>>,
    /// The region this connection was opened in. Kept because a directory
    /// bucket that states no region of its own is in this one, and "the
    /// region we asked in" has to be answerable without asking the SDK for a
    /// value it treats as optional.
    region: String,
    sso_session: Option<String>,
}

impl S3ObjectStore {
    /// Build a store for an opened connection.
    pub fn new(connection: &Connection) -> Self {
        Self::over(
            connection.sdk_config().clone(),
            connection.name(),
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
            directory: Arc::default(),
            elsewhere: Arc::default(),
            profile: profile.to_owned(),
            endpoint,
            region: region.to_owned(),
            sso_session: sso_session.map(ToOwned::to_owned),
        }
    }

    /// The buckets the general listing returns, which is not all of them.
    async fn general_buckets(&self) -> Result<Vec<Bucket>> {
        match self.buckets_with_page_size(true).await {
            // The page size is an optimisation for one implementation, and
            // one that another rejects outright. Asked, refused, asked again
            // without it — which is finding out what an endpoint implements
            // by asking, exactly as `ADR-0002` finds out what a credential
            // may do. Branching on which vendor the endpoint belongs to
            // would be the same mistake one layer down.
            Err(Error::NotImplemented { .. }) => self.buckets_with_page_size(false).await,
            other => other,
        }
    }

    /// Whether this connection is one that can hold directory buckets at all.
    ///
    /// Directory buckets are an AWS construct. A connection addressing an
    /// S3-compatible service cannot have them, and asking earns a
    /// `NotImplemented` this application would then have to explain away — a
    /// failure it created by asking a question that could not apply. The
    /// endpoint is what distinguishes the two, and it is already known here.
    fn offers_directory_buckets(&self) -> bool {
        self.config.endpoint_url().is_none()
    }

    /// The account's directory buckets, which the general listing never
    /// returns.
    ///
    /// Every page, because a partial account presented as the whole one is a
    /// lie the user cannot see. The SDK routes this to the regional control
    /// plane and signs it; nothing here names an endpoint.
    async fn directory_buckets(&self) -> Result<Vec<Bucket>> {
        if !self.offers_directory_buckets() {
            return Ok(Vec::new());
        }

        let mut pages = self.client.list_directory_buckets().into_paginator().send();
        let mut buckets = Vec::new();

        while let Some(page) = pages.next().await {
            let page = page.map_err(|error| {
                classify(
                    &SdkFailure::from_sdk(&error),
                    &self.call(LIST_DIRECTORY_BUCKETS_ACTION, &self.endpoint, None),
                )
            })?;

            let mut known = self.directory.lock().expect("not poisoned");
            buckets.extend(page.buckets().iter().map(|bucket| {
                let mapped = Bucket {
                    name: bucket.name().unwrap_or_default().to_owned(),
                    created: bucket
                        .creation_date()
                        .and_then(|date| date.fmt(DateTimeFormat::DateTime).ok()),
                    region: directory_bucket_region(bucket.bucket_arn(), &self.region),
                    kind: BucketKind::Directory,
                };
                known.insert(mapped.name.clone());
                mapped
            }));
        }

        Ok(buckets)
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

    /// The region this connection has learned `bucket` actually lives in.
    ///
    /// The lock is held for one lookup and never across an await, the same
    /// discipline `client_for` and `read_action` keep.
    fn region_learned_for(&self, bucket: &str) -> Option<Region> {
        self.elsewhere
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(bucket)
            .map(|region| Region::Known(region.clone()))
    }

    /// Remember where a redirect said a bucket lives, so the next read of it
    /// is addressed there rather than paying for the redirect again.
    fn remember_region(&self, bucket: &str, region: &str) {
        self.elsewhere
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(bucket.to_owned(), region.to_owned());
    }

    /// One page of a location, addressed to `region` — or to the connection's
    /// own client when there is nothing better known.
    ///
    /// Hands back the SDK's own error rather than a cause: the caller has to
    /// look at the failure before deciding whether it is one, and classifying
    /// here would throw away the redirect on the way past.
    async fn read_object(
        &self,
        bucket: &str,
        key: &str,
        region: Option<&Region>,
    ) -> std::result::Result<GetObjectOutput, SdkError<GetObjectError, HttpResponse>> {
        let client = match region {
            Some(region) => self.client_for(region),
            None => self.client.clone(),
        };
        client.get_object().bucket(bucket).key(key).send().await
    }

    /// What a failed object read is reported as. `s3:GetObject` whatever the
    /// bucket kind: directory buckets gate reads through the session the same
    /// way (`CreateSession` grants), but the permission the *user* can act on
    /// is the object-read one.
    fn get_failure(&self, failure: &SdkFailure, bucket: &str, region: Option<&Region>) -> Error {
        let endpoint = match region {
            Some(region) => self.endpoint_for(region),
            None => self.endpoint.clone(),
        };
        classify(failure, &self.call("s3:GetObject", &endpoint, Some(bucket)))
    }

    async fn read_page(
        &self,
        location: &Location,
        cursor: Option<&Cursor>,
        region: Option<&Region>,
    ) -> std::result::Result<ListObjectsV2Output, SdkError<ListObjectsV2Error, HttpResponse>> {
        let client = match region {
            Some(region) => self.client_for(region),
            None => self.client.clone(),
        };
        list_objects_request(&client, location, cursor).send().await
    }

    /// What a failed read of `location`, addressed to `region`, means.
    fn read_failure(
        &self,
        failure: &SdkFailure,
        location: &Location,
        region: Option<&Region>,
    ) -> Error {
        let endpoint = match region {
            Some(region) => self.endpoint_for(region),
            None => self.endpoint.clone(),
        };
        classify(
            failure,
            &self.call(
                self.read_action(&location.bucket),
                &endpoint,
                Some(&location.bucket),
            ),
        )
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

    /// What a read of `bucket` required, which depends on what the bucket is.
    fn read_action(&self, bucket: &str) -> &'static str {
        if self
            .directory
            .lock()
            .expect("not poisoned")
            .contains(bucket)
        {
            SESSION_ACTION
        } else {
            LIST_BUCKET_ACTION
        }
    }

    /// The context a failure needs to name what the user must fix.
    ///
    /// `bucket` is a parameter rather than something derived here because
    /// only the caller knows whether its call was about one: an account
    /// listing is about none, and a defaulted `None` would let a bucket-scoped
    /// call quietly lose the name a cause needs to state.
    fn call<'a>(
        &'a self,
        iam_action: &'static str,
        endpoint: &'a str,
        bucket: Option<&'a str>,
    ) -> CallContext<'a> {
        CallContext {
            profile: &self.profile,
            endpoint,
            iam_action,
            sso_session: self.sso_session.as_deref(),
            bucket,
        }
    }
}

#[async_trait]
impl ObjectStore for S3ObjectStore {
    async fn list_buckets(&self) -> Result<AccountListing> {
        // Both at once. They are independent requests to different endpoints,
        // so the pair costs what the slower one costs; one after the other
        // would double the latency of the most common screen in the
        // application to no purpose.
        let (general, directory) = tokio::join!(self.general_buckets(), self.directory_buckets());

        combine(general, directory)
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
                    &self.call(
                        self.read_action(scope.bucket_name()),
                        &self.endpoint_for(region),
                        Some(scope.bucket_name()),
                    ),
                )
            })?;

        Ok(())
    }

    async fn get_object(&self, bucket: &str, key: &str) -> Result<ObjectContent> {
        // The same follow-once contract as `list_objects`, for the same
        // reason: a redirect that names a region is a call that has been told
        // where to go, and following it twice is a request that never
        // settles. The learned region is shared with the listing path, so a
        // bucket the listing already followed is read right the first time.
        let addressed_to = self.region_learned_for(bucket);

        let answer = match self.read_object(bucket, key, addressed_to.as_ref()).await {
            Ok(answer) => answer,
            Err(error) => {
                let failure = SdkFailure::from_sdk(&error);
                let Some(region) = failure.redirect_region() else {
                    return Err(self.get_failure(&failure, bucket, addressed_to.as_ref()));
                };
                let region = Region::Known(region.to_owned());

                let answer =
                    self.read_object(bucket, key, Some(&region))
                        .await
                        .map_err(|error| {
                            self.get_failure(&SdkFailure::from_sdk(&error), bucket, Some(&region))
                        })?;

                if let Region::Known(name) = &region {
                    self.remember_region(bucket, name);
                }
                answer
            }
        };

        // The service's own length for progress, never trusted further than
        // that: the stream decides where the object actually ends.
        let size = answer
            .content_length()
            .and_then(|len| u64::try_from(len).ok());
        Ok(ObjectContent {
            size,
            body: Box::new(SdkRead { body: answer.body }),
        })
    }

    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        path: &std::path::Path,
        if_absent: IfAbsent,
    ) -> Result<PutOutcome> {
        let region = self.region_learned_for(bucket);
        let client = match &region {
            Some(region) => self.client_for(region),
            None => self.client.clone(),
        };

        let body = aws_sdk_s3::primitives::ByteStream::from_path(path)
            .await
            .map_err(|error| Error::Destination {
                detail: error.to_string(),
            })?;

        let mut request = client.put_object().bucket(bucket).key(key).body(body);
        if if_absent == IfAbsent::Refuse {
            // The guarantee, in the request the service executes rather than
            // in a check this process performs and hopes to win.
            request = request.if_none_match("*");
        }

        match request.send().await {
            Ok(_) => Ok(PutOutcome::Created),
            Err(error) => {
                let failure = SdkFailure::from_sdk(&error);
                // Asked before classification, exactly as the redirect is:
                // a precondition that refused a taken key is the mechanism
                // working, and it must not become a failure the user is sent
                // to fix.
                if if_absent == IfAbsent::Refuse && failure.precondition_failed() {
                    return Ok(PutOutcome::KeyTaken);
                }
                let endpoint = match &region {
                    Some(region) => self.endpoint_for(region),
                    None => self.endpoint.clone(),
                };
                let classified = classify(
                    &failure,
                    &self.call("s3:PutObject", &endpoint, Some(bucket)),
                );
                // An endpoint that will not do conditional writes is its own
                // answer rather than a failed write — nothing was written,
                // and the user is owed the choice, not an error.
                if if_absent == IfAbsent::Refuse
                    && matches!(classified, Error::NotImplemented { .. })
                {
                    return Ok(PutOutcome::ConditionUnsupported);
                }
                Err(classified)
            }
        }
    }

    async fn list_objects(&self, location: &Location, cursor: Option<&Cursor>) -> Result<Page> {
        // A bucket this connection has already been redirected about is
        // addressed to its own region from the start. Discovering it once and
        // then paying for the redirect on every page afterwards would make
        // the discovery worth nothing.
        let addressed_to = self.region_learned_for(&location.bucket);

        let (answer, served_from) = match self
            .read_page(location, cursor, addressed_to.as_ref())
            .await
        {
            Ok(answer) => (answer, None),
            Err(error) => {
                let failure = SdkFailure::from_sdk(&error);
                // Asked before it is classified, because a redirect that
                // names a region is not a failure yet — it is a call that has
                // been told where to go.
                let Some(region) = failure.redirect_region() else {
                    return Err(self.read_failure(&failure, location, addressed_to.as_ref()));
                };
                let region = Region::Known(region.to_owned());

                // Once. The reissue below is addressed to the region the
                // service itself named, so a second redirect is the service
                // contradicting itself, and following it again would turn a
                // wrong region into a request that never settles.
                let answer = self
                    .read_page(location, cursor, Some(&region))
                    .await
                    .map_err(|error| {
                        self.read_failure(&SdkFailure::from_sdk(&error), location, Some(&region))
                    })?;

                // Remembered only after a read that worked. A region that
                // answered nothing is not knowledge worth keeping, and
                // storing it would send every later page somewhere this
                // connection has never successfully reached.
                if let Region::Known(name) = &region {
                    self.remember_region(&location.bucket, name);
                }
                (answer, Some(region))
            }
        };

        // Every rule about what a listing shows is applied in `listing`, over
        // the service's own answer — the adapter's job is to hand that answer
        // over unaltered, which is what keeps the rules testable without one.
        Ok(listing::page_at(
            &location.prefix,
            answer
                .common_prefixes()
                .iter()
                .filter_map(|group| group.prefix().map(ToOwned::to_owned))
                .collect(),
            answer.contents().iter().map(object_from).collect(),
            answer
                .next_continuation_token()
                .map(|token| Cursor(token.to_owned())),
            served_from,
        ))
    }
}

/// The request for one page of a location's contents.
///
/// Kept apart from sending it for the same reason the bucket listing is: a
/// fluent builder can be read back in a test and a sent request cannot.
///
/// `delimiter` is what makes folders exist at all — without it the service
/// returns every key beneath the prefix, however deep, and there is nothing to
/// infer a hierarchy from.
fn list_objects_request(
    client: &Client,
    location: &Location,
    cursor: Option<&Cursor>,
) -> ListObjectsV2FluentBuilder {
    let request = client
        .list_objects_v2()
        .bucket(&location.bucket)
        .delimiter("/")
        .prefix(location.prefix.as_str());
    match cursor {
        Some(Cursor(token)) => request.continuation_token(token),
        None => request,
    }
}

/// Map one SDK object to the domain type.
fn object_from(object: &SdkObject) -> Object {
    Object {
        key: object.key().unwrap_or_default().to_owned(),
        // A key the service reported no size for is reported as zero rather
        // than guessed at: the only objects this happens to are the
        // zero-length ones anyway.
        size: object.size().unwrap_or(0).max(0) as u64,
        last_modified: object
            .last_modified()
            .and_then(|at| at.fmt(DateTimeFormat::DateTimeWithOffset).ok()),
        storage_class: object
            .storage_class()
            .map(|class| class.as_str().to_owned()),
        etag: object.e_tag().map(ToOwned::to_owned),
    }
}

impl S3ObjectStore {
    /// Every bucket, optionally asking for a page size.
    ///
    /// `paged` buys AWS's per-bucket regions, and costs a round trip against
    /// an endpoint that does not define the parameter. Without it a listing
    /// still works everywhere and simply reports no region, which
    /// `Region::Unknown` was written to carry.
    async fn buckets_with_page_size(&self, paged: bool) -> Result<Vec<Bucket>> {
        // Paginated: an account past the service's page size would otherwise
        // be silently truncated, which reads exactly like a smaller account.
        // The page size travels with every page the paginator asks for, so
        // regions come back on all of them, not just the first.
        let request = if paged {
            list_buckets_request(&self.client)
        } else {
            self.client.list_buckets()
        };
        let mut pages = request.into_paginator().send();
        let mut buckets = Vec::new();

        while let Some(page) = pages.next().await {
            let page = page.map_err(|error| {
                classify(
                    &SdkFailure::from_sdk(&error),
                    &self.call(LIST_BUCKETS_ACTION, &self.endpoint, None),
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
        kind: BucketKind::General,
    }
}

/// What the two listings, together, mean.
///
/// A free function on purpose: this is the whole policy of the change, and
/// keeping it out of the async call makes every case above assertable without
/// a network.
fn combine(general: Result<Vec<Bucket>>, directory: Result<Vec<Bucket>>) -> Result<AccountListing> {
    match (general, directory) {
        (Ok(mut general), Ok(directory)) => {
            general.extend(directory);
            Ok(AccountListing::complete(general))
        }
        (Ok(buckets), Err(error)) => partial(buckets, BucketKind::Directory, error),
        (Err(error), Ok(buckets)) => partial(buckets, BucketKind::General, error),
        // Both refused is a refusal, not a partial result. The general
        // listing's cause is the one reported: it is the question the user
        // thinks they asked.
        (Err(general), Err(_)) => Err(general),
    }
}

/// One listing answered and the other did not.
///
/// Only an authorization denial makes this a partial result. A denial is a
/// durable fact about these credentials, and the buckets that did come back
/// are still true. Anything else — an unreachable network, an expired session
/// — is a condition of the moment that applies to both calls equally, and
/// presenting half an account as though it were whole would hide it behind
/// buckets that happen to have arrived first.
fn partial(buckets: Vec<Bucket>, kind: BucketKind, error: Error) -> Result<AccountListing> {
    match error {
        // The action comes from the error rather than from the caller, so what
        // is reported is what the classifier decided the call required.
        Error::AccessDenied { iam_action } => Ok(AccountListing {
            buckets,
            refused: Some(RefusedListing {
                kind,
                action: iam_action,
            }),
        }),
        other => Err(other),
    }
}

/// The region a directory bucket lives in.
///
/// An ARN states the bucket's region as fact, whereas the listing region is an
/// assumption that happens to hold. A directory bucket that exists is always in
/// a region, so an absent, malformed, or blank-region ARN falls back to the
/// region the listing was made against rather than leaving the bucket unknown.
fn directory_bucket_region(arn: Option<&str>, listing_region: &str) -> Region {
    let region = arn
        .and_then(|arn| {
            let mut parts = arn.split(':');
            if parts.next()? != "arn" {
                return None;
            }
            let _partition = parts.next()?;
            let _service = parts.next()?;
            let region = parts.next()?.trim();
            let _account = parts.next()?;
            let _resource = parts.next()?;
            if region.is_empty() {
                None
            } else {
                Some(region)
            }
        })
        .unwrap_or(listing_region);

    Region::Known(region.to_owned())
}

/// The adapter's [`ObjectRead`]: the SDK's byte stream behind the port's
/// pull. A failure mid-body arrives from `try_next` and is reported as the
/// network event it is — there is no HTTP response left to classify by then,
/// only a broken body.
struct SdkRead {
    body: aws_sdk_s3::primitives::ByteStream,
}

#[async_trait]
impl ObjectRead for SdkRead {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        match self.body.try_next().await {
            Ok(chunk) => Ok(chunk.map(|bytes| bytes.to_vec())),
            Err(error) => Err(Error::Network {
                detail: format!("the object's body broke mid-read: {error}"),
            }),
        }
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
    use crate::types::Prefix;
    use aws_config::SdkConfig;
    use aws_sdk_s3::config::retry::RetryConfig;
    use aws_sdk_s3::config::{
        AppName, BehaviorVersion, ConfigBag, Credentials, IdentityCache, Region as SdkRegion,
        RuntimeComponents, SharedCredentialsProvider,
    };
    use aws_sdk_s3::primitives::DateTime;
    use aws_sdk_s3::primitives::SdkBody;
    use aws_smithy_http_client::test_util::{ReplayEvent, StaticReplayClient};
    use aws_smithy_runtime_api::client::http::{
        HttpClient, HttpConnector, HttpConnectorFuture, HttpConnectorSettings, SharedHttpConnector,
    };
    use aws_smithy_runtime_api::client::identity::{
        Identity, IdentityCachePartition, IdentityFuture, ResolveCachedIdentity,
        SharedIdentityResolver,
    };
    use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
    use aws_smithy_runtime_api::client::result::ConnectorError;
    use aws_smithy_runtime_api::http::StatusCode;
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

    fn a_bucket(name: &str, kind: BucketKind) -> Bucket {
        Bucket {
            name: name.to_owned(),
            created: None,
            region: Region::Known(CONNECTION_REGION.to_owned()),
            kind,
        }
    }

    fn denied(action: &'static str) -> Error {
        Error::AccessDenied { iam_action: action }
    }

    #[test]
    fn both_listings_answering_make_one_list() {
        let listing = combine(
            Ok(vec![a_bucket("logs", BucketKind::General)]),
            Ok(vec![a_bucket(
                "fast--apse1-az1--x-s3",
                BucketKind::Directory,
            )]),
        )
        .expect("nothing was refused");

        assert_eq!(listing.buckets.len(), 2);
        assert!(listing.refused.is_none());
    }

    #[test]
    fn the_general_listing_being_refused_keeps_the_directory_buckets() {
        // The account this change exists for: no permission to list ordinary
        // buckets, eight directory buckets that are perfectly visible.
        let listing = combine(
            Err(denied(LIST_BUCKETS_ACTION)),
            Ok(vec![a_bucket(
                "fast--apse1-az1--x-s3",
                BucketKind::Directory,
            )]),
        )
        .expect("a refusal of one listing is not a failure of both");

        assert_eq!(listing.buckets.len(), 1, "what came back is still true");
        let refused = listing.refused.expect("the refusal must be stated");
        assert_eq!(refused.kind, BucketKind::General);
        assert_eq!(
            refused.action, LIST_BUCKETS_ACTION,
            "the action reported is the one the refused call required"
        );
    }

    #[test]
    fn the_directory_listing_being_refused_keeps_the_general_buckets() {
        let listing = combine(
            Ok(vec![a_bucket("logs", BucketKind::General)]),
            Err(denied(LIST_DIRECTORY_BUCKETS_ACTION)),
        )
        .expect("the mirror of the case above");

        assert_eq!(listing.buckets.len(), 1);
        let refused = listing.refused.expect("the refusal must be stated");
        assert_eq!(refused.kind, BucketKind::Directory);
        assert_eq!(
            refused.action, LIST_DIRECTORY_BUCKETS_ACTION,
            "never s3:ListAllMyBuckets — that is a different permission, and \
             asking for the wrong one costs a round trip through whoever grants it"
        );
    }

    #[test]
    fn both_refused_is_a_refusal_not_an_empty_account() {
        let error = combine(
            Err(denied(LIST_BUCKETS_ACTION)),
            Err(denied(LIST_DIRECTORY_BUCKETS_ACTION)),
        )
        .expect_err("nothing was listed and the user may not list anything");

        assert!(
            matches!(error, Error::AccessDenied { iam_action } if iam_action == LIST_BUCKETS_ACTION),
            "the general listing is the question the user thinks they asked"
        );
    }

    #[test]
    fn a_failure_that_is_not_a_denial_fails_the_whole_listing() {
        // A network that is down applies to both calls. Presenting the half
        // that happened to arrive as the whole account would hide it.
        let error = combine(
            Ok(vec![a_bucket("logs", BucketKind::General)]),
            Err(Error::Network {
                detail: "the control plane could not be reached".to_owned(),
            }),
        )
        .expect_err("a transient failure is not a partial result");

        assert!(matches!(error, Error::Network { .. }));
    }

    /// A real object read through the real adapter, end to end.
    ///
    /// `#[ignore]`d for the same reason as its neighbour: it needs an
    /// account. It exists because two things about `GetObject` cannot be
    /// reasoned out from here — whether the body streams in more than one
    /// chunk against a real endpoint (the double scripts that; only the
    /// network proves it), and what the stated length is for an object the
    /// listing reported with a different size than the read answers.
    ///
    /// ```text
    /// CAIXONHO_PROFILE=<profile> CAIXONHO_GET_BUCKET=<name> CAIXONHO_GET_KEY=<key> \
    ///   cargo test -p caixonho-core this_machine_reading -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "needs a real account and a readable object"]
    async fn this_machine_reading_one_object() {
        let profile = std::env::var("CAIXONHO_PROFILE").expect("CAIXONHO_PROFILE");
        let bucket = std::env::var("CAIXONHO_GET_BUCKET").expect("CAIXONHO_GET_BUCKET");
        let key = std::env::var("CAIXONHO_GET_KEY").expect("CAIXONHO_GET_KEY");

        let config = aws_config::defaults(BehaviorVersion::latest())
            .profile_name(&profile)
            .load()
            .await;
        let region = config
            .region()
            .map(|region| region.to_string())
            .expect("the profile states a region");
        let store = S3ObjectStore::over(config, &profile, &region, None);

        let mut content = store
            .get_object(&bucket, &key)
            .await
            .expect("the object opens");
        println!("stated size: {:?}", content.size);

        let mut total = 0u64;
        let mut chunks = 0u32;
        while let Some(chunk) = content.body.next_chunk().await.expect("the body holds") {
            total += chunk.len() as u64;
            chunks += 1;
        }
        println!("read {total} bytes in {chunks} chunks");
        if let Some(stated) = content.size {
            assert_eq!(total, stated, "the stream and the stated size disagree");
        }
    }

    /// Whether a real endpoint honours `If-None-Match: *`, observed rather
    /// than trusted.
    ///
    /// `#[ignore]`d: it needs an account and it **writes**. It is the only
    /// place the central guarantee of `XONHO-0020` can be checked at all —
    /// every unit test proves that the *double* refuses a taken key, which
    /// says nothing about whether the service does. It writes the key twice:
    /// the first conditional write should be `Created`, the second
    /// `KeyTaken`. A second `Created` means this endpoint ignores the
    /// condition, which is the undetectable-in-production case design.md
    /// names — and this test is where it stops being undetectable.
    ///
    /// It leaves an object behind on purpose: deleting it is
    /// `XONHO-0021`'s verb and this change has no business having one.
    ///
    /// ```text
    /// CAIXONHO_PROFILE=<profile> CAIXONHO_PUT_BUCKET=<name> CAIXONHO_PUT_KEY=<key> \
    ///   cargo test -p caixonho-core this_machine_writing -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "needs a real account, and writes to it"]
    async fn this_machine_writing_one_object_twice() {
        let profile = std::env::var("CAIXONHO_PROFILE").expect("CAIXONHO_PROFILE");
        let bucket = std::env::var("CAIXONHO_PUT_BUCKET").expect("CAIXONHO_PUT_BUCKET");
        let key = std::env::var("CAIXONHO_PUT_KEY").expect("CAIXONHO_PUT_KEY");

        let config = aws_config::defaults(BehaviorVersion::latest())
            .profile_name(&profile)
            .load()
            .await;
        let region = config
            .region()
            .map(|region| region.to_string())
            .expect("the profile states a region");
        let store = S3ObjectStore::over(config, &profile, &region, None);

        let dir = std::env::temp_dir().join("caixonho-live-put");
        std::fs::create_dir_all(&dir).expect("fixture dir");
        let path = dir.join("probe.txt");
        std::fs::write(&path, b"caixonho conditional-write probe").expect("fixture file");

        let first = store
            .put_object(&bucket, &key, &path, IfAbsent::Refuse)
            .await
            .expect("the first write is allowed");
        println!("first conditional write: {first:?}");

        let second = store
            .put_object(&bucket, &key, &path, IfAbsent::Refuse)
            .await
            .expect("the second is answered, not failed");
        println!("second conditional write: {second:?}");

        assert_eq!(
            second,
            PutOutcome::KeyTaken,
            "this endpoint does not enforce If-None-Match — the no-clobber guarantee is \
             not real here, which is exactly what this test exists to find out"
        );
        let _ = first;
    }

    /// What a real refusal of a directory bucket looks like, end to end.
    ///
    /// `#[ignore]`d: it needs an account, a directory bucket, and a permission
    /// the caller does not have. It exists because the shape of this failure
    /// could not be reasoned out — the session is obtained inside the SDK's
    /// auth scheme, so what reaches us is whatever that resolver left in the
    /// chain, and guessing at it is how a classifier ends up matching nothing.
    ///
    /// It lists **through the same store** before reading, because that is the
    /// second defect this found: the knowledge of which buckets are directory
    /// buckets lives on the store that listed them, and a store built for the
    /// listing and dropped takes it away, leaving the read to name the wrong
    /// permission.
    ///
    /// ```text
    /// CAIXONHO_PROFILE=<profile> CAIXONHO_DIRECTORY_BUCKET=<name> \
    ///   cargo test -p caixonho-core this_machine_opening -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "needs a real directory bucket the caller may not open"]
    async fn this_machine_opening_a_directory_bucket() {
        let profile = std::env::var("CAIXONHO_PROFILE").expect("CAIXONHO_PROFILE");
        let bucket = std::env::var("CAIXONHO_DIRECTORY_BUCKET").expect("CAIXONHO_DIRECTORY_BUCKET");

        let config = aws_config::defaults(BehaviorVersion::latest())
            .profile_name(&profile)
            .load()
            .await;
        let region = config
            .region()
            .map(|region| region.to_string())
            .expect("the profile states a region");
        let store = S3ObjectStore::over(config, &profile, &region, None);

        let listing = store.list_buckets().await.expect("the account lists");
        println!(
            "listed {} buckets, {} of them directory",
            listing.buckets.len(),
            listing
                .buckets
                .iter()
                .filter(|bucket| bucket.kind == BucketKind::Directory)
                .count()
        );

        let answer = store.list_objects(&Location::bucket(&bucket), None).await;

        match answer {
            Ok(_) => println!("ALLOWED — this bucket opens, so it proves nothing here"),
            Err(cause) => {
                println!("classified as: {cause:?}");

                match cause {
                    Error::AccessDenied { iam_action } => assert_eq!(
                        iam_action, SESSION_ACTION,
                        "reading a directory bucket is refused at the session, and naming any \
                         other permission sends the user to ask for one that changes nothing"
                    ),
                    other => panic!(
                        "expected a denial the user can act on, got {other:?} — this is the run \
                         that caught it arriving as a mystery"
                    ),
                }
            }
        }
    }

    #[tokio::test]
    async fn a_custom_endpoint_is_never_asked_for_directory_buckets() {
        // The HTTP client refuses everything, so the call returning an empty
        // list *is* the assertion that nothing was sent. A guard that merely
        // looked right would fail here.
        let config = SdkConfig::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(SdkRegion::new(CONNECTION_REGION))
            .endpoint_url("https://object-store.example.invalid")
            .credentials_provider(SharedCredentialsProvider::new(test_credentials()))
            .identity_cache(IdentityCache::lazy().build())
            .http_client(RefusingHttpClient)
            .build();
        let store = S3ObjectStore::over(config, "work", CONNECTION_REGION, None);

        assert!(
            !store.offers_directory_buckets(),
            "an S3-compatible service cannot hold directory buckets"
        );
        assert_eq!(
            store
                .directory_buckets()
                .await
                .expect("asking nothing cannot fail"),
            Vec::new(),
            "nothing was sent, so there is nothing to report and no error to \
             explain away"
        );
    }

    #[tokio::test]
    async fn an_aws_connection_is_asked_for_directory_buckets() {
        // The mirror of the test above, and the reason it is not vacuous: on a
        // connection with no custom endpoint the call is issued, so the
        // refusing client turns it into a failure rather than an empty list.
        let store = S3ObjectStore::over(
            probing_config(&RecordingIdentityCache::default()),
            "work",
            CONNECTION_REGION,
            None,
        );

        assert!(store.offers_directory_buckets());
        assert!(
            store.directory_buckets().await.is_err(),
            "the request was actually issued"
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
    fn a_listing_of_a_location_asks_the_service_to_group_by_separator() {
        // `delimiter` is what makes folders exist. Without it the service
        // returns every key beneath the prefix however deep, and there is no
        // hierarchy to infer — the listing would be one flat pour of keys.
        let here = Location::at("photos-bucket", Prefix::parse("holidays/2026"));

        let request = list_objects_request(&client(), &here, None);

        assert_eq!(request.get_delimiter(), &Some("/".to_owned()));
        assert_eq!(request.get_prefix(), &Some("holidays/2026/".to_owned()));
        assert_eq!(request.get_bucket(), &Some("photos-bucket".to_owned()));
        assert_eq!(
            request.get_continuation_token(),
            &None,
            "a first page continues nothing"
        );
    }

    #[test]
    fn continuing_a_listing_hands_the_service_its_own_token_back() {
        let here = Location::bucket("photos-bucket");
        let cursor = Cursor("opaque-token".to_owned());

        let request = list_objects_request(&client(), &here, Some(&cursor));

        assert_eq!(
            request.get_continuation_token(),
            &Some("opaque-token".to_owned())
        );
        assert_eq!(
            request.get_prefix(),
            &Some(String::new()),
            "the bucket root narrows nothing"
        );
    }

    #[test]
    fn an_object_crosses_the_port_with_what_the_service_said_about_it() {
        let object = SdkObject::builder()
            .key("holidays/2026/beach.jpg")
            .size(2_048)
            .storage_class(aws_sdk_s3::types::ObjectStorageClass::Standard)
            .e_tag("\"d41d8cd98f00b204e9800998ecf8427e\"")
            .build();

        let mapped = object_from(&object);

        assert_eq!(mapped.key, "holidays/2026/beach.jpg");
        assert_eq!(mapped.size, 2_048);
        assert_eq!(mapped.storage_class.as_deref(), Some("STANDARD"));
        assert_eq!(
            mapped.etag.as_deref(),
            Some("\"d41d8cd98f00b204e9800998ecf8427e\""),
            "carried although nothing renders it yet, so the port need not \
             change when the remaining columns arrive"
        );
    }

    #[test]
    fn an_object_the_service_gave_no_size_for_is_zero_rather_than_a_guess() {
        let mapped = object_from(&SdkObject::builder().key("marker/").build());

        assert_eq!(mapped.size, 0);
        assert_eq!(mapped.last_modified, None);
        assert_eq!(mapped.storage_class, None);
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

    #[test]
    fn a_well_formed_arn_yields_the_region_named_in_the_arn() {
        let arn = "arn:aws:s3express:ap-southeast-1:123456789012:bucket/example--usw2-az1--x-s3";

        assert_eq!(
            directory_bucket_region(Some(arn), "us-east-1"),
            Region::Known("ap-southeast-1".to_owned()),
            "the ARN states the bucket's region as fact"
        );
    }

    #[test]
    fn no_arn_yields_the_region_the_listing_was_made_against() {
        assert_eq!(
            directory_bucket_region(None, "us-east-1"),
            Region::Known("us-east-1".to_owned()),
            "without an ARN, the listing region is the only evidence"
        );
    }

    #[test]
    fn a_malformed_arn_yields_the_listing_region_never_a_fragment() {
        let malformed_examples = [
            "not-an-arn",
            "arn",
            "arn:aws",
            "arn:aws:s3express",
            "arn:aws:s3express:ap-southeast-1",
            "arn:aws:s3express:ap-southeast-1:123456789012",
            "something:else:entirely:here:123:bucket/foo",
        ];

        for malformed in malformed_examples {
            assert_eq!(
                directory_bucket_region(Some(malformed), "us-east-1"),
                Region::Known("us-east-1".to_owned()),
                "malformed ARN {malformed:?} must fall back to listing region"
            );
        }
    }

    #[test]
    fn an_arn_whose_region_field_is_empty_yields_the_listing_region() {
        let blank_region_arns = [
            "arn:aws:s3express::123456789012:bucket/example--usw2-az1--x-s3",
            "arn:aws:s3express:   :123456789012:bucket/example--usw2-az1--x-s3",
        ];

        for arn in blank_region_arns {
            assert_eq!(
                directory_bucket_region(Some(arn), "us-east-1"),
                Region::Known("us-east-1".to_owned()),
                "empty region in {arn:?} must fall back to listing region"
            );
        }
    }

    /// A page the SDK will actually parse. A shape it would reject fails the
    /// test instead of quietly passing it.
    const ONE_OBJECT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>reports</Name>
  <Prefix></Prefix>
  <KeyCount>1</KeyCount>
  <MaxKeys>1000</MaxKeys>
  <Delimiter>/</Delimiter>
  <IsTruncated>false</IsTruncated>
  <Contents>
    <Key>q1.csv</Key>
    <LastModified>2026-08-21T00:00:00.000Z</LastModified>
    <Size>12</Size>
  </Contents>
</ListBucketResult>"#;

    /// What S3 answers a read addressed to the wrong region with.
    fn redirect_naming(region: &str) -> HttpResponse {
        let mut response = HttpResponse::new(
            StatusCode::try_from(301).expect("a valid status"),
            SdkBody::from("<Error><Code>PermanentRedirect</Code></Error>"),
        );
        response
            .headers_mut()
            .insert("x-amz-bucket-region", region.to_owned());
        response
    }

    fn a_page() -> HttpResponse {
        HttpResponse::new(
            StatusCode::try_from(200).expect("a valid status"),
            SdkBody::from(ONE_OBJECT),
        )
    }

    /// A connection whose every call is answered from a script.
    ///
    /// Retries are off so the request count means what it says: these tests
    /// assert how many times the request went out, and a retry policy would
    /// make that a property of the retry policy instead of a property of the
    /// code under test.
    fn replaying_config(replay: &StaticReplayClient) -> SdkConfig {
        SdkConfig::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(SdkRegion::new(CONNECTION_REGION))
            .credentials_provider(SharedCredentialsProvider::new(test_credentials()))
            .identity_cache(IdentityCache::lazy().build())
            .retry_config(RetryConfig::disabled())
            .http_client(replay.clone())
            .build()
    }

    fn replaying_store(responses: Vec<HttpResponse>) -> (S3ObjectStore, StaticReplayClient) {
        let replay = StaticReplayClient::new(
            responses
                .into_iter()
                .map(|response| ReplayEvent::new(HttpRequest::new(SdkBody::empty()), response))
                .collect(),
        );
        let store = S3ObjectStore::over(
            replaying_config(&replay),
            "work",
            CONNECTION_REGION,
            Some("corp"),
        );
        (store, replay)
    }

    fn reports() -> Location {
        Location {
            bucket: "reports".to_owned(),
            prefix: Prefix::root(),
        }
    }

    fn hosts(replay: &StaticReplayClient) -> Vec<String> {
        replay
            .actual_requests()
            .map(|request| request.uri().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn a_bucket_in_another_region_is_read_from_the_region_the_service_named() {
        let (store, replay) = replaying_store(vec![redirect_naming("us-west-2"), a_page()]);

        let page = store
            .list_objects(&reports(), None)
            .await
            .expect("the redirect is followed and the read succeeds");

        assert_eq!(page.objects.len(), 1, "the second answer is the page");
        assert_eq!(
            page.served_from,
            Some(Region::Known("us-west-2".to_owned())),
            "the page says which region actually served it"
        );

        // The result alone would pass even if the reissue went back to the
        // same region and the script happened to answer it — asserting the
        // request is what proves the named region was used.
        let hosts = hosts(&replay);
        assert_eq!(hosts.len(), 2, "asked twice: {hosts:?}");
        assert!(
            hosts[0].contains(CONNECTION_REGION),
            "the first request goes to the connection's own region: {hosts:?}"
        );
        assert!(
            hosts[1].contains("us-west-2"),
            "the reissue goes where the service said: {hosts:?}"
        );
    }

    #[tokio::test]
    async fn a_service_that_redirects_the_reissue_is_reported_rather_than_followed() {
        // A service that redirects a request already addressed to the region
        // it named has contradicted itself. Following again turns a wrong
        // region into a loop, so the second answer is reported.
        let (store, replay) = replaying_store(vec![
            redirect_naming("us-west-2"),
            redirect_naming("eu-west-1"),
        ]);

        let outcome = store.list_objects(&reports(), None).await;

        assert!(outcome.is_err(), "the second redirect is not followed");
        assert_eq!(hosts(&replay).len(), 2, "and nothing is asked a third time");
    }

    #[tokio::test]
    async fn a_bucket_already_known_to_live_elsewhere_is_addressed_there_from_the_start() {
        let (store, replay) =
            replaying_store(vec![redirect_naming("us-west-2"), a_page(), a_page()]);

        store
            .list_objects(&reports(), None)
            .await
            .expect("the first read follows the redirect");
        let second = store
            .list_objects(&reports(), None)
            .await
            .expect("the second read needs no redirect");

        let hosts = hosts(&replay);
        assert_eq!(hosts.len(), 3, "three requests in total: {hosts:?}");
        assert!(
            hosts[2].contains("us-west-2"),
            "the second read is addressed to the discovered region on its first try: {hosts:?}"
        );
        assert_eq!(
            second.served_from, None,
            "nothing was corrected this time — the page came from where it was addressed"
        );
    }
}
