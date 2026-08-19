//! The observed-capability model.
//!
//! S3 has no API that enumerates a caller's effective permissions, so
//! caixonho never *declares* what the user can do — it records what has been
//! *observed*, one operation at a time, and is honest about everything else.
//!
//! This module holds that model and the store that remembers it. The
//! invariant that must survive every addition here is the three-valued logic
//! below: `Unknown` is the default, and only evidence moves a capability out
//! of it. "Being probed" is deliberately not a fourth value — it is a fact
//! about our own activity, not a claim about the world, so it lives beside
//! this model (the scheduler's in-flight set) and the view combines the two.

use std::collections::HashMap;

/// What we currently know about one operation on one scope
/// (a bucket or a prefix), for one set of credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Observation {
    /// No evidence yet. This is the default and must render as "unknown",
    /// never as "denied".
    #[default]
    Unknown,
    /// A probe or a real operation succeeded.
    Allowed,
    /// A real `AccessDenied` was observed. Only an access-denial error maps
    /// here — expired tokens, wrong regions, network failures and missing
    /// buckets are different states and must never be folded into this one.
    Denied,
}

/// The set of capabilities caixonho tracks per scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capability {
    pub list: Observation,
    pub read: Observation,
    /// Write is special: it is never probed automatically (a write probe
    /// creates an object). It moves out of `Unknown` only through a real
    /// user-initiated operation.
    pub write: Observation,
    pub delete: Observation,
}

/// Identifies the set of credentials an observation was made under.
///
/// A profile name alone will not do: re-authenticating the same profile
/// produces different credentials, and everything observed under the previous
/// ones has to go. Only [`CapabilityStore`] mints these, so an observation
/// cannot be attributed to credentials that never existed — and a probe issued
/// before a switch cannot land under the credentials that replaced it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CredentialsId {
    profile: String,
    epoch: u64,
}

impl CredentialsId {
    /// The profile these credentials were resolved for.
    pub fn profile(&self) -> &str {
        &self.profile
    }
}

/// What an observation is about.
///
/// A whole bucket, or one prefix inside it. Only buckets are probed today;
/// `XONHO-0006` brings prefixes, and the key already tells the two apart, so
/// landing that adds a probe rather than rekeying what is stored here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scope {
    bucket: String,
    prefix: Option<String>,
}

impl Scope {
    /// A whole bucket.
    pub fn bucket(name: impl Into<String>) -> Self {
        Self {
            bucket: name.into(),
            prefix: None,
        }
    }

    /// One prefix inside a bucket. Capability on a prefix is its own
    /// question: a policy may grant it where the bucket root is denied, and
    /// neither one is evidence about the other.
    pub fn prefix(bucket: impl Into<String>, prefix: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            prefix: Some(prefix.into()),
        }
    }

    /// The bucket this scope lives in.
    pub fn bucket_name(&self) -> &str {
        &self.bucket
    }

    /// The prefix within that bucket, when the scope names one.
    pub fn key_prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }
}

/// What has been observed, per set of credentials and per scope.
///
/// In memory and for one session only: nothing is persisted across restarts,
/// and nothing expires on a timer. An observation leaves exactly two ways —
/// the credentials change (a profile switch, or a re-authentication), or a
/// newer observation of the same scope replaces it. A real operation is an
/// observation like any other, which is what lets a successful listing settle
/// a scope without any probe being issued for it.
#[derive(Debug, Default)]
pub struct CapabilityStore {
    /// The credentials everything in `observed` was gathered under. `None`
    /// until a profile has been opened at all.
    credentials: Option<CredentialsId>,
    /// Bumped on every credentials change, so a set of credentials is never
    /// confused with the one that replaced it.
    epochs: u64,
    observed: HashMap<Scope, Capability>,
}

impl CapabilityStore {
    /// An empty store, with no credentials yet in play.
    pub fn new() -> Self {
        Self::default()
    }

    /// Note that a different set of credentials is now in play — a switch to
    /// another profile, or a re-authentication of the one already open — and
    /// discard everything the previous ones observed.
    ///
    /// Returns the key to record observations under from here on.
    pub fn credentials_changed(&mut self, profile: &str) -> CredentialsId {
        self.epochs += 1;
        let credentials = CredentialsId {
            profile: profile.to_owned(),
            epoch: self.epochs,
        };
        self.credentials = Some(credentials.clone());
        self.observed.clear();
        credentials
    }

    /// The credentials observations are being recorded under, if any.
    pub fn credentials(&self) -> Option<&CredentialsId> {
        self.credentials.as_ref()
    }

