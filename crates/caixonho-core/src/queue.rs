//! Holding more transfers than run at once (`XONHO-0028`).
//!
//! A pure structure: it decides *which* work should be running and nothing
//! else. It starts nothing, awaits nothing, and knows nothing about S3 — the
//! caller starts what [`Queue::ready`] hands back and reports what became of
//! it. That split is what makes the scheduling testable without a runtime,
//! and the scheduling is the part with the interesting failures in it.
//!
//! The bound is not decoration. `PROJECT_BRIEF.md` §4.4 names the failure it
//! prevents — *"many small files into one prefix"* is the classic trigger for
//! `503 SlowDown` — and a queue that started everything at once would meet it
//! on its first real use.

/// Which transfer, minted per accepted item.
///
/// The same discipline `ConnectionId` carries and for the same reason: an
/// event that names no item is silently applied to the wrong one, which shows
/// as one file's progress moving for another file's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransferId(pub u64);

/// Where one item stands.
///
/// `Asking` and `Waiting` are deliberately different states rather than one
/// "not running". An item waiting for a slot is waiting on this queue; an item
/// waiting for a person to answer a collision is waiting on a human, which may
/// be minutes — and holding a concurrency slot through that would let two
/// unanswered questions stall a queue of twenty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Accepted, no slot yet.
    Waiting,
    /// Started, occupying a slot.
    Running,
    /// Stopped on a question only the user can answer. Holds no slot.
    Asking,
    /// Ended well.
    Finished,
    /// Ended badly, and can be tried again.
    Failed,
    /// Stopped because the user said so. Distinct from `Failed`: nothing went
    /// wrong, and offering to retry what someone deliberately stopped reads as
    /// the application arguing.
    Cancelled,
}

impl Standing {
    /// Whether an item in this state is using one of the bound's slots.
    fn holds_a_slot(self) -> bool {
        matches!(self, Self::Running)
    }

    /// Whether nothing more will happen to this item on its own.
    fn is_settled(self) -> bool {
        matches!(self, Self::Finished | Self::Failed | Self::Cancelled)
    }
}

/// One item and what the caller wanted to remember about it.
#[derive(Debug, Clone)]
pub struct Item<T> {
    pub id: TransferId,
    pub standing: Standing,
    pub payload: T,
}

/// Work waiting, work running, and what became of the rest.
///
/// Generic over the payload so core holds no window types: what a transfer
/// *is* stays where it is rendered, and what a queue *does* stays here.
#[derive(Debug)]
pub struct Queue<T> {
    bound: usize,
    items: Vec<Item<T>>,
    next: u64,
}

impl<T> Queue<T> {
    /// A queue running at most `bound` at once.
    ///
    /// A bound of zero would accept work and never start it, which is a queue
    /// that has quietly stopped rather than one that is busy, so it is raised
    /// to one. Silently doing nothing is the failure that takes longest to
    /// notice.
    pub fn new(bound: usize) -> Self {
        Self {
            bound: bound.max(1),
            items: Vec::new(),
            next: 0,
        }
    }

    /// Take on `payload`, to be started when a slot frees.
    pub fn accept(&mut self, payload: T) -> TransferId {
        self.next += 1;
        let id = TransferId(self.next);
        self.items.push(Item {
            id,
            standing: Standing::Waiting,
            payload,
        });
        id
    }

    /// What should start now, marked as running.
    ///
    /// Returns rather than starts: the caller owns the runtime, and a
    /// scheduler that spawned would need one to be tested.
    pub fn ready(&mut self) -> Vec<TransferId> {
        let free = self.bound.saturating_sub(self.running());
        let mut starting = Vec::new();
        for item in self.items.iter_mut() {
            if starting.len() == free {
                break;
            }
            if item.standing == Standing::Waiting {
                item.standing = Standing::Running;
                starting.push(item.id);
            }
        }
        starting
    }

