//! Credentials this application holds itself, rather than finding in
//! `~/.aws`.
//!
//! A credential entered in the app is split in two the moment it arrives, and
//! the split is the design. The name, the region and the access key id are
//! ordinary configuration — readable, diffable, no different from any other
//! setting. The secret access key and the session token go to the operating
//! system's credential store and nowhere else (`stored-credentials` spec,
//! "Secrets live only in the operating system credential store"). Losing the
//! configuration therefore loses no secret, and reading the configuration
//! discloses none.
//!
//! Three rules bind this module:
//!
//! - **Nothing here writes `~/.aws/credentials`.** It would be the shortest
//!   route to a working connection and it is refused: that file is shared
//!   with every other AWS tool on the machine, and editing it on the user's
//!   behalf is a side effect nobody asked for. A stored credential is handed
//!   to the SDK as static credentials for that client instead (design.md,
//!   "Stored credentials use a static provider, not a written file").
//! - **A store that cannot be used is its own cause.** Locked, refused or
//!   absent — never a generic failure, and never an access denial
//!   ([`CredentialStoreProblem`]).
//! - **The store's own error is read but never echoed.** `keyring` reports a
//!   payload it could not decode by attaching the payload, and here that
//!   payload *is* the secret. Only the kind of failure crosses into
//!   [`Error`] — the same rule `classify.rs` follows for SDK error chains,
//!   for the same reason.

use crate::error::{CredentialStoreProblem, Error, Result};

/// The part of a stored credential that is not secret.
///
/// Ordinary configuration: whoever holds the list of connections holds these,
/// in the clear. Nothing in this module persists one — that is the caller's
/// business, and the point of keeping them apart from the secret is that it
/// can be done in the open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCredential {
    name: String,
    region: String,
    access_key_id: String,
}

impl StoredCredential {
    /// A credential the user has entered, minus its secret.
    pub fn new(
        name: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            region: region.into(),
            access_key_id: access_key_id.into(),
        }
    }

    /// What this connection is called. Also the key its secret is filed
    /// under in the credential store.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The region this credential is used in.
    pub fn region(&self) -> &str {
        &self.region
    }

    /// The access key id. Public material: it identifies the key, it does not
    /// authenticate with it.
    pub fn access_key_id(&self) -> &str {
        &self.access_key_id
    }
}

/// The part of a stored credential that is.
///
/// Exists only between the form that produced it and the credential store, or
/// between the credential store and the SDK client it signs for. It is never
/// written to a file, never logged, and never rendered — see the hand-written
/// [`std::fmt::Debug`] below, which is what keeps a stray `{:?}` from being
/// the exception.
#[derive(Clone, PartialEq, Eq)]
pub struct CredentialSecret {
    secret_access_key: String,
    session_token: Option<String>,
}

impl CredentialSecret {
    /// A long-lived credential's secret, or a temporary one's when a session
    /// token comes with it.
    pub fn new(secret_access_key: impl Into<String>, session_token: Option<String>) -> Self {
        Self {
            secret_access_key: secret_access_key.into(),
            session_token: session_token.filter(|token| !token.is_empty()),
        }
    }

    /// The secret access key.
    pub(crate) fn secret_access_key(&self) -> &str {
        &self.secret_access_key
    }

    /// The session token, when this is a temporary credential.
    pub(crate) fn session_token(&self) -> Option<&str> {
        self.session_token.as_deref()
    }
}

impl std::fmt::Debug for CredentialSecret {
    /// Hand-written so that the one thing this type exists to carry cannot
    /// reach a log, a panic message or an error through the derive. It still
    /// says whether a session token is present, because that is configuration
    /// — a temporary credential behaves differently from a long-lived one —
    /// and it is not the token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialSecret")
            .field("secret_access_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Which half of a stored credential a store entry holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SecretField {
    /// The secret access key. Every stored credential has one.
    SecretAccessKey,
    /// The session token, which only a temporary credential has.
    SessionToken,
}

impl SecretField {
    /// The service name this field's entries are filed under.
    ///
    /// The field goes in the service and the connection's name in the
    /// account, rather than both being glued into one string with a
    /// separator. A connection is named by whatever the user types, and a
    /// separator inside a name is a way for one connection to address
    /// another's entry.
    pub(crate) fn service(self) -> &'static str {
        match self {
            Self::SecretAccessKey => "caixonho secret access key",
            Self::SessionToken => "caixonho session token",
        }
    }
}

