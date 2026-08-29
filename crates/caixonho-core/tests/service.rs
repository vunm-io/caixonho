//! A real S3 service the tests start themselves (`XONHO-0031`).
//!
//! The adapter is the one file that turns this project's intentions into
//! HTTP, and neither of its existing test tiers sends a request: `StoreDouble`
//! answers **above** it and `StaticReplayClient` replays bytes **below** it.
//! Between them sits the question neither asks — does the request we build
//! mean, to a real service, what we believe it means?
//!
//! # What this cannot prove
//!
//! Stated here rather than discovered later, because a passing suite mistaken
//! for coverage it does not have is worse than no suite. `not_covered` below
//! holds a test per exclusion, so the reasons fail loudly if they stop being
//! true.
//!
//! - **Directory buckets and Local Zones.** Nothing emulates
//!   `s3express:CreateSession`, the `{base}--{zone}--x-s3` naming, or a
//!   directory that vanishes with its last object. This is the feature the
//!   application exists for, and accepting it stays the owner's.
//! - **Versioning, delete markers and Undo.** `s3s-fs` has no versioning, so
//!   `XONHO-0021`'s Undo cannot be exercised here.
//! - **Denials.** The service has no IAM and refuses nothing. Classification
//!   is covered below the adapter by the replay tests, which is its right
//!   place.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use s3s::auth::SimpleAuth;
use s3s::host::MultiDomain;
use s3s::service::S3ServiceBuilder;
use s3s_fs::FileSystem;
use tokio::net::TcpListener;

/// The keys the service accepts. Not real, and never anything's actual keys:
/// AWS's own documentation example, so that a grep for a leaked credential
/// finds a string every reader recognises as fictional.
pub const ACCESS_KEY_ID: &str = "AKIAIOSFODNN7EXAMPLE";
pub const SECRET_ACCESS_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";

/// The region the service is addressed in. Any is fine — it signs nothing that
/// depends on the region being real — so it is a plausible one for this
/// project rather than `us-east-1`, which the SDK treats specially.
pub const REGION: &str = "ap-southeast-1";