    /// How many are using a slot.
    pub fn running(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.standing.holds_a_slot())
            .count()
    }

    /// Record what became of one item.
    ///
    /// An id this queue does not hold is **ignored**, which is the whole point
    /// of minting ids: an event about an item that has been cleared is a late
    /// answer about something the user has moved on from, and applying it to
    /// whatever sits at that position now is the defect this prevents.
    pub fn settled(&mut self, id: TransferId, standing: Standing) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.standing = standing;
        }
    }

    /// Stop one item, if it has not already ended.
    pub fn cancel(&mut self, id: TransferId) {
        if let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.id == id && !item.standing.is_settled())
        {
            item.standing = Standing::Cancelled;
        }
    }

    /// Stop everything that has not ended.
    ///
    /// Waiting items are cancelled too — never started is still never going to
    /// happen, and leaving them Waiting would show a queue that claims work is
    /// coming after the user stopped it.
    pub fn cancel_all(&mut self) {
        for item in self.items.iter_mut() {
            if !item.standing.is_settled() {
                item.standing = Standing::Cancelled;
            }
        }
    }

    /// Put every failed item back in the queue.
    ///
    /// Only failed ones. A cancelled item is one the user stopped on purpose,
    /// and sweeping it back in with a "retry failed" would be the application
    /// overruling them.
    pub fn retry_failed(&mut self) {
        for item in self.items.iter_mut() {
            if item.standing == Standing::Failed {
                item.standing = Standing::Waiting;
            }
        }
    }

    /// Forget what finished, keeping everything else.
    ///
    /// `Finished` only: a failed item still has a reason worth reading and a
    /// retry worth offering, and a cancelled one is the user's own record of
    /// what they stopped.
    pub fn clear_finished(&mut self) {
        self.items
            .retain(|item| item.standing != Standing::Finished);
    }

    /// Forget one item entirely, whatever it was doing.
    ///
    /// What a per-item dismiss needs, and `clear_finished` cannot do it: a
    /// *failed* item is not finished, so clearing leaves it — correctly, it
    /// still has a reason worth reading — but the user must be able to say
    /// "I have read it" without retrying it.
    pub fn forget(&mut self, id: TransferId) {
        self.items.retain(|item| item.id != id);
    }

    /// This item is waiting on a person now, and gives up its slot.
    pub fn asking(&mut self, id: TransferId) {
        self.settled(id, Standing::Asking);
    }

    /// The person answered; it wants a slot again.
    pub fn answered(&mut self, id: TransferId) {
        if let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.id == id && item.standing == Standing::Asking)
        {
            item.standing = Standing::Waiting;
        }
    }

    /// How many have finished, of how many there are.
    pub fn progress(&self) -> (usize, usize) {
        (
            self.items
                .iter()
                .filter(|item| item.standing == Standing::Finished)
                .count(),
            self.items.len(),
        )
    }

    /// Whether there is anything to show at all.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Every item, in the order they were accepted.
    pub fn items(&self) -> &[Item<T>] {
        &self.items
    }

    /// One item's payload, to be updated as progress arrives.
    pub fn payload_mut(&mut self, id: TransferId) -> Option<&mut T> {
        self.items
            .iter_mut()
            .find(|item| item.id == id)
            .map(|item| &mut item.payload)
    }

    /// What one item is doing, if this queue still holds it.
    pub fn standing(&self, id: TransferId) -> Option<Standing> {
        self.items
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.standing)
    }
}

#[cfg(test)]
mod tests {
    //! The interesting tests here are about *containment*: a failure that
    //! stays in its own item, an id that reaches its own item, and a slot that
    //! a human question does not hold. Each of those failing shows on screen
    //! as a different file's progress.

    use super::*;

