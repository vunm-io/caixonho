//! Scheduling capability probes.
//!
//! The model in [`crate::capability`] says what is known; this module decides
//! what to ask about next. It exists because the answer costs a request: an
//! account can hold far more buckets than a window shows, and probing all of
//! them the moment a list arrives would spend an account's request budget on
//! rows nobody is looking at.
//!
//! So the frontend reports what the user is looking at, debounced, and the
//! scheduler decides: it skips what has already been observed, skips what it
//! is already asking about, and keeps no more than [`IN_FLIGHT_BUDGET`]
//! requests open at once. The rest of the viewport waits its turn, and is
//! replaced wholesale by the next report — a row that scrolled out of sight is
//! no longer a row the user is looking at.
//!
//! Two things live here rather than in the model. The in-flight set, because
//! "being probed" is a fact about our own activity and not a claim about the
//! world: [`crate::capability::Observation`] keeps its three values and the
//! view combines them with what is in flight. And the queue, because it
//! belongs to one connection — it is discarded with the credentials it was
//! gathered for, rather than outliving them.
//!
//! Nothing here blocks. Submitting a viewport spawns what the budget allows
//! and returns; results reach the model from a runtime thread, long after the
//! list is on screen.

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::runtime::Handle;

use crate::capability::{CapabilityStore, CredentialsId, Observation, Scope, observation_for};
use crate::diagnostics;
use crate::store::ObjectStore;
use crate::types::{Bucket, Region};

/// How many probes may be open at once.
///
/// Deliberately small, and deliberately not configurable. A scroll through a
/// large account can name hundreds of rows in a second, and the budget is what
/// keeps that from becoming hundreds of requests — and from being answered
/// with throttling, which is no evidence about permission at all, so a
/// throttled probe costs a request and learns nothing. A screenful still
/// settles within a few round trips, because a probe that returns hands its
/// place to the next row immediately.
pub const IN_FLIGHT_BUDGET: usize = 4;

/// One row of a viewport: what to ask about, and where to ask.
///
/// The region travels with the scope because object operations are
/// region-scoped — the same request sent to the wrong region is answered with
/// a redirect, which is no evidence either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeTarget {
    scope: Scope,
    region: Region,
}

impl ProbeTarget {
    /// A target naming its scope and the region it lives in.
    pub fn new(scope: Scope, region: Region) -> Self {
        Self { scope, region }
    }

    /// What this target asks about.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// The region the request has to be routed to.
    pub fn region(&self) -> &Region {
        &self.region
    }
}

impl From<&Bucket> for ProbeTarget {
    /// A listed bucket is the target of its own probe, which is what lets a
    /// frontend hand the rows it is already rendering straight to the
    /// scheduler.
    fn from(bucket: &Bucket) -> Self {
        Self::new(Scope::bucket(bucket.name.as_str()), bucket.region.clone())
    }
}

/// What is being asked about now, and what is waiting for a turn.
#[derive(Debug, Default)]
struct Schedule {
    /// The scopes with a probe open. Never larger than [`IN_FLIGHT_BUDGET`];
    /// that invariant is the budget.
    in_flight: HashSet<Scope>,
    /// The rest of the latest viewport, in the order it was reported.
    pending: VecDeque<ProbeTarget>,
}

/// Told which scope has settled, once its result is recorded.
///
/// A frontend passes one that hands the scope to its own executor — the same
/// bridge a listing crosses. It is called on a runtime thread, so it must do
/// nothing but pass the message on.
pub type ProbeSink = Arc<dyn Fn(Scope) + Send + Sync>;

/// Decides which scopes to probe, and keeps the promise that few are probed
/// at once.
///
/// One per connection: it probes through that connection's store, records
/// under the credentials that connection was opened with, and is thrown away
/// with both.
#[derive(Clone)]
pub(crate) struct ProbeScheduler {
    runtime: Handle,
    store: Arc<dyn ObjectStore>,
    /// The session's own store, shared rather than copied — an observation
    /// made on a runtime thread is the one the frontend reads.
    capabilities: Arc<Mutex<CapabilityStore>>,
    /// The credentials this scheduler probes for, minted by the capability
    /// store when the connection was opened. Every probe is issued under them
    /// and hands them back with its result, so a result that outlived a
    /// profile switch is refused rather than attributed to whatever replaced
    /// them.
    credentials: CredentialsId,
    /// Told, after the fact, which scope has settled. The frontend has no
    /// other way to learn it: probing runs on runtime threads and writes into
    /// the capability store, which nothing watches.
    settled: ProbeSink,
    /// Shared by every clone, because each probe carries one to report back
    /// through.
    schedule: Arc<Mutex<Schedule>>,
}

