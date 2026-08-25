//! What the app hands core once, and asks for work through afterwards.
//!
//! The app owns exactly one multi-thread tokio runtime and passes its
//! [`Handle`] here; core never builds a runtime of its own. That is what lets
//! the future CLI reuse this crate — it owns its own runtime — and it keeps
//! every network call off the render thread by construction: work is spawned
//! onto the handle, and the result comes back as one tagged message.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::runtime::Handle;

use crate::adapter::S3ObjectStore;
use crate::capability::{Capability, CapabilityStore, CredentialsId, Observation, Scope};
use crate::connection::{self, Connection, ConnectionSource};
use crate::connections::{self, ConfigDirectory, ConnectionFile};
use crate::credentials::{CredentialSecret, Keyring, SecretStore, StoredCredential};
use crate::diagnostics;
use crate::error::{Error, Result};
use crate::outcome::{Outcome, TaggedOutcome};
use crate::probe::{ProbeScheduler, ProbeSink, ProbeTarget};
use crate::profiles::ConfigPaths;
use crate::sso::{Abandon, DeviceAuthorization, RealTime, SignInLocation, SignInOutcome};
use crate::sso_adapter::SsoOidcSignIn;
use crate::store::ObjectStore;
use crate::tls::HttpStack;
use crate::transfer::{self, Cancel, Collision, DownloadOutcome, UploadOutcome};
use crate::types::{ConnectionId, Cursor, Location, Page};

/// The long-lived context every request runs in.
#[derive(Clone)]
pub struct Session {
    runtime: Handle,
    http: HttpStack,
    paths: ConfigPaths,
    /// Where the secret half of a stored credential is kept.
    ///
    /// Behind an `Arc<dyn _>` so a test can put a double where the OS
    /// keychain goes, through the same door production uses. Holding one
    /// costs nothing at startup: `keyring` initialises the platform store
    /// lazily, on the first entry anyone builds.
    secrets: Arc<dyn SecretStore>,
    /// Where the half of a stored connection that is not secret is kept, so
    /// that a connection entered here is still offered after a restart.
    ///
    /// Behind an `Arc<dyn _>` for the same reason the credential store is: a
    /// test puts a double where the platform's config directory goes, through
    /// the same door production uses. Holding one costs nothing at startup —
    /// the path is resolved per call, and nothing is read until something is
    /// asked of it.
    connections: Arc<dyn ConnectionFile>,
    /// Shared by every clone, so an observation made on a runtime thread is
    /// the same one the frontend reads. The lock is only ever held for a map
    /// lookup — never across an await.
    capabilities: Arc<Mutex<CapabilityStore>>,
    /// The probe scheduler for the connection currently open, if one is.
    ///
    /// Shared by every clone for the same reason the store above is: a
    /// viewport reported through the frontend's clone has to reach the probes
    /// running on the runtime's. Replaced when a connection opens, so a
    /// scheduler never outlives the store it probes through.
    scheduler: Arc<Mutex<Option<ProbeScheduler>>>,
    /// The store for the connection currently open, if one is.
    ///
    /// Kept because a listing is not a one-off. Reading a location, then
    /// walking into a folder, then asking for its next page are three
    /// requests against the same connection, and re-opening it for each would
    /// re-resolve its credentials each time — seven seconds on this machine,
    /// twenty-six on the first run of a day, both measured. `XONHO-0004`
    /// removed that wait from startup; browsing must not put it back, once
    /// per folder.
    ///
    /// Shared by every clone and replaced when a connection opens, exactly
    /// like the scheduler above, so a store never outlives the connection it
    /// speaks for.
    store: Arc<Mutex<Option<Arc<dyn ObjectStore>>>>,
    /// Where settled probes are announced, once a frontend has asked to hear
    /// about them. Shared, and read when a scheduler is installed, so
    /// registering once covers every connection opened afterwards.
    settled: Arc<Mutex<Option<ProbeSink>>>,
}

impl std::fmt::Debug for Session {
    /// Hand-written because a session now carries the frontend's sink, which
    /// is a closure and has no `Debug`. Terse for the same reason the
    /// scheduler's is: the rest is behind locks this must not take.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Build a session around the app's runtime.
    pub fn new(runtime: Handle, http: HttpStack, paths: ConfigPaths) -> Self {
        Self {
            runtime,
            http,
            paths,
            secrets: Arc::new(Keyring),
            connections: Arc::new(ConfigDirectory),
            capabilities: Arc::new(Mutex::new(CapabilityStore::new())),
            scheduler: Arc::default(),
            store: Arc::default(),
            settled: Arc::default(),
        }
    }

    /// The same session, reading and writing secrets somewhere else.
    ///
    /// Test-only: the OS keychain is the only credential store the app ships
    /// with, and a way to redirect it in production would be a way to put
    /// secrets somewhere the spec forbids.
    #[cfg(test)]
    pub(crate) fn with_secret_store(mut self, secrets: Arc<dyn SecretStore>) -> Self {
        self.secrets = secrets;
        self
    }

    /// The same session, remembering connections somewhere else.
    ///
    /// Test-only, for the same reason as [`Self::with_secret_store`]: the
    /// platform's own config directory is the only place the app writes, and
    /// a way to redirect it in production would be a way to scatter a user's
    /// connections across the disk.
    #[cfg(test)]
    pub(crate) fn with_connection_file(mut self, connections: Arc<dyn ConnectionFile>) -> Self {
        self.connections = connections;
        self
    }

    /// Where this session reads profiles from.
    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    /// The connections entered in this application on a previous run.
    ///
    /// Read at startup, so a credential typed in yesterday is offered again
    /// today. Synchronous and cheap — one small file, no credential store and
    /// no network — like [`crate::discover`] beside it, which the frontend
    /// already calls while building its first screen.
    ///
    /// A file that cannot be read is an error rather than an empty list: a
    /// machine whose connections could not be read is not a machine with no
    /// connections, and quietly saying the second would invite the user to
    /// enter the credential again on top of the one already there.
    pub fn stored_connections(&self) -> Result<Vec<StoredCredential>> {
        connections::list(self.connections.as_ref())
    }

    /// The credentials observations are currently recorded under, if a
    /// profile has been opened at all.
    ///
    /// Whoever issues a probe takes this key and hands it back with the
    /// result; a result carrying a key that is no longer current is refused
    /// rather than attributed to the credentials that replaced it.
    pub fn credentials(&self) -> Option<CredentialsId> {
        self.capabilities().credentials().cloned()
    }

    /// What has been observed about `scope` under `credentials`.
    pub fn capability(&self, credentials: &CredentialsId, scope: &Scope) -> Capability {
        self.capabilities().capability(credentials, scope)
    }

    /// Whether `scope` still lacks evidence about listing it.
    pub fn needs_list_probe(&self, credentials: &CredentialsId, scope: &Scope) -> bool {
        self.capabilities().needs_list_probe(credentials, scope)
    }

    /// Record what a completed list operation or probe showed.
    pub fn observe_list(
        &self,
        credentials: &CredentialsId,
        scope: Scope,
        observation: Observation,
    ) -> bool {
        self.capabilities()
            .observe_list(credentials, scope, observation)
    }

    /// Declare that different credentials are now in play, discarding
    /// everything the previous ones observed.
    ///
    /// [`Self::open`] does this for both cases the spec names — switching
    /// profile and re-authenticating — so a frontend only needs this when
    /// credentials change without a connection being opened.
    pub fn credentials_changed(&self, profile: &str) -> CredentialsId {
        self.capabilities().credentials_changed(profile)
    }

