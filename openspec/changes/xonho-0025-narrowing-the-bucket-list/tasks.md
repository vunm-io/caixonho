# Tasks — XONHO-0025 narrowing the bucket list

> The window is where this lives, so the tests are window tests
> (`XONHO-0015`'s seam). The one that carries the change is not "the filter
> filters" — it is that an **unknown** bucket is never hidden.

## 1. The predicates

- [ ] 1.1 One predicate per narrowing, composed with `and` [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs` (or the table delegate beside it)
  - Done criteria: kind, name and no-access each decide one bucket, and the
    shown set is those satisfying all of them plus the existing region
    choice. **One pass, one count** — four passes with four counts is how the
    number comes to disagree with the rows. Tests: each alone; two together;
    clearing one leaves the other in force.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 1.2 Accessible-only is `!= Denied`, never `== Open` [dispatch: main]
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
    - a probe settling to denied while the narrowing is on removes the row and
      moves the count.
  - **Ablate it**: change the predicate to `== Open` and confirm the
    `Unobserved` test *and* the viewport test both go red. If only the render
    test goes red, the suite is not guarding the failure that matters — the
    broken version fails silently and permanently, which is why it gets an
    ablation rather than a comment.
  - Verification: `cargo test -p caixonho-gui`

## 2. The controls

- [ ] 2.1 Kind becomes a control, not a badge [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: all / directory / general purpose, beside the region
    selector. The existing "All directory buckets" badge is *derived* — it
    says every loaded row happens to be one — so decide and record here
    whether it survives beside a control that can mean the same thing. Two
    things saying almost the same thing is how a screen starts lying.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.2 Name filter and the honest count [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a text field; a count reading "showing N of M"; and an
    empty state that says the buckets were **hidden by the narrowing** —
    distinct from an account that holds none. That distinction is the brief's
    filter-honesty rule and is a test, not a nicety.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.3 Narrowings reset when the connection changes [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: choosing another connection clears them. Test it. A filter
    set on one account and silently applied to another is how a user comes to
    believe a bucket is missing.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.4 The screenshot harness covers the narrowed states [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a narrowed list, and the hidden-by-narrowing empty state,
    each get a frame and each is **pixel-distinct** from every other.
  - Verification: `cargo test -p caixonho-gui`

## 3. Close-out

- [ ] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Verification: the commands themselves

- [ ] 3.2 CI green on both targets, run id recorded here [dispatch: main]
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

- [ ] 3.4 Reader-facing documents [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`,
    `docs/planned-changes.md`
  - Done criteria: §4.2's filter row says which half is now built and which
    half (server-side prefix search) is not; a roadmap row; and
    `planned-changes.md` carries the **remembered bucket selection** question
    with the home already chosen for it and the reason. Counts by
    `scripts/count-requirements.sh`.
  - Verification: the script's totals match the tables

- [ ] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Done criteria: the five questions answered here, question 2 read the wide
    way, and question 4 asked in the form `XONHO-0023` learned it: **what did
    this change do to the evidence?** — here, whether hiding rows changed
    what the viewport reports for probing, which decides what gets probed at
    all.
  - Verification: the recorded findings
