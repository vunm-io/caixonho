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
use crate::types::{AccountListing, Cursor, KeyPage, Location, Page, Region};

/// Object-storage operations behind one object-safe async trait.
///
/// Starts with exactly what this slice needs; later slices extend it
/// (`list_objects`, transfers) rather than replacing it.
#[async_trait::async_trait]
pub trait ObjectStore: Send + Sync {
    /// List the buckets visible to this connection.
    ///
    /// An account with no buckets is an empty listing — the empty answer is a
    /// truthful result, never an error (`bucket-listing` spec). A listing that
    /// was refused where the other one answered rides along in
    /// [`AccountListing::refused`] rather than failing the call.
    async fn list_buckets(&self) -> Result<AccountListing>;

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

    /// Read one page of what `location` holds.
    ///
    /// The prefixes directly beneath it and the objects directly within it,
    /// grouped by the service rather than filtered here — an object store has
    /// no directories, and the folders a user sees are the groupings it
    /// reports (`object-browsing`, "Folders are inferred").
    ///
    /// `cursor` continues a listing the service said was incomplete. Whether
    /// more remains travels back in the page rather than being hidden inside
    /// the fetching, because the interface has to be able to say so: a listing
    /// that quietly stops early reads exactly like a small folder.
    ///
    /// A location the credentials may not read is an `Err` carrying that
    /// cause, and never an empty page. The distinction is the point of the
    /// whole project — an empty folder and a refused one must never be the
    /// same answer.
    async fn list_objects(&self, location: &Location, cursor: Option<&Cursor>) -> Result<Page>;

    /// Read one page of every key under `location`'s prefix, at every depth
    /// (`XONHO-0030`).
    ///
    /// The same call as [`Self::list_objects`] with the delimiter left off,
    /// which is the whole difference: nothing is grouped, so nothing is
    /// hidden a level down. A folder *is* the set of keys sharing a
    /// beginning, so this is the only listing that can say how big one is or
    /// what deleting it would remove — reading it level by level would count
    /// the top of the tree and call it the tree.
    ///
    /// Paged for [`Self::list_objects`]' reason: the caller has to be able to
    /// tell "that is all of them" from "that is as far as I got", and a walk
    /// that quietly stops early would report a total it does not have.
    async fn list_keys_under(
        &self,
        location: &Location,
        cursor: Option<&Cursor>,
    ) -> Result<KeyPage>;

    /// Read one object's content (`XONHO-0007`).
    ///
    /// `bucket` and the full `key`, not a [`Location`]: a location's prefix
    /// names a folder, and reusing it here would put a folder where a key
    /// belongs and leave the reader to guess which one was meant.
    ///
    /// The content is **pulled** — chunks until `None` — rather than returned
    /// whole, because an object may be arbitrarily large and progress has to
    /// be countable somewhere. A refusal is an `Err` with its classified
    /// cause; a failure *after* the first chunk arrives through the stream
    /// itself, as an error and never as a shorter object.
    async fn get_object(&self, bucket: &str, key: &str) -> Result<ObjectContent>;

    /// Delete one object (`XONHO-0021`).
    ///
    /// The response is the oracle on reversibility: [`Deleted::marker`] holds
    /// the delete marker's version id exactly when the service reported that
    /// one was created — a versioned bucket's answer — and `None` when the
    /// deletion is what it looks like. No `GetBucketVersioning` probe stands
    /// in front of this: it would spend a call and a permission predicting
    /// what this response states as fact, and a prediction can be stale
    /// where the response cannot.
    ///
    /// S3 answers success for a key that holds nothing; that arrives here as
    /// an ordinary `Deleted` and is reported as what the service said.
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<Deleted>;

    /// Remove a delete marker, restoring the object it hides (`XONHO-0021`).
    ///
    /// Its own method rather than a parameter on [`Self::delete_object`]:
    /// the two have different permission surfaces (`s3:DeleteObject` vs
    /// `s3:DeleteObjectVersion`), different failure wordings, and this is
    /// the one that may run without a confirmation — it restores.
    async fn remove_marker(&self, bucket: &str, key: &str, version_id: &str) -> Result<()>;

    /// Read the first `bytes` of one object (`XONHO-0008`).
    ///
    /// A ranged request, not a parameter dressing on [`Self::get_object`]:
    /// the response shape differs — a range answer names the whole object's
    /// size while delivering only the head, and that pair is exactly what an
    /// honest truncation line needs.
    async fn get_object_head(&self, bucket: &str, key: &str, bytes: u64) -> Result<ObjectHead>;