    /// The capability store, with a poisoned lock recovered rather than
    /// propagated: a panic elsewhere must not take the whole session down,
    /// and the worst this data can be is stale — which the next observation
    /// corrects.
    fn capabilities(&self) -> MutexGuard<'_, CapabilityStore> {
        self.capabilities
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Report the rows the user is looking at, so their list capability can
    /// be probed.
    ///
    /// Returns immediately: probes are issued on the runtime and nothing here
    /// waits on one, so a frontend calls this from its render path without
    /// paying for it (`capability-awareness`, "Probing is lazy, budgeted and
    /// non-blocking"). Report the viewport as it changes — the latest report
    /// replaces the previous one, and only what the budget allows is asked at
    /// any moment.
    ///
    /// Does nothing until a connection is open: there would be nothing to
    /// probe through, and no credentials to attribute an answer to.
    pub fn submit_viewport(&self, viewport: &[ProbeTarget]) {
        if let Some(scheduler) = self.scheduler() {
            scheduler.submit_viewport(viewport);
        }
    }

    /// The scopes with a probe open right now.
    ///
    /// A scope in here has been asked about and has not answered: it is
    /// neither unknown, allowed nor denied, and the frontend presents it as
    /// being probed (`capability-awareness`, "A pending probe is distinct from
    /// no evidence"). That state lives here rather than in [`Observation`],
    /// which holds claims about the world and not facts about our own
    /// activity.
    pub fn probes_in_flight(&self) -> HashSet<Scope> {
        self.scheduler()
            .map(|scheduler| scheduler.in_flight())
            .unwrap_or_default()
    }

    /// Whether a probe is open for `scope`, without copying the whole set to
    /// answer about one row.
    pub fn is_probing(&self, scope: &Scope) -> bool {
        self.scheduler()
            .is_some_and(|scheduler| scheduler.is_probing(scope))
    }

    /// The scheduler for the connection currently open.
    ///
    /// Cloned out rather than borrowed so the slot's lock is never held while
    /// the scheduler works — it takes locks of its own.
    fn scheduler(&self) -> Option<ProbeScheduler> {
        self.scheduler_slot().clone()
    }

