# Tasks — XONHO-0027 a bucket list you choose once

> Core, so TDD (`AGENTS.md` §7). The tests that carry this one are about
> *absence*: a connection nobody chose for shows everything, a preferences
> file that will not read shows everything, and a chosen bucket that has gone
> is passed over rather than reported. Every one of those failing looks like
> a bucket that has gone missing.

## 1. The store

- [x] 1.1 A view-preferences port and its double [dispatch: main]
      - Done in `main` (2026-08-26). The port is **one method smaller** than
        `ConnectionFile`, and the missing one says what this file is: that
        trait has `location` because a connections failure has to name the
        file it could not read. Nothing here ever surfaces a failure, so no
        message needs the path — clippy found it before the review did.
  - Paths: `crates/caixonho-core/src/preferences.rs` (new),
    `crates/caixonho-core/src/lib.rs`
  - Done criteria: a trait in the shape `ConnectionFile` establishes — read
    and write a per-connection choice, behind `Arc<dyn _>`, with a double for
    tests. **No test may write into the developer's own configuration
    directory**; that is what the double is for, and it is the rule that file
    already follows. Red first.
  - Verification: `cargo test -p caixonho-core preferences::`

- [x] 1.2 The file, and what an unreadable one means [dispatch: main]
      - Done in `main` (2026-08-26), nine tests, every one about *absence*.
      - **Hand-encoded, and that was a decision rather than an oversight.**
        `caixonho-core` carries neither `serde` nor a TOML crate —
        `connections.rs` writes its own — so a derive would have cost two
        dependency trees for a display preference, which is exactly the sort
        of thing `XONHO-0017`'s audit exists to notice. The format is simpler
        than the one already hand-written there.
      - The decoder **skips** what it cannot read where `connections.rs`
        **refuses**, and the difference is deliberate: losing a credential's
        existence is worth telling someone about, and losing a display
        preference is worth showing them their buckets.
  - Paths: `crates/caixonho-core/src/preferences.rs`
  - Done criteria: written beside the connections file, never inside it. Red
    first, and the red tests are the ones about absence:
    - no file at all → every connection reads as "no choice";
    - a malformed file → the same, and **the listing still works**;
    - a choice written for one connection is not read for another;
    - a choice round-trips a restart (write, drop, read again).
  - **This must never fail a listing.** A preferences file that cannot be read
    means show everything — the behaviour before this change existed.
  - Verification: `cargo test -p caixonho-core preferences::`

## 2. The window

- [x] 2.1 The chosen set is the fifth predicate [dispatch: main]
      - Done in `main` (2026-08-26). One pass, one count, joining the other
        four. Loaded on a connection change rather than reset — the one
        exception, and `clear_narrowing` now takes the choice out and puts it
        back so the reset cannot swallow it.
      - Ablated: treating an empty choice as no choice turns
        `choosing_nothing_is_not_the_same_as_not_choosing` red.
  - Paths: `crates/caixonho-gui/src/app.rs`,
    `crates/caixonho-gui/src/views/buckets.rs`
  - Done criteria: it joins `Narrowing`'s single pass rather than filtering
    beside it — one pass, one count, which is what `XONHO-0025` established
    and what keeps the number agreeing with the rows. Unlike the other four
    it is **loaded** on a connection change rather than reset. Tests: a
    connection with a choice lists that subset; without one lists everything;
    switching connections swaps the choice rather than carrying it.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.2 Choosing, from the account's own listing [dispatch: main]
      - Done in `main` (2026-08-26). The ticked set is held apart from the
        live choice, so abandoning the chooser changes nothing — a picker that
        edited the choice as you ticked would apply half a decision.
      - Opening it with nothing recorded ticks **everything**, because
        everything is showing; opening to an empty picker would look like a
        fresh empty choice. Ablated: defaulting to empty turns
        `the_chooser_opens_ticked_to_what_is_showing` red.
      - **The harness caught a real defect here.** The chooser was first wired
        into the strip row beneath a *bucket's contents*, where it could never
        appear — it belongs to the account screen. `XONHO-0009`'s
        pixel-distinctness assertion is what said so: the frame came out
        identical to `account-04-loaded`.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a way to pick which buckets to keep, offered from the list
    itself; the choice is written when confirmed.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.3 The listing says the choice is in force [dispatch: main]
      - Done in `main` (2026-08-26), and **the first version did not meet the
        requirement**. `show_all_buckets` set the choice to `None`, which is
        indistinguishable from never having chosen: the explanation vanished
        and there was no way back. The spec says *"the recorded choice is
        still there to return to"*.
      - Found because an ablation **did not bite**. Flipping the show-all path
        turned nothing red, and reading the test showed why: it asserted only
        that every bucket was listed, never that the choice survived. The test
        was named for a requirement it did not check.
      - Fixed with `showing_all` beside `chosen`, so setting aside and giving
        up are two states rather than one, and the row offers **Back to my
        buckets**. The test now asserts both halves.
      - The line moved to its own row under the controls: it is a statement
        about the list rather than a control, and as a seventh item on that
        row it squeezed the search field down to a few characters — visible in
        `account-04c` before the fix.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a reduced listing says it is a chosen subset and how many
    buckets the account holds, and offers "show all" **without discarding the
    choice**. Test both: that it says so, and that showing all leaves the
    choice recorded.
  - **Three states must read differently**: an account holding nothing, an
    account narrowed to nothing by `XONHO-0025`, and an account reduced by a
    remembered choice. Assert the three are distinguishable, because they are
    one sentence away from each other and the third is the one that reads as
    a bug.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.4 A chosen bucket the account no longer has [dispatch: main]
      - Done in `main` (2026-08-26). Listed if present, passed over if not,
        nothing failed — and the test asserts explicitly that **the recorded
        choice is unchanged**, because pruning it would quietly forget a
        bucket that was absent for one session.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: passed over — listed if present, skipped if not, nothing
    reported as failed, and **the recorded choice left exactly as it was**.
    Test the last part explicitly: pruning it would quietly forget a bucket
    that was absent for one session.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.5 The screenshot harness covers the chosen-subset states
      [dispatch: main]
      - Done in `main` (2026-08-26): `account-04c-chosen-subset`,
        `account-04d-chosen-subset-showing-all` and
        `account-04e-choosing-buckets`, pixel-distinct.
      - **Looked at beside `account-04b-narrowed-to-nothing`**, which the task
        asked for and which is the only way 2.3 could be judged. They read
        differently: 04b is an empty pane saying the filters did it; 04c is a
        short list with a line saying one of four was chosen.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a reduced listing and the chooser each get a frame,
    pixel-distinct, **driven through the controls**. Then look at them beside
    `account-04b-narrowed-to-nothing`: if a reduced listing and a narrowed
    one are hard to tell apart on screen, 2.3 is not done however green its
    tests are.
  - Verification: `cargo test -p caixonho-gui`, and looking