    /// Write one local file to `key` (`XONHO-0020`).
    ///
    /// `if_absent` decides whether the write is conditional. With
    /// [`IfAbsent::Refuse`] the *service* refuses a key that already exists,
    /// which is the whole guarantee: a check this application performs before
    /// an unconditional write is stale the moment it returns, and the race it
    /// loses is one that loses rarely — the worst frequency, because no live
    /// check will ever show it.
    ///
    /// A taken key is therefore [`PutOutcome::KeyTaken`] and **not** an
    /// `Err`: a precondition that did its job is not a failure, and reporting
    /// it as one would put it in the vocabulary the failure panel reserves
    /// for things that went wrong.
    ///
    /// The body is read from `path` as a stream; the caller has already
    /// established the file's size and refused anything a single request
    /// cannot carry.
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        path: &std::path::Path,
        if_absent: IfAbsent,
    ) -> Result<PutOutcome>;

    /// Write the zero-byte marker that makes `key` a folder (`XONHO-0024`).
    ///
    /// Its own method rather than a `put_object` with an empty file, and the
    /// reason is not tidiness: `put_object` takes a path because it streams a
    /// file the user chose, and handing it a temporary empty file to make a
    /// folder would put a filesystem in the middle of an operation that has
    /// nothing to do with one.
    ///
    /// `key` must already end in `/` — [`crate::folder::key_for`] is the one
    /// place that decides what a folder key looks like.
    ///
    /// Only a general purpose bucket may be given this. A directory bucket
    /// removes a directory as soon as it is empty, so the marker would be gone
    /// before the next listing; that refusal is decided above this, from the
    /// kind already in the listing, so no request is spent learning it.
    async fn create_folder(&self, bucket: &str, key: &str) -> Result<()>;
}

/// The first stretch of one object, with the size of the whole.
pub struct ObjectHead {
    /// The whole object's size, when the ranged response named it —
    /// distinct from the head's own length, and the honest half of
    /// "first 64 KiB *of 4.2 MiB*".
    pub total: Option<u64>,
    /// The head's bytes, pulled like any other body.
    pub body: Box<dyn ObjectRead>,
}

impl std::fmt::Debug for ObjectHead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectHead")
            .field("total", &self.total)
            .finish_non_exhaustive()
    }
}

/// What a delete came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deleted {
    /// The delete marker's version id, when the service created one — the
    /// proof an undo exists, and the token it needs.
    pub marker: Option<String>,
}

/// Whether a write refuses to replace what is already at the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfAbsent {
    /// Conditional: the service refuses a key that exists.
    Refuse,
    /// Unconditional. Reachable only from a user answering a question about
    /// that specific object — this is the one way this application ever
    /// replaces one.
    Replace,
}

/// What a write came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutOutcome {
    /// The object is at the key.
    Created,
    /// Something is already there, and [`IfAbsent::Refuse`] was asked for.
    KeyTaken,
    /// The endpoint answered that it does not implement the condition — so
    /// the guarantee is unavailable here, and this is not the same as the
    /// write failing.
    ConditionUnsupported,
}

/// One object's content, being read.
///
/// `Debug` is written by hand because the body is a trait object; what a
/// test failure wants to see is the size anyway.
pub struct ObjectContent {
    /// The size the service stated, when it stated one. Progress is a
    /// fraction only when this is `Some`; the object may also turn out to
    /// disagree with it, which the writer treats as the stream's problem to
    /// reveal, not this field's to promise.
    pub size: Option<u64>,
    /// Where the bytes come from.
    pub body: Box<dyn ObjectRead>,
}

impl std::fmt::Debug for ObjectContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectContent")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// A pull-based reader: the object-safe form of a byte stream.
///
/// Deliberately not a `Stream`: nothing else in the port needs the futures
/// machinery, and one `async fn` per pull is exactly as testable and keeps
/// the crate's dependency set where `XONHO-0017` audited it.
#[async_trait::async_trait]
pub trait ObjectRead: Send {
    /// The next chunk, `None` once the object is complete.
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>>;
}

#[cfg(any(test, feature = "test-support"))]
pub mod double {
    //! Hand-rolled test double: one constructor per canned behaviour, so a
    //! test names the scenario it simulates instead of assembling state.

    use super::ObjectStore;
    use crate::capability::Scope;
    use crate::error::{Error, Result};
    use crate::types::{AccountListing, Bucket, Cursor, KeyPage, Location, Page, Region};

    /// A canned [`ObjectStore`] for tests.
    pub struct StoreDouble {
        outcome: Outcome,
        /// What a listing answers with when the canned outcome succeeds.
        /// Empty unless a test says otherwise, so every existing constructor
        /// keeps meaning what it meant.
        page: Page,
        /// What `get_object` serves. `Unscripted` unless a test says
        /// otherwise, for the same reason `page` defaults empty.
        content: Content,
        /// What `put_object` does. Accepting by default: a test about a
        /// write's *outcome* should say so, and one that merely writes
        /// should not have to.
        writes: Writes,
        /// What `delete_object` and `remove_marker` do. Unversioned by
        /// default, for the same reason `writes` defaults to accepting.
        removals: Removals,
        /// Every version id `remove_marker` was called with, so a test can
        /// assert the *right* marker was removed rather than merely that
        /// something was.
        removed_markers: std::sync::Mutex<Vec<String>>,
        /// Every folder key `create_folder` was asked for, so a test can
        /// assert both *what* was made and — the harder half — that a
        /// directory bucket's refusal spent no request at all.
        folders_made: std::sync::Mutex<Vec<(String, String)>>,
        /// How many object reads were served, so a test can assert a path
        /// that promises not to fetch really fetched nothing.
        gets: std::sync::atomic::AtomicU32,
        /// What the flat listing answers with, page by page. Empty unless a
        /// test says otherwise. A `Vec` of pages rather than one page because
        /// the thing worth testing about a walk is that it *continues* — a
        /// double serving one page forever could only ever prove the first
        /// step.
        under: Vec<KeyPage>,
        /// Every key deleted, in order, so a test can assert which objects a
        /// bulk delete actually removed rather than only how many.
        deleted: std::sync::Mutex<Vec<String>>,
    }