    /// Whether a connection is open to read locations through.
    #[cfg(test)]
    fn store_slot(&self) -> Option<Arc<dyn ObjectStore>> {
        self.store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn scheduler_slot(&self) -> MutexGuard<'_, Option<ProbeScheduler>> {
        self.scheduler
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Point this session's probing at `store`, for the credentials the
    /// capability store just minted.
    ///
    /// Its own function so a test can put a double where the S3 adapter goes,
    /// through the same door production uses.
    fn install_scheduler(&self, store: Arc<dyn ObjectStore>, credentials: CredentialsId) {
        // Parked here rather than at the call site so the scheduler and the
        // store can never come from different connections: one function
        // installs both, or neither.
        *self.store.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&store));
        let settled = self
            .settled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| Arc::new(|_| {}));
        *self.scheduler_slot() = Some(ProbeScheduler::new(
            self.runtime.clone(),
            store,
            Arc::clone(&self.capabilities),
            credentials,
            settled,
        ));
    }

    /// Point this session at `store`, as opening a connection would.
    ///
    /// Through `install_scheduler` — the door production uses — so the probe
    /// scheduler is installed with it. A double wired past the scheduler would
    /// leave every capability observation in a window test a fiction: the rows
    /// would say "checking…" for ever, and a test asserting on them would be
    /// asserting on the absence of an answer.
    ///
    /// `credentials` comes from [`Self::credentials_changed`], the same value
    /// a real connection mints, so observations are attributed to credentials
    /// that exist rather than to none.
    #[cfg(any(test, feature = "test-support"))]
    pub fn install_object_store(&self, store: Arc<dyn ObjectStore>, credentials: CredentialsId) {
        self.install_scheduler(store, credentials);
    }

    /// Hear about each scope whose probe has settled.
    ///
    /// `announce` runs on a runtime thread, so a frontend should do nothing in
    /// it but hand the scope to its own executor — the same rule as
    /// [`Self::spawn_listing`]. Registering replaces any previous sink and
    /// applies from the next connection opened.
    pub fn on_probe_settled<F>(&self, announce: F)
    where
        F: Fn(Scope) + Send + Sync + 'static,
    {
        *self.settled.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(announce));
    }

    /// Open a connection and list its buckets, off the caller's thread.
    ///
    /// `source` is a named profile or a stored credential; nothing below this
    /// point behaves differently for the two, which is the point of the type
    /// (design.md, "A connection is a source, not a profile"). A bare name
    /// still means a profile, so a caller that has one may pass it as it is.
    ///
    /// `deliver` is called exactly once with the tagged result. It runs on a
    /// runtime thread, so a frontend should do nothing in it but hand the
    /// message to its own executor.
    pub fn spawn_listing<S, F>(&self, id: ConnectionId, source: S, deliver: F)
    where
        S: Into<ConnectionSource>,
        F: FnOnce(TaggedOutcome) + Send + 'static,
    {
        let source = source.into();
        let session = self.clone();
        self.runtime.spawn(async move {
            // Recorded here rather than inside the listing, because this is
            // where the outcome the user will be shown is settled — and a log
            // that disagrees with the screen is worse than none.
            let outcome = match session.list_buckets(id, &source).await {
                Ok(buckets) => {
                    diagnostics::listing_settled(id, source.name(), Ok(buckets.buckets.len()));
                    Outcome::Loaded(buckets)
                }
                Err(error) => {
                    diagnostics::listing_settled(id, source.name(), Err(&error));
                    Outcome::Failed(error)
                }
            };
            deliver(TaggedOutcome::new(id, outcome));
        });
    }

    /// Read one page of a location, off the caller's thread.
    ///
    /// Through the store the open connection already built, not a new one:
    /// re-opening would re-resolve credentials for every folder entered, and
    /// on a machine whose credentials come from an external process that is
    /// seconds each time. A caller with no connection open gets
    /// [`Error::MissingConfiguration`] rather than a silent empty page —
    /// asking to browse before choosing where is a mistake, not a location
    /// that holds nothing.
    ///
    /// `deliver` is called exactly once, on a runtime thread, so a frontend
    /// should do nothing in it but hand the result to its own executor — the
    /// same rule as [`Self::spawn_listing`].
    pub fn spawn_objects<F>(&self, location: Location, cursor: Option<Cursor>, deliver: F)
    where
        F: FnOnce(Result<Page>) + Send + 'static,
    {
        let store = self
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();

        self.runtime.spawn(async move {
            let outcome = match store {
                Some(store) => store.list_objects(&location, cursor.as_ref()).await,
                None => Err(crate::error::Error::MissingConfiguration {
                    profile: None,
                    detail: "no connection is open to read a location through".into(),
                }),
            };

            // Recorded where the outcome is settled, for the same reason the
            // bucket listing is: a log that disagrees with the screen is
            // worse than no log at all.
            diagnostics::location_settled(
                &location.bucket,
                location.prefix.as_str(),
                outcome
                    .as_ref()
                    .map(|page| (page.folders.len(), page.objects.len(), page.is_truncated())),
            );

            deliver(outcome);
        });
    }

    /// Open a connection for `source` and list what it can see.
    async fn list_buckets(
        &self,
        id: ConnectionId,
        source: &ConnectionSource,
    ) -> Result<crate::types::AccountListing> {
        // Through the store `open` installs, never a second one built here.
        // The listing is where this connection learns which of its buckets are
        // directory buckets, and a store dropped at the end of this line takes
        // that with it — leaving the read of a location to report the wrong
        // permission, which is exactly what it did.
        self.open(id, source.clone()).await?;
        let store = self
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        match store {
            Some(store) => store.list_buckets().await,
            None => Err(crate::error::Error::MissingConfiguration {
                profile: None,
                detail: "no connection is open to list through".into(),
            }),
        }
    }

    /// Download one object to a directory, off the caller's thread
    /// (`XONHO-0007`).
    ///
    /// Through the store the open connection already built, for
    /// [`Self::spawn_objects`]' reason: re-opening would re-resolve
    /// credentials per download. The returned [`Cancel`] stops it between
    /// chunks — cooperatively, so the task is still alive to clean up, log
    /// the outcome and deliver it; an aborted task could do none of those.
    ///
    /// `deliver` is called exactly once, on a runtime thread. `progress` is
    /// called after every chunk with cumulative bytes and the size when the
    /// service stated one; both must hand off to the caller's own executor.
    pub fn spawn_download<P, F>(
        &self,
        bucket: String,
        key: String,
        directory: std::path::PathBuf,
        collision: Collision,
        mut progress: P,
        deliver: F,
    ) -> Cancel
    where
        P: FnMut(u64, Option<u64>) + Send + 'static,
        F: FnOnce(DownloadOutcome) + Send + 'static,
    {
        let cancel = Cancel::default();
        let handle = cancel.clone();
        let store = self
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        self.runtime.spawn(async move {
            let Some(store) = store else {
                deliver(DownloadOutcome::Failed(Error::MissingConfiguration {
                    profile: None,
                    detail: "no connection is open to download through".into(),
                }));
                return;
            };

            let (name, mapped) = match transfer::resolve_destination(&directory, &key, collision) {
                transfer::Resolved::Taken { name } => {
                    // Nothing moved, so nothing is logged: the log records
                    // transfers, and this deliberately was not one yet.
                    deliver(DownloadOutcome::NameTaken { name });
                    return;
                }
                transfer::Resolved::Write { name, mapped } => (name, mapped),
            };

            let content = match store.get_object(&bucket, &key).await {
                Ok(content) => content,
                Err(cause) => {
                    diagnostics::transfer_settled(
                        &bucket,
                        0,
                        diagnostics::TransferSettled::Failed(&cause),
                    );
                    deliver(DownloadOutcome::Failed(cause));
                    return;
                }
            };

            let final_path = directory.join(&name);
            match transfer::pump(content, &final_path, &cancel, &mut progress).await {
                Ok(bytes) => {
                    diagnostics::transfer_settled(
                        &bucket,
                        bytes,
                        diagnostics::TransferSettled::Finished,
                    );
                    deliver(DownloadOutcome::Finished {
                        name,
                        mapped,
                        bytes,
                    });
                }
                Err(transfer::PumpEnd::Cancelled) => {
                    diagnostics::transfer_settled(
                        &bucket,
                        0,
                        diagnostics::TransferSettled::Cancelled,
                    );
                    deliver(DownloadOutcome::Cancelled);
                }
                Err(transfer::PumpEnd::Failed(cause)) => {
                    diagnostics::transfer_settled(
                        &bucket,
                        0,
                        diagnostics::TransferSettled::Failed(&cause),
                    );
                    deliver(DownloadOutcome::Failed(cause));
                }
            }
        });
        handle
    }

    /// Send one local file to `key`, off the caller's thread
    /// (`XONHO-0020`).
    ///
    /// Same contract as [`Self::spawn_download`]: through the store the open
    /// connection built, `deliver` exactly once on a runtime thread, and a
    /// cooperative [`Cancel`].
    ///
    /// `replace` is the user's answer to a taken key and nothing else. When
    /// it is `false` the write is conditional and the service refuses a key
    /// that exists — which is the whole guarantee, and why there is no
    /// existence check anywhere in this function.
    pub fn spawn_upload<F>(
        &self,
        bucket: String,
        key: String,
        path: std::path::PathBuf,
        collision: Collision,
        deliver: F,
    ) -> Cancel
    where
        F: FnOnce(UploadOutcome) + Send + 'static,
    {
        let cancel = Cancel::default();
        let handle = cancel.clone();
        let store = self
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        self.runtime.spawn(async move {
            let Some(store) = store else {
                deliver(UploadOutcome::Failed(Error::MissingConfiguration {
                    profile: None,
                    detail: "no connection is open to upload through".into(),
                }));
                return;
            };

            // Before a byte leaves: a file too large for one request is
            // refused now rather than after sending most of it.
            let bytes = match transfer::sized_for_one_request(&path) {
                Ok(bytes) => bytes,
                Err(cause) => {
                    diagnostics::upload_settled(
                        &bucket,
                        0,
                        diagnostics::TransferSettled::Failed(&cause),
                    );
                    deliver(UploadOutcome::Failed(cause));
                    return;
                }
            };

            // Cancelling before the request goes out is still a cancel.
            if cancel.is_cancelled() {
                diagnostics::upload_settled(&bucket, 0, diagnostics::TransferSettled::Cancelled);
                deliver(UploadOutcome::Cancelled);
                return;
            }

            let outcome = match collision {
                // Replace is the one unconditional write in this codebase,
                // and it is reachable only from a user's answer.
                Collision::Replace => {
                    attempt(
                        &*store,
                        &bucket,
                        &key,
                        &path,
                        crate::store::IfAbsent::Replace,
                    )
                    .await
                }
                Collision::Ask => {
                    attempt(
                        &*store,
                        &bucket,
                        &key,
                        &path,
                        crate::store::IfAbsent::Refuse,
                    )
                    .await
                }
                Collision::KeepBoth => keep_both(&*store, &bucket, &key, &path, &cancel).await,
            };

            let (settled, outcome) = match outcome {
                Ok(PutResult::Created { key, stepped_aside }) => (
                    diagnostics::TransferSettled::Finished,
                    UploadOutcome::Finished {
                        key,
                        stepped_aside,
                        bytes,
                    },
                ),
                // A taken key and an endpoint without the condition are both
                // questions rather than events: nothing moved, so nothing is
                // logged as having moved.
                Ok(PutResult::Taken) => {
                    deliver(UploadOutcome::KeyTaken { key });
                    return;
                }
                Ok(PutResult::Unsupported) => {
                    deliver(UploadOutcome::ConditionUnsupported { key });
                    return;
                }
                Ok(PutResult::Cancelled) => {
                    diagnostics::upload_settled(
                        &bucket,
                        0,
                        diagnostics::TransferSettled::Cancelled,
                    );
                    deliver(UploadOutcome::Cancelled);
                    return;
                }
                Err(cause) => {
                    diagnostics::upload_settled(
                        &bucket,
                        0,
                        diagnostics::TransferSettled::Failed(&cause),
                    );
                    deliver(UploadOutcome::Failed(cause));
                    return;
                }
            };
            diagnostics::upload_settled(&bucket, bytes, settled);
            deliver(outcome);
        });
        handle
    }

    /// Delete one object, off the caller's thread (`XONHO-0021`).
    ///
    /// The caller has already taken the second act — the named-key
    /// confirmation is the window's job, and this function trusts it the
    /// way `spawn_upload` trusts that `Replace` was an answer. No `Cancel`:
    /// a delete is one request, and a cancel that raced it would leave the
    /// user unsure whether the object exists, which is worse than the
    /// moment of waiting.
    pub fn spawn_delete<F>(&self, bucket: String, key: String, deliver: F)
    where
        F: FnOnce(DeleteOutcome) + Send + 'static,
    {
        let store = self
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        self.runtime.spawn(async move {
            let Some(store) = store else {
                deliver(DeleteOutcome::Failed(Error::MissingConfiguration {
                    profile: None,
                    detail: "no connection is open to delete through".into(),
                }));
                return;
            };
            match store.delete_object(&bucket, &key).await {
                Ok(deleted) => {
                    diagnostics::delete_settled(&bucket, deleted.marker.is_some(), None);
                    deliver(DeleteOutcome::Gone {
                        marker: deleted.marker,
                    });
                }
                Err(cause) => {
                    diagnostics::delete_settled(&bucket, false, Some(&cause));
                    deliver(DeleteOutcome::Failed(cause));
                }
            }
        });
    }

    /// Remove a delete marker, off the caller's thread (`XONHO-0021`).
    ///
    /// The undo. Needs no confirmation — it restores — which is exactly why
    /// it exists as its own spawn rather than a mode of the delete.
    pub fn spawn_undo_delete<F>(&self, bucket: String, key: String, version_id: String, deliver: F)
    where
        F: FnOnce(UndoOutcome) + Send + 'static,
    {
        let store = self
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        self.runtime.spawn(async move {
            let Some(store) = store else {
                deliver(UndoOutcome::Failed(Error::MissingConfiguration {
                    profile: None,
                    detail: "no connection is open to restore through".into(),
                }));
                return;
            };
            match store.remove_marker(&bucket, &key, &version_id).await {
                Ok(()) => {
                    diagnostics::undo_settled(&bucket, None);
                    deliver(UndoOutcome::Restored);
                }
                Err(cause) => {
                    diagnostics::undo_settled(&bucket, Some(&cause));
                    deliver(UndoOutcome::Failed(cause));
                }
            }
        });
    }

    /// Save a stored credential, off the caller's thread.
    ///
    /// Both halves: the name, region and access key id to the configuration
    /// file, the secret to the credential store — so the connection is still
    /// there after a restart, and no secret is left filed under a name the
    /// application has forgotten (design.md, "A stored connection is
    /// remembered, or it should not be offered at all").
    ///
    /// On the runtime for the same reason every network call is: reaching the
    /// credential store can raise a prompt the user has to answer, and a
    /// render thread waiting on one is a frozen window (design.md, Risks).
    ///
    /// `deliver` is called exactly once, on a runtime thread, and is handed
    /// back the credential's configuration half so a frontend can offer the
    /// connection the moment it is real. A store that refuses, or a file that
    /// cannot be written, is reported as itself and leaves nothing half done.
    pub fn spawn_save_credential<F>(
        &self,
        credential: StoredCredential,
        secret: CredentialSecret,
        deliver: F,
    ) where
        F: FnOnce(Result<StoredCredential>) + Send + 'static,
    {
        let secrets = Arc::clone(&self.secrets);
        let file = Arc::clone(&self.connections);
        self.runtime.spawn(async move {
            let saved =
                connections::remember(file.as_ref(), secrets.as_ref(), &credential, &secret);
            // By name, and by name alone. `secret` is in scope here and there
            // is no function in `diagnostics` it would fit into.
            diagnostics::credential_saved(credential.name(), saved.as_ref().map(|&()| ()));
            deliver(saved.map(|()| credential));
        });
    }

    /// Sign in to an Identity Center session, off the caller's thread.
    ///
    /// Two callbacks and not one. `show` fires as soon as there is a code for
    /// the user to read, which has to happen *while* they are being waited
    /// for; `deliver` fires once, at the end, with the session, the fact that
    /// it was abandoned, or the cause. A single callback at the end would
    /// leave the window with nothing to display during the only part of this
    /// that takes time (`sso-sign-in`: what is happening is visible while it
    /// is happening).
    ///
    /// The obtained session is written to the token cache here rather than by
    /// the caller: it is what makes the session usable, and a caller that
    /// forgot would produce a sign-in that appeared to work and changed
    /// nothing.
    pub fn spawn_sign_in<S, F>(&self, at: SignInLocation, abandon: Abandon, show: S, deliver: F)
    where
        S: FnOnce(DeviceAuthorization) + Send + 'static,
        F: FnOnce(Result<SignInOutcome>) + Send + 'static,
    {
        let port = SsoOidcSignIn::new(self.http.clone());
        self.runtime.spawn(async move {
            let outcome = crate::sso::sign_in(&port, &RealTime, &at, &abandon, |authorization| {
                show(authorization.clone())
            })
            .await;
            let outcome = match outcome {
                Ok(SignInOutcome::Session(obtained)) => match crate::sso::home_dir() {
                    Some(home) => crate::sso::write_session(&home, &at, &obtained)
                        .map(|_| SignInOutcome::Session(obtained)),
                    None => Err(Error::TokenCacheNotWritable {
                        path: "~/.aws/sso/cache".to_owned(),
                        detail: "this account has no home directory to write it in".to_owned(),
                    }),
                },
                other => other,
            };
            diagnostics::sign_in_settled(at.label(), outcome.as_ref());
            deliver(outcome);
        });
    }

    /// Forget a stored credential, off the caller's thread.
    ///
    /// Deletes what the credential store holds for `name` first and the
    /// configuration entry second. The order is the point: the other way
    /// round, a failure would leave a secret in the keychain under a name this
    /// application can no longer see, name or delete.
    pub fn spawn_forget_credential<F>(&self, name: String, deliver: F)
    where
        F: FnOnce(Result<()>) + Send + 'static,
    {
        let secrets = Arc::clone(&self.secrets);
        let file = Arc::clone(&self.connections);
        self.runtime.spawn(async move {
            let forgotten = connections::forget(file.as_ref(), secrets.as_ref(), &name);
            diagnostics::credential_forgotten(&name, forgotten.as_ref().map(|&()| ()));
            deliver(forgotten);
        });
    }

    /// Open a connection for `source`.
    ///
    /// Opening is the moment the credentials change: it is how a switch to
    /// another connection reaches core, and how a re-authentication of the
    /// current one does too. Both discard every observation gathered so far —
    /// unconditionally, and before the attempt, because a connection that
    /// fails to open must not leave the previous one's evidence standing
    /// under its name (`capability-awareness`, "Observations are scoped to
    /// the credentials that produced them").
    ///
    /// The previous connection's probe scheduler goes with them, for the same
    /// reason and one more: it probes through a store built for credentials
    /// that are no longer in play, and its queue is a viewport of an account
    /// that is no longer on screen. A connection that comes up gets a
    /// scheduler of its own, so reporting a viewport starts probing without
    /// the frontend having to wire anything up.
    ///
    /// None of that reads `source`. Whether the credentials came from
    /// `~/.aws` or from this application's own store is settled inside
    /// `connection::open` and is invisible from here upwards.
    pub async fn open<S: Into<ConnectionSource>>(
        &self,
        id: ConnectionId,
        source: S,
    ) -> Result<Connection> {
        let source = source.into();
        let credentials = self.credentials_changed(source.name());
        *self.scheduler_slot() = None;
        *self.store.lock().unwrap_or_else(PoisonError::into_inner) = None;

        let connection =
            connection::open(id, &source, &self.paths, &self.http, self.secrets.as_ref()).await?;
        self.install_scheduler(Arc::new(S3ObjectStore::new(&connection)), credentials);
        Ok(connection)
    }
}

