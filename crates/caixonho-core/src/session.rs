//! What the app hands core once, and asks for work through afterwards.
//!
//! The app owns exactly one multi-thread tokio runtime and passes its
//! [`Handle`] here; core never builds a runtime of its own. That is what lets
//! the future CLI reuse this crate — it owns its own runtime — and it keeps
//! every network call off the render thread by construction: work is spawned
//! onto the handle, and the result comes back as one tagged message.

use tokio::runtime::Handle;

use crate::adapter::S3ObjectStore;
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
}

impl Session {
    /// Build a session around the app's runtime.
    pub fn new(runtime: Handle, http: HttpStack, paths: ConfigPaths) -> Self {
        Self {
            runtime,
            http,
            paths,
        }
    }

    /// Where this session reads profiles from.
    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
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
    pub async fn open(&self, id: ConnectionId, profile: &str) -> Result<Connection> {
        connection::open(id, profile, &self.paths, &self.http).await
    }
}