impl ProbeScheduler {
    /// A scheduler probing `store` for `credentials`.
    ///
    /// `credentials` must be what the capability store minted for the
    /// connection `store` belongs to; [`crate::Session`] is the only caller,
    /// and it passes on what opening the connection returned.
    pub(crate) fn new(
        runtime: Handle,
        store: Arc<dyn ObjectStore>,
        capabilities: Arc<Mutex<CapabilityStore>>,
        credentials: CredentialsId,
        settled: ProbeSink,
    ) -> Self {
        Self {
            runtime,
            store,
            capabilities,
            credentials,
            schedule: Arc::default(),
            settled,
        }
    }

    /// Report the rows the user is looking at.
    ///
    /// Returns as soon as it has spawned what the budget allows: nothing here
    /// waits on a probe, and the caller goes straight back to rendering.
    ///
    /// The viewport *replaces* what was queued, because a row scrolled out of
    /// sight is no longer worth a request. Probes already open are left to
    /// finish — the answer is evidence whether or not the row is still shown,
    /// and cancelling would pay for the request without keeping what it
    /// bought.
    pub(crate) fn submit_viewport(&self, viewport: &[ProbeTarget]) {
        {
            let mut schedule = self.schedule();
            schedule.pending.clear();
            schedule.pending.extend(viewport.iter().cloned());
        }
        self.pump();
    }

    /// The scopes with a probe open right now.
    ///
    /// This is what lets a frontend show a row as being probed without
    /// [`Observation`] gaining a fourth value — one that would be a claim
    /// about our own activity sitting in a type that otherwise only holds
    /// claims about the world.
    pub(crate) fn in_flight(&self) -> HashSet<Scope> {
        self.schedule().in_flight.clone()
    }

    /// Whether a probe is open for `scope`, without copying the whole set to
    /// answer about one row.
    pub(crate) fn is_probing(&self, scope: &Scope) -> bool {
        self.schedule().in_flight.contains(scope)
    }

    /// Fill the budget from the queue, and start what that admits.
    ///
    /// Admission and spawning are separate so no lock is held across a spawn.
    fn pump(&self) {
        for target in self.admit() {
            self.issue(target);
        }
    }

    /// Take as many queued rows as the budget has room for, skipping the ones
    /// no longer worth a request.
    ///
    /// A row dropped here is dropped for good: it is either settled, or being
    /// settled, or belongs to credentials nobody is using. A row whose probe
    /// came back with no evidence — a throttled account, an unreachable
    /// network — is not requeued either, deliberately: retrying inside the
    /// scheduler would hammer exactly the account that asked us to slow down.
    /// It is still unobserved, so the next viewport report picks it up.
    fn admit(&self) -> Vec<ProbeTarget> {
        let mut schedule = self.schedule();
        if !self.credentials_are_current() {
            // The store behind this scheduler was built for credentials that
            // have been replaced. Whatever it answered would be refused by
            // the model anyway, so the request is pure cost.
            schedule.pending.clear();
            return Vec::new();
        }

        let mut admitted = Vec::new();
        while schedule.in_flight.len() < IN_FLIGHT_BUDGET {
            let Some(target) = schedule.pending.pop_front() else {
                break;
            };
            if schedule.in_flight.contains(&target.scope) {
                continue;
            }
            if !self
                .capabilities()
                .needs_list_probe(&self.credentials, &target.scope)
            {
                continue;
            }
            schedule.in_flight.insert(target.scope.clone());
            admitted.push(target);
        }
        admitted
    }

