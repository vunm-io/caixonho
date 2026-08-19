//! Opening a connection: one chosen source → resolved credentials → a
//! configuration the S3 adapter can build a client from.
//!
//! A connection comes from one of two places, and [`ConnectionSource`] is the
//! only type that knows which. For a named profile, resolution is the SDK's
//! job — static keys, `role_arn` + `source_profile` chains and SSO tokens
//! already cached by the AWS CLI all come from its provider chain, and
//! reimplementing that precedence here would be a second source of truth that
//! drifts (`connections` spec, "Credential resolution"). For a credential
//! this application holds, the secret comes out of the operating system's
//! credential store and is handed to the SDK as static credentials for that
//! client — never written to `~/.aws/credentials` (design.md, "Stored
//! credentials use a static provider, not a written file").
//!
//! What this module owns is everything around that: pointing the chain at the
//! right files, giving both paths the shared HTTP client so credential and
//! SSO calls use the same trust material as S3 calls, and refusing to
//! continue when the region is missing. Everything above a [`Connection`] —
//! listing, probing, the capability store — takes the connection and stays
//! unaware of where it came from.

use aws_config::BehaviorVersion;
use aws_runtime::env_config::file::{EnvConfigFileKind, EnvConfigFiles};
use aws_sdk_s3::config::{Credentials, Region as SdkRegion};

use crate::credentials::{self, SecretStore, StoredCredential};
use crate::diagnostics::{self, SourceKind};
use crate::error::{Error, Result};
use crate::profiles::ConfigPaths;
use crate::tls::HttpStack;
use crate::types::ConnectionId;

/// How the SDK is told where a stored credential came from, in the
/// credentials it signs with. Diagnostic only; it never leaves the process.
const STORED_CREDENTIAL_PROVIDER: &str = "caixonho-stored-credential";

/// Where a connection's credentials come from.
///
/// One type rather than a second path parallel to the profile one: a parallel
/// path would double every call site above it and guarantee the two drift
/// (design.md, "A connection is a source, not a profile"). To someone
/// connecting, both are just somewhere to connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionSource {
    /// A profile named in the AWS shared configuration files. The SDK's
    /// provider chain resolves it, whatever it turns out to be made of.
    Profile(String),
    /// A credential this application holds. Its configuration travels here;
    /// its secret is fetched from the credential store as the connection
    /// opens, and lives nowhere in between.
    Stored(StoredCredential),
}

impl ConnectionSource {
    /// What this connection is called.
    ///
    /// A profile's name, or a stored credential's. It is what the user chose
    /// in the list, so it is what a failure names back to them.
    pub fn name(&self) -> &str {
        match self {
            Self::Profile(profile) => profile,
            Self::Stored(credential) => credential.name(),
        }
    }

    /// Which kind of place this connection's credentials come from.
    ///
    /// Nothing below this point behaves differently for the two — that is the
    /// point of the type. It is recorded because someone reading a log needs
    /// it: a signature that will not verify means something different for a
    /// profile the AWS CLI also uses than for a key typed into this
    /// application, and the two are fixed in different places.
    pub(crate) fn kind(&self) -> SourceKind {
        match self {
            Self::Profile(_) => SourceKind::Profile,
            Self::Stored(_) => SourceKind::Stored,
        }
    }
}

impl From<String> for ConnectionSource {
    /// A bare name is a profile. That is what a connection was before this
    /// type existed, and it keeps every call site that names one honest.
    fn from(profile: String) -> Self {
        Self::Profile(profile)
    }
}

impl From<&str> for ConnectionSource {
    fn from(profile: &str) -> Self {
        Self::Profile(profile.to_owned())
    }
}

impl From<StoredCredential> for ConnectionSource {
    fn from(credential: StoredCredential) -> Self {
        Self::Stored(credential)
    }
}

/// An opened connection: what it is called, where it points, and the resolved
/// configuration the adapter turns into an S3 client.
///
/// Holding the id here is what lets a late response be dropped instead of
/// being rendered as if it belonged to the connection the user switched to.
///
/// Nothing here records which source opened it, and nothing above it asks:
/// that is the property this change exists to preserve.
#[derive(Debug, Clone)]
pub struct Connection {
    id: ConnectionId,
    name: String,
    region: String,
    sso_session: Option<String>,
    sdk: aws_config::SdkConfig,
}

impl Connection {
    /// Which connection this is.
    pub fn id(&self) -> ConnectionId {
        self.id
    }

    /// What this connection is called — the profile's name, or the stored
    /// credential's.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The region this connection resolved to.
    pub fn region(&self) -> &str {
        &self.region
    }