/// The operating system's credential store, as this application uses it.
///
/// A port, so the rules above are testable without a real keychain: every
/// test in this module runs against [`double::SecretStoreDouble`], and
/// [`Keyring`] — the one implementation that touches the machine — stays thin
/// enough to read in a sitting. Whether the keychain really refuses to
/// disclose needs a real keychain and is checked by hand (design.md, Risks).
pub(crate) trait SecretStore: std::fmt::Debug + Send + Sync {
    /// Put `secret` in the store for this connection's `field`, replacing
    /// whatever was there.
    fn put(&self, connection: &str, field: SecretField, secret: &str) -> Result<()>;

    /// What the store holds for this connection's `field`.
    ///
    /// `Ok(None)` is an answer: the store was reached and holds nothing —
    /// forgotten, or never saved. Only a store that could not answer at all
    /// is an error.
    fn get(&self, connection: &str, field: SecretField) -> Result<Option<String>>;

    /// Remove what the store holds for this connection's `field`.
    ///
    /// Removing what is not there succeeds: forgetting a long-lived
    /// credential must not report a failure over the session token it never
    /// had.
    fn delete(&self, connection: &str, field: SecretField) -> Result<()>;
}

/// Save `secret` for `credential`, in the credential store and nowhere else.
///
/// The configuration half of `credential` is not written by this function at
/// all. It is ordinary configuration and belongs to whoever holds the list of
/// connections; only the name is used here, as the key the secret is filed
/// under.
pub(crate) fn save(
    store: &dyn SecretStore,
    credential: &StoredCredential,
    secret: &CredentialSecret,
) -> Result<()> {
    let name = credential.name();
    store.put(
        name,
        SecretField::SecretAccessKey,
        secret.secret_access_key(),
    )?;

    let token = match secret.session_token() {
        Some(token) => store.put(name, SecretField::SessionToken, token),
        // A credential saved without a token must not inherit one from
        // whatever stood under this name before it.
        None => store.delete(name, SecretField::SessionToken),
    };
    if token.is_err() {
        // Telling the user the credential was not saved while half of it sits
        // in the store would be false, and the half left behind is the half
        // that signs requests. Best effort: if the store is refusing, it will
        // refuse this too, and the cause reported is still the real one.
        let _ = store.delete(name, SecretField::SecretAccessKey);
    }
    token
}

/// The secret half of the credential stored under `name`.
///
/// A store that holds nothing under this name is [`Error::NoCredentials`],
/// not a store failure: the connection is configured and its secret is gone —
/// forgotten here, or removed from the keychain by hand — which is a
/// different fact, with a different fix, from a store that could not be
/// reached.
pub(crate) fn load(store: &dyn SecretStore, name: &str) -> Result<CredentialSecret> {
    let Some(secret_access_key) = store.get(name, SecretField::SecretAccessKey)? else {
        return Err(Error::NoCredentials {
            profile: name.to_owned(),
        });
    };

    Ok(CredentialSecret {
        secret_access_key,
        session_token: store.get(name, SecretField::SessionToken)?,
    })
}

/// Delete everything the store holds for `name`.
///
/// Both fields are attempted whatever the first one does, so a store that
/// refuses one entry cannot leave the other standing; the first failure is
/// what is reported.
pub(crate) fn forget(store: &dyn SecretStore, name: &str) -> Result<()> {
    let key = store.delete(name, SecretField::SecretAccessKey);
    let token = store.delete(name, SecretField::SessionToken);
    key.and(token)
}

/// The real credential store: Keychain Services on macOS, the Credential
/// Manager on Windows — the two v1 targets.
///
/// A unit struct because it holds nothing: `keyring` initialises the
/// platform store once, lazily, on the first entry anyone builds. Nothing
/// here reaches the machine until a call does, which is what lets a
/// [`crate::Session`] carry one without startup paying for it.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Keyring;