## 3. Close-out

- [x] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-26): fmt and clippy exit 0, 375 core + 88
        window green (8 + 1 ignored).
  - Verification: the commands themselves

- [x] 3.2 CI green on both targets, run id recorded here [dispatch: main]
      - Run `32954957225` on `5084e05`: `build (windows-latest)`,
        `build (macos-latest)`, `dependency audit` and `rustfmt` all success.
        That run's artifacts carry everything through `XONHO-0027`.
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 3.3 Live: the work account, chosen once and still chosen tomorrow
      [dispatch: main]
  - Done criteria: on the owner's machine — choose from the account that
    lists eleven buckets, **quit the application entirely**, reopen, and
    select that connection. The chosen buckets are what is listed, and the
    screen says why. Then a connection with no choice, which must show
    everything.
  - Verification: what was seen, quoted

- [x] 3.4 Reader-facing documents [dispatch: main]
      - Done in `main` (2026-08-26). §4.2's filter row gains the fifth
        narrowing; the `[S]` favourites line **gains a row of its own**, at
        partial, listed because a built `[S]` left unlisted makes the section
        read emptier than it is — the rule §4.5 already follows. Roadmap rows
        for `XONHO-0025` and `XONHO-0027`, the first of which was missing.
      - The count moved 24 → 25 rows, and the summary paragraph was corrected
        by the script rather than by eye.
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`,
    `docs/planned-changes.md`
  - Done criteria: §4.2's rows updated — including the `[S]` favourites line;
    a roadmap row; and the parked section on where a per-connection
    preference lives gets its outcome. **Counts by the script.**
  - Verification: the script's totals match the tables

- [x] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
      - Run 2026-08-26, before the live check.
      - **Q1: one requirement was missed and then met** — see 2.3. Nothing
        else departed: the store is where the design said, keyed as the design
        said, and the chosen set joined the single pass rather than filtering
        beside it.
      - **Q2:** §4.2's two rows and two roadmap rows, one of which
        (`XONHO-0025`) had been missing since it landed — the same failure
        this question caught for `XONHO-0022` yesterday. Two changes in a row
        with an absent roadmap row is not bad luck; the row is written at
        close-out and close-out is when attention is lowest.
      - **Q3:** `PreferencesFile::location`, `PreferencesFileDouble::contents`
        and the double's `location` impl were all written and then deleted.
        `Session::with_preferences_file` was **kept** and given the test it
        was missing instead — it was unused because nothing exercised the
        session seam at all, which is a gap rather than surplus API.
      - **Q4, and it has a specific answer this time.** This change decides
        which buckets reach `shown`, and `targets()` is built from `shown` —
        so a bucket outside the choice is **never probed**. That is
        intended and it is not `XONHO-0025`'s trap: an unchosen bucket is
        not hidden pending an answer it can never get, it is hidden because
        the user said so, and Show all brings it back and probes it. Said out
        loud because the two are one line apart in the same predicate.
      - What no test covers: that the file survives a real restart on a real
        disk. `PreferencesDirectory` has no test — every test uses the double,
        by the rule that nothing may write into the developer's own config
        directory. 3.3 is that check.
      - **Q5:** nothing discovered and left in a transcript.
  - Done criteria: the five questions answered here. Question 4 in
    `XONHO-0023`'s form, and it has a specific answer to give: this change
    decides which buckets reach the viewport, so — like `XONHO-0025` — it
    decides what is **probed**. Say whether a bucket outside the choice is
    still probed, and whether that is what was intended.
  - Verification: the recorded findings
