# Tasks — XONHO-0025 narrowing the bucket list

> The window is where this lives, so the tests are window tests
> (`XONHO-0015`'s seam). The one that carries the change is not "the filter
> filters" — it is that an **unknown** bucket is never hidden.

## 1. The predicates

- [x] 1.1 One predicate per narrowing, composed with `and` [dispatch: main]
      - Done in `main` (2026-08-26). The narrowing moved **into the delegate**
        (`BucketsDelegate::narrow`), which was not the plan and is forced by
        the domain: one of the four predicates is an *observation*, and only
        the delegate can read it. Doing it in the window would have meant
        handing the window the capability store.
      - `KindChoice` lives in the window, not core — unlike `RegionChoice`,
        which is in core because a region choice has to be **derived** from a
        listing. There are two kinds of bucket and there always will be, so
        core learns nothing by holding it. This kept the proposal's promise
        that core is untouched: **zero lines changed under
        `crates/caixonho-core/`**.
  - Paths: `crates/caixonho-gui/src/app.rs` (or the table delegate beside it)
  - Done criteria: kind, name and no-access each decide one bucket, and the
    shown set is those satisfying all of them plus the existing region
    choice. **One pass, one count** — four passes with four counts is how the
    number comes to disagree with the rows. Tests: each alone; two together;
    clearing one leaves the other in force.
  - Verification: `cargo test -p caixonho-gui`

- [x] 1.2 Accessible-only is `!= Denied`, never `== Open` [dispatch: main]
      - Done in `main` (2026-08-26), and **ablated exactly as the task
        demanded**: flipping the predicate to `== Open` turns
        `accessible_only_keeps_a_bucket_nothing_is_known_about` **and**
        `accessible_only_still_reports_the_unanswered_for_probing` red, and
        nothing else in 70 window tests.
      - The viewport assertion is the one that earns its place. A test on
        rendered rows alone would have passed the broken version for the wrong
        reason — the bug is not that the row is missing now, it is that the row
        can never come back, and only the submitted probe targets show that.
      - **Correction, 2026-08-26, and it is the worse of the two failures
        this change had.** The done-criteria below list six tests. Five were
        written. The sixth — a probe settling to denied while the narrowing is
        on — was never implemented, and the note above nonetheless read as
        though the set were complete. The owner hit the resulting defect on
        the first screen they tried the filter on.
      - A gap admitted gets looked at; a gap claimed as covered does not.
        Struck through where it was planned, and fixed in 2.6.
      - **`Probing` is covered by construction, not by a test.** Getting a
        probe genuinely in flight in a window test needs the core probe
        double, and the predicate treats `Probing` and `Unobserved`
        identically — both are "not `Denied`". Said out loud rather than
        implied: the state is handled, the *test* for it does not exist.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the predicate removes observed denials and keeps everything
    unanswered. Tests, and the first two are the ones this task exists for:
    - a bucket whose access is **`Unobserved`** is still shown;
    - a bucket whose probe is **in flight** is still shown;
    - **the unanswered ones are still reported as visible**, i.e. they still
      reach `targets()` and can still be probed — asserted on the submitted
      viewport, not on the rendered rows. This is the one that catches the
      self-starving version, and rendering alone would not catch it;
    - a bucket observed **denied** is hidden;
    - a bucket whose read failed with an expired session / unreachable network
      / wrong region is **still shown**, because none of those is a denial
      (`capability-awareness`);
    - ~~a probe settling to denied while the narrowing is on removes the row
      and moves the count~~ — **planned here and never written. It is the
      defect the owner found; see 2.6.**
  - **Ablate it**: change the predicate to `== Open` and confirm the
    `Unobserved` test *and* the viewport test both go red. If only the render
    test goes red, the suite is not guarding the failure that matters — the
    broken version fails silently and permanently, which is why it gets an
    ablation rather than a comment.
  - Verification: `cargo test -p caixonho-gui`

## 2. The controls

- [x] 2.1 Kind becomes a control, not a badge [dispatch: main]
      - Done in `main` (2026-08-26). **The badge stays**, and the decision the
        task asked for is this: it says something the control does not. "All
        directory buckets" is *derived* — every shown row happens to be one —
        which is a different sentence from "show me directory buckets". An
        account can be entirely directory buckets without anyone having asked.
      - So the badge is suppressed only while the kind control already says the
        same thing (`kind == Any`). Two controls saying one thing is where a
        screen starts lying about which is in force.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: all / directory / general purpose, beside the region
    selector. The existing "All directory buckets" badge is *derived* — it
    says every loaded row happens to be one — so decide and record here
    whether it survives beside a control that can mean the same thing. Two
    things saying almost the same thing is how a screen starts lying.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.2 Name filter and the honest count [dispatch: main]
      - Done in `main` (2026-08-26), and **one part of it was deleted again**,
        which is the finding worth keeping. A "showing N of M loaded" chip was
        written beside the controls, and the first screenshot showed it sitting
        directly above a status bar that has read "N of M buckets" since
        `XONHO-0005`. The requirement was **already met** by code this change
        did not write.
      - The chip went. Better than removing it: the status bar now takes both
        numbers from `shown_of_loaded()` instead of counting `shown` itself and
        the total from the listing — one sentence, one source. It had two, and
        this change would have given them three more chances to disagree.
      - The empty state distinguishes hidden-by-narrowing from an empty
        account, and keeps the controls above it so undoing is one click.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a text field; a count reading "showing N of M"; and an
    empty state that says the buckets were **hidden by the narrowing** —
    distinct from an account that holds none. That distinction is the brief's
    filter-honesty rule and is a test, not a nicety.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.3 Narrowings reset when the connection changes [dispatch: main]
      - Done in `main` (2026-08-26). The **controls** are reset with the
        state, not just the state: a cleared narrowing whose text box still
        holds a word is a screen disagreeing with itself.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: choosing another connection clears them. Test it. A filter
    set on one account and silently applied to another is how a user comes to
    believe a bucket is missing.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.4 The screenshot harness covers the narrowed states [dispatch: main]
      - Done in `main` (2026-08-26): `account-04a-narrowed` and
        `account-04b-narrowed-to-nothing`, both pixel-distinct.
      - **The harness reproduced `XONHO-0009`'s original bug twice, and I wrote
        it both times.** The first version of each frame set `narrowing`
        directly, so the images showed a filter in force above a selector
        reading "All kinds" and an empty text box — states no user can reach.
        The distinctness assertion did not catch it, because two impossible
        states are still distinct. Both frames now drive the controls.
      - The second version of 04a also came out **empty**, because the
        fixture's buckets are all general purpose and the frame had asked for
        directory ones — a frame claiming "narrowed list" that showed no list.
        It now narrows by name to one row. Looking at the image is what caught
        both; no assertion would have.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a narrowed list, and the hidden-by-narrowing empty state,
    each get a frame and each is **pixel-distinct** from every other.
  - Verification: `cargo test -p caixonho-gui`

## 3. Close-out

- [x] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-26): fmt and clippy exit 0, 349 core + 70
        window green (8 + 1 ignored). Clippy caught a `Narrowing::is_any` used
        only by a test — removed rather than kept, per `AGENTS.md` Q3.
  - Verification: the commands themselves

