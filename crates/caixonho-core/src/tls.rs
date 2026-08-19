//! One HTTP client, one place where TLS is decided.
//!
//! The same client is handed to the credential/SSO loader *and* to the S3
//! client. That is the whole point: the credential path builds its own client
//! by default, so configuring only the service client is exactly how
//! enterprise TLS interception ends up working everywhere except sign-in
//! (`connections` spec, "Enterprise trust material is honoured").
//!
//! There is deliberately no way to disable verification — trust material is
//! configurable, verification is not (repo invariant 4).
//!
//! ## Why not `rustls-platform-verifier`
//!
//! The change planned this module around `rustls-platform-verifier`.
//! `aws-smithy-http-client` 1.3.0 has no supported way to install a custom
//! `rustls::ClientConfig`: the public TLS knobs are `tls::Provider` and
//! `tls::TrustStore`, and the one injection point that would take a custom
//! connector (`Builder::build_with_connector_fn`) is `#[doc(hidden)]`.
//! Building a security-critical path on a hidden API — or hand-rolling an
//! `HttpClient` and owning pooling, timeouts and proxy support with it — buys
//! less than it costs, because the supported `TrustStore` already satisfies
//! every clause of the requirement: platform roots come from the OS trust
//! store, and `AWS_CA_BUNDLE` / `SSL_CERT_FILE` are added on top of them.
//!
//! The difference that remains: `rustls-platform-verifier` delegates the
//! verification decision to the platform (and so honours per-certificate
//! trust overrides and revocation policy), while the trust store here snapshots
//! the platform's root certificates into rustls. Revisit if a real enterprise
//! machine turns up a chain the OS accepts and we reject.

use std::path::{Path, PathBuf};

use aws_smithy_http_client::{Builder, tls};
use aws_smithy_runtime_api::client::http::SharedHttpClient;

use crate::error::{Error, Result};

/// The HTTP client every AWS call in this process shares.
///
/// Wraps the SDK's client so no AWS type appears in a public signature: the
/// app builds one of these at startup and hands it to every connection it
/// opens, which is what keeps connection pooling and trust configuration
/// from being per-profile accidents.
#[derive(Debug, Clone)]
pub struct HttpStack {
    client: SharedHttpClient,
}

impl HttpStack {
    /// Build the shared client, trusting the OS store plus any bundle named
    /// by `AWS_CA_BUNDLE` or `SSL_CERT_FILE`.
    pub fn from_env() -> Result<Self> {
        Self::with_ca_bundle(ca_bundle_from_env().as_deref())
    }

    /// Build the shared client with an explicit extra bundle (or none).
    pub fn with_ca_bundle(ca_bundle: Option<&Path>) -> Result<Self> {
        // Native roots stay on even when a bundle is supplied: a corporate
        // bundle is normally the interception CA alone, and dropping the
        // public roots with it would break every endpoint that is not
        // intercepted.
        let mut trust_store = tls::TrustStore::default().with_native_roots(true);

        if let Some(path) = ca_bundle {
            let pem = std::fs::read(path).map_err(|source| Error::MissingConfiguration {
                profile: None,
                detail: format!(
                    "cannot read the certificate bundle at `{}`: {source}",
                    path.display()
                ),
            })?;
            trust_store = trust_store.with_pem_certificate(pem);
        }

        let context = tls::TlsContext::builder()
            .with_trust_store(trust_store)
            .build()
            .map_err(|source| Error::MissingConfiguration {
                profile: None,
                detail: format!("certificate trust material was rejected: {source}"),
            })?;

        Ok(Self {
            client: Builder::new()
                .tls_provider(tls::Provider::Rustls(
                    tls::rustls_provider::CryptoMode::AwsLc,
                ))
                .tls_context(context)
                .build_https(),
        })
    }

    /// The wrapped client, for the modules that talk to the SDK.
    pub(crate) fn client(&self) -> SharedHttpClient {
        self.client.clone()
    }
}

/// Which certificate bundle the environment asks us to trust, if any.
fn ca_bundle_from_env() -> Option<PathBuf> {
    pick_ca_bundle(
        std::env::var_os("AWS_CA_BUNDLE").map(PathBuf::from),
        std::env::var_os("SSL_CERT_FILE").map(PathBuf::from),
    )
}

/// `AWS_CA_BUNDLE` wins over `SSL_CERT_FILE` — the AWS-specific variable is
/// the more deliberate statement when both are set. An empty value is
/// treated as unset rather than as a path to nowhere.
fn pick_ca_bundle(
    aws_ca_bundle: Option<PathBuf>,
    ssl_cert_file: Option<PathBuf>,
) -> Option<PathBuf> {
    [aws_ca_bundle, ssl_cert_file]
        .into_iter()
        .flatten()
        .find(|path| !path.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    //! `connections` spec, "Enterprise trust material is honoured".

    use super::*;

    #[test]
    fn aws_ca_bundle_wins_over_ssl_cert_file() {
        let picked = pick_ca_bundle(
            Some(PathBuf::from("/etc/corp/aws.pem")),
            Some(PathBuf::from("/etc/ssl/cert.pem")),
        );
        assert_eq!(picked, Some(PathBuf::from("/etc/corp/aws.pem")));
    }

    #[test]
    fn ssl_cert_file_is_used_when_it_is_the_only_one_set() {
        let picked = pick_ca_bundle(None, Some(PathBuf::from("/etc/ssl/cert.pem")));
        assert_eq!(picked, Some(PathBuf::from("/etc/ssl/cert.pem")));
    }

    #[test]
    fn an_empty_value_counts_as_unset() {
        assert_eq!(pick_ca_bundle(Some(PathBuf::new()), None), None);
        assert_eq!(
            pick_ca_bundle(
                Some(PathBuf::new()),
                Some(PathBuf::from("/etc/ssl/cert.pem"))
            ),
            Some(PathBuf::from("/etc/ssl/cert.pem"))
        );
        assert_eq!(pick_ca_bundle(None, None), None);
    }

    #[test]
    fn the_os_trust_store_alone_builds_a_client() {
        assert!(HttpStack::with_ca_bundle(None).is_ok());
    }

    #[test]
    fn an_unreadable_bundle_is_a_configuration_error_not_a_trust_failure() {
        let missing = std::env::temp_dir().join("caixonho-no-such-bundle.pem");
        let _ = std::fs::remove_file(&missing);

        match HttpStack::with_ca_bundle(Some(&missing)) {
            Err(Error::MissingConfiguration { profile, detail }) => {
                assert!(profile.is_none(), "the bundle is not profile-scoped");
                assert!(
                    detail.contains("certificate bundle"),
                    "message must point at the bundle, got: {detail}"
                );
            }
            other => panic!("expected MissingConfiguration, got {other:?}"),
        }
    }

    #[test]
    fn a_readable_bundle_is_added_to_the_trust_store() {
        let path = std::env::temp_dir().join("caixonho-test-bundle.pem");
        std::fs::write(
            &path,
            b"-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----\n",
        )
        .expect("write bundle");

        let built = HttpStack::with_ca_bundle(Some(&path));
        let _ = std::fs::remove_file(&path);

        assert!(built.is_ok(), "a readable bundle must build a client");
    }
}
