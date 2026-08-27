# Tasks — XONHO-0028 a queue that holds more than one

> The runner is core, so TDD (`AGENTS.md` §7). The tests that carry this one
> are about **containment**: a failure that stays in its own item, an event
> that reaches its own item, and a slot that a human question does not hold.
> Each of those failing looks, on screen, like a different file's progress.
>
> **Routing, decided 2026-08-27 rather than defaulted.** Every remaining task
> is `[dispatch: main]`, and the reason is not that `agy` is unavailable — it
> is installed and is this workspace's second-priority executor for
> external-ok work. It earns nothing *here*: tasks 2.1–2.5 all edit
> `crates/caixonho-gui/src/app.rs` and run strictly in sequence, so handing
> one to an external executor while this session holds the file for the other
> four makes ownership ambiguous, which the dispatch contract exists to
> prevent. 2.2 additionally carries the design decision `design.md` left open,
> and 3.4/3.5 are judgment by definition. 3.1 and 3.2 are mechanical but are
> verification this session must run anyway.
>
> What *would* suit `agy` in this repo, when it comes up: the archive-and-sync
> chores that `XONHO-0020` (6.5) and `XONHO-0019` (4.4) are waiting on, which
> are self-contained, mechanical, and touch no file this session holds.

## 1. The runner

- [x] 1.1 Work waits, and starts as slots free [dispatch: main]
      - Done in `main` (2026-08-27), in a new `caixonho-core::queue`.
      - **It turned out to want no runtime at all**, and that shaped it: the
        queue decides *which* work should run and starts nothing. `ready()`
        returns the ids that may begin; the caller owns the spawning and
        reports back. So thirteen tests run with no async, no tokio, no
        doubles and no timing — a scheduler tested against a clock is a
        scheduler that flakes.
      - A bound of zero is raised to one. A queue that accepts work and never
        starts it is not busy, it has silently stopped, and silence is the
        failure that takes longest to notice.
  - Paths: `crates/caixonho-core/src/transfer.rs` or a new `queue` module
  - Done criteria: something that accepts more work than it runs, runs up to
    a bound, and starts a waiting item as a running one ends. Red first.
    Tests: with a bound of two and five items, exactly two run at once; as
    each ends the next starts; all five end. Use the probe double's held-work
    shape (`probe::double::HeldProbes`) if it fits — a runner test that needs
    real timing is a runner test that will flake.
  - Verification: `cargo test -p caixonho-core`

- [x] 1.2 Every event names its item [dispatch: main]
      - Done in `main` (2026-08-27). `TransferId` minted per accepted item,
        and `settled` **ignores** an id the queue no longer holds.
      - **Ablated as the task demanded**, and the ablation was written to be
        the plausible mistake rather than an obvious one: looking the item up
        by *position* instead of by id. That is what a hurried version does,
        it works perfectly until something is cleared, and then it applies a
        late answer to whatever moved into that slot. One test red.
  - Paths: as 1.1, `crates/caixonho-core/src/session.rs`
  - Done criteria: progress and settlement carry an item id, minted per
    accepted transfer. Tests: two items running, each event attributable;
    an event for an item that has been cancelled or cleared is **dropped**,
    not applied.
  - **Ablate it**: deliver an event untagged, or apply the wrong item's, and
    confirm a test goes red. An untagged event is *silently* wrong — one
    file's bar moving for another's bytes — and silent wrongness is the
    failure mode this whole task exists for.
  - Verification: `cargo test -p caixonho-core`

- [x] 1.3 A failure stays in its own item [dispatch: main]
      - Done in `main` (2026-08-27): with a bound of two and five items where
        the second fails, the other four still run and finish, and the failed
        one keeps its standing.
  - Paths: as 1.1
  - Done criteria: one item failing leaves the others running and the waiting
    ones starting. Red first: with a bound of two and five items where the
    second fails, the other four still finish.
  - Verification: `cargo test -p caixonho-core`

- [x] 1.4 Waiting for a human is not waiting for a slot [dispatch: main]
      - Done in `main` (2026-08-27), and the design's prediction held: it
        **fell out of the modelling** rather than needing special handling.
        `Waiting` and `Asking` are separate states, `holds_a_slot` is true for
        neither `Asking` nor anything settled, and the test was three lines.
      - Ablated: making `Asking` hold a slot turns it red — which is the
        version where two unanswered questions stall a queue of twenty.
      - A third ablation beyond the plan, because the same class of mistake
        lives next door: `retry_failed` sweeping cancelled items back in.
        Red. A cancelled transfer is one the user stopped on purpose, and
        retrying it would be the application overruling them.
  - Paths: as 1.1
  - Done criteria: an item waiting on a collision answer occupies no slot,
    and other items run past it. Test: bound of one, item A hits a collision,
    item B runs to completion while A is still unanswered. Answering A then
    lets it proceed.
  - This is the state the design says falls out of modelling
    waiting-for-a-slot and waiting-for-an-answer as *different* things. If the
    test is hard to write, that modelling is the thing to fix.
  - Verification: `cargo test -p caixonho-core`