/// How one delete ended (`XONHO-0021`).
#[derive(Debug)]
pub enum DeleteOutcome {
    /// The service accepted the delete. `marker` is the delete marker's
    /// version id when one was created — the proof an undo exists, and its
    /// token.
    Gone { marker: Option<String> },
    /// Failed, with the classified cause. The object's row stays.
    Failed(Error),
}

/// How one undo ended (`XONHO-0021`).
#[derive(Debug)]
pub enum UndoOutcome {
    /// The marker is gone; the object is back.
    Restored,
    /// Failed — the marker still stands, and nothing claims otherwise.
    Failed(Error),
}

/// What one or more conditional attempts came to, inside `spawn_upload`.
enum PutResult {
    Created { key: String, stepped_aside: bool },
    Taken,
    Unsupported,
    Cancelled,
}

/// One write, at one key.
async fn attempt(
    store: &dyn ObjectStore,
    bucket: &str,
    key: &str,
    path: &std::path::Path,
    if_absent: crate::store::IfAbsent,
) -> Result<PutResult> {
    use crate::store::PutOutcome;
    Ok(
        match store.put_object(bucket, key, path, if_absent).await? {
            PutOutcome::Created => PutResult::Created {
                key: key.to_owned(),
                stepped_aside: false,
            },
            PutOutcome::KeyTaken => PutResult::Taken,
            PutOutcome::ConditionUnsupported => PutResult::Unsupported,
        },
    )
}