    /// The `sso_session` this profile belongs to, when it declares one —
    /// the name `aws sso login --sso-session <name>` needs.
    ///
    /// Always `None` for a stored credential: there is no session to
    /// re-establish, and telling someone to run `aws sso login` over a key
    /// they typed in would be advice they have nothing to act on.
    pub fn sso_session(&self) -> Option<&str> {
        self.sso_session.as_deref()
    }

    /// The resolved SDK configuration, for the adapter that builds clients.
    pub(crate) fn sdk_config(&self) -> &aws_config::SdkConfig {
        &self.sdk
    }
}

/// Open a connection for `source`.
///
/// Succeeding here means the configuration resolved, not that the credentials
/// work: the SDK's providers are lazy, so an expired session or a denied
/// policy surfaces on the first real call and is classified there. That is
/// deliberate — the spec requires a connection with unusable credentials to
/// stay listed and fail only when used.
///
/// A stored credential is the one exception to that laziness, and only by one
/// step: its secret has to come out of the credential store here, so a store
/// that will not open is reported now rather than as a mysterious signing
/// failure later.
pub(crate) async fn open(
    id: ConnectionId,
    source: &ConnectionSource,
    paths: &ConfigPaths,
    http: &HttpStack,
    secrets: &dyn SecretStore,
) -> Result<Connection> {
    let opened = match source {
        ConnectionSource::Profile(profile) => open_profile(id, profile, paths, http).await,
        ConnectionSource::Stored(credential) => open_stored(id, credential, http, secrets).await,
    };

    // One place, after both paths, so the log cannot come to disagree with
    // itself about what a connection is. The name and the region are ordinary
    // configuration; what resolved them is not recorded because nothing here
    // has it, which is exactly the property that keeps a secret out.
    match &opened {
        Ok(connection) => {
            diagnostics::connection_opened(id, source.name(), source.kind(), connection.region())
        }
        Err(error) => diagnostics::connection_refused(id, source.name(), source.kind(), error),
    }
    opened
}

/// Open a connection for a profile in the AWS shared configuration.
async fn open_profile(
    id: ConnectionId,
    profile: &str,
    paths: &ConfigPaths,
    http: &HttpStack,
) -> Result<Connection> {
    let mut loader = aws_config::defaults(BehaviorVersion::latest())
        .profile_name(profile)
        .http_client(http.client());

    // Only files that exist may enter the set. Naming an absent file does not
    // merely add nothing: the SDK fails to load the whole set, so a perfectly
    // good `config` stops resolving too — which is what happens on every
    // machine that has `~/.aws/config` and no `~/.aws/credentials`, the normal
    // shape of an SSO-only setup.
    //
    // `EnvConfigFiles::build` also panics on an empty set, so the loader keeps
    // its own defaults when nothing is left to give it.
    let mut files = EnvConfigFiles::builder();
    let mut any_file = false;
    for (kind, path) in [
        (EnvConfigFileKind::Config, paths.config.as_ref()),
        (EnvConfigFileKind::Credentials, paths.credentials.as_ref()),
    ] {
        if let Some(path) = path.filter(|path| path.is_file()) {
            files = files.with_file(kind, path.clone());
            any_file = true;
        }
    }
    if any_file {
        loader = loader.profile_files(files.build());
    }

    let sdk = loader.load().await;
    let region = require_region(profile, sdk.region().map(AsRef::as_ref))?;

    Ok(Connection {
        id,
        name: profile.to_owned(),
        region,
        sso_session: crate::profiles::sso_session(paths, profile),
        sdk,
    })
}

/// Open a connection for a credential this application holds.
///
/// The secret is handed to the SDK as static credentials for this client and
/// nothing else. It is never written to `~/.aws/credentials`: that file is
/// shared with every other AWS tool on the machine, and editing it on the
/// user's behalf is a side effect nobody asked for — the spec says so rather
/// than leaving it to taste.
///
/// The same loader the profile path uses, so both connections get whatever
/// the SDK's defaults are — retries, timeouts, stalled-stream protection —
/// and a stored connection cannot quietly behave differently from a
/// discovered one. Only the region and the credentials are pinned, and both
/// of those outrank anything the environment or the shared files would have
/// supplied.
async fn open_stored(
    id: ConnectionId,
    credential: &StoredCredential,
    http: &HttpStack,
    secrets: &dyn SecretStore,
) -> Result<Connection> {
    // Before the store, not after: a credential with no region is a
    // configuration mistake, and finding it out must not cost the user a
    // keychain prompt for a connection that cannot open anyway.
    //
    // Its own message rather than the profile path's: a stored credential
    // carries its region as configuration, so "none is set in the
    // environment" would be advice about somewhere this never looks.
    let region = credential.region().trim();
    if region.is_empty() {
        return Err(Error::MissingConfiguration {
            profile: Some(credential.name().to_owned()),
            detail: "no region is configured for this stored credential".into(),
        });
    }
    let region = region.to_owned();
    let secret = credentials::load(secrets, credential.name())?;

    let sdk = aws_config::defaults(BehaviorVersion::latest())
        .region(SdkRegion::new(region.clone()))
        .credentials_provider(Credentials::new(
            credential.access_key_id(),
            secret.secret_access_key(),
            secret.session_token().map(ToOwned::to_owned),
            None,
            STORED_CREDENTIAL_PROVIDER,
        ))
        .http_client(http.client())
        .load()
        .await;

    Ok(Connection {
        id,
        name: credential.name().to_owned(),
        region,
        // A typed key belongs to no SSO session. See `Connection::sso_session`.
        sso_session: None,
        sdk,
    })
}