## 2. The window

- [x] 2.1 The window holds a queue [dispatch: main]
      - Done in `main` (2026-08-27). `TransferPhase` is **unchanged**, as the
        task said to record either way: five phases describing one transfer's
        end were right for one and are right for each of many.
      - `TransferEvent` gained a wrapper rather than a field: `Tagged { id,
        event }`. Every progress report and every settlement now names the
        transfer it belongs to, and `apply_transfer` returns early on an id
        the queue no longer holds.
      - Two of the converted tests turned out to be **better** for it.
        `a_settlement_after_dismissal_is_dropped` used to clear the single
        slot and hope; it now queues an item, forgets it, and delivers an
        event for that exact id — which is the guarantee stated directly
        instead of approximated.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: `transfer: Option<Transfer>` becomes the queue.
    `TransferPhase` is **unchanged** — five phases describing one transfer's
    end were right for one and are right for each of many. Record here if
    that turned out false; it would mean the design was wrong about what a
    transfer is.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.2 A panel, and where it lives [dispatch: main]
      - Done in `main` (2026-08-27). **Decided: under the listing, in the
        strip's own slot, height-capped with the rows scrolling inside.**
      - The design set the constraint — a queue of twenty must not hide what
        the user is browsing — and a capped box is the only shape that costs
        the same screen for twenty as for two. The alternative considered was
        the summary-line-that-expands, and it was declined because this window
        has no expand-and-collapse anywhere else: inventing one for the queue
        would make the queue the odd control rather than the busy one.
      - Header carries `N of M transferred` and the three queue-wide actions;
        each row keeps the phase rendering that already existed.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: each item listed with what it is, where it is going, its
    progress and its end; the queue's own count of finished-of-total.
  - **The design left where it lives open on purpose.** Decide it here and
    write down what was decided and why. The constraint: a queue of twenty
    must not hide the listing the user is browsing. The strips this window
    uses are one row and do not stretch to twenty.
  - Verification: `cargo test -p caixonho-gui`, and looking at the frames

- [x] 2.3 Cancel one, cancel all, retry failed, clear finished
      [dispatch: main]
      - Done in `main` (2026-08-27). `Queue::forget` was added for the
        per-item dismiss: `clear_finished` cannot do it, because a **failed**
        item is not finished — correctly, it still has a reason worth reading
        — but the user must be able to say "I have read it" without retrying
        it.
      - `cancel_queue` cancels each running transfer's own `Cancel` **and**
        marks the queue: marking without cancelling would leave bytes moving
        for a transfer the screen calls stopped.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: all four. Tests: cancelling the queue stops the running
    and prevents the waiting, and both report **cancelled** rather than
    failed; clearing removes only what finished, leaving running, waiting and
    failed; retrying a failed item does not disturb one in flight.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.4 An empty queue says nothing [dispatch: main]
      - Done in `main` (2026-08-27): `queue_panel` returns `None` on an empty
        queue rather than an empty frame.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: with nothing in it the panel is absent, not an empty
    frame. Small, and the kind of thing that ships as a permanent grey box.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.5 The screenshot harness covers the queue [dispatch: main]
      - Done in `main` (2026-08-27): `bucket-17-queue-running` and
        `bucket-18-queue-asking-while-others-run`, pixel-distinct.
      - **And it earned its place immediately.** The first render showed a
        header reading *"0 of 4 transferred"* directly above a row saying
        *"Uploaded `daily/ledger.csv`"*. The count was wrong in the picture
        and right in the code — the harness had set a phase without the
        matching standing.
      - Which is the finding, not the fix: **phase and standing were two
        sources for one fact**, and nothing made them agree. Production set
        both together in `apply_transfer` and would have gone on doing so
        until someone added a third place. `TransferPhase::standing()` now
        decides once, and `apply_transfer`, `enqueue_settled` and the harness
        all ask it.
      - The two axes really are different, and the method says why: `Running`
        maps to `None`, because only the queue can tell `Waiting` from
        `Running` — neither has reached the service, so no phase describes
        them. Everything else is derivable and is now derived.
      - Second time this session an image has caught something no assertion
        did (`XONHO-0027`'s chooser was the first). The gap it cannot see is
        still width — see the harness note in `docs/planned-changes.md`.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: several running with one failed, everything finished, and
    a collision asked of one item while others run — each a frame, each
    pixel-distinct, each **driven through the controls**.
  - Then **look at them beside the single-transfer strips** they replace, and
    beside the listing. Two of `XONHO-0027`'s defects were only visible in an
    image and one was only visible at a narrower width — see the harness note
    in `docs/planned-changes.md`.
  - Verification: `cargo test -p caixonho-gui`, and looking

## 3. Close-out

- [x] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-27): fmt and clippy exit 0, 389 core + 93
        window green (8 + 1 ignored).
  - Verification: the commands themselves