impl SecretStore for Keyring {
    fn put(&self, connection: &str, field: SecretField, secret: &str) -> Result<()> {
        entry(connection, field)
            .and_then(|entry| entry.set_password(secret))
            .map_err(|error| store_failure(connection, &error))
    }

    fn get(&self, connection: &str, field: SecretField) -> Result<Option<String>> {
        match entry(connection, field).and_then(|entry| entry.get_password()) {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(store_failure(connection, &error)),
        }
    }

    fn delete(&self, connection: &str, field: SecretField) -> Result<()> {
        match entry(connection, field).and_then(|entry| entry.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(store_failure(connection, &error)),
        }
    }
}

/// The platform entry one field of one connection lives in.
fn entry(connection: &str, field: SecretField) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(field.service(), connection)
}

/// What this crate makes of a failure from the credential store.
///
/// The `keyring` error is destructured and dropped. It is never stringified
/// into the message and never kept as a source: `BadEncoding` and
/// `BadDataFormat` carry the bytes the store handed back, which for us is the
/// secret itself, and a reporter that walks an error's chain would print
/// them. `no_credential_store_failure_ever_discloses_the_secret` is what
/// keeps that true.
fn store_failure(connection: &str, error: &keyring::Error) -> Error {
    Error::CredentialStore {
        connection: connection.to_owned(),
        problem: problem_for(error),
    }
}

/// Which cause a `keyring` failure is.
///
/// Read against the macOS backend's own mapping (`apple-native-keyring-store`
/// 1.0.2, `src/keychain.rs`): `errSecNotAvailable`, `errSecReadOnly`,
/// `errSecNoSuchKeychain` and the write-permission code all arrive as
/// `NoStorageAccess`, which is a keychain that will not open — locked. A
/// prompt the user declines arrives as `PlatformFailure`, with every other
/// platform status; that is the store declining to do the thing, which is
/// refused. `NoEntry` never reaches here: it means the store answered and
/// holds nothing, which the callers above read as an answer.
fn problem_for(error: &keyring::Error) -> CredentialStoreProblem {
    match error {
        keyring::Error::NoStorageAccess(_) => CredentialStoreProblem::Locked,
        // No store was installed, or the platform has none this build knows
        // how to use. Neither is something the user can unlock.
        keyring::Error::NoDefaultStore | keyring::Error::NotSupportedByStore(_) => {
            CredentialStoreProblem::Absent
        }
        keyring::Error::Invalid(parameter, _) if parameter == "platform" => {
            CredentialStoreProblem::Absent
        }
        // Everything else: the store was there and the request did not
        // happen. `keyring::Error` is `#[non_exhaustive]`, so a variant added
        // upstream lands here rather than failing the build — and lands as
        // the honest answer, not as a guess about locking or absence.
        _ => CredentialStoreProblem::Refused,
    }
}

#[cfg(test)]
pub(crate) mod double {
    //! A credential store a test can hold in its hand: what it was given,
    //! what it will refuse, and nothing that touches the machine.

    use std::collections::BTreeMap;
    use std::sync::{Mutex, PoisonError};

    use super::{SecretField, SecretStore};
    use crate::error::{CredentialStoreProblem, Error, Result};

    /// What a call to the store is asked about — the map key the double
    /// files entries under, and what a test reads back.
    pub(crate) type Held = BTreeMap<(String, SecretField), String>;

    /// A [`SecretStore`] that keeps what it is given in memory.
    #[derive(Debug, Default)]
    pub(crate) struct SecretStoreDouble {
        held: Mutex<Held>,
        /// The cause every refused call reports, and which field is refused —
        /// `None` for all of them.
        refusing: Option<(CredentialStoreProblem, Option<SecretField>)>,
    }

    impl SecretStoreDouble {
        /// A store that accepts everything, holding nothing yet.
        pub(crate) fn open() -> Self {
            Self::default()
        }