    /// What is known about `scope` under `credentials`.
    ///
    /// A scope nobody has observed reads as [`Observation::Unknown`] — and so
    /// does every scope asked about under credentials that are no longer in
    /// play. Never as denied: absence of evidence is not a denial.
    pub fn capability(&self, credentials: &CredentialsId, scope: &Scope) -> Capability {
        if !self.is_current(credentials) {
            return Capability::default();
        }
        self.observed.get(scope).copied().unwrap_or_default()
    }

    /// Whether `scope` still lacks evidence about listing it.
    ///
    /// This is the question the probe scheduler asks: a scope that has been
    /// observed — allowed or denied, by a probe or by a real operation — is
    /// not worth a request.
    pub fn needs_list_probe(&self, credentials: &CredentialsId, scope: &Scope) -> bool {
        self.capability(credentials, scope).list == Observation::Unknown
    }

    /// Record what a completed list operation showed — a probe or a real
    /// listing, the model does not distinguish them.
    ///
    /// Returns whether anything was recorded. Two things are refused, both
    /// silently and both on purpose:
    ///
    /// - a result belonging to credentials that are no longer in play, which
    ///   is a probe that outlived the switch that invalidated it;
    /// - [`Observation::Unknown`], which is what an expired session, a
    ///   rejected credential, a wrong region, an unreachable network or
    ///   throttling amounts to. Those keep their own cause elsewhere and are
    ///   no evidence about permission, so they must neither record a denial
    ///   nor erase what is already known.
    pub fn observe_list(
        &mut self,
        credentials: &CredentialsId,
        scope: Scope,
        observation: Observation,
    ) -> bool {
        if !self.is_current(credentials) || observation == Observation::Unknown {
            return false;
        }
        // Only `list` is touched. `write` in particular is never probed and
        // never inferred: it moves only through an operation the user asked
        // for.
        self.observed.entry(scope).or_default().list = observation;
        true
    }

    fn is_current(&self, credentials: &CredentialsId) -> bool {
        self.credentials.as_ref() == Some(credentials)
    }
}

#[cfg(test)]
mod tests {
    //! `capability-awareness` spec: "Capability is observed, never declared",
    //! "Only a denial may be presented as a denial", "Write capability is
    //! never probed" and "Observations are scoped to the credentials that
    //! produced them".

    use super::*;

    #[test]
    fn capability_defaults_to_unknown_everywhere() {
        let cap = Capability::default();
        assert_eq!(cap.list, Observation::Unknown);
        assert_eq!(cap.read, Observation::Unknown);
        assert_eq!(cap.write, Observation::Unknown);
        assert_eq!(cap.delete, Observation::Unknown);
    }

    #[test]
    fn a_store_that_has_opened_no_profile_holds_no_credentials() {
        let store = CapabilityStore::new();

        assert!(store.credentials().is_none());
    }

    #[test]
    fn an_unobserved_scope_reads_unknown_and_still_wants_a_probe() {
        let mut store = CapabilityStore::new();
        let credentials = store.credentials_changed("work");
        let logs = Scope::bucket("logs");

        assert_eq!(
            store.capability(&credentials, &logs).list,
            Observation::Unknown
        );
        assert!(store.needs_list_probe(&credentials, &logs));
    }

    #[test]
    fn observations_are_keyed_by_scope() {
        let mut store = CapabilityStore::new();
        let credentials = store.credentials_changed("work");

        store.observe_list(&credentials, Scope::bucket("logs"), Observation::Denied);

        assert_eq!(
            store.capability(&credentials, &Scope::bucket("logs")).list,
            Observation::Denied
        );
        assert_eq!(
            store
                .capability(&credentials, &Scope::bucket("backups"))
                .list,
            Observation::Unknown,
            "one bucket's denial is no evidence about another's"
        );
    }

    #[test]
    fn a_prefix_is_a_different_scope_from_the_bucket_holding_it() {
        // `XONHO-0006` probes prefixes. The key already tells a prefix apart
        // from its bucket, so landing that means adding a probe — not rekeying
        // what is stored here, and not rewriting the callers that store it.
        let mut store = CapabilityStore::new();
        let credentials = store.credentials_changed("work");

        store.observe_list(&credentials, Scope::bucket("logs"), Observation::Allowed);

        assert_eq!(
            store
                .capability(&credentials, &Scope::prefix("logs", "2026/"))
                .list,
            Observation::Unknown,
            "listing a bucket is no evidence about listing one prefix in it"
        );
    }