- [x] 3.2 CI green on both targets, run id recorded here [dispatch: main]
      - Run `33052383544` on `ff9cd5a`: `build (windows-latest)`,
        `build (macos-latest)`, `dependency audit` and `rustfmt` all success.
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 3.3 Live: twenty files, one of them doomed [dispatch: main]
  - Done criteria: on the owner's machine, upload enough files at once to
    exceed the bound, with **one destination deliberately already taken** so a
    collision is asked mid-queue. Expected: the bound is respected, the others
    do not stall while the question waits, the answer applies to that item
    only, and the queue finishes. Then cancel a queue mid-flight, retry a
    failed item, clear the finished.
  - **Written to be falsifiable in the way this project has learned to**: name
    the *timing*, not the outcome. `XONHO-0025`'s live check found a real
    defect because it said "turn it on before the probes settle"; "check the
    filter works" would have passed.
  - Verification: what was seen, quoted, with the log's own lines

- [x] 3.4 Reader-facing documents [dispatch: main]
      - Done in `main` (2026-08-27). §4.4's queue row **stays partial**, and
        now names which parts are missing and why — pause and resume declined
        until multipart can make them honest, throughput and ETA left as
        guesses that deserve their own thinking.
      - The three rows that were blocked on a queue no longer say so.
        Multipart's parts are the first thing that needs one; adaptive
        concurrency is now precisely "choose the bound from the service's own
        throttling" rather than "needs the queue"; drag and drop keeps its own
        API question and loses the other half.
      - **A correction to something said in this session**: on first reading
        the count I took the queue row's pre-existing `partial` for a claim
        about work that did not exist. It was not. It read *"No queue, no
        aggregate, no pause/resume/retry — the panel is its own change"* —
        which is exactly how a status row should hold a partial. Worth
        recording because the opposite mistake, a note claiming more than was
        built, is one this session really did make (`XONHO-0025` 2.6), and
        suspicion is not the same as evidence.
      - Counts by the script: `§4.4` unmoved at 1 done, 3 partial, 3 not
        started — correct, because `partial` → `partial`.
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`
  - Done criteria: §4.4's queue row moves to **partial**, and says **which
    part** is missing — pause, resume, throughput and ETA — rather than
    reading as done. The rows for multipart, adaptive concurrency and drag and
    drop stop saying they are blocked on a queue that now exists. Counts by
    the script.
  - Verification: the script's totals match the tables

- [x] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
      - Run 2026-08-27, before the live check, as with the last seven.
      - **Q1, and it has the answer the task predicted.** This change declines
        two things the brief's own queue line names. Pause and resume are an
        argued amendment, not a defect: a `PutObject` is one stream, stopping
        it *is* cancelling it, and a Pause that restarted from zero would lie
        about the bytes already sent — honest pausing needs multipart, a
        separate `[M]`. Throughput and ETA are deferred more weakly, as
        arithmetic that deserves its own thinking; that is a judgement, and
        `requirements-status.md` now records both so a reader meets them
        without reading this file.
      - **Q2:** §4.4's queue row keeps `partial` and says which parts are
        missing; the three rows that were blocked on a queue stop saying so;
        a roadmap row. And a correction to a claim made in this very session
        is recorded in 3.4 — the row's old note was honest and I briefly said
        otherwise.
      - **Q3:** `Queue::forget` was added for a real caller, not for later.
        `Standing::holds_a_slot` and `is_settled` are both used. The
        `TransferEvent`/`Tagged` split leaves no orphan.
      - **Q4, in the form this project settled on: what did this change do to
        the evidence?** It made a whole class of defect *invisible to the
        instruments that existed*. A window with one transfer could not
        misattribute an event; now it can, and only the id prevents it — which
        is why that is the one thing with a dedicated ablation aimed at the
        plausible mistake (lookup by position) rather than an obvious one.
      - What no test covers: **concurrency against a real service**. Every
        queue test is a pure state machine with no runtime, deliberately, and
        the numbers that matter — whether four at once throttles this account,
        whether a real collision mid-queue behaves — live only in 3.3.
      - Also uncovered: quitting with transfers in flight. Named absent in the
        proposal rather than discovered later.
      - **Q5:** nothing discovered and left in a transcript. The harness's
        blindness to width was already parked; this change added a second
        instance of an image catching what an assertion could not, recorded
        in 2.5.
  - Done criteria: the five questions answered here. Question 1 has a known
    answer to write up honestly — **this change declines two things the
    brief's own line names**, and the review is where that is either an
    argued amendment or a defect. Question 4 in the form this project has
    settled on: what did this change do to the evidence?
  - Verification: the recorded findings