/// Keep both: try `key (2)`, `key (3)`, … until one is free.
///
/// A loop of conditional writes rather than a listing, because a listing is
/// stale the moment it returns and the service is the only honest source of
/// which keys are free. Bounded — reaching the bound is reported rather than
/// silently abandoned, which is the failure mode a loop like this otherwise
/// has.
async fn keep_both(
    store: &dyn ObjectStore,
    bucket: &str,
    key: &str,
    path: &std::path::Path,
    cancel: &Cancel,
) -> Result<PutResult> {
    for n in 2..(2 + transfer::KEEP_BOTH_ATTEMPTS) {
        if cancel.is_cancelled() {
            return Ok(PutResult::Cancelled);
        }
        let candidate = transfer::beside(key, n);
        match attempt(
            store,
            bucket,
            &candidate,
            path,
            crate::store::IfAbsent::Refuse,
        )
        .await?
        {
            PutResult::Created { key, .. } => {
                return Ok(PutResult::Created {
                    key,
                    stepped_aside: true,
                });
            }
            PutResult::Taken => continue,
            other => return Ok(other),
        }
    }
    Err(Error::Unexpected {
        detail: format!(
            "gave up after {} attempts to find a free name beside that key — \
             every one of them was taken",
            transfer::KEEP_BOTH_ATTEMPTS
        ),
    })
}

#[cfg(test)]
mod tests {
    //! `capability-awareness` spec, "Observations are scoped to the
    //! credentials that produced them" — this is the wiring end: what makes
    //! the store forget. The model itself is covered in `capability.rs`, and
    //! the scheduling rules in `probe.rs`; what is asserted here is that the
    //! session hands the two to each other.
    //!
    //! And `stored-credentials` at the seam: that a connection opened from a
    //! credential this application holds is treated exactly like one opened
    //! from a profile, and that saving a credential leaves the AWS shared
    //! files alone.

    use super::*;
    use crate::capability::{Observation, Scope};
    use crate::connections::double::ConnectionFileDouble;
    use crate::credentials;
    use crate::credentials::double::SecretStoreDouble;
    use crate::error::{ConnectionsProblem, CredentialStoreProblem, Error};
    use crate::probe::double::{HeldProbes, settle, until};
    use crate::store::double::StoreDouble;
    use crate::types::Region;
    use std::path::PathBuf;

    /// The secret the fixtures put through the store. Not a real key.
    const SECRET: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("caixonho-session-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("fixture dir");
            Self { dir }
        }

        /// Where this fixture's session reads shared configuration from.
        ///
        /// Names a credentials file that does not exist, exactly as the real
        /// app does — and, below, so that a test can watch whether anything
        /// ever creates it.
        fn paths(&self) -> ConfigPaths {
            ConfigPaths {
                config: Some(self.dir.join("config")),
                credentials: Some(self.dir.join("credentials")),
            }
        }