    /// Ask about one scope, on a runtime thread.
    fn issue(&self, target: ProbeTarget) {
        let scheduler = self.clone();
        self.runtime.spawn(async move {
            let probe = scheduler
                .store
                .probe_list(&target.scope, &target.region)
                .await;
            // `observation_for` decides what the answer is evidence of; this
            // module does not get a second opinion on what counts as a denial.
            scheduler.finish(target.scope, observation_for(&probe));
        });
    }

    /// Record what a probe showed and give its place back.
    fn finish(&self, scope: Scope, observation: Observation) {
        // Handed back under the same credentials the probe was issued with.
        // `false` here is a probe that outlived a profile switch — the right
        // outcome, and not an error: the observation belongs to credentials
        // nobody is using any more.
        self.capabilities()
            .observe_list(&self.credentials, scope.clone(), observation);
        // What the probe was evidence of, not what it answered: the answer is
        // the classifier's business and is already recorded wherever it
        // failed. This is the decision.
        diagnostics::probe_settled(&scope, observation);
        // After recording, so a row never reads "no evidence yet" in the gap
        // between its probe returning and its result landing.
        self.schedule().in_flight.remove(&scope);
        self.pump();
        // Last, so a frontend woken by this finds the store already updated and
        // the row no longer counted as being probed. Without it nothing would
        // tell the window a row had settled, and rows would sit at "probing"
        // until some unrelated event happened to redraw them.
        (self.settled)(scope);
    }

    /// Whether the credentials this scheduler probes for are still the ones
    /// in play.
    fn credentials_are_current(&self) -> bool {
        self.capabilities().credentials() == Some(&self.credentials)
    }