    #[test]
    fn observations_are_keyed_by_credentials_and_a_switch_discards_them() {
        let mut store = CapabilityStore::new();
        let work = store.credentials_changed("work");
        store.observe_list(&work, Scope::bucket("logs"), Observation::Denied);

        let personal = store.credentials_changed("personal");

        assert_ne!(work, personal);
        assert_eq!(
            store.credentials().map(CredentialsId::profile),
            Some("personal")
        );
        assert_eq!(
            store.capability(&personal, &Scope::bucket("logs")).list,
            Observation::Unknown,
            "the new profile starts from no evidence"
        );
        assert_eq!(
            store.capability(&work, &Scope::bucket("logs")).list,
            Observation::Unknown,
            "the previous profile's observations are discarded, not merely shadowed"
        );
    }

    #[test]
    fn re_authenticating_the_same_profile_discards_them_too() {
        let mut store = CapabilityStore::new();
        let before = store.credentials_changed("work");
        store.observe_list(&before, Scope::bucket("logs"), Observation::Denied);

        let after = store.credentials_changed("work");

        assert_ne!(
            before, after,
            "same profile, different credentials — a signed-in session is not the one that expired"
        );
        assert_eq!(
            store.capability(&after, &Scope::bucket("logs")).list,
            Observation::Unknown
        );
    }

    #[test]
    fn a_result_from_credentials_no_longer_in_play_is_refused() {
        let mut store = CapabilityStore::new();
        let before = store.credentials_changed("work");
        let after = store.credentials_changed("personal");

        let recorded = store.observe_list(&before, Scope::bucket("logs"), Observation::Denied);

        assert!(
            !recorded,
            "a probe issued before the switch must not land under the credentials that replaced it"
        );
        assert_eq!(
            store.capability(&after, &Scope::bucket("logs")).list,
            Observation::Unknown
        );
    }

    #[test]
    fn a_successful_operation_records_allowed_without_a_probe() {
        let mut store = CapabilityStore::new();
        let credentials = store.credentials_changed("work");
        let logs = Scope::bucket("logs");
        assert!(store.needs_list_probe(&credentials, &logs));

        // Nothing here is a probe: this is what a real listing of the bucket
        // reports when it comes back with data.
        let recorded = store.observe_list(&credentials, logs.clone(), Observation::Allowed);

        assert!(recorded);
        assert_eq!(
            store.capability(&credentials, &logs).list,
            Observation::Allowed
        );
        assert!(
            !store.needs_list_probe(&credentials, &logs),
            "the evidence is already in hand; there is nothing left to probe for"
        );
    }

    #[test]
    fn a_denial_is_recorded_as_denied() {
        let mut store = CapabilityStore::new();
        let credentials = store.credentials_changed("work");
        let logs = Scope::bucket("logs");

        assert!(store.observe_list(&credentials, logs.clone(), Observation::Denied));

        assert_eq!(
            store.capability(&credentials, &logs).list,
            Observation::Denied
        );
        assert!(!store.needs_list_probe(&credentials, &logs));
    }

    #[test]
    fn a_failure_that_is_not_a_denial_leaves_the_model_untouched() {
        // Expired session, rejected credentials, wrong region, unreachable
        // network, throttling: each keeps its own cause elsewhere and says
        // nothing about permission, so none of them may write here.
        let mut store = CapabilityStore::new();
        let credentials = store.credentials_changed("work");
        let logs = Scope::bucket("logs");
        store.observe_list(&credentials, logs.clone(), Observation::Allowed);

        let recorded = store.observe_list(&credentials, logs.clone(), Observation::Unknown);

        assert!(!recorded);
        assert_eq!(
            store.capability(&credentials, &logs).list,
            Observation::Allowed,
            "a failure with no evidence in it must not erase evidence we have"
        );

        let backups = Scope::bucket("backups");
        assert!(!store.observe_list(&credentials, backups.clone(), Observation::Unknown));
        assert!(
            store.needs_list_probe(&credentials, &backups),
            "still unobserved, so still worth probing again"
        );
    }

    #[test]
    fn observing_list_never_moves_write_read_or_delete_out_of_unknown() {
        let mut store = CapabilityStore::new();
        let credentials = store.credentials_changed("work");
        let logs = Scope::bucket("logs");

        store.observe_list(&credentials, logs.clone(), Observation::Allowed);

        let capability = store.capability(&credentials, &logs);
        assert_eq!(
            capability.write,
            Observation::Unknown,
            "write is never probed and never inferred from another capability"
        );
        assert_eq!(capability.read, Observation::Unknown);
        assert_eq!(capability.delete, Observation::Unknown);
    }
}