        /// A session reading a config file that declares `profile`.
        ///
        /// Its remembered connections go to a double, always: a test that
        /// saved one through the real file would write into the developer's
        /// own configuration directory, which is exactly what nothing here is
        /// allowed to touch.
        fn session(&self, profile: &str) -> Session {
            let paths = self.paths();
            std::fs::write(
                paths.config.as_ref().expect("a config path"),
                format!(
                    "[profile {profile}]\nregion = ap-southeast-1\n\
                     aws_access_key_id = AKIAEXAMPLE\n\
                     aws_secret_access_key = wJalrEXAMPLEKEY\n"
                ),
            )
            .expect("write fixture");
            Session::new(
                Handle::current(),
                HttpStack::with_ca_bundle(None).expect("client builds"),
                paths,
            )
            .with_connection_file(Arc::new(ConnectionFileDouble::empty()))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// The stored credential the tests below connect with.
    fn stored() -> StoredCredential {
        StoredCredential::new("typed-in", "ap-southeast-1", "AKIAIOSFODNN7EXAMPLE")
    }

    /// A credential store already holding [`stored`]'s secret.
    fn store_holding_the_secret() -> Arc<SecretStoreDouble> {
        let store = Arc::new(SecretStoreDouble::open());
        credentials::save(
            store.as_ref(),
            &stored(),
            &CredentialSecret::new(SECRET, None),
        )
        .expect("an open store accepts it");
        store
    }

    #[tokio::test]
    async fn a_session_that_has_opened_nothing_has_observed_nothing() {
        let fixture = Fixture::new("fresh");

        let session = fixture.session("work");

        assert!(session.credentials().is_none());
    }

    #[tokio::test]
    async fn clones_of_a_session_share_one_store() {
        // `spawn_listing` hands a clone to the runtime; an observation made
        // there has to be visible to the frontend holding the original.
        let fixture = Fixture::new("shared");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");

        let worker = session.clone();
        worker.observe_list(&credentials, Scope::bucket("logs"), Observation::Denied);

        assert_eq!(
            session
                .capability(&credentials, &Scope::bucket("logs"))
                .list,
            Observation::Denied
        );
    }

    #[tokio::test]
    async fn opening_a_connection_discards_what_the_previous_credentials_observed() {
        // Both events the spec names come through here: selecting another
        // profile, and re-authenticating the one already selected.
        let fixture = Fixture::new("switch");
        let session = fixture.session("work");
        let before = session.credentials_changed("work");
        session.observe_list(&before, Scope::bucket("logs"), Observation::Allowed);

        session
            .open(ConnectionId(1), "work")
            .await
            .expect("the fixture profile declares a region");

        let now = session.credentials().expect("a profile is open");
        assert_ne!(now, before);
        assert_eq!(
            session.capability(&now, &Scope::bucket("logs")).list,
            Observation::Unknown,
            "nothing observed before the connection was opened may survive it"
        );
        assert!(session.needs_list_probe(&now, &Scope::bucket("logs")));
    }

    /// One row of a viewport, as a frontend reports it.
    fn row(name: &str) -> ProbeTarget {
        ProbeTarget::new(
            Scope::bucket(name),
            Region::Known("ap-southeast-1".to_owned()),
        )
    }

    #[tokio::test]
    async fn a_session_with_no_connection_open_probes_nothing() {
        // A viewport can be reported before anything is open — the frontend
        // renders first. There is nothing to probe through and no credentials
        // to attribute an answer to, so the report is simply dropped.
        let fixture = Fixture::new("no-connection");
        let session = fixture.session("work");

        session.submit_viewport(&[row("logs")]);

        assert!(session.probes_in_flight().is_empty());
        assert!(!session.is_probing(&Scope::bucket("logs")));
    }

    #[tokio::test]
    async fn a_viewport_reported_to_a_session_is_probed_through_its_store() {
        let fixture = Fixture::new("probing");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        let store = HeldProbes::open();
        session.install_scheduler(Arc::new(store.clone()), credentials.clone());

        session.submit_viewport(&[row("logs"), row("backups")]);

        until("both rows to settle", || store.finished() == 2).await;
        assert_eq!(
            session
                .capability(&credentials, &Scope::bucket("logs"))
                .list,
            Observation::Allowed,
            "what the probe found has to reach the store the frontend reads"
        );
        assert!(!session.is_probing(&Scope::bucket("logs")));
    }

    #[tokio::test]
    async fn opening_a_connection_gives_the_session_something_to_probe_through() {
        let fixture = Fixture::new("install");
        let session = fixture.session("work");
        assert!(session.scheduler().is_none());

        session
            .open(ConnectionId(1), "work")
            .await
            .expect("the fixture profile declares a region");

        assert!(
            session.scheduler().is_some(),
            "a connection that comes up is one a viewport can be probed against"
        );
    }

    #[tokio::test]
    async fn opening_a_connection_also_gives_the_session_something_to_read_through() {
        // The store and the scheduler are installed together, so they can
        // never come from different connections.
        let fixture = Fixture::new("readable");
        let session = fixture.session("work");
        assert!(session.store_slot().is_none());

        session
            .open(ConnectionId(1), "work")
            .await
            .expect("the fixture profile declares a region");

        assert!(
            session.store_slot().is_some(),
            "a connection that comes up is one a location can be read through"
        );
    }

    #[tokio::test]
    async fn reading_a_location_before_choosing_a_connection_is_a_mistake_not_an_empty_folder() {
        // The distinction this project exists to keep. Nothing is open, so
        // there is nothing to read — which is not the same as a location that
        // holds nothing, and must not arrive looking like one.
        let fixture = Fixture::new("unopened");
        let session = fixture.session("work");
        let (tell, heard) = tokio::sync::oneshot::channel();

        session.spawn_objects(Location::bucket("holiday"), None, move |outcome| {
            let _ = tell.send(outcome);
        });

        match heard.await.expect("the callback runs once") {
            Err(Error::MissingConfiguration { detail, .. }) => {
                assert!(detail.contains("no connection"), "{detail}");
            }
            other => panic!("expected MissingConfiguration, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_session_given_a_store_reads_locations_through_it() {
        // The door `XONHO-0015` opens for the window: no connection is opened,
        // no profile is resolved and no keychain is touched, and a location
        // still reads. Without this a window test would have to bring the
        // machine's `~/.aws` with it, and would answer differently on every
        // machine that ran it.
        let fixture = Fixture::new("installed-store");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        let page = Page {
            more: Some(Cursor("from-the-double".to_owned())),
            ..Page::default()
        };
        session.install_object_store(
            Arc::new(StoreDouble::allows_listing().listing(page)),
            credentials,
        );

        let (tell, heard) = tokio::sync::oneshot::channel();
        session.spawn_objects(Location::bucket("holiday"), None, move |outcome| {
            let _ = tell.send(outcome);
        });

        match heard.await.expect("the callback runs once") {
            Ok(page) => assert_eq!(
                page.more,
                Some(Cursor("from-the-double".to_owned())),
                "the page came from somewhere other than the double"
            ),
            other => panic!("expected the double's page, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_switch_takes_away_what_the_previous_connection_was_read_through() {
        // A store outliving its connection would read the next location with
        // the last account's credentials, which is the same class of mistake
        // as attributing an observation to credentials that replaced it.
        let fixture = Fixture::new("switched");
        let session = fixture.session("work");

        session
            .open(ConnectionId(1), "work")
            .await
            .expect("the fixture profile declares a region");
        assert!(session.store_slot().is_some());

        let _ = session.open(ConnectionId(2), "nowhere").await;

        assert!(
            session.store_slot().is_none(),
            "a connection that failed to open leaves nothing to read through"
        );
    }

    #[tokio::test]
    async fn a_viewport_reported_after_a_switch_never_reaches_the_previous_connection() {
        // The switch invalidates the credentials the old scheduler probes
        // for, and its store belongs to a connection nobody is looking at.
        let fixture = Fixture::new("retired");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        let previous = HeldProbes::open();
        session.install_scheduler(Arc::new(previous.clone()), credentials);

        session.credentials_changed("personal");
        session.submit_viewport(&[row("logs"), row("backups")]);
        settle().await;

        assert!(
            previous.asked().is_empty(),
            "the previous connection must not be asked anything on the new \
             profile's behalf"
        );
    }

    #[tokio::test]
    async fn a_connection_that_fails_to_open_still_discards_them() {
        // A switch to a profile that cannot resolve must not leave the
        // previous profile's observations standing under the new name.
        let fixture = Fixture::new("failed-switch");
        let session = fixture.session("work");
        let before = session.credentials_changed("work");
        session.observe_list(&before, Scope::bucket("logs"), Observation::Denied);

        // Whether the configuration resolves is beside the point: the
        // discard is not conditional on the connection coming up.
        let _ = session
            .open(ConnectionId(2), "profile-that-is-not-in-the-file")
            .await;

        let now = session.credentials().expect("a profile was attempted");
        assert_ne!(now, before);
        assert_eq!(
            session.capability(&now, &Scope::bucket("logs")).list,
            Observation::Unknown
        );
    }

    #[tokio::test]
    async fn a_connection_opens_from_either_source_and_everything_above_it_is_the_same() {
        // The property this change exists to preserve. Above the connection
        // — the capability store, the probe scheduler, the listing — nothing
        // asks where the credentials came from, so a profile and a stored
        // credential have to arrive in the same state and leave the same
        // state behind them.
        let fixture = Fixture::new("either-source");
        let session = fixture
            .session("work")
            .with_secret_store(store_holding_the_secret());

        for (case, source, name) in [
            ("a named profile", ConnectionSource::from("work"), "work"),
            (
                "a stored credential",
                ConnectionSource::from(stored()),
                "typed-in",
            ),
        ] {
            let before = session.credentials_changed("whatever-was-open");
            session.observe_list(&before, Scope::bucket("logs"), Observation::Denied);

            let connection = session
                .open(ConnectionId(3), source)
                .await
                .unwrap_or_else(|error| panic!("{case} must open: {error}"));

            assert_eq!(connection.id(), ConnectionId(3), "{case}");
            assert_eq!(connection.name(), name, "{case}");
            assert!(!connection.region().is_empty(), "{case}");

            let now = session.credentials().expect("a connection is open");
            assert_ne!(now, before, "{case}: opening mints new credentials");
            assert_eq!(
                now.profile(),
                name,
                "{case}: observations are attributed to what the user chose"
            );
            assert_eq!(
                session.capability(&now, &Scope::bucket("logs")).list,
                Observation::Unknown,
                "{case}: nothing observed before the connection was opened may survive it"
            );
            assert!(
                session.scheduler().is_some(),
                "{case}: a connection that comes up is one a viewport can be probed against"
            );
        }
    }

    #[tokio::test]
    async fn a_stored_connection_that_cannot_reach_its_secret_discards_the_previous_evidence_too() {
        // The same rule as a profile that fails to resolve: the discard is
        // not conditional on the connection coming up, whichever source was
        // asked for.
        let fixture = Fixture::new("stored-failure");
        let session =
            fixture
                .session("work")
                .with_secret_store(Arc::new(SecretStoreDouble::refusing(
                    CredentialStoreProblem::Locked,
                )));
        let before = session.credentials_changed("work");
        session.observe_list(&before, Scope::bucket("logs"), Observation::Allowed);

        let opened = session.open(ConnectionId(4), stored()).await;

        assert!(matches!(opened, Err(Error::CredentialStore { .. })));
        let now = session.credentials().expect("a connection was attempted");
        assert_ne!(now, before);
        assert_eq!(
            session.capability(&now, &Scope::bucket("logs")).list,
            Observation::Unknown
        );
    }

    #[tokio::test]
    async fn a_saved_credential_never_reaches_the_aws_shared_files() {
        // The shortest route to a working connection is to write
        // `~/.aws/credentials`, and the spec refuses it: that file belongs to
        // every other AWS tool on the machine, and editing it on the user's
        // behalf is a side effect nobody asked for. This watches the very
        // paths this session was given.
        let fixture = Fixture::new("shared-files-untouched");
        let store = Arc::new(SecretStoreDouble::open());
        let session = fixture
            .session("work")
            .with_secret_store(Arc::clone(&store) as Arc<dyn SecretStore>);
        let paths = fixture.paths();
        let config_before =
            std::fs::read(paths.config.as_ref().expect("a config path")).expect("the fixture");

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_save_credential(
            stored(),
            CredentialSecret::new(SECRET, None),
            move |saved| {
                let _ = tell.send(saved.map(|credential| credential.name().to_owned()));
            },
        );
        let saved = told.await.expect("the callback runs exactly once");

        assert_eq!(saved.expect("an open store accepts it"), "typed-in");
        assert_eq!(
            store
                .holds()
                .values()
                .filter(|held| held.as_str() == SECRET)
                .count(),
            1,
            "the secret went to the credential store"
        );
        assert!(
            !paths
                .credentials
                .as_ref()
                .expect("a credentials path")
                .exists(),
            "the AWS shared credentials file must not be created on the user's behalf"
        );
        assert_eq!(
            std::fs::read(paths.config.as_ref().expect("a config path")).expect("the fixture"),
            config_before,
            "nor may the shared config be edited"
        );
    }

    #[tokio::test]
    async fn forgetting_a_connection_deletes_what_the_store_held_for_it() {
        let fixture = Fixture::new("forget");
        let store = store_holding_the_secret();
        let session = fixture
            .session("work")
            .with_secret_store(Arc::clone(&store) as Arc<dyn SecretStore>);

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_forget_credential("typed-in".to_owned(), move |forgotten| {
            let _ = tell.send(forgotten);
        });

        told.await
            .expect("the callback runs exactly once")
            .expect("an open store forgets it");
        assert!(
            store.holds().is_empty(),
            "a forgotten connection leaves nothing behind to be signed with"
        );
    }

    #[tokio::test]
    async fn a_credential_saved_through_a_session_is_offered_again_after_a_restart() {
        // The defect 4.0 was added for: the secret went to the keychain,
        // nothing kept the rest, and the connection vanished at exit while
        // its secret stayed behind. A second session over the same file is
        // what a restart looks like from here.
        let fixture = Fixture::new("remembered-across-restarts");
        let file = Arc::new(ConnectionFileDouble::empty());
        let session = fixture
            .session("work")
            .with_secret_store(Arc::new(SecretStoreDouble::open()))
            .with_connection_file(Arc::clone(&file) as Arc<dyn ConnectionFile>);

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_save_credential(
            stored(),
            CredentialSecret::new(SECRET, None),
            move |saved| {
                let _ = tell.send(saved.map(|_| ()));
            },
        );
        told.await
            .expect("the callback runs exactly once")
            .expect("an open store and a writable file accept it");

        let restarted = fixture
            .session("work")
            .with_connection_file(Arc::clone(&file) as Arc<dyn ConnectionFile>);
        assert_eq!(
            restarted
                .stored_connections()
                .expect("the file this session wrote is one it can read"),
            vec![stored()]
        );
    }

    #[tokio::test]
    async fn forgetting_through_a_session_stops_the_connection_being_offered() {
        let fixture = Fixture::new("forget-both-halves");
        let store = store_holding_the_secret();
        let file = Arc::new(ConnectionFileDouble::empty());
        let session = fixture
            .session("work")
            .with_secret_store(Arc::clone(&store) as Arc<dyn SecretStore>)
            .with_connection_file(Arc::clone(&file) as Arc<dyn ConnectionFile>);
        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_save_credential(stored(), CredentialSecret::new(SECRET, None), move |s| {
            let _ = tell.send(s.map(|_| ()));
        });
        told.await.expect("once").expect("accepted");

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_forget_credential("typed-in".to_owned(), move |forgotten| {
            let _ = tell.send(forgotten);
        });
        told.await
            .expect("the callback runs exactly once")
            .expect("an open store and a writable file forget it");

        assert!(
            session.stored_connections().expect("readable").is_empty(),
            "a connection that has been forgotten is not one to keep offering"
        );
        assert!(
            store.holds().is_empty(),
            "and nothing of it is left in the credential store"
        );
    }

    #[tokio::test]
    async fn a_session_whose_connections_cannot_be_read_says_so_rather_than_showing_none() {
        // An empty list would say this machine has no stored connections,
        // which is a different statement and a false one — and it invites the
        // user to enter a credential on top of one already there.
        let fixture = Fixture::new("unreadable-connections");
        let session = fixture.session("work").with_connection_file(Arc::new(
            ConnectionFileDouble::empty().unreadable(ConnectionsProblem::Unreadable),
        ));

        match session.stored_connections() {
            Err(Error::Connections {
                problem: ConnectionsProblem::Unreadable,
                ..
            }) => {}
            other => panic!("expected Connections/Unreadable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_store_that_refuses_a_save_says_so_through_the_callback() {
        let fixture = Fixture::new("refused-save");
        let session =
            fixture
                .session("work")
                .with_secret_store(Arc::new(SecretStoreDouble::refusing(
                    CredentialStoreProblem::Refused,
                )));

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_save_credential(
            stored(),
            CredentialSecret::new(SECRET, None),
            move |saved| {
                let _ = tell.send(saved.map(|_| ()));
            },
        );

        match told.await.expect("the callback runs exactly once") {
            Err(Error::CredentialStore {
                connection,
                problem,
            }) => {
                assert_eq!(connection, "typed-in");
                assert_eq!(problem, CredentialStoreProblem::Refused);
            }
            other => panic!("expected CredentialStore, got {other:?}"),
        }
    }

    // ---- Downloading (XONHO-0007) ----

    fn a_download_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("caixonho-session-download-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir");
        dir
    }

    /// `object-transfer` spec, "Downloading an object" — through the session:
    /// the store the connection installed serves the bytes, the file appears
    /// whole, and the outcome arrives exactly once.
    #[tokio::test]
    async fn a_download_through_the_installed_store_delivers_the_file() {
        let fixture = Fixture::new("download-happy");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        session.install_object_store(
            Arc::new(StoreDouble::serving_chunks(vec![
                b"pail ".to_vec(),
                b"of bytes".to_vec(),
            ])),
            credentials,
        );
        let dir = a_download_dir("happy");

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_download(
            "reports".to_owned(),
            "daily/summary.csv".to_owned(),
            dir.clone(),
            crate::transfer::Collision::Ask,
            |_, _| {},
            move |outcome| {
                let _ = tell.send(outcome);
            },
        );

        match told.await.expect("delivered exactly once") {
            crate::transfer::DownloadOutcome::Finished {
                name,
                mapped,
                bytes,
            } => {
                assert_eq!(name, "summary.csv");
                assert_eq!(mapped, crate::transfer::MappingOutcome::Unchanged);
                assert_eq!(bytes, 13);
            }
            other => panic!("expected Finished, got {other:?}"),
        }
        assert_eq!(
            std::fs::read(dir.join("summary.csv")).expect("the file exists"),
            b"pail of bytes"
        );
    }

    /// The existing-file question crosses the session without anything
    /// having been transferred.
    #[tokio::test]
    async fn a_taken_name_comes_back_as_the_question_it_is() {
        let fixture = Fixture::new("download-taken");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        session.install_object_store(
            Arc::new(StoreDouble::serving_chunks(vec![b"new".to_vec()])),
            credentials,
        );
        let dir = a_download_dir("taken");
        std::fs::write(dir.join("summary.csv"), b"the original").unwrap();

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_download(
            "reports".to_owned(),
            "daily/summary.csv".to_owned(),
            dir.clone(),
            crate::transfer::Collision::Ask,
            |_, _| {},
            move |outcome| {
                let _ = tell.send(outcome);
            },
        );

        match told.await.expect("delivered") {
            crate::transfer::DownloadOutcome::NameTaken { name } => {
                assert_eq!(name, "summary.csv");
            }
            other => panic!("expected NameTaken, got {other:?}"),
        }
        assert_eq!(
            std::fs::read(dir.join("summary.csv")).expect("untouched"),
            b"the original",
            "asking is not transferring"
        );
    }

    // ---- Uploading (XONHO-0020) ----

    fn a_local_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("caixonho-session-upload-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir");
        let path = dir.join("payload.bin");
        std::fs::write(&path, bytes).expect("fixture file");
        path
    }

    fn uploading(
        session: &Session,
        path: std::path::PathBuf,
        collision: transfer::Collision,
    ) -> tokio::sync::oneshot::Receiver<UploadOutcome> {
        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_upload(
            "reports".to_owned(),
            "daily/summary.csv".to_owned(),
            path,
            collision,
            move |outcome| {
                let _ = tell.send(outcome);
            },
        );
        told
    }

    #[tokio::test]
    async fn an_upload_creates_the_object_and_reports_its_key() {
        let fixture = Fixture::new("upload-happy");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        session.install_object_store(Arc::new(StoreDouble::allows_listing()), credentials);

        let told = uploading(
            &session,
            a_local_file("happy", b"thirteen byte"),
            transfer::Collision::Ask,
        );

        match told.await.expect("delivered exactly once") {
            UploadOutcome::Finished {
                key,
                stepped_aside,
                bytes,
            } => {
                assert_eq!(key, "daily/summary.csv");
                assert!(!stepped_aside, "nothing was in the way");
                assert_eq!(bytes, 13);
            }
            other => panic!("expected Finished, got {other:?}"),
        }
    }

    /// The guarantee, at the session seam: a taken key comes back as the
    /// question, and the caller is the one who decides.
    #[tokio::test]
    async fn a_taken_key_comes_back_as_a_question_not_a_replacement() {
        let fixture = Fixture::new("upload-taken");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        session.install_object_store(Arc::new(StoreDouble::key_taken()), credentials);

        let told = uploading(
            &session,
            a_local_file("taken", b"new"),
            transfer::Collision::Ask,
        );

        match told.await.expect("delivered") {
            UploadOutcome::KeyTaken { key } => assert_eq!(key, "daily/summary.csv"),
            other => panic!("expected KeyTaken, got {other:?}"),
        }
    }

    /// Keep-both steps aside, and — the assertion this change exists for —
    /// every write it made was conditional, so the object that was already
    /// there could not have been replaced by any of them.
    #[tokio::test]
    async fn keep_both_steps_aside_without_ever_writing_unconditionally() {
        let fixture = Fixture::new("upload-keep-both");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        // This double refuses *every* conditional write and accepts every
        // unconditional one. So keep-both exhausting its attempts proves
        // something stronger than a green happy path: it never once fell
        // back to an unconditional write to make progress.
        session.install_object_store(Arc::new(StoreDouble::key_taken()), credentials);

        let told = uploading(
            &session,
            a_local_file("keep-both", b"new"),
            transfer::Collision::KeepBoth,
        );

        match told.await.expect("delivered") {
            UploadOutcome::Failed(Error::Unexpected { detail }) => {
                assert!(
                    detail.contains("every one of them was taken"),
                    "giving up is reported, not silent: {detail}"
                );
            }
            other => panic!("keep-both must give up rather than replace anything, got {other:?}"),
        }
    }

    /// Keep-both's **success** path, which the exhaustion test above cannot
    /// reach: two candidates taken, the third free. Added by the close-out
    /// review, which asked what was asserted but not verified and found this
    /// — the path a user actually walks was covered only at its failure end.
    #[tokio::test]
    async fn keep_both_takes_the_first_free_key_and_says_it_stepped_aside() {
        let fixture = Fixture::new("upload-keep-both-succeeds");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        // `summary (2).csv` and `summary (3).csv` are taken; the fourth
        // candidate is free.
        session.install_object_store(Arc::new(StoreDouble::taken_until(2)), credentials);

        let told = uploading(
            &session,
            a_local_file("keep-both-ok", b"new"),
            transfer::Collision::KeepBoth,
        );

        match told.await.expect("delivered") {
            UploadOutcome::Finished {
                key,
                stepped_aside,
                bytes,
            } => {
                assert_eq!(
                    key, "daily/summary (4).csv",
                    "numbering starts at 2 and walks up past what is taken"
                );
                assert!(stepped_aside, "the window has to be able to say so");
                assert_eq!(bytes, 3);
            }
            other => panic!("expected Finished, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn replace_is_the_only_way_an_object_is_overwritten() {
        let fixture = Fixture::new("upload-replace");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        session.install_object_store(Arc::new(StoreDouble::key_taken()), credentials);

        let told = uploading(
            &session,
            a_local_file("replace", b"new"),
            transfer::Collision::Replace,
        );

        match told.await.expect("delivered") {
            UploadOutcome::Finished { key, .. } => assert_eq!(key, "daily/summary.csv"),
            other => panic!("expected Finished, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_endpoint_without_the_condition_is_a_question_too() {
        let fixture = Fixture::new("upload-unsupported");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        session.install_object_store(Arc::new(StoreDouble::condition_unsupported()), credentials);

        let told = uploading(
            &session,
            a_local_file("unsupported", b"new"),
            transfer::Collision::Ask,
        );

        assert!(
            matches!(
                told.await.expect("delivered"),
                UploadOutcome::ConditionUnsupported { .. }
            ),
            "the guarantee being unavailable is the user's decision to make"
        );
    }

    #[tokio::test]
    async fn a_cancel_before_the_request_is_still_a_cancel() {
        let fixture = Fixture::new("upload-cancelled");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        session.install_object_store(Arc::new(StoreDouble::allows_listing()), credentials);

        let (tell, told) = tokio::sync::oneshot::channel();
        let cancel = session.spawn_upload(
            "reports".to_owned(),
            "daily/summary.csv".to_owned(),
            a_local_file("cancelled", b"new"),
            transfer::Collision::Ask,
            move |outcome| {
                let _ = tell.send(outcome);
            },
        );
        cancel.cancel();

        // Either it was cancelled before the write went out, or the write
        // beat the flag — both are honest, and neither may report the
        // object as created *and* cancelled.
        let outcome = told.await.expect("delivered");
        assert!(
            matches!(
                outcome,
                UploadOutcome::Cancelled | UploadOutcome::Finished { .. }
            ),
            "got {outcome:?}"
        );
    }

    // ---- Deleting (XONHO-0021) ----

    #[tokio::test]
    async fn a_delete_reports_the_marker_the_service_created() {
        let fixture = Fixture::new("delete-versioned");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        session.install_object_store(Arc::new(StoreDouble::versioned("mk-7")), credentials);

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_delete(
            "reports".to_owned(),
            "daily/summary.csv".to_owned(),
            move |o| {
                let _ = tell.send(o);
            },
        );

        match told.await.expect("delivered exactly once") {
            DeleteOutcome::Gone { marker } => assert_eq!(marker.as_deref(), Some("mk-7")),
            other => panic!("expected Gone, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_unversioned_delete_reports_no_way_back() {
        let fixture = Fixture::new("delete-unversioned");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        session.install_object_store(Arc::new(StoreDouble::allows_listing()), credentials);

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_delete(
            "reports".to_owned(),
            "daily/summary.csv".to_owned(),
            move |o| {
                let _ = tell.send(o);
            },
        );

        match told.await.expect("delivered") {
            DeleteOutcome::Gone { marker } => assert!(marker.is_none()),
            other => panic!("expected Gone, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_undo_restores_through_the_exact_marker() {
        let fixture = Fixture::new("delete-undo");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        let double = Arc::new(StoreDouble::versioned("mk-7"));
        session.install_object_store(double.clone(), credentials);

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_undo_delete(
            "reports".to_owned(),
            "daily/summary.csv".to_owned(),
            "mk-7".to_owned(),
            move |o| {
                let _ = tell.send(o);
            },
        );

        assert!(matches!(
            told.await.expect("delivered"),
            UndoOutcome::Restored
        ));
        assert_eq!(double.markers_removed(), vec!["mk-7".to_owned()]);
    }

    #[tokio::test]
    async fn a_refused_undo_does_not_claim_restoration() {
        let fixture = Fixture::new("delete-undo-refused");
        let session = fixture.session("work");
        let credentials = session.credentials_changed("work");
        session.install_object_store(
            Arc::new(StoreDouble::marker_removal_refused("mk-7")),
            credentials,
        );

        let (tell, told) = tokio::sync::oneshot::channel();
        session.spawn_undo_delete(
            "reports".to_owned(),
            "daily/summary.csv".to_owned(),
            "mk-7".to_owned(),
            move |o| {
                let _ = tell.send(o);
            },
        );

        match told.await.expect("delivered") {
            UndoOutcome::Failed(Error::AccessDenied { iam_action }) => {
                assert_eq!(iam_action, "s3:DeleteObjectVersion");
            }
            other => panic!("expected the named refusal, got {other:?}"),
        }
    }
}
