//! What a request produced, and which connection it belongs to.
//!
//! Results reach the frontend as messages rather than shared state, and every
//! message carries the [`ConnectionId`] it was issued for. That is what makes
//! "switching profiles" a structural property instead of a discipline: a
//! response that arrives after the user switched belongs to a connection that
//! is no longer active, and is dropped rather than rendered as if it were the
//! new profile's data (`connections` spec, "Switching profiles without
//! restart").

use crate::error::Error;
use crate::types::{AccountListing, ConnectionId};

/// The state of one listing request.
#[derive(Debug)]
pub enum Outcome {
    /// Issued, nothing back yet.
    Loading,
    /// The service answered. An empty listing is a real answer: the account
    /// has no buckets. The listing also carries what was refused, when one of
    /// the two listings was and the other was not.
    Loaded(AccountListing),
    /// The call failed, with the cause already classified.
    Failed(Error),
}

/// An outcome together with the connection that produced it.
#[derive(Debug)]
pub struct TaggedOutcome {
    /// The connection this outcome belongs to.
    pub connection: ConnectionId,
    /// What happened.
    pub outcome: Outcome,
}

impl TaggedOutcome {
    /// Tag an outcome with its connection.
    pub fn new(connection: ConnectionId, outcome: Outcome) -> Self {
        Self {
            connection,
            outcome,
        }
    }
}

/// Holds the latest outcome for whichever connection is active.
///
/// The frontend renders [`Self::state`] and never has to reason about
/// ordering: everything stale is refused here.
#[derive(Debug)]
pub struct ActiveOutcome {
    active: ConnectionId,
    state: Outcome,
}

impl ActiveOutcome {
    /// Start on `connection`, with a request assumed in flight.
    pub fn new(connection: ConnectionId) -> Self {
        Self {
            active: connection,
            state: Outcome::Loading,
        }
    }

    /// Which connection is being displayed.
    pub fn active(&self) -> ConnectionId {
        self.active
    }

    /// What to render.
    pub fn state(&self) -> &Outcome {
        &self.state
    }

    /// Switch to another connection.
    ///
    /// The previous connection's results — including its error — go with it,
    /// so nothing from the old profile can survive on screen under the new
    /// one's name.
    pub fn switch_to(&mut self, connection: ConnectionId) {
        self.active = connection;
        self.state = Outcome::Loading;
    }

    /// Take an outcome if it belongs to the active connection.
    ///
    /// Returns `false` when the outcome was dropped as stale — the caller
    /// needs no other reaction, but tests and logs benefit from knowing.
    pub fn accept(&mut self, tagged: TaggedOutcome) -> bool {
        if tagged.connection != self.active {
            return false;
        }
        self.state = tagged.outcome;
        true
    }
}

#[cfg(test)]
mod tests {
    //! `connections` spec, "Switching profiles without restart" — both
    //! scenarios, plus the race the design exists to make impossible.

    use super::*;
    use crate::types::{Bucket, BucketKind, Region};

    fn bucket(name: &str) -> Bucket {
        Bucket {
            name: name.to_owned(),
            created: None,
            region: Region::Unknown,
            kind: BucketKind::General,
        }
    }

    #[test]
    fn an_outcome_for_the_active_connection_is_displayed() {
        let mut active = ActiveOutcome::new(ConnectionId(1));

        let accepted = active.accept(TaggedOutcome::new(
            ConnectionId(1),
            Outcome::Loaded(AccountListing::complete(vec![bucket("logs")])),
        ));

        assert!(accepted);
        match active.state() {
            Outcome::Loaded(listing) => assert_eq!(listing.buckets.len(), 1),
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn a_late_answer_from_the_previous_profile_is_dropped() {
        let mut active = ActiveOutcome::new(ConnectionId(1));
        active.accept(TaggedOutcome::new(
            ConnectionId(1),
            Outcome::Loaded(AccountListing::complete(vec![bucket("from-first-profile")])),
        ));

        active.switch_to(ConnectionId(2));
        let accepted = active.accept(TaggedOutcome::new(
            ConnectionId(1),
            Outcome::Loaded(AccountListing::complete(vec![bucket(
                "late-from-first-profile",
            )])),
        ));

        assert!(!accepted, "a stale outcome must not be taken");
        assert!(
            matches!(active.state(), Outcome::Loading),
            "the new profile must still be loading, not showing the old data"
        );
    }

    #[test]
    fn switching_clears_the_previous_profiles_results() {
        let mut active = ActiveOutcome::new(ConnectionId(1));
        active.accept(TaggedOutcome::new(
            ConnectionId(1),
            Outcome::Loaded(AccountListing::complete(vec![bucket("logs")])),
        ));

        active.switch_to(ConnectionId(2));

        assert!(matches!(active.state(), Outcome::Loading));
        assert_eq!(active.active(), ConnectionId(2));
    }

    #[test]
    fn switching_away_from_a_failing_profile_clears_the_error() {
        let mut active = ActiveOutcome::new(ConnectionId(1));
        active.accept(TaggedOutcome::new(
            ConnectionId(1),
            Outcome::Failed(Error::NoCredentials {
                profile: "broken".to_owned(),
            }),
        ));

        active.switch_to(ConnectionId(2));

        assert!(
            matches!(active.state(), Outcome::Loading),
            "the previous profile's error must not survive the switch"
        );
    }
}
