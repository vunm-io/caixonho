//! Opening a connection: one chosen profile → resolved credentials → a
//! configuration the S3 adapter can build a client from.
//!
//! Resolution itself is the SDK's job — static keys, `role_arn` +
//! `source_profile` chains and SSO tokens already cached by the AWS CLI all
//! come from its provider chain, and reimplementing that precedence here
//! would be a second source of truth that drifts (`connections` spec,
//! "Credential resolution").
//!
//! What this module owns is everything around that chain: pointing it at the
//! right files, giving it the shared HTTP client so credential and SSO calls
//! use the same trust material as S3 calls, and refusing to continue when the
//! region is missing.

use aws_config::BehaviorVersion;
use aws_runtime::env_config::file::{EnvConfigFileKind, EnvConfigFiles};

use crate::error::{Error, Result};
use crate::profiles::ConfigPaths;
use crate::tls::HttpStack;
use crate::types::ConnectionId;

/// An opened connection: which profile it is, where it points, and the
/// resolved configuration the adapter turns into an S3 client.
///
/// Holding the id here is what lets a late response be dropped instead of
/// being rendered as if it belonged to the profile the user switched to.
#[derive(Debug, Clone)]
pub struct Connection {
    id: ConnectionId,
    profile: String,
    region: String,
    /// Read by the S3 adapter (section 5 of this change).
    #[allow(dead_code)]
    sdk: aws_config::SdkConfig,
}

impl Connection {
    /// Which connection this is.
    pub fn id(&self) -> ConnectionId {
        self.id
    }

    /// The profile this connection was opened for.
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// The region this connection resolved to.
    pub fn region(&self) -> &str {
        &self.region
    }

    /// The resolved SDK configuration, for the adapter that builds clients.
    #[allow(dead_code)]
    pub(crate) fn sdk_config(&self) -> &aws_config::SdkConfig {
        &self.sdk
    }
}

/// Open a connection for `profile`.
///
/// Succeeding here means the configuration resolved, not that the credentials
/// work: the SDK's providers are lazy, so an expired session or a denied
/// policy surfaces on the first real call and is classified there. That is
/// deliberate — the spec requires a profile with unusable credentials to stay
/// listed and fail only when used.
pub async fn open(
    id: ConnectionId,
    profile: &str,
    paths: &ConfigPaths,
    http: &HttpStack,
) -> Result<Connection> {
    let mut loader = aws_config::defaults(BehaviorVersion::latest())
        .profile_name(profile)
        .http_client(http.client());

    // `EnvConfigFiles::build` panics on an empty file set, so only override the
    // SDK's own defaults when we actually have a path to give it.
    let mut files = EnvConfigFiles::builder();
    let mut any_file = false;
    if let Some(config) = paths.config.as_ref() {
        files = files.with_file(EnvConfigFileKind::Config, config.clone());
        any_file = true;
    }
    if let Some(credentials) = paths.credentials.as_ref() {
        files = files.with_file(EnvConfigFileKind::Credentials, credentials.clone());
        any_file = true;
    }
    if any_file {
        loader = loader.profile_files(files.build());
    }

    let sdk = loader.load().await;
    let region = require_region(profile, sdk.region().map(AsRef::as_ref))?;

    Ok(Connection {
        id,
        profile: profile.to_owned(),
        region,
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

    use super::*;
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

        let connection = open(ConnectionId(7), "work", &paths, &http)
            .await
            .expect("configuration resolves");

        assert_eq!(connection.id(), ConnectionId(7));
        assert_eq!(connection.profile(), "work");
        // Not asserted as an exact value: an ambient AWS_REGION legitimately
        // outranks the profile, and this test must not depend on the
        // developer's environment being empty.
        assert!(!connection.region().is_empty());
    }
}