/// A missing region is a configuration error, never a silent default.
///
/// Defaulting would send calls to someone else's region and surface as a
/// bucket that "does not exist" — a wrong answer dressed as a real one.
fn require_region(profile: &str, region: Option<&str>) -> Result<String> {
    match region {
        Some(region) if !region.trim().is_empty() => Ok(region.to_owned()),
        _ => Err(Error::MissingConfiguration {
            profile: Some(profile.to_owned()),
            detail: "no region is configured for this profile and none is set in the environment"
                .into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    //! `connections` spec, "Credential resolution" — the region rule in full,
    //! and the wiring around the SDK chain. The chain itself is verified by
    //! hand against a real profile (design.md, Risks).
    //!
    //! And `stored-credentials` — that a connection opens from a credential
    //! this application holds exactly as it does from a profile, against a
    //! double of the credential store so no test touches a real keychain.

    use super::*;
    use crate::credentials::double::SecretStoreDouble;
    use crate::error::CredentialStoreProblem;
    use std::path::PathBuf;

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("caixonho-connection-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("fixture dir");
            Self { dir }
        }

        fn write(&self, file: &str, contents: &str) -> PathBuf {
            let path = self.dir.join(file);
            std::fs::write(&path, contents).expect("write fixture");
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// A credential store holding one usable stored credential.
    fn store_holding(credential: &StoredCredential) -> SecretStoreDouble {
        let store = SecretStoreDouble::open();
        credentials::save(
            &store,
            credential,
            &crate::credentials::CredentialSecret::new(
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                None,
            ),
        )
        .expect("an open store accepts it");
        store
    }

    fn stored() -> StoredCredential {
        StoredCredential::new("typed-in", "ap-southeast-1", "AKIAIOSFODNN7EXAMPLE")
    }

    #[test]
    fn a_configured_region_is_used_as_is() {
        assert_eq!(
            require_region("work", Some("ap-southeast-1")).expect("region present"),
            "ap-southeast-1"
        );
    }

    #[test]
    fn a_missing_region_names_the_profile_and_is_a_configuration_error() {
        match require_region("work", None) {
            Err(Error::MissingConfiguration { profile, detail }) => {
                assert_eq!(profile.as_deref(), Some("work"));
                assert!(
                    detail.contains("region"),
                    "message must say region: {detail}"
                );
            }
            other => panic!("expected MissingConfiguration, got {other:?}"),
        }
    }

    #[test]
    fn a_blank_region_is_treated_as_missing_not_as_a_region() {
        assert!(matches!(
            require_region("work", Some("   ")),
            Err(Error::MissingConfiguration { .. })
        ));
    }

    #[tokio::test]
    async fn a_named_but_absent_credentials_file_does_not_hide_the_config() {
        // Regression: the app reported "no region is configured" for a
        // default profile that declared one, because `ConfigPaths` names
        // `~/.aws/credentials` whether or not it exists and one absent file
        // makes the SDK drop the entire set — the normal shape of an
        // SSO-only machine.
        let fixture = Fixture::new("default-region");
        let config = fixture.write("config", "[default]\nregion = ap-southeast-1\n");
        let paths = ConfigPaths {
            config: Some(config),
            // The real app always names a credentials path, whether or not
            // the file exists — this is what it looked like in the failure.
            credentials: Some(fixture.dir.join("credentials-that-do-not-exist")),
        };
        let http = HttpStack::with_ca_bundle(None).expect("client builds");

        let connection = open(
            ConnectionId(1),
            &ConnectionSource::from("default"),
            &paths,
            &http,
            &SecretStoreDouble::open(),
        )
        .await
        .expect("the default profile declares a region");

        assert!(!connection.region().is_empty());
    }

    #[tokio::test]
    async fn opening_a_profile_carries_its_identity_through() {
        let fixture = Fixture::new("identity");
        let config = fixture.write(
            "config",
            "[profile work]\nregion = ap-southeast-1\n\
             aws_access_key_id = AKIAEXAMPLE\n\
             aws_secret_access_key = wJalrEXAMPLEKEY\n",
        );
        let paths = ConfigPaths {
            config: Some(config),
            credentials: None,
        };
        let http = HttpStack::with_ca_bundle(None).expect("client builds");

        let connection = open(
            ConnectionId(7),
            &ConnectionSource::from("work"),
            &paths,
            &http,
            &SecretStoreDouble::open(),
        )
        .await
        .expect("configuration resolves");

        assert_eq!(connection.id(), ConnectionId(7));
        assert_eq!(connection.name(), "work");
        // Not asserted as an exact value: an ambient AWS_REGION legitimately
        // outranks the profile, and this test must not depend on the
        // developer's environment being empty.
        assert!(!connection.region().is_empty());
    }

    #[test]
    fn a_bare_name_is_a_profile_and_a_stored_credential_is_itself() {
        // The conversion every call site that still names a profile relies
        // on. If a name ever stopped meaning "profile", every one of them
        // would silently open something else.
        assert_eq!(
            ConnectionSource::from("work"),
            ConnectionSource::Profile("work".to_owned())
        );
        assert_eq!(
            ConnectionSource::from("work".to_owned()),
            ConnectionSource::Profile("work".to_owned())
        );
        assert_eq!(
            ConnectionSource::from(stored()),
            ConnectionSource::Stored(stored())
        );
    }

    #[test]
    fn a_source_is_named_by_what_the_user_chose_whichever_kind_it_is() {
        assert_eq!(ConnectionSource::from("work").name(), "work");
        assert_eq!(ConnectionSource::from(stored()).name(), "typed-in");
    }

    #[tokio::test]
    async fn opening_a_stored_credential_carries_its_identity_through_like_a_profile_does() {
        let credential = stored();
        let store = store_holding(&credential);
        let http = HttpStack::with_ca_bundle(None).expect("client builds");

        let connection = open(
            ConnectionId(7),
            &ConnectionSource::from(credential),
            // A stored credential is this application's own: the shared
            // configuration is not consulted for it, and naming no files at
            // all is how that is asserted.
            &ConfigPaths {
                config: None,
                credentials: None,
            },
            &http,
            &store,
        )
        .await
        .expect("the store holds this credential's secret");

        assert_eq!(connection.id(), ConnectionId(7));
        assert_eq!(connection.name(), "typed-in");
        assert_eq!(
            connection.region(),
            "ap-southeast-1",
            "a stored credential carries its own region; nothing in the \
             environment outranks it"
        );
        assert_eq!(
            connection.sso_session(),
            None,
            "a typed key belongs to no SSO session to sign back into"
        );
    }

    #[tokio::test]
    async fn a_stored_credential_whose_secret_is_gone_has_no_credentials() {
        // Forgotten here, or removed from the keychain by hand. Either way
        // the configuration is still there and the secret is not, which is a
        // different fact from a store that would not open.
        let http = HttpStack::with_ca_bundle(None).expect("client builds");

        let opened = open(
            ConnectionId(1),
            &ConnectionSource::from(stored()),
            &ConfigPaths {
                config: None,
                credentials: None,
            },
            &http,
            &SecretStoreDouble::open(),
        )
        .await;

        match opened {
            Err(Error::NoCredentials { profile }) => assert_eq!(profile, "typed-in"),
            other => panic!("expected NoCredentials, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_credential_store_that_will_not_open_is_reported_as_itself() {
        let http = HttpStack::with_ca_bundle(None).expect("client builds");

        let opened = open(
            ConnectionId(1),
            &ConnectionSource::from(stored()),
            &ConfigPaths {
                config: None,
                credentials: None,
            },
            &http,
            &SecretStoreDouble::refusing(CredentialStoreProblem::Locked),
        )
        .await;

        match opened {
            Err(Error::CredentialStore {
                connection,
                problem,
            }) => {
                assert_eq!(connection, "typed-in");
                assert_eq!(problem, CredentialStoreProblem::Locked);
            }
            other => panic!("expected CredentialStore, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_stored_credential_with_no_region_never_reaches_the_credential_store() {
        // A configuration mistake must not cost the user a keychain prompt
        // for a connection that cannot open whatever the prompt answers.
        let credential = StoredCredential::new("typed-in", "   ", "AKIAIOSFODNN7EXAMPLE");
        let store = store_holding(&credential);
        let http = HttpStack::with_ca_bundle(None).expect("client builds");

        let opened = open(
            ConnectionId(1),
            &ConnectionSource::from(credential),
            &ConfigPaths {
                config: None,
                credentials: None,
            },
            &http,
            &store,
        )
        .await;

        match opened {
            Err(Error::MissingConfiguration { profile, detail }) => {
                assert_eq!(profile.as_deref(), Some("typed-in"));
                assert!(
                    detail.contains("region"),
                    "message must say region: {detail}"
                );
            }
            other => panic!("expected MissingConfiguration, got {other:?}"),
        }
    }
}