        /// A store that refuses every call with `problem`.
        pub(crate) fn refusing(problem: CredentialStoreProblem) -> Self {
            Self {
                held: Mutex::default(),
                refusing: Some((problem, None)),
            }
        }

        /// A store that accepts everything except `field`.
        ///
        /// The half-written case: a credential whose secret access key goes
        /// in and whose session token does not.
        pub(crate) fn refusing_only(problem: CredentialStoreProblem, field: SecretField) -> Self {
            Self {
                held: Mutex::default(),
                refusing: Some((problem, Some(field))),
            }
        }

        /// Everything the store is holding.
        pub(crate) fn holds(&self) -> Held {
            self.held
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        /// Whether this call is one the store refuses, and with what.
        fn refusal(&self, connection: &str, field: SecretField) -> Option<Error> {
            match self.refusing {
                Some((problem, refused)) if refused.is_none_or(|refused| refused == field) => {
                    Some(Error::CredentialStore {
                        connection: connection.to_owned(),
                        problem,
                    })
                }
                _ => None,
            }
        }
    }

    impl SecretStore for SecretStoreDouble {
        fn put(&self, connection: &str, field: SecretField, secret: &str) -> Result<()> {
            if let Some(refusal) = self.refusal(connection, field) {
                return Err(refusal);
            }
            self.held
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert((connection.to_owned(), field), secret.to_owned());
            Ok(())
        }

        fn get(&self, connection: &str, field: SecretField) -> Result<Option<String>> {
            if let Some(refusal) = self.refusal(connection, field) {
                return Err(refusal);
            }
            Ok(self
                .held
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&(connection.to_owned(), field))
                .cloned())
        }

        fn delete(&self, connection: &str, field: SecretField) -> Result<()> {
            if let Some(refusal) = self.refusal(connection, field) {
                return Err(refusal);
            }
            self.held
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&(connection.to_owned(), field));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    //! `stored-credentials` spec — every requirement that does not need a
    //! real keychain: what saving puts where, what loading returns, what
    //! forgetting removes, and what a store that will not co-operate is
    //! reported as. The keychain itself is exercised by hand (tasks.md 6.3);
    //! what is verified here is everything above it.

    /// The real keychain, not the double — run it with
    /// `cargo test -p caixonho-core -- --ignored`.
    ///
    /// Ignored because it writes to the machine's own credential store and can
    /// raise a prompt, which is not something a test run should do behind
    /// someone's back. It exists because every other test here proves the
    /// *reasoning* against a double: nothing proved that a real keychain hands
    /// back exactly what it was given, and a secret that comes back altered is
    /// indistinguishable, from the outside, from a key the user typed wrongly.
    #[test]
    #[ignore = "writes to this machine's credential store"]
    fn a_real_credential_store_returns_exactly_what_it_was_given() {
        // Deliberately awkward: base64 padding, a slash, a quote, a backslash
        // and a tab are all things a store or a format could mangle.
        let secret = "aB3/xY9+zQ==weird\"chars\\and\ttabs";
        let token = "IQoJb3JpZ2luX2VjE\n//multiline+token/==";
        let name = "caixonho-test-please-delete";

        let store = Keyring;
        let credential = StoredCredential::new(name, "ap-southeast-1", "AKIAEXAMPLE");
        save(
            &store,
            &credential,
            &CredentialSecret::new(secret, Some(token.to_owned())),
        )
        .expect("the store accepted the secret");

        let read = load(&store, name);
        forget(&store, name).expect("the store released it again");
        let read = read.expect("the store returned the secret");

        assert_eq!(
            read.secret_access_key(),
            secret,
            "a secret that comes back altered looks exactly like a key typed wrongly"
        );
        assert_eq!(read.session_token(), Some(token));
    }

    use super::double::SecretStoreDouble;
    use super::*;

    /// The secret the tests put through the store. Not a real key — but it
    /// is treated as one everywhere below, which is the point.
    const SECRET: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    const TOKEN: &str = "FQoGZXIvYXdzEExampleSessionToken";

    fn credential() -> StoredCredential {
        StoredCredential::new("work", "ap-southeast-1", "AKIAIOSFODNN7EXAMPLE")
    }