    fn schedule(&self) -> MutexGuard<'_, Schedule> {
        self.schedule.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The capability store, with a poisoned lock recovered rather than
    /// propagated, exactly as [`crate::Session`] does: the worst this data can
    /// be is stale, and the next observation corrects it.
    ///
    /// Taken while [`Self::schedule`] is held and never the other way round.
    fn capabilities(&self) -> MutexGuard<'_, CapabilityStore> {
        self.capabilities
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for ProbeScheduler {
    /// Terse on purpose: the store is a trait object with no `Debug`, and the
    /// schedule is behind a lock this must not take — formatting that can
    /// deadlock is worse than formatting that says little.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProbeScheduler")
            .field("credentials", &self.credentials)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
pub(crate) mod double {
    //! The probing test harness: a store whose probes can be held open, and
    //! the two waits the tests around it need.
    //!
    //! Holding probes open is the whole point. A double that answers
    //! instantly can never have two probes in flight at once, so every
    //! assertion about the budget would pass without testing anything.

    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
    use std::time::Duration;

    use tokio::sync::Semaphore;

    use crate::capability::Scope;
    use crate::error::Result;
    use crate::store::ObjectStore;
    use crate::types::Region;

    /// How long a wait may take before the test gives up and fails rather
    /// than hanging.
    const PATIENCE: Duration = Duration::from_secs(5);

    /// Long enough for a probe that should never have been issued to show up.
    const A_BEAT: Duration = Duration::from_millis(50);

    /// An [`ObjectStore`] whose probes stay open until the test lets them go.
    #[derive(Clone)]
    pub(crate) struct HeldProbes {
        /// Shut until `release`. Every probe waits on it, so several are
        /// genuinely in flight at once.
        gate: Arc<Semaphore>,
        log: Arc<Mutex<Log>>,
        answer: fn() -> Result<()>,
    }

    /// What the store was asked, and how much of it at once.
    #[derive(Debug, Default)]
    struct Log {
        asked: Vec<String>,
        running: usize,
        most_at_once: usize,
        finished: usize,
    }

    impl HeldProbes {
        /// Probes stay open until [`Self::release`].
        pub(crate) fn held() -> Self {
            Self {
                gate: Arc::new(Semaphore::new(0)),
                log: Arc::default(),
                answer: || Ok(()),
            }
        }

        /// Probes answer as soon as they run, and answer "you may list this".
        pub(crate) fn open() -> Self {
            let store = Self::held();
            store.release();
            store
        }

        /// Probes answer as soon as they run, with whatever `answer` says.
        pub(crate) fn answering(answer: fn() -> Result<()>) -> Self {
            let store = Self {
                answer,
                ..Self::held()
            };
            store.release();
            store
        }

        /// Let every held probe — and every later one — complete.
        pub(crate) fn release(&self) {
            self.gate.close();
        }

        /// The buckets the store was asked about, in the order it was asked.
        pub(crate) fn asked(&self) -> Vec<String> {
            self.log().asked.clone()
        }

        /// How many probes have come back.
        pub(crate) fn finished(&self) -> usize {
            self.log().finished
        }

        /// The most probes that were ever open at the same moment.
        pub(crate) fn most_at_once(&self) -> usize {
            self.log().most_at_once
        }

        fn begin(&self, scope: &Scope) {
            let mut log = self.log();
            log.asked.push(scope.bucket_name().to_owned());
            log.running += 1;
            log.most_at_once = log.most_at_once.max(log.running);
        }

        fn end(&self) {
            let mut log = self.log();
            log.running -= 1;
            log.finished += 1;
        }

        fn log(&self) -> MutexGuard<'_, Log> {
            self.log.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }

    #[async_trait::async_trait]
    impl ObjectStore for HeldProbes {
        async fn list_buckets(&self) -> Result<crate::types::AccountListing> {
            Ok(crate::types::AccountListing::default())
        }

        async fn probe_list(&self, scope: &Scope, _region: &Region) -> Result<()> {
            self.begin(scope);
            // Open until the gate is released — which is what makes "several
            // at once" reachable, and the budget worth asserting.
            let _ = self.gate.acquire().await;
            self.end();
            (self.answer)()
        }

        /// This double exists to hold probes open and count them; nothing here
        /// lists a location, so it answers with nothing rather than pretending.
        async fn list_objects(
            &self,
            _location: &crate::types::Location,
            _cursor: Option<&crate::types::Cursor>,
        ) -> Result<crate::types::Page> {
            Ok(crate::types::Page::default())
        }
    }

    /// Wait for `condition`, failing loudly rather than hanging if it never
    /// holds.
    pub(crate) async fn until(what: &str, condition: impl Fn() -> bool) {
        let waiting = async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        };
        tokio::time::timeout(PATIENCE, waiting)
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
    }

    /// Give work that should not happen a chance to happen, so asserting that
    /// it did not means something.
    pub(crate) async fn settle() {
        tokio::time::sleep(A_BEAT).await;
    }
}

#[cfg(test)]
mod tests {
    //! `capability-awareness` spec: "Probing is lazy, budgeted and
    //! non-blocking" — both scenarios — and "A pending probe is distinct from
    //! no evidence".

    use super::double::{HeldProbes, settle, until};
    use super::*;
    use crate::error::Error;
    use crate::types::BucketKind;

    /// A viewport with more rows than the budget can carry at once, so the
    /// cap has something to bite on.
    const CROWDED_VIEWPORT: usize = 12;

    /// A capability store with one profile open and nothing observed yet.
    fn capabilities() -> (Arc<Mutex<CapabilityStore>>, CredentialsId) {
        let mut store = CapabilityStore::new();
        let credentials = store.credentials_changed("work");
        (Arc::new(Mutex::new(store)), credentials)
    }

    fn scheduler(
        store: &HeldProbes,
        capabilities: &Arc<Mutex<CapabilityStore>>,
        credentials: &CredentialsId,
    ) -> ProbeScheduler {
        scheduler_announcing(store, capabilities, credentials, Arc::new(|_| {}))
    }

    fn scheduler_announcing(
        store: &HeldProbes,
        capabilities: &Arc<Mutex<CapabilityStore>>,
        credentials: &CredentialsId,
        settled: ProbeSink,
    ) -> ProbeScheduler {
        ProbeScheduler::new(
            Handle::current(),
            Arc::new(store.clone()),
            Arc::clone(capabilities),
            credentials.clone(),
            settled,
        )
    }

    /// The rows a frontend reports for the buckets it is showing.
    fn viewport(names: &[&str]) -> Vec<ProbeTarget> {
        names.iter().map(|name| row(name)).collect()
    }

