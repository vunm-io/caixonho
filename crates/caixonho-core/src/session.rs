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
use crate::connection::{self, Connection};
use crate::error::Result;
use crate::outcome::{Outcome, TaggedOutcome};
use crate::probe::{ProbeScheduler, ProbeTarget};
use crate::profiles::ConfigPaths;
use crate::store::ObjectStore;
use crate::tls::HttpStack;
use crate::types::ConnectionId;

/// The long-lived context every request runs in.
#[derive(Debug, Clone)]
pub struct Session {
    runtime: Handle,
    http: HttpStack,
    paths: ConfigPaths,
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
}

impl Session {
    /// Build a session around the app's runtime.
    pub fn new(runtime: Handle, http: HttpStack, paths: ConfigPaths) -> Self {
        Self {
            runtime,
            http,
            paths,
            capabilities: Arc::new(Mutex::new(CapabilityStore::new())),
            scheduler: Arc::default(),
        }
    }

    /// Where this session reads profiles from.
    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
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
        *self.scheduler_slot() = Some(ProbeScheduler::new(
            self.runtime.clone(),
            store,
            Arc::clone(&self.capabilities),
            credentials,
        ));
    }

    /// Open a connection and list its buckets, off the caller's thread.
    ///
    /// `deliver` is called exactly once with the tagged result. It runs on a
    /// runtime thread, so a frontend should do nothing in it but hand the
    /// message to its own executor.
    pub fn spawn_listing<F>(&self, id: ConnectionId, profile: String, deliver: F)
    where
        F: FnOnce(TaggedOutcome) + Send + 'static,
    {
        let session = self.clone();
        self.runtime.spawn(async move {
            let outcome = match session.list_buckets(id, &profile).await {
                Ok(buckets) => Outcome::Loaded(buckets),
                Err(error) => Outcome::Failed(error),
            };
            deliver(TaggedOutcome::new(id, outcome));
        });
    }

    /// Open a connection for `profile` and list what it can see.
    async fn list_buckets(&self, id: ConnectionId, profile: &str) -> Result<Vec<crate::Bucket>> {
        let connection = self.open(id, profile).await?;
        S3ObjectStore::new(&connection).list_buckets().await
    }

    /// Open a connection for `profile`.
    ///
    /// Opening is the moment the credentials change: it is how a profile
    /// switch reaches core, and how a re-authentication of the current
    /// profile does too. Both discard every observation gathered so far —
    /// unconditionally, and before the attempt, because a profile that fails
    /// to open must not leave the previous one's evidence standing under its
    /// name (`capability-awareness`, "Observations are scoped to the
    /// credentials that produced them").
    ///
    /// The previous connection's probe scheduler goes with them, for the same
    /// reason and one more: it probes through a store built for credentials
    /// that are no longer in play, and its queue is a viewport of an account
    /// that is no longer on screen. A connection that comes up gets a
    /// scheduler of its own, so reporting a viewport starts probing without
    /// the frontend having to wire anything up.
    pub async fn open(&self, id: ConnectionId, profile: &str) -> Result<Connection> {
        let credentials = self.credentials_changed(profile);
        *self.scheduler_slot() = None;

        let connection = connection::open(id, profile, &self.paths, &self.http).await?;
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

    use super::*;
    use crate::capability::{Observation, Scope};
    use crate::probe::double::{HeldProbes, settle, until};
    use crate::types::Region;
    use std::path::PathBuf;

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

        /// A session reading a config file that declares `profile`.
        fn session(&self, profile: &str) -> Session {
            let path = self.dir.join("config");
            std::fs::write(
                &path,
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
                ConfigPaths {
                    config: Some(path),
                    credentials: None,
                },
            )
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
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
}
