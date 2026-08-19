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
use crate::error::Result;
use crate::outcome::{Outcome, TaggedOutcome};
use crate::probe::{ProbeScheduler, ProbeSink, ProbeTarget};
use crate::profiles::ConfigPaths;
use crate::store::ObjectStore;
use crate::tls::HttpStack;
use crate::types::ConnectionId;

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
                    diagnostics::listing_settled(id, source.name(), Ok(buckets.len()));
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

    /// Open a connection for `source` and list what it can see.
    async fn list_buckets(
        &self,
        id: ConnectionId,
        source: &ConnectionSource,
    ) -> Result<Vec<crate::Bucket>> {
        let connection = self.open(id, source.clone()).await?;
        S3ObjectStore::new(&connection).list_buckets().await
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

        let connection =
            connection::open(id, &source, &self.paths, &self.http, self.secrets.as_ref()).await?;
        self.install_scheduler(Arc::new(S3ObjectStore::new(&connection)), credentials);
        Ok(connection)
    }
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
}