    fn long_lived() -> CredentialSecret {
        CredentialSecret::new(SECRET, None)
    }

    fn temporary() -> CredentialSecret {
        CredentialSecret::new(SECRET, Some(TOKEN.to_owned()))
    }

    #[test]
    fn saving_puts_the_secret_in_the_store_and_the_rest_nowhere() {
        // The split the spec asks for: name, region and access key id are
        // ordinary configuration and never reach the credential store; the
        // secret reaches nothing else.
        let store = SecretStoreDouble::open();
        let credential = credential();

        save(&store, &credential, &long_lived()).expect("an open store accepts it");

        let held = store.holds();
        assert_eq!(
            held.get(&("work".to_owned(), SecretField::SecretAccessKey))
                .map(String::as_str),
            Some(SECRET)
        );
        for configuration in [
            credential.name(),
            credential.region(),
            credential.access_key_id(),
        ] {
            assert!(
                !held.values().any(|held| held == configuration),
                "`{configuration}` is ordinary configuration and has no business in \
                 the credential store"
            );
        }
    }

    #[test]
    fn a_temporary_credentials_session_token_is_stored_with_it() {
        let store = SecretStoreDouble::open();

        save(&store, &credential(), &temporary()).expect("an open store accepts it");

        assert_eq!(
            store
                .holds()
                .get(&("work".to_owned(), SecretField::SessionToken))
                .map(String::as_str),
            Some(TOKEN)
        );
    }

    #[test]
    fn a_credential_saved_without_a_token_is_left_holding_no_token() {
        // Editing in place is not offered — a credential is forgotten and
        // entered again (design.md, Non-Goals). Re-using a name must
        // therefore not leave the previous credential's session token behind
        // to be signed with by the new one.
        let store = SecretStoreDouble::open();
        save(&store, &credential(), &temporary()).expect("an open store accepts it");

        save(&store, &credential(), &long_lived()).expect("an open store accepts it");

        assert!(
            !store
                .holds()
                .contains_key(&("work".to_owned(), SecretField::SessionToken)),
            "the previous credential's session token must not outlive it"
        );
    }

    #[test]
    fn what_was_saved_is_what_loading_returns() {
        let store = SecretStoreDouble::open();
        let secret = temporary();
        save(&store, &credential(), &secret).expect("an open store accepts it");

        let loaded = load(&store, "work").expect("the store holds it");

        assert_eq!(loaded, secret);
        assert_eq!(loaded.secret_access_key(), SECRET);
        assert_eq!(loaded.session_token(), Some(TOKEN));
    }

    #[test]
    fn a_connection_the_store_holds_nothing_for_has_no_credentials_rather_than_a_broken_store() {
        // Two different facts with two different fixes: the store answered
        // and holds nothing, versus the store could not answer at all.
        let store = SecretStoreDouble::open();

        match load(&store, "work") {
            Err(Error::NoCredentials { profile }) => assert_eq!(profile, "work"),
            other => panic!("expected NoCredentials, got {other:?}"),
        }
    }

    #[test]
    fn forgetting_a_credential_deletes_everything_the_store_held_for_it() {
        let store = SecretStoreDouble::open();
        save(&store, &credential(), &temporary()).expect("an open store accepts it");

        forget(&store, "work").expect("an open store forgets it");

        assert!(
            store.holds().is_empty(),
            "a forgotten connection leaves nothing behind to be signed with"
        );
        assert!(
            matches!(load(&store, "work"), Err(Error::NoCredentials { .. })),
            "and nothing left to load"
        );
    }

    #[test]
    fn forgetting_leaves_every_other_connection_alone() {
        let store = SecretStoreDouble::open();
        save(&store, &credential(), &temporary()).expect("an open store accepts it");
        let other = StoredCredential::new("personal", "eu-west-1", "AKIAOTHEREXAMPLE");
        save(&store, &other, &long_lived()).expect("an open store accepts it");

        forget(&store, "work").expect("an open store forgets it");

        assert_eq!(
            load(&store, "personal")
                .expect("the other connection is untouched")
                .secret_access_key(),
            SECRET
        );
    }