    fn row(name: &str) -> ProbeTarget {
        ProbeTarget::new(
            Scope::bucket(name),
            Region::Known("ap-southeast-1".to_owned()),
        )
    }

    /// `count` distinct rows, named so a failure says which one.
    fn rows(count: usize) -> Vec<ProbeTarget> {
        (0..count).map(|n| row(&format!("bucket-{n:03}"))).collect()
    }

    /// What a sink was told, in the order it was told.
    type Announced = Arc<Mutex<Vec<(Scope, Observation)>>>;

    /// A sink that records what it was told, and checks the store was
    /// already updated when it was told.
    fn recording_sink(
        capabilities: &Arc<Mutex<CapabilityStore>>,
        credentials: &CredentialsId,
    ) -> (ProbeSink, Announced) {
        let announced: Announced = Arc::default();
        let recorded = Arc::clone(&announced);
        let capabilities = Arc::clone(capabilities);
        let credentials = credentials.clone();
        let sink: ProbeSink = Arc::new(move |scope: Scope| {
            // Read through the store from inside the announcement: a frontend
            // told a row settled must find the result already there.
            let observation = capabilities
                .lock()
                .expect("not poisoned")
                .capability(&credentials, &scope)
                .list;
            recorded
                .lock()
                .expect("not poisoned")
                .push((scope, observation));
        });
        (sink, announced)
    }

    #[tokio::test]
    async fn a_settled_probe_is_announced_with_its_result_already_recorded() {
        let (capabilities, credentials) = capabilities();
        let (sink, announced) = recording_sink(&capabilities, &credentials);
        let store = HeldProbes::open();
        let scheduler = scheduler_announcing(&store, &capabilities, &credentials, sink);

        scheduler.submit_viewport(&viewport(&["logs"]));

        until("the probe to be announced", || {
            !announced.lock().expect("not poisoned").is_empty()
        })
        .await;
        assert_eq!(
            announced.lock().expect("not poisoned").as_slice(),
            [(Scope::bucket("logs"), Observation::Allowed)],
            "a row is told it settled, and told after the answer landed"
        );
    }

    #[tokio::test]
    async fn a_probe_that_settles_nothing_is_announced_too() {
        let (capabilities, credentials) = capabilities();
        let (sink, announced) = recording_sink(&capabilities, &credentials);
        let store = HeldProbes::answering(|| {
            Err(Error::Network {
                detail: "connection reset".to_owned(),
            })
        });
        let scheduler = scheduler_announcing(&store, &capabilities, &credentials, sink);

        scheduler.submit_viewport(&viewport(&["logs"]));

        until("the probe to be announced", || {
            !announced.lock().expect("not poisoned").is_empty()
        })
        .await;
        assert_eq!(
            announced.lock().expect("not poisoned").as_slice(),
            [(Scope::bucket("logs"), Observation::Unknown)],
            "the row learned nothing, but it must stop saying it is being probed"
        );
    }

    /// What the model knows about listing `bucket`.
    fn observed(
        capabilities: &Arc<Mutex<CapabilityStore>>,
        credentials: &CredentialsId,
        bucket: &str,
    ) -> Observation {
        capabilities
            .lock()
            .expect("not poisoned")
            .capability(credentials, &Scope::bucket(bucket))
            .list
    }

    #[test]
    fn a_listed_bucket_is_the_target_of_its_own_probe() {
        let bucket = Bucket {
            name: "logs".to_owned(),
            created: None,
            region: Region::Known("eu-west-1".to_owned()),
            kind: BucketKind::General,
        };

        let target = ProbeTarget::from(&bucket);

        assert_eq!(target.scope(), &Scope::bucket("logs"));
        assert_eq!(
            target.region(),
            &Region::Known("eu-west-1".to_owned()),
            "a probe routed anywhere else is answered with a redirect, which is \
             no evidence about permission"
        );
    }