    enum Outcome {
        Buckets(Vec<Bucket>),
        Fail(fn() -> Error),
    }

    /// What `put_object` answers with. Independent of the rest for the same
    /// reason `Content` is.
    enum Writes {
        /// Accepts every write.
        Accepts,
        /// Every conditional write meets a taken key; an unconditional one
        /// succeeds — the shape a real bucket has when the key exists.
        KeyTaken,
        /// The first `n` conditional writes meet a taken key; the one after
        /// that is free. What keep-both actually walks into, and what
        /// `KeyTaken` alone cannot express.
        TakenUntil(std::sync::atomic::AtomicU32),
        /// The endpoint does not implement the condition.
        ConditionUnsupported,
        /// The write itself is refused.
        Refused(fn() -> Error),
    }

    /// What `delete_object` answers with — and what `remove_marker` does.
    enum Removals {
        /// Deletes report no marker: the unversioned shape.
        Unversioned,
        /// Deletes report a marker with this version id.
        Versioned(&'static str),
        /// The delete itself is refused.
        Refused(fn() -> Error),
        /// Deletes succeed with a marker, but removing it is refused — the
        /// shape of `s3:DeleteObject` granted without
        /// `s3:DeleteObjectVersion`.
        MarkerRefused(&'static str, fn() -> Error),
    }

    /// What `get_object` answers with. Independent of `Outcome`: a test
    /// scripting content should not have to decide bucket-listing behaviour
    /// to do it.
    enum Content {
        /// No content scripted; `get_object` refuses like a listing failure
        /// would, so a test that forgot to script content hears about it.
        Unscripted,
        /// Chunks served whole, then a clean end.
        Chunks(Vec<Vec<u8>>),
        /// Chunks served, then the stream breaks.
        BreaksAfter(Vec<Vec<u8>>),
        /// The read itself is refused.
        Refused(fn() -> Error),
    }

    impl StoreDouble {
        /// Succeeds with the given buckets.
        pub fn with_buckets(buckets: Vec<Bucket>) -> Self {
            Self {
                outcome: Outcome::Buckets(buckets),
                page: Page::default(),
                content: Content::Unscripted,
                writes: Writes::Accepts,
                removals: Removals::Unversioned,
                removed_markers: std::sync::Mutex::new(Vec::new()),
                folders_made: std::sync::Mutex::new(Vec::new()),
                gets: std::sync::atomic::AtomicU32::new(0),
                under: Vec::new(),
                deleted: std::sync::Mutex::new(Vec::new()),
            }
        }

        /// The page a listing answers with. Chained onto any succeeding
        /// constructor: `StoreDouble::allows_listing().listing(page)`.
        pub fn listing(mut self, page: Page) -> Self {
            self.page = page;
            self
        }

        /// The pages the flat listing answers with, in order. Chained the
        /// same way `listing` is.
        pub fn under(mut self, pages: Vec<KeyPage>) -> Self {
            self.under = pages;
            self
        }

        /// Every key `delete_object` has been called with, in order.
        pub fn deleted_keys(&self) -> Vec<String> {
            self.deleted
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }

        /// Succeeds with an empty account.
        pub fn empty_account() -> Self {
            Self::with_buckets(Vec::new())
        }

        /// Fails with `no credentials` for the given profile.
        pub fn no_credentials() -> Self {
            Self::failing(|| Error::NoCredentials {
                profile: "double".into(),
            })
        }

        /// Fails with an expired session.
        pub fn expired_session() -> Self {
            Self::failing(|| Error::SessionRejected {
                profile: "double".into(),
                sso_session: Some("corp".into()),
                problem: crate::error::SessionProblem::Expired,
            })
        }

        /// Fails with a TLS trust failure.
        pub fn tls_trust() -> Self {
            Self::failing(|| Error::TlsTrust {
                endpoint: "s3.example.test".into(),
            })
        }

        /// Fails with an unreachable network.
        pub fn network() -> Self {
            Self::failing(|| Error::Network {
                detail: "connection refused (double)".into(),
            })
        }

        /// Fails with a service-side denial of the listing.
        pub fn access_denied() -> Self {
            Self::failing(|| Error::AccessDenied {
                iam_action: "s3:ListAllMyBuckets",
            })
        }

        /// Answers a probe: the credentials may list this bucket, and it
        /// holds nothing.
        pub fn allows_listing() -> Self {
            Self::with_buckets(Vec::new())
        }

        /// Fails with a service-side denial of listing one bucket's contents
        /// — what a probe meets when the policy withholds `s3:ListBucket`.
        pub fn bucket_access_denied() -> Self {
            Self::failing(|| Error::AccessDenied {
                iam_action: "s3:ListBucket",
            })
        }

        /// Fails because the credentials themselves were refused as invalid
        /// — a different cause from an expired session, and a different fix.
        pub fn rejected_credentials() -> Self {
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
        pub fn wrong_region() -> Self {
            Self::failing(|| Error::Unexpected {
                detail: "the service reported `PermanentRedirect` (HTTP 301)".into(),
            })
        }

        /// Fails the way a busy account answers: slow down. Not a denial,
        /// and it must never be recorded as one — a throttled account would
        /// otherwise render as a wall of locks.
        pub fn throttled() -> Self {
            Self::failing(|| Error::Unexpected {
                detail: "the service reported `SlowDown` (HTTP 503)".into(),
            })
        }

        /// Fails with a missing-configuration error.
        pub fn missing_configuration() -> Self {
            Self::failing(|| Error::MissingConfiguration {
                profile: Some("double".into()),
                detail: "no region configured (double)".into(),
            })
        }

        /// Fails with an unclassifiable error.
        pub fn unexpected() -> Self {
            Self::failing(|| Error::Unexpected {
                detail: "internal service error (double)".into(),
            })
        }

        /// Serves these chunks for any `get_object`, then a clean end.
        pub fn serving_chunks(chunks: Vec<Vec<u8>>) -> Self {
            let mut double = Self::allows_listing();
            double.content = Content::Chunks(chunks);
            double
        }

        /// Serves these chunks for any `get_object`, then breaks — the shape
        /// of a connection lost mid-object, which must arrive as an error
        /// and never as a shorter object.
        pub fn content_breaking_after(chunks: Vec<Vec<u8>>) -> Self {
            let mut double = Self::allows_listing();
            double.content = Content::BreaksAfter(chunks);
            double
        }

        /// Deletes answer with this delete marker — the versioned shape.
        pub fn versioned(marker: &'static str) -> Self {
            let mut double = Self::allows_listing();
            double.removals = Removals::Versioned(marker);
            double
        }

        /// Refuses `delete_object` the way a policy without
        /// `s3:DeleteObject` answers.
        pub fn delete_refused() -> Self {
            let mut double = Self::allows_listing();
            double.removals = Removals::Refused(|| Error::AccessDenied {
                iam_action: "s3:DeleteObject",
            });
            double
        }

        /// Deletes succeed with `marker`, but removing it is refused — the
        /// undo-denied shape.
        pub fn marker_removal_refused(marker: &'static str) -> Self {
            let mut double = Self::allows_listing();
            double.removals = Removals::MarkerRefused(marker, || Error::AccessDenied {
                iam_action: "s3:DeleteObjectVersion",
            });
            double
        }

        /// How many object reads this double has served.
        pub fn gets_served(&self) -> u32 {
            self.gets.load(std::sync::atomic::Ordering::SeqCst)
        }

        /// Every `(bucket, key)` a folder was asked to be made at.
        ///
        /// Empty is the assertion that matters most: a directory bucket's
        /// refusal must be decided from the kind already in the listing, so it
        /// spends no request — and only this can tell "refused without asking"
        /// apart from "asked and was refused".
        pub fn folders_made(&self) -> Vec<(String, String)> {
            self.folders_made
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }

        /// The version ids `remove_marker` has been called with.
        pub fn markers_removed(&self) -> Vec<String> {
            self.removed_markers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }

        /// Every conditional write meets a taken key.
        pub fn key_taken() -> Self {
            let mut double = Self::allows_listing();
            double.writes = Writes::KeyTaken;
            double
        }

        /// The first `taken` conditional writes are refused; the next
        /// succeeds. A bucket where `x.csv` and `x (2).csv` exist and
        /// `x (3).csv` does not is `taken_until(2)`.
        pub fn taken_until(taken: u32) -> Self {
            let mut double = Self::allows_listing();
            double.writes = Writes::TakenUntil(std::sync::atomic::AtomicU32::new(taken));
            double
        }

        /// The endpoint does not implement conditional writes.
        pub fn condition_unsupported() -> Self {
            let mut double = Self::allows_listing();
            double.writes = Writes::ConditionUnsupported;
            double
        }

        /// Refuses `put_object` the way a policy without `s3:PutObject`
        /// answers.
        pub fn put_refused() -> Self {
            let mut double = Self::allows_listing();
            double.writes = Writes::Refused(|| Error::AccessDenied {
                iam_action: "s3:PutObject",
            });
            double
        }

        /// Refuses `get_object` the way a policy without `s3:GetObject`
        /// answers.
        pub fn get_refused() -> Self {
            let mut double = Self::allows_listing();
            double.content = Content::Refused(|| Error::AccessDenied {
                iam_action: "s3:GetObject",
            });
            double
        }

        fn failing(make: fn() -> Error) -> Self {
            Self {
                outcome: Outcome::Fail(make),
                page: Page::default(),
                content: Content::Unscripted,
                writes: Writes::Accepts,
                removals: Removals::Unversioned,
                removed_markers: std::sync::Mutex::new(Vec::new()),
                folders_made: std::sync::Mutex::new(Vec::new()),
                gets: std::sync::atomic::AtomicU32::new(0),
                under: Vec::new(),
                deleted: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    /// The break `content_breaking_after` serves: a network failure, the
    /// ordinary way to lose an object mid-body.
    fn network_break() -> Error {
        Error::Network {
            detail: "connection lost mid-object (double)".into(),
        }
    }

    /// The pulled-content half of the double: chunks front-to-back, then
    /// the scripted ending.
    struct ReadDouble {
        chunks: std::collections::VecDeque<Vec<u8>>,
        then: Option<fn() -> Error>,
    }

    #[async_trait::async_trait]
    impl super::ObjectRead for ReadDouble {
        async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
            if let Some(chunk) = self.chunks.pop_front() {
                return Ok(Some(chunk));
            }
            match self.then.take() {
                Some(make) => Err(make()),
                None => Ok(None),
            }
        }
    }

    #[async_trait::async_trait]
    impl ObjectStore for StoreDouble {
        async fn list_buckets(&self) -> Result<AccountListing> {
            match &self.outcome {
                Outcome::Buckets(buckets) => Ok(AccountListing::complete(buckets.clone())),
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

        async fn delete_object(&self, _bucket: &str, key: &str) -> Result<super::Deleted> {
            self.deleted
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(key.to_owned());
            Ok(match &self.removals {
                Removals::Unversioned => super::Deleted { marker: None },
                Removals::Versioned(marker) | Removals::MarkerRefused(marker, _) => {
                    super::Deleted {
                        marker: Some((*marker).to_owned()),
                    }
                }
                Removals::Refused(make) => return Err(make()),
            })
        }

        async fn remove_marker(&self, _bucket: &str, _key: &str, version_id: &str) -> Result<()> {
            if let Removals::MarkerRefused(_, make) = &self.removals {
                return Err(make());
            }
            self.removed_markers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(version_id.to_owned());
            Ok(())
        }

        /// Folder markers answer from the same scripted `Writes` a file write
        /// does, minus the conditional branches: a folder has no keep-both.
        async fn create_folder(&self, bucket: &str, key: &str) -> Result<()> {
            self.folders_made
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((bucket.to_owned(), key.to_owned()));
            match &self.writes {
                Writes::Refused(make) => Err(make()),
                _ => Ok(()),
            }
        }

        /// Writes answer from the scripted `Writes`. The double reads the
        /// file, so a test that scripts a path which does not exist hears
        /// about it here rather than getting a false success.
        async fn put_object(
            &self,
            _bucket: &str,
            _key: &str,
            path: &std::path::Path,
            if_absent: super::IfAbsent,
        ) -> Result<super::PutOutcome> {
            std::fs::metadata(path).map_err(|error| Error::Destination {
                detail: error.to_string(),
            })?;
            Ok(match (&self.writes, if_absent) {
                (Writes::Refused(make), _) => return Err(make()),
                (Writes::ConditionUnsupported, super::IfAbsent::Refuse) => {
                    super::PutOutcome::ConditionUnsupported
                }
                (Writes::KeyTaken, super::IfAbsent::Refuse) => super::PutOutcome::KeyTaken,
                (Writes::TakenUntil(left), super::IfAbsent::Refuse) => {
                    // Counts down across calls, so a caller walking
                    // candidates meets a run of taken keys and then a free
                    // one — which is the shape keep-both is written for.
                    if left
                        .fetch_update(
                            std::sync::atomic::Ordering::SeqCst,
                            std::sync::atomic::Ordering::SeqCst,
                            |n| n.checked_sub(1),
                        )
                        .is_ok()
                    {
                        super::PutOutcome::KeyTaken
                    } else {
                        super::PutOutcome::Created
                    }
                }
                // Unconditional writes land whatever is there — including
                // under `ConditionUnsupported`, where the condition was never
                // the question.
                _ => super::PutOutcome::Created,
            })
        }

        /// The head is the first `bytes` of the same scripted content, with
        /// the content's full size as the total — which is exactly the
        /// relationship the real service maintains, so a test that scripts
        /// one body gets both reads consistent for free.
        async fn get_object_head(
            &self,
            bucket: &str,
            key: &str,
            bytes: u64,
        ) -> Result<super::ObjectHead> {
            let mut content = self.get_object(bucket, key).await?;
            let total = content.size;
            let mut gathered: Vec<u8> = Vec::new();
            while (gathered.len() as u64) < bytes {
                match content.body.next_chunk().await? {
                    Some(chunk) => gathered.extend_from_slice(&chunk),
                    None => break,
                }
            }
            gathered.truncate(usize::try_from(bytes).unwrap_or(usize::MAX));
            Ok(super::ObjectHead {
                total,
                body: Box::new(ReadDouble {
                    chunks: std::iter::once(gathered).collect(),
                    then: None,
                }),
            })
        }

        /// Content comes from the scripted `Content`, not from `Outcome`:
        /// a test choosing what a read serves should not have to decide what
        /// a listing does.
        async fn get_object(&self, _bucket: &str, _key: &str) -> Result<super::ObjectContent> {
            self.gets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let (chunks, then): (&[Vec<u8>], Option<fn() -> Error>) = match &self.content {
                Content::Unscripted => {
                    return Err(Error::Unexpected {
                        detail: "this double scripts no content — use serving_chunks, \
                                 content_breaking_after or get_refused"
                            .into(),
                    });
                }
                Content::Refused(make) => return Err(make()),
                Content::Chunks(chunks) => (chunks, None),
                Content::BreaksAfter(chunks) => (chunks, Some(network_break as fn() -> Error)),
            };
            let size = then
                .is_none()
                .then(|| chunks.iter().map(|c| c.len() as u64).sum());
            Ok(super::ObjectContent {
                size,
                body: Box::new(ReadDouble {
                    chunks: chunks.iter().cloned().collect(),
                    then,
                }),
            })
        }

        /// And to the listing: a double that can read answers with its page,
        /// a double that fails fails the same way. A refusal is therefore an
        /// `Err` here too, never an empty page.
        async fn list_objects(
            &self,
            _location: &Location,
            _cursor: Option<&Cursor>,
        ) -> Result<Page> {
            match &self.outcome {
                Outcome::Buckets(_) => Ok(self.page.clone()),
                Outcome::Fail(make) => Err(make()),
            }
        }

        /// Serves the scripted pages in order, keyed by the cursor it handed
        /// out last time. A cursor is `page-<n>`, which is opaque to the
        /// caller exactly as the service's own token is — a walk that peeked
        /// inside one would be relying on something the real service never
        /// promised.
        async fn list_keys_under(
            &self,
            _location: &Location,
            cursor: Option<&Cursor>,
        ) -> Result<KeyPage> {
            if let Outcome::Fail(make) = &self.outcome {
                return Err(make());
            }
            let index = match cursor {
                None => 0,
                Some(Cursor(token)) => match token.strip_prefix("page-") {
                    Some(n) => n.parse::<usize>().unwrap_or(usize::MAX),
                    None => usize::MAX,
                },
            };
            let Some(page) = self.under.get(index) else {
                return Ok(KeyPage::default());
            };
            // The token is written here rather than by the test, so a page's
            // `more` says only *whether* there is more and this decides
            // *where* — the split the real service has.
            Ok(KeyPage {
                keys: page.keys.clone(),
                more: page
                    .more
                    .is_some()
                    .then(|| Cursor(format!("page-{}", index + 1))),
            })
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

    use super::double::StoreDouble;
    use super::{IfAbsent, ObjectStore, PutOutcome};
    use crate::capability::{CapabilityStore, Observation, Scope, observation_for};
    use crate::error::Error;
    use crate::types::{
        Bucket, BucketKind, Cursor, Folder, Location, Object, Page, Prefix, Region,
    };

    fn bucket(name: &str, created: Option<&str>) -> Bucket {
        Bucket {
            name: name.into(),
            created: created.map(Into::into),
            region: Region::Unknown,
            kind: BucketKind::General,
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

        let listed = store
            .list_buckets()
            .await
            .expect("listing must succeed")
            .buckets;

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
                kind: BucketKind::General,
            },
            Bucket {
                name: "backups".into(),
                created: None,
                region: Region::Unknown,
                kind: BucketKind::General,
            },
        ];
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::with_buckets(canned));

        let listed = store
            .list_buckets()
            .await
            .expect("listing must succeed")
            .buckets;

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

        assert!(listed.buckets.is_empty());
        assert!(
            listed.refused.is_none(),
            "an empty account refused nothing — the two are different answers"
        );
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

    #[tokio::test]
    async fn a_location_crosses_the_port_as_folders_and_objects() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::allows_listing().listing(Page {
            folders: vec![Folder {
                prefix: Prefix::parse("photos/vacation/"),
            }],
            objects: vec![Object {
                key: "photos/cat.jpg".to_owned(),
                size: 1_024,
                last_modified: None,
                storage_class: None,
                etag: None,
            }],
            more: None,
            served_from: None,
        }));

        let page = store
            .list_objects(&Location::at("holiday", Prefix::parse("photos")), None)
            .await
            .expect("a readable location");

        assert_eq!(page.folders[0].name(), "vacation");
        assert_eq!(page.objects[0].key, "photos/cat.jpg");
        assert!(!page.is_truncated(), "the service said this was all of it");
    }

    #[tokio::test]
    async fn a_truncated_page_says_so_rather_than_looking_like_a_small_folder() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::allows_listing().listing(Page {
            more: Some(Cursor("next".to_owned())),
            served_from: None,
            ..Page::default()
        }));

        let page = store
            .list_objects(&Location::bucket("holiday"), None)
            .await
            .expect("a readable location");

        assert!(
            page.is_truncated(),
            "a listing that stops early without saying so is indistinguishable \
             from one that has ended"
        );
    }

    #[tokio::test]
    async fn a_refused_location_is_an_error_and_never_an_empty_page() {
        // The distinction the whole project turns on, one level below the
        // bucket list: emptiness is a fact about a location that was read.
        // A refusal is not a location that holds nothing.
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::bucket_access_denied());

        match store.list_objects(&Location::bucket("holiday"), None).await {
            Err(Error::AccessDenied { iam_action }) => assert_eq!(iam_action, "s3:ListBucket"),
            other => panic!("expected AccessDenied, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn every_other_cause_stays_itself_across_the_port_too() {
        // None of these is a refusal, and none of them is emptiness. A window
        // that collapsed them would send the user to fix the wrong thing.
        let cases: [(StoreDouble, &str); 3] = [
            (StoreDouble::expired_session(), "SessionRejected"),
            (StoreDouble::network(), "Network"),
            (StoreDouble::tls_trust(), "TlsTrust"),
        ];

        for (double, expected) in cases {
            let store: Box<dyn ObjectStore> = Box::new(double);
            let outcome = store.list_objects(&Location::bucket("holiday"), None).await;

            let error = outcome.expect_err("this double fails");
            let named = match error {
                Error::SessionRejected { .. } => "SessionRejected",
                Error::Network { .. } => "Network",
                Error::TlsTrust { .. } => "TlsTrust",
                other => panic!("unexpected cause: {other:?}"),
            };
            assert_eq!(named, expected);
        }
    }

    /// `object-transfer` spec, "Downloading an object" — the port half:
    /// content arrives as pulled chunks, byte-identical in sum, with the
    /// size when one was stated.
    #[tokio::test]
    async fn an_object_is_read_as_the_chunks_it_came_in() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::serving_chunks(vec![
            b"hello ".to_vec(),
            b"little ".to_vec(),
            b"pail".to_vec(),
        ]));

        let mut content = store
            .get_object("reports", "daily/summary.csv")
            .await
            .expect("this double serves");
        assert_eq!(content.size, Some(17));

        let mut gathered = Vec::new();
        while let Some(chunk) = content.body.next_chunk().await.expect("no failure canned") {
            gathered.extend_from_slice(&chunk);
        }
        assert_eq!(gathered, b"hello little pail");
    }

    /// A failure after some bytes must arrive as an error from the stream,
    /// not as a shorter object — the writer's no-partial-file rule depends
    /// on being told.
    #[tokio::test]
    async fn a_mid_stream_failure_is_an_error_not_a_shorter_object() {
        let store: Box<dyn ObjectStore> =
            Box::new(StoreDouble::content_breaking_after(vec![b"first".to_vec()]));

        let mut content = store
            .get_object("reports", "big.bin")
            .await
            .expect("starts fine");
        let first = content
            .body
            .next_chunk()
            .await
            .expect("first chunk arrives");
        assert_eq!(first.as_deref(), Some(b"first".as_slice()));

        let outcome = content.body.next_chunk().await;
        assert!(
            matches!(outcome, Err(Error::Network { .. })),
            "got {outcome:?}"
        );
    }

    /// A refused read names the permission it needed, in the same shape
    /// every other refusal already has.
    #[tokio::test]
    async fn a_refused_read_names_the_get_permission() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::get_refused());

        match store.get_object("reports", "secret.pdf").await {
            Err(Error::AccessDenied { iam_action }) => assert_eq!(iam_action, "s3:GetObject"),
            other => panic!("expected AccessDenied, got {other:?}"),
        }
    }