- [x] 3.2 CI green on both targets, run id recorded here [dispatch: main]
      - Run `32924996318` on `e714ad8`: `build (windows-latest)`,
        `build (macos-latest)`, `dependency audit` and `rustfmt` all success.
        Both changes landed in that one commit — see its message for why.
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 3.3 Live: the work account, which is what prompted this
      [dispatch: main]
  - Done criteria: on the owner's machine, open the account that lists eleven
    buckets and can open three. Turn the narrowing on **immediately, before
    the probes have settled** — that is the case that catches the self-starving
    version — and confirm the list settles at **three, not fewer and not
    zero**. Then reopen the account with it already on, which is how it will
    actually be used. Then narrow by kind and by name. What was seen, here.
  - Verification: the counts, quoted

- [x] 3.4 Reader-facing documents [dispatch: main]
      - Done in `main` (2026-08-26). §4.2's filter row **stays partial** and
        now names which half is missing — server-side `prefix` search is a
        request, not a narrowing of loaded rows. Moving the row to done would
        have been the drift. Counts by the script: unmoved, as expected.
      - A roadmap row added. `planned-changes.md` already carries the
        remembered-selection question with its home chosen, written while the
        change was planned rather than after.
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`,
    `docs/planned-changes.md`
  - Done criteria: §4.2's filter row says which half is now built and which
    half (server-side prefix search) is not; a roadmap row; and
    `planned-changes.md` carries the **remembered bucket selection** question
    with the home already chosen for it and the reason. Counts by
    `scripts/count-requirements.sh`.
  - Verification: the script's totals match the tables

- [x] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
      - Run 2026-08-26, before the live check.
      - **Q1: built what was asked, and one thing was deliberately *not*
        built.** The owner asked for the filter framed as "show accessible";
        it is named that and behaves that way. What was not built is
        `== Open`, and the reason is in the design and guarded by an ablation.
        No departure from the proposal otherwise: core is untouched, which was
        a promise and is now a measurement (**zero lines under
        `crates/caixonho-core/`**).
      - **Q2, and it changed the change.** The status bar has read
        "N of M buckets" since `XONHO-0005`. The count this change was about
        to add was a **second** answer to a question already answered — found
        by looking at the first screenshot, not by reading code. The chip went
        and the status bar's two number-sources became one. §4.2's row stays
        **partial** and names its missing half; roadmap row added; rows either
        side re-read and still true.
      - **Q3:** `Narrowing::is_any` was written, used only by a test, and
        deleted — clippy found it before the review did. Nothing else added
        that production does not call.
      - **Q4, in the form `XONHO-0023` taught: what did this change do to the
        evidence?** It very nearly broke it. The viewport that drives probing
        is built from the *narrowed* list, so this change decides what gets
        observed at all. That is the whole reason for the predicate, the
        viewport test and the ablation. The gaps that remain, named:
        - `Probing` has no test, only the same code path as `Unobserved`;
        - every assertion is a window test against a scripted world; whether
          the owner's real account settles at three buckets is 3.3;
        - the harness proves nothing on its own — it produced two impossible
          states this session and only looking caught them.
      - **Q5:** nothing discovered and left in a transcript. The
        remembered-selection question and the directory-bucket finding are
        both in `planned-changes.md`.
  - Done criteria: the five questions answered here, question 2 read the wide
    way, and question 4 asked in the form `XONHO-0023` learned it: **what did
    this change do to the evidence?** — here, whether hiding rows changed
    what the viewport reports for probing, which decides what gets probed at
    all.
  - Verification: the recorded findings

- [x] 2.5 It is a search box, so it says search [dispatch: main]
      - Raised by the owner on 2026-08-26: the name control *"looks more like
        a search box"*. It does, and checking why turned an impression into a
        measurement.
      - `list_buckets` drains its paginator before returning
        (`adapter.rs:795`), so the account listing has **every** bucket in
        hand. The brief's filter/search distinction — a filter covers loaded
        rows, a search covers everything, and the UI must say which — is real
        for the *object* listing, which is paginated and lazy
        (`XONHO-0006`). On this list there is no gap for it to describe.
      - So "Filter by name" was the less honest word, not the more careful
        one: it implies something unfetched that does not exist. Now "Search
        buckets".
      - Worth carrying forward: when the object listing gets its name control,
        that one **is** a filter over loaded rows and must say so — which is
        the half of §4.2 this change deliberately left unbuilt.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.6 A settling probe re-narrows the list [dispatch: main]
      - **The defect 1.2 planned a test for and did not write.** Found live by
        the owner, 2026-08-26: switch account, turn "Accessible only" on
        immediately, and every refused bucket stays listed for ever — each
        wearing its own **No access** badge, which is the most literal way a
        filter can look broken.
      - A settled probe only called `cx.notify()`: it redrew. Four of the five
        narrowings read data that cannot change while the user sits still; the
        accessibility one reads an **observation**, and observations arrive
        *after* the click that turned it on. The list was frozen at the moment
        nothing had been answered — the worst moment available.
      - Fixed with `probe_settled`, which re-narrows **only when that
        narrowing is on**: it is the only one an answer can move, and paying
        for a re-narrow on every probe otherwise would be work for nothing.
      - Two tests, and the second earns its place as much as the first: a
        denial settling while the narrowing is on removes its row, and a probe
        settling while it is **off** changes nothing. Ablated: reverting to a
        bare `cx.notify()` turns the first red and leaves the second green.
      - The channel half — probe sink to `probe_settled` — is one line
        verified by reading, as `XONHO-0023` did for `spawn_sign_in`.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Verification: `cargo test -p caixonho-gui`