    #[test]
    fn forgetting_a_credential_that_never_had_a_session_token_is_not_a_failure() {
        let store = SecretStoreDouble::open();
        save(&store, &credential(), &long_lived()).expect("an open store accepts it");

        forget(&store, "work").expect("an absent session token is not a failure");
    }

    #[test]
    fn a_store_that_refuses_is_reported_as_its_own_cause_and_nothing_is_written() {
        // The spec's scenario in full: the user is told the credential was
        // not saved and why, and there is no second place the secret went.
        for problem in [
            CredentialStoreProblem::Locked,
            CredentialStoreProblem::Refused,
            CredentialStoreProblem::Absent,
        ] {
            let store = SecretStoreDouble::refusing(problem);

            match save(&store, &credential(), &temporary()) {
                Err(Error::CredentialStore {
                    connection,
                    problem: reported,
                }) => {
                    assert_eq!(connection, "work");
                    assert_eq!(reported, problem);
                }
                other => panic!("expected CredentialStore/{problem:?}, got {other:?}"),
            }
            assert!(
                store.holds().is_empty(),
                "{problem:?}: a refused save writes nothing, here or anywhere else"
            );
        }
    }

    #[test]
    fn a_store_that_refuses_the_session_token_leaves_no_half_saved_credential() {
        // The half left behind would be the half that signs requests: a
        // connection that looks saved, is offered, and cannot authenticate.
        let store = SecretStoreDouble::refusing_only(
            CredentialStoreProblem::Refused,
            SecretField::SessionToken,
        );

        let saved = save(&store, &credential(), &temporary());

        assert!(matches!(saved, Err(Error::CredentialStore { .. })));
        assert!(
            store.holds().is_empty(),
            "the secret access key that did go in has to come back out"
        );
    }

    #[test]
    fn a_store_that_refuses_is_reported_when_loading_and_when_forgetting_too() {
        let store = SecretStoreDouble::refusing(CredentialStoreProblem::Locked);

        assert!(matches!(
            load(&store, "work"),
            Err(Error::CredentialStore {
                problem: CredentialStoreProblem::Locked,
                ..
            })
        ));
        assert!(matches!(
            forget(&store, "work"),
            Err(Error::CredentialStore {
                problem: CredentialStoreProblem::Locked,
                ..
            })
        ));
    }

    #[test]
    fn a_store_that_cannot_be_used_is_never_reported_as_an_access_denial() {
        // The invariant this crate is built around: only a service-side
        // authorization refusal may be presented as one. A keychain the user
        // has not unlocked is not an IAM policy, and sending someone to edit
        // a policy document over a locked keychain is the failure mode the
        // whole error enum exists to prevent.
        for problem in [
            CredentialStoreProblem::Locked,
            CredentialStoreProblem::Refused,
            CredentialStoreProblem::Absent,
        ] {
            let store = SecretStoreDouble::refusing(problem);

            for attempt in [
                save(&store, &credential(), &long_lived()),
                load(&store, "work").map(|_| ()),
                forget(&store, "work"),
            ] {
                assert!(
                    !matches!(attempt, Err(Error::AccessDenied { .. })),
                    "{problem:?} must never read as a denial"
                );
                assert!(
                    matches!(attempt, Err(Error::CredentialStore { .. })),
                    "{problem:?} is a cause of its own, not a generic failure: {attempt:?}"
                );
            }
        }
    }

    #[test]
    fn each_kind_of_store_failure_keeps_its_own_cause() {
        // The one judgement the keyring adapter makes, tested without a
        // keychain. Read against `apple-native-keyring-store` 1.0.2's own
        // mapping — see `problem_for`.
        let cases: [(keyring::Error, CredentialStoreProblem); 5] = [
            (
                keyring::Error::NoStorageAccess(Box::new(std::io::Error::other("errSecReadOnly"))),
                CredentialStoreProblem::Locked,
            ),
            (
                keyring::Error::NoDefaultStore,
                CredentialStoreProblem::Absent,
            ),
            (
                keyring::Error::NotSupportedByStore("no secrets here".to_owned()),
                CredentialStoreProblem::Absent,
            ),
            (
                keyring::Error::Invalid("platform".to_owned(), "unsupported".to_owned()),
                CredentialStoreProblem::Absent,
            ),
            (
                keyring::Error::PlatformFailure(Box::new(std::io::Error::other(
                    "errSecAuthFailed",
                ))),
                CredentialStoreProblem::Refused,
            ),
        ];

        for (error, expected) in &cases {
            assert_eq!(
                problem_for(error),
                *expected,
                "{error} must be reported as {expected:?}"
            );
        }
    }