    /// `object-transfer` delta (`XONHO-0020`) — the port half of the
    /// guarantee: a taken key comes back as an outcome the caller can ask
    /// the user about, never as an error.
    #[tokio::test]
    async fn a_taken_key_is_an_outcome_and_not_a_failure() {
        let file = a_file("port-taken", b"contents");
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::key_taken());

        let outcome = store
            .put_object("reports", "daily/summary.csv", &file, IfAbsent::Refuse)
            .await
            .expect("a refused precondition is not an Err");
        assert_eq!(outcome, PutOutcome::KeyTaken);
    }

    /// And the only way past it: an unconditional write, which the same
    /// double accepts because the key existing was never the obstacle.
    #[tokio::test]
    async fn replacing_is_the_unconditional_write() {
        let file = a_file("port-replace", b"contents");
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::key_taken());

        let outcome = store
            .put_object("reports", "daily/summary.csv", &file, IfAbsent::Replace)
            .await
            .expect("writes");
        assert_eq!(outcome, PutOutcome::Created);
    }

    #[tokio::test]
    async fn an_endpoint_without_the_condition_says_so_rather_than_writing() {
        let file = a_file("port-unsupported", b"contents");
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::condition_unsupported());

        let outcome = store
            .put_object("reports", "k", &file, IfAbsent::Refuse)
            .await
            .expect("not an error either");
        assert_eq!(
            outcome,
            PutOutcome::ConditionUnsupported,
            "the guarantee being unavailable is its own answer, not a write"
        );
    }

    #[tokio::test]
    async fn a_refused_write_names_the_put_permission() {
        let file = a_file("port-denied", b"contents");
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::put_refused());

        match store
            .put_object("reports", "k", &file, IfAbsent::Refuse)
            .await
        {
            Err(Error::AccessDenied { iam_action }) => assert_eq!(iam_action, "s3:PutObject"),
            other => panic!("expected AccessDenied, got {other:?}"),
        }
    }

    fn a_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("caixonho-store-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir");
        let path = dir.join("file.bin");
        std::fs::write(&path, bytes).expect("fixture file");
        path
    }

    /// `object-deletion` spec (`XONHO-0021`) — the port half: the response
    /// is the oracle on reversibility.
    #[tokio::test]
    async fn an_unversioned_delete_reports_no_marker() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::allows_listing());

        let deleted = store
            .delete_object("reports", "daily/summary.csv")
            .await
            .expect("deletes");
        assert_eq!(deleted.marker, None, "no marker means no undo is offered");
    }

    #[tokio::test]
    async fn a_versioned_delete_carries_the_marker_that_undoes_it() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::versioned("mk-123"));

        let deleted = store
            .delete_object("reports", "daily/summary.csv")
            .await
            .expect("deletes");
        assert_eq!(deleted.marker.as_deref(), Some("mk-123"));
    }

    /// Undo removes exactly the marker the delete reported — not "a"
    /// version, the one the response named.
    #[tokio::test]
    async fn undo_removes_exactly_the_reported_marker() {
        let double = std::sync::Arc::new(StoreDouble::versioned("mk-123"));
        let store: std::sync::Arc<dyn ObjectStore> = double.clone();

        let deleted = store
            .delete_object("reports", "daily/summary.csv")
            .await
            .expect("deletes");
        let marker = deleted.marker.expect("versioned");
        store
            .remove_marker("reports", "daily/summary.csv", &marker)
            .await
            .expect("restores");

        assert_eq!(double.markers_removed(), vec!["mk-123".to_owned()]);
    }

    #[tokio::test]
    async fn a_refused_delete_names_the_delete_permission() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::delete_refused());

        match store.delete_object("reports", "k").await {
            Err(Error::AccessDenied { iam_action }) => {
                assert_eq!(iam_action, "s3:DeleteObject")
            }
            other => panic!("expected AccessDenied, got {other:?}"),
        }
    }

    /// The asymmetric grant: allowed to delete, not to un-delete. The undo's
    /// refusal names the version permission, which is the one to go ask for.
    #[tokio::test]
    async fn a_refused_undo_names_the_version_permission() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::marker_removal_refused("mk-9"));

        let deleted = store.delete_object("reports", "k").await.expect("deletes");
        let marker = deleted.marker.expect("the delete still reported it");
        match store.remove_marker("reports", "k", &marker).await {
            Err(Error::AccessDenied { iam_action }) => {
                assert_eq!(iam_action, "s3:DeleteObjectVersion")
            }
            other => panic!("expected AccessDenied, got {other:?}"),
        }
    }

    /// `object-preview` spec — the port half of the truncation line: a head
    /// delivers its stretch and the whole object's size, as a pair.
    #[tokio::test]
    async fn a_head_carries_its_stretch_and_the_whole_objects_size() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::serving_chunks(vec![
            b"0123456789".to_vec(),
            b"abcdefghij".to_vec(),
        ]));

        let mut head = store
            .get_object_head("reports", "big.log", 5)
            .await
            .expect("serves");
        assert_eq!(
            head.total,
            Some(20),
            "the whole object's size, not the head's"
        );

        let mut gathered = Vec::new();
        while let Some(chunk) = head.body.next_chunk().await.expect("holds") {
            gathered.extend_from_slice(&chunk);
        }
        assert_eq!(gathered, b"01234", "exactly the asked-for stretch");
    }

    #[tokio::test]
    async fn a_head_of_a_small_object_is_the_whole_object() {
        let store: Box<dyn ObjectStore> =
            Box::new(StoreDouble::serving_chunks(vec![b"tiny".to_vec()]));

        let mut head = store
            .get_object_head("reports", "s.txt", 65536)
            .await
            .expect("serves");
        assert_eq!(head.total, Some(4));
        let mut gathered = Vec::new();
        while let Some(chunk) = head.body.next_chunk().await.expect("holds") {
            gathered.extend_from_slice(&chunk);
        }
        assert_eq!(gathered, b"tiny");
    }

    #[tokio::test]
    async fn a_refused_head_names_the_get_permission() {
        let store: Box<dyn ObjectStore> = Box::new(StoreDouble::get_refused());

        match store.get_object_head("reports", "k.txt", 64).await {
            Err(Error::AccessDenied { iam_action }) => assert_eq!(iam_action, "s3:GetObject"),
            other => panic!("expected AccessDenied, got {other:?}"),
        }
    }
}