    #[tokio::test]
    async fn a_viewport_probes_every_scope_nobody_has_observed_yet() {
        let (capabilities, credentials) = capabilities();
        let store = HeldProbes::open();
        let scheduler = scheduler(&store, &capabilities, &credentials);

        scheduler.submit_viewport(&viewport(&["logs", "backups", "archive"]));

        until("the viewport to settle", || store.finished() == 3).await;
        let mut asked = store.asked();
        asked.sort();
        assert_eq!(asked, ["archive", "backups", "logs"]);
        for bucket in ["logs", "backups", "archive"] {
            assert_eq!(
                observed(&capabilities, &credentials, bucket),
                Observation::Allowed,
                "{bucket} answered its probe"
            );
        }
    }

    #[tokio::test]
    async fn a_scope_already_observed_produces_no_request() {
        let (capabilities, credentials) = capabilities();
        {
            let mut store = capabilities.lock().expect("not poisoned");
            store.observe_list(&credentials, Scope::bucket("logs"), Observation::Allowed);
            store.observe_list(&credentials, Scope::bucket("backups"), Observation::Denied);
        }
        let store = HeldProbes::open();
        let scheduler = scheduler(&store, &capabilities, &credentials);

        scheduler.submit_viewport(&viewport(&["logs", "backups", "archive"]));

        until("the one unobserved row to settle", || {
            store.finished() == 1 && scheduler.in_flight().is_empty()
        })
        .await;
        settle().await;
        assert_eq!(
            store.asked(),
            ["archive"],
            "evidence already in hand is not worth a request — and it does not \
             matter whether that evidence was an allowance or a denial"
        );
        assert_eq!(
            observed(&capabilities, &credentials, "logs"),
            Observation::Allowed
        );
        assert_eq!(
            observed(&capabilities, &credentials, "backups"),
            Observation::Denied
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn no_more_probes_are_open_at_once_than_the_budget_allows() {
        // The assertions below are only worth anything while the viewport
        // outnumbers the budget: that is what forces rows to wait, and waiting
        // is what the budget is. Checked at compile time, so raising the
        // budget past it cannot quietly un-test the cap.
        const {
            assert!(
                IN_FLIGHT_BUDGET < CROWDED_VIEWPORT,
                "raise CROWDED_VIEWPORT above the budget or this test asserts nothing"
            );
        }
        let (capabilities, credentials) = capabilities();
        // Held open, so the probes genuinely overlap. Against a store that
        // answers instantly they would run one after another and the cap would
        // never be reached, let alone tested.
        let store = HeldProbes::held();
        let scheduler = scheduler(&store, &capabilities, &credentials);

        scheduler.submit_viewport(&rows(CROWDED_VIEWPORT));

        until("the budget to fill", || {
            store.asked().len() >= IN_FLIGHT_BUDGET
        })
        .await;
        settle().await;
        assert_eq!(
            store.asked().len(),
            IN_FLIGHT_BUDGET,
            "{CROWDED_VIEWPORT} rows were reported and nothing can complete while \
             the gate is shut, so this is the entire burst the submission produced"
        );
        assert_eq!(store.most_at_once(), IN_FLIGHT_BUDGET);
        assert_eq!(scheduler.in_flight().len(), IN_FLIGHT_BUDGET);
        assert_eq!(store.finished(), 0, "the gate is still shut");

        store.release();

        until("every reported row to settle", || {
            store.finished() == CROWDED_VIEWPORT
        })
        .await;
        assert_eq!(
            store.most_at_once(),
            IN_FLIGHT_BUDGET,
            "the cap has to hold while the queue drains, not only on the first burst"
        );
        assert!(
            store.most_at_once() < CROWDED_VIEWPORT,
            "a cap that admits the whole viewport is no cap"
        );
        let mut asked = store.asked();
        asked.sort();
        asked.dedup();
        assert_eq!(
            asked.len(),
            CROWDED_VIEWPORT,
            "every reported row is probed exactly once — the budget delays rows, \
             it does not strand them"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_scope_already_being_probed_is_never_probed_again_alongside_itself() {
        let (capabilities, credentials) = capabilities();
        let store = HeldProbes::held();
        let scheduler = scheduler(&store, &capabilities, &credentials);

        // A duplicated row, then the same viewport reported again the way a
        // debounced scroll reports it while the first probes are still out.
        scheduler.submit_viewport(&viewport(&["logs", "logs", "backups"]));
        until("both probes to start", || store.asked().len() >= 2).await;
        scheduler.submit_viewport(&viewport(&["logs", "backups"]));
        settle().await;

        assert_eq!(
            store.asked().iter().filter(|name| *name == "logs").count(),
            1,
            "one scope, one question at a time — a second request would cost the \
             budget twice to learn the same thing"
        );
        assert_eq!(store.asked().len(), 2);
        assert_eq!(scheduler.in_flight().len(), 2);

        store.release();

        until("both to settle", || store.finished() == 2).await;
        settle().await;
        assert_eq!(
            store.asked().len(),
            2,
            "and once settled, reporting the row again is not worth a request either"
        );
    }

    #[tokio::test]
    async fn a_scope_with_a_probe_open_is_visible_as_being_probed() {
        // "A pending probe is distinct from no evidence": the frontend has to
        // be able to say "being probed" without `Observation` growing a fourth
        // value for a fact about ourselves.
        let (capabilities, credentials) = capabilities();
        let store = HeldProbes::held();
        let scheduler = scheduler(&store, &capabilities, &credentials);

        scheduler.submit_viewport(&viewport(&["logs"]));

        until("the probe to be open", || !scheduler.in_flight().is_empty()).await;
        assert_eq!(
            scheduler.in_flight(),
            HashSet::from([Scope::bucket("logs")])
        );
        assert!(scheduler.is_probing(&Scope::bucket("logs")));
        assert!(!scheduler.is_probing(&Scope::bucket("backups")));
        assert_eq!(
            observed(&capabilities, &credentials, "logs"),
            Observation::Unknown,
            "a probe in flight is not evidence; it is a question we have asked"
        );

        store.release();

        until("the probe to come back", || store.finished() == 1).await;
        until("the scope to leave the in-flight set", || {
            !scheduler.is_probing(&Scope::bucket("logs"))
        })
        .await;
        assert_eq!(
            observed(&capabilities, &credentials, "logs"),
            Observation::Allowed
        );
    }

    #[tokio::test]
    async fn a_large_account_is_probed_only_where_the_user_is_looking() {
        // The spec's scenario: the list is opened for an account holding far
        // more buckets than fit on screen.
        let account = rows(200);
        let visible = &account[40..40 + CROWDED_VIEWPORT];
        let (capabilities, credentials) = capabilities();
        let store = HeldProbes::open();
        let scheduler = scheduler(&store, &capabilities, &credentials);

        scheduler.submit_viewport(visible);

        until("the visible rows to settle", || {
            store.finished() == CROWDED_VIEWPORT && scheduler.in_flight().is_empty()
        })
        .await;
        settle().await;
        let mut asked = store.asked();
        asked.sort();
        let mut expected: Vec<String> = visible
            .iter()
            .map(|row| row.scope().bucket_name().to_owned())
            .collect();
        expected.sort();
        assert_eq!(
            asked,
            expected,
            "the rows on screen, and not one row more: an account of {} costs {} \
             requests to open",
            account.len(),
            CROWDED_VIEWPORT
        );
        for offscreen in account.iter().filter(|row| !visible.contains(row)) {
            assert_eq!(
                observed(&capabilities, &credentials, offscreen.scope().bucket_name()),
                Observation::Unknown,
                "{} was never shown, so it stays unprobed and unknown",
                offscreen.scope().bucket_name()
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn submitting_a_viewport_returns_without_waiting_on_a_probe() {
        // "Results arrive while the user reads the list": rendering may not
        // wait on a probe, so nothing the frontend calls may either.
        let (capabilities, credentials) = capabilities();
        // Never released: no probe this submission issues can complete for the
        // whole of this test.
        let store = HeldProbes::held();
        let scheduler = scheduler(&store, &capabilities, &credentials);
        let reported = rows(CROWDED_VIEWPORT);

        // On a thread of its own, reporting back through a channel with a
        // deadline: a submission that waited on its probes — none of which can
        // finish here — fails this test rather than hanging it. A plain
        // thread and not `spawn_blocking`, because the runtime waits for its
        // blocking tasks on the way out and would hang there instead.
        let (report, returned) = std::sync::mpsc::channel();
        let submitting = scheduler.clone();
        std::thread::spawn(move || {
            submitting.submit_viewport(&reported);
            let _ = report.send(());
        });
        returned
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("submitting a viewport must not wait on a probe");

        assert_eq!(
            store.finished(),
            0,
            "the call returned while every probe it issued was still open"
        );
        until("the probes it issued to be open", || {
            !scheduler.in_flight().is_empty()
        })
        .await;
        assert_eq!(
            observed(&capabilities, &credentials, "bucket-000"),
            Observation::Unknown,
            "and it returned before any of them could be evidence of anything"
        );
    }

    #[tokio::test]
    async fn a_denial_settles_the_scope_and_is_not_asked_about_twice() {
        let (capabilities, credentials) = capabilities();
        let store = HeldProbes::answering(|| {
            Err(Error::AccessDenied {
                iam_action: "s3:ListBucket",
            })
        });
        let scheduler = scheduler(&store, &capabilities, &credentials);

        scheduler.submit_viewport(&viewport(&["logs"]));
        until("the probe to settle", || store.finished() == 1).await;
        scheduler.submit_viewport(&viewport(&["logs"]));
        settle().await;

        assert_eq!(
            observed(&capabilities, &credentials, "logs"),
            Observation::Denied
        );
        assert_eq!(
            store.asked().len(),
            1,
            "a denial is evidence like any other; asking again would cost a \
             request to learn nothing new"
        );
    }

    #[tokio::test]
    async fn a_failure_that_is_not_a_denial_records_nothing_and_gives_its_place_back() {
        // Throttling is the case that matters: it is what a budget too large
        // provokes, it is no evidence about permission, and a scheduler that
        // held a place for it would wedge itself after a handful of them.
        let (capabilities, credentials) = capabilities();
        let store = HeldProbes::answering(|| {
            Err(Error::Unexpected {
                detail: "the service reported `SlowDown` (HTTP 503)".to_owned(),
            })
        });
        let scheduler = scheduler(&store, &capabilities, &credentials);

        scheduler.submit_viewport(&rows(CROWDED_VIEWPORT));

        until("every row to be answered", || {
            store.finished() == CROWDED_VIEWPORT
        })
        .await;
        until("the budget to come back", || {
            scheduler.in_flight().is_empty()
        })
        .await;
        for row in rows(CROWDED_VIEWPORT) {
            assert_eq!(
                observed(&capabilities, &credentials, row.scope().bucket_name()),
                Observation::Unknown,
                "{} was throttled, which says nothing about permission",
                row.scope().bucket_name()
            );
        }
    }

    #[tokio::test]
    async fn a_scheduler_whose_credentials_were_replaced_issues_nothing() {
        let (capabilities, credentials) = capabilities();
        let store = HeldProbes::open();
        let scheduler = scheduler(&store, &capabilities, &credentials);

        capabilities
            .lock()
            .expect("not poisoned")
            .credentials_changed("personal");
        scheduler.submit_viewport(&viewport(&["logs", "backups"]));
        settle().await;

        assert!(
            store.asked().is_empty(),
            "this scheduler probes through a connection opened for credentials \
             nobody is using any more; its answers would be refused, so the \
             requests are pure cost"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_probe_that_lands_after_a_profile_switch_records_nothing() {
        let (capabilities, credentials) = capabilities();
        let store = HeldProbes::held();
        let scheduler = scheduler(&store, &capabilities, &credentials);
        scheduler.submit_viewport(&viewport(&["logs"]));
        until("the probe to be open", || !scheduler.in_flight().is_empty()).await;

        // The user switches profile while the probe is still out.
        let switched = capabilities
            .lock()
            .expect("not poisoned")
            .credentials_changed("personal");
        store.release();

        until("the probe to come back", || store.finished() == 1).await;
        settle().await;
        assert_eq!(
            observed(&capabilities, &switched, "logs"),
            Observation::Unknown,
            "a probe that outlived its credentials must not land under the ones \
             that replaced them"
        );
        assert!(
            scheduler.in_flight().is_empty(),
            "and its place comes back either way"
        );
    }
}