    #[test]
    fn no_credential_store_failure_ever_discloses_the_secret() {
        // The redaction rule from the store's side. `classify.rs` guards the
        // other end, where a secret could arrive off the wire; this is where
        // it could arrive out of the keychain, and the route is concrete:
        // `keyring::Error::BadEncoding` and `BadDataFormat` are *defined* as
        // carrying the payload the store handed back, and here that payload
        // is the secret access key. Copying the store's error into a message
        // — or keeping it as a source for a reporter to walk — is the one way
        // it reaches a log or the UI.
        //
        // The platform-error cases carry the secret too. That is defensive
        // rather than observed: the invariant is that no store error is
        // echoed, not that the ones we have seen happen to be harmless.
        let cases = [
            keyring::Error::BadEncoding(SECRET.as_bytes().to_vec()),
            keyring::Error::BadDataFormat(
                SECRET.as_bytes().to_vec(),
                Box::new(std::io::Error::other("could not decrypt")),
            ),
            keyring::Error::BadStoreFormat(format!("unreadable entry holding {SECRET}")),
            keyring::Error::PlatformFailure(Box::new(std::io::Error::other(format!(
                "the keychain rejected {SECRET} with token {TOKEN}"
            )))),
            keyring::Error::NoStorageAccess(Box::new(std::io::Error::other(format!(
                "locked while holding {SECRET}"
            )))),
        ];

        for error in &cases {
            let reported = store_failure("work", error);
            let rendered = format!("{reported} {reported:?}");

            for secret in [SECRET, TOKEN] {
                // Two spellings, because the dangerous variants carry the
                // secret as *bytes*: `{:?}` on `BadEncoding` prints
                // `[119, 74, 97, ...]`, and searching the rendered message for
                // the readable string would sail straight past a verbatim
                // disclosure of the very variant whose documentation says it
                // hands the payload back. Found by mutating this test: with
                // the store's error stringified into the message, the readable
                // check alone passed for `BadEncoding` and only tripped three
                // cases later.
                for disclosure in [secret.to_owned(), format!("{:?}", secret.as_bytes())] {
                    assert!(
                        !rendered.contains(&disclosure),
                        "`{secret}` reached a user-visible message as `{disclosure}` \
                         via {error:?}: {rendered}"
                    );
                }
            }
            assert!(
                rendered.contains("work"),
                "the connection is what the report may name: {rendered}"
            );
        }
    }

    #[test]
    fn a_secret_never_shows_itself_in_a_debug_rendering() {
        // The derive would have printed both. Anything that formats a value
        // it did not expect to hold a secret — a panic message, a log line,
        // a `{:?}` in an error — goes through this.
        let rendered = format!("{:?}", temporary());

        assert!(!rendered.contains(SECRET), "{rendered}");
        assert!(!rendered.contains(TOKEN), "{rendered}");
        assert!(
            rendered.contains("session_token: Some"),
            "whether a credential is temporary is configuration, not a secret: {rendered}"
        );
    }

    #[test]
    fn an_empty_session_token_is_no_session_token() {
        // A form with an untouched optional field hands back an empty
        // string; storing one would make a long-lived credential look
        // temporary and be signed as one.
        assert_eq!(
            CredentialSecret::new(SECRET, Some(String::new())).session_token(),
            None
        );
    }

    #[test]
    fn the_two_halves_of_a_credential_are_filed_apart() {
        assert_ne!(
            SecretField::SecretAccessKey.service(),
            SecretField::SessionToken.service(),
            "one entry per field, or saving the second would overwrite the first"
        );
    }
}
