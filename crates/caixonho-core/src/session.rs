//! What the app hands core once, and asks for work through afterwards.
//!
//! The app owns exactly one multi-thread tokio runtime and passes its
//! [`Handle`] here; core never builds a runtime of its own. That is what lets
//! the future CLI reuse this crate — it owns its own runtime — and it keeps
//! every network call off the render thread by construction: work is spawned
//! onto the handle, and the result comes back as one tagged message.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::runtime::Handle;

use crate::adapter::S3ObjectStore;
use crate::capability::{Capability, CapabilityStore, CredentialsId, Observation, Scope};
use crate::connection::{self, Connection};
use crate::error::Result;
use crate::outcome::{Outcome, TaggedOutcome};
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
}

impl Session {
    /// Build a session around the app's runtime.
    pub fn new(runtime: Handle, http: HttpStack, paths: ConfigPaths) -> Self {
        Self {
            runtime,
            http,
            paths,
            capabilities: Arc::new(Mutex::new(CapabilityStore::new())),
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
    pub async fn open(&self, id: ConnectionId, profile: &str) -> Result<Connection> {
        self.credentials_changed(profile);
        connection::open(id, profile, &self.paths, &self.http).await
    }
}

#[cfg(test)]
mod tests {
    //! `capability-awareness` spec, "Observations are scoped to the
    //! credentials that produced them" — this is the wiring end: what makes
    //! the store forget. The model itself is covered in `capability.rs`.

    use super::*;
    use crate::capability::{Observation, Scope};
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