/// A running S3 service, and the temporary directory behind it.
///
/// Both are dropped together. The directory going with the server is what
/// keeps one test's bucket from being another test's surprise.
pub struct Service {
    base_url: String,
    root: tempfile::TempDir,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Service {
    /// Start one, on a port the operating system chooses.
    ///
    /// **Port 0, never a constant.** `cargo test` runs these concurrently, and
    /// a fixed port is a flake that reads as a product bug — the failure looks
    /// like a refused connection, which is exactly what a real misconfigured
    /// endpoint looks like too.
    ///
    /// The base domain is `localhost`, which is what makes virtual-hosted
    /// addressing work without touching production code: `reports.localhost`
    /// resolves to `127.0.0.1`, and `s3s` reads the bucket back out of the
    /// `Host` header.
    pub async fn start() -> Self {
        let root = tempfile::tempdir().expect("a temporary directory");
        let fs = FileSystem::new(root.path()).expect("a filesystem-backed service");

        // Bound **before** the service is built, because the base domain has
        // to carry the port and the port is the OS's to choose.
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .expect("a port the OS chose");
        let port = listener.local_addr().expect("a bound address").port();

        let service = {
            let mut builder = S3ServiceBuilder::new(fs);
            builder.set_auth(SimpleAuth::from_single(ACCESS_KEY_ID, SECRET_ACCESS_KEY));
            // `localhost:<port>`, not `localhost`. The SDK sends
            // `Host: reports.localhost:54321`, and `s3s` matches a virtual
            // host by `strip_suffix(base_domain)` — which the port defeats.
            // Without the port every bucket request silently falls back to
            // path-style, and the service answers a *different operation*
            // with HTTP 200; what reaches the caller is not a connection
            // error but "the service answered HTTP 200", because the SDK
            // could not parse the body as the response it asked for.
            //
            // Found the slow way. It is also the one thing in this harness
            // that would look like a product bug rather than a test bug.
            builder
                .set_host(MultiDomain::new(&[format!("localhost:{port}")]).expect("a base domain"));
            builder.build()
        };

        let (shutdown, mut stop) = tokio::sync::oneshot::channel();
        let http = ConnBuilder::new(TokioExecutor::new());
        let service = Arc::new(service);

        let handle = tokio::spawn(async move {
            loop {
                let socket = tokio::select! {
                    accepted = listener.accept() => match accepted {
                        Ok((socket, _)) => socket,
                        Err(_) => continue,
                    },
                    _ = &mut stop => break,
                };
                // `into_owned`, and the connection built inside the task:
                // the builder is borrowed by `serve_connection`, and a
                // connection spawned while holding that borrow outlives it.
                let connection = http
                    .serve_connection(TokioIo::new(socket), service.as_ref().clone())
                    .into_owned();
                tokio::spawn(async move {
                    let _ = connection.await;
                });
            }
        });

        Self {
            // `localhost` rather than `127.0.0.1`: the SDK derives the
            // virtual host from this, and `reports.127.0.0.1` is not a name.
            base_url: format!("http://localhost:{port}"),
            root,
            shutdown: Some(shutdown),
            handle: Some(handle),
        }
    }

    /// Where to point a client.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Make a bucket.
    ///
    /// Through the filesystem rather than through the port, because the port
    /// has no `create_bucket` — this application never makes one, which is
    /// itself a decision worth not quietly undoing to make a test easier.
    pub fn with_bucket(&self, name: &str) -> &Self {
        std::fs::create_dir_all(self.root.path().join(name)).expect("a bucket directory");
        self
    }

    /// Put an object there, without going through the port.
    ///
    /// Seeding is not the thing under test, and seeding through `put_object`
    /// would make every listing test depend on writes working — so a failure
    /// in one would show up as a failure in all of them.
    pub fn with_object(&self, bucket: &str, key: &str, bytes: &[u8]) -> &Self {
        let path = self.root.path().join(bucket).join(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the key's parents");
        }
        std::fs::write(path, bytes).expect("the object is written");
        self
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

/// Where a session pointed at `service` reads its configuration from.
///
/// A temporary AWS config file naming an `endpoint_url` and static keys —
/// **not** a hand-built `SdkConfig` handed to `S3ObjectStore::over`.
///
/// That costs a few lines and buys this tier the thing it exists for.
/// Building the configuration here would prove that *a* correctly-configured
/// adapter works while leaving untested whether the application configures
/// one, and this project has already shipped a defect of exactly that shape —
/// a store built and dropped, so the connection lost what the listing had
/// learned about its buckets.
///
/// The returned directory must outlive the session: dropping it takes the
/// config file with it, and the SDK reads that file lazily.
pub fn config_for(service: &Service) -> (tempfile::TempDir, caixonho_core::ConfigPaths) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let config = dir.path().join("config");
    std::fs::write(
        &config,
        format!(
            "[profile {PROFILE}]\n\
             region = {REGION}\n\
             endpoint_url = {}\n\
             aws_access_key_id = {ACCESS_KEY_ID}\n\
             aws_secret_access_key = {SECRET_ACCESS_KEY}\n",
            service.base_url()
        ),
    )
    .expect("the config file is written");

    let paths = caixonho_core::ConfigPaths {
        config: Some(config),
        // Named and absent, exactly as on a machine with an SSO-only setup —
        // the shape `connection::open` has a whole comment about, so the tier
        // exercises it rather than avoiding it.
        credentials: Some(dir.path().join("credentials")),
    };
    (dir, paths)
}

/// The profile name the config file declares, and what a failure names back.
pub const PROFILE: &str = "local";

/// An adapter talking to `service`, reached the way the application reaches
/// one: through [`config_for`] and `Session::open`.
pub struct Connected {
    pub store: std::sync::Arc<dyn caixonho_core::ObjectStore>,
    _config: tempfile::TempDir,
}

impl Connected {
    pub async fn to(service: &Service) -> Self {
        let (dir, paths) = config_for(service);
        let session = caixonho_core::Session::new(
            tokio::runtime::Handle::current(),
            caixonho_core::HttpStack::with_ca_bundle(None).expect("a client"),
            paths,
        );

        let connection = session
            .open(
                caixonho_core::ConnectionId(1),
                caixonho_core::ConnectionSource::Profile(PROFILE.to_owned()),
            )
            .await
            .expect("the local service is reachable");

        Self {
            store: std::sync::Arc::new(caixonho_core::S3ObjectStore::new(&connection)),
            _config: dir,
        }
    }
}