    /// Accept `n` items and return their ids.
    fn accept(queue: &mut Queue<&'static str>, names: &[&'static str]) -> Vec<TransferId> {
        names.iter().map(|name| queue.accept(name)).collect()
    }

    #[test]
    fn only_the_bound_runs_at_once() {
        let mut queue = Queue::new(2);
        accept(&mut queue, &["a", "b", "c", "d", "e"]);

        assert_eq!(queue.ready().len(), 2, "five accepted, two slots");
        assert_eq!(queue.running(), 2);
        assert!(
            queue.ready().is_empty(),
            "asking again with no slot free must start nothing"
        );
    }

    #[test]
    fn a_waiting_item_starts_as_a_slot_frees() {
        let mut queue = Queue::new(2);
        let ids = accept(&mut queue, &["a", "b", "c"]);
        let started = queue.ready();

        queue.settled(started[0], Standing::Finished);

        let next = queue.ready();
        assert_eq!(next.len(), 1, "one ended, so one may start");
        assert_eq!(next[0], ids[2], "and it is the one that waited longest");
    }

    #[test]
    fn every_item_eventually_runs() {
        let mut queue = Queue::new(2);
        accept(&mut queue, &["a", "b", "c", "d", "e"]);

        let mut finished = 0;
        while finished < 5 {
            for id in queue.ready() {
                queue.settled(id, Standing::Finished);
                finished += 1;
            }
        }

        assert_eq!(queue.progress(), (5, 5));
    }

    /// The twentieth file must not be lost because the fourth was.
    #[test]
    fn one_failure_does_not_stop_the_others() {
        let mut queue = Queue::new(2);
        let ids = accept(&mut queue, &["a", "b", "c", "d", "e"]);
        let started = queue.ready();

        queue.settled(started[1], Standing::Failed);

        let mut finished = 0;
        loop {
            let ready = queue.ready();
            if ready.is_empty() {
                break;
            }
            for id in ready {
                queue.settled(id, Standing::Finished);
                finished += 1;
            }
        }
        queue.settled(started[0], Standing::Finished);
        finished += 1;

        assert_eq!(finished, 4, "the other four still ran");
        assert_eq!(queue.standing(ids[1]), Some(Standing::Failed));
    }

    /// Waiting on a person is not waiting on this queue.
    #[test]
    fn an_item_asking_a_question_holds_no_slot() {
        let mut queue = Queue::new(1);
        let ids = accept(&mut queue, &["a", "b"]);
        let started = queue.ready();
        assert_eq!(started, vec![ids[0]]);

        queue.asking(ids[0]);

        assert_eq!(queue.running(), 0, "a question is not work");
        assert_eq!(
            queue.ready(),
            vec![ids[1]],
            "the queue must run past an unanswered question, or two of them \
             stall twenty files"
        );
    }

    #[test]
    fn an_answered_item_wants_its_slot_back() {
        let mut queue = Queue::new(1);
        let ids = accept(&mut queue, &["a"]);
        queue.ready();
        queue.asking(ids[0]);

        queue.answered(ids[0]);

        assert_eq!(queue.standing(ids[0]), Some(Standing::Waiting));
        assert_eq!(queue.ready(), vec![ids[0]]);
    }

    /// The reason ids exist. An event about an item that has been cleared must
    /// not land on whatever sits at that position now.
    #[test]
    fn an_event_for_an_item_this_queue_no_longer_holds_is_dropped() {
        let mut queue = Queue::new(2);
        let ids = accept(&mut queue, &["a", "b"]);
        queue.ready();
        queue.settled(ids[0], Standing::Finished);
        queue.clear_finished();

        // The late answer arrives for an item nobody is watching any more.
        queue.settled(ids[0], Standing::Failed);

        assert_eq!(queue.standing(ids[0]), None, "it is gone and stays gone");
        assert_eq!(
            queue.standing(ids[1]),
            Some(Standing::Running),
            "and the survivor was not touched by it"
        );
    }

    #[test]
    fn cancelling_the_queue_stops_the_running_and_the_waiting() {
        let mut queue = Queue::new(2);
        let ids = accept(&mut queue, &["a", "b", "c"]);
        queue.ready();

        queue.cancel_all();

        for id in &ids {
            assert_eq!(
                queue.standing(*id),
                Some(Standing::Cancelled),
                "a waiting item that will never start is cancelled, not still waiting"
            );
        }
        assert!(queue.ready().is_empty());
    }

    #[test]
    fn cancelling_does_not_reopen_what_already_ended() {
        let mut queue = Queue::new(2);
        let ids = accept(&mut queue, &["a", "b"]);
        queue.ready();
        queue.settled(ids[0], Standing::Finished);

        queue.cancel_all();

        assert_eq!(
            queue.standing(ids[0]),
            Some(Standing::Finished),
            "a finished transfer really did finish"
        );
    }

    #[test]
    fn clearing_keeps_everything_that_is_not_finished() {
        let mut queue = Queue::new(3);
        let ids = accept(&mut queue, &["a", "b", "c", "d"]);
        queue.ready();
        queue.settled(ids[0], Standing::Finished);
        queue.settled(ids[1], Standing::Failed);
        queue.settled(ids[2], Standing::Cancelled);

        queue.clear_finished();

        assert_eq!(queue.standing(ids[0]), None);
        assert_eq!(queue.standing(ids[1]), Some(Standing::Failed));
        assert_eq!(queue.standing(ids[2]), Some(Standing::Cancelled));
        assert_eq!(queue.standing(ids[3]), Some(Standing::Waiting));
    }

    #[test]
    fn retrying_takes_the_failed_and_leaves_the_cancelled() {
        let mut queue = Queue::new(2);
        let ids = accept(&mut queue, &["a", "b"]);
        queue.ready();
        queue.settled(ids[0], Standing::Failed);
        queue.settled(ids[1], Standing::Cancelled);

        queue.retry_failed();

        assert_eq!(queue.standing(ids[0]), Some(Standing::Waiting));
        assert_eq!(
            queue.standing(ids[1]),
            Some(Standing::Cancelled),
            "the user stopped this one on purpose; sweeping it back in overrules them"
        );
    }

    #[test]
    fn a_bound_of_zero_is_a_queue_that_has_stopped_so_it_is_raised() {
        let mut queue = Queue::new(0);
        accept(&mut queue, &["a"]);

        assert_eq!(queue.ready().len(), 1);
    }

    #[test]
    fn forgetting_one_item_leaves_the_rest() {
        let mut queue = Queue::new(2);
        let ids = accept(&mut queue, &["a", "b"]);
        queue.ready();
        queue.settled(ids[0], Standing::Failed);

        queue.forget(ids[0]);

        assert_eq!(queue.standing(ids[0]), None);
        assert_eq!(queue.standing(ids[1]), Some(Standing::Running));
    }

    #[test]
    fn an_empty_queue_says_so() {
        let queue: Queue<&str> = Queue::new(2);

        assert!(queue.is_empty());
        assert_eq!(queue.progress(), (0, 0));
    }
}
