# Tasks — XONHO-0027 a bucket list you choose once

> Core, so TDD (`AGENTS.md` §7). The tests that carry this one are about
> *absence*: a connection nobody chose for shows everything, a preferences
> file that will not read shows everything, and a chosen bucket that has gone
> is passed over rather than reported. Every one of those failing looks like
> a bucket that has gone missing.

## 1. The store

- [ ] 1.1 A view-preferences port and its double [dispatch: main]
  - Paths: `crates/caixonho-core/src/preferences.rs` (new),
    `crates/caixonho-core/src/lib.rs`
  - Done criteria: a trait in the shape `ConnectionFile` establishes — read
    and write a per-connection choice, behind `Arc<dyn _>`, with a double for
    tests. **No test may write into the developer's own configuration
    directory**; that is what the double is for, and it is the rule that file
    already follows. Red first.
  - Verification: `cargo test -p caixonho-core preferences::`

- [ ] 1.2 The file, and what an unreadable one means [dispatch: main]
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

- [ ] 2.1 The chosen set is the fifth predicate [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`,
    `crates/caixonho-gui/src/views/buckets.rs`
  - Done criteria: it joins `Narrowing`'s single pass rather than filtering
    beside it — one pass, one count, which is what `XONHO-0025` established
    and what keeps the number agreeing with the rows. Unlike the other four
    it is **loaded** on a connection change rather than reset. Tests: a
    connection with a choice lists that subset; without one lists everything;
    switching connections swaps the choice rather than carrying it.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.2 Choosing, from the account's own listing [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a way to pick which buckets to keep, offered from the list
    itself; the choice is written when confirmed.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.3 The listing says the choice is in force [dispatch: main]
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

- [ ] 2.4 A chosen bucket the account no longer has [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: passed over — listed if present, skipped if not, nothing
    reported as failed, and **the recorded choice left exactly as it was**.
    Test the last part explicitly: pruning it would quietly forget a bucket
    that was absent for one session.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.5 The screenshot harness covers the chosen-subset states
      [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a reduced listing and the chooser each get a frame,
    pixel-distinct, **driven through the controls**. Then look at them beside
    `account-04b-narrowed-to-nothing`: if a reduced listing and a narrowed
    one are hard to tell apart on screen, 2.3 is not done however green its
    tests are.
  - Verification: `cargo test -p caixonho-gui`, and looking

## 3. Close-out

- [ ] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Verification: the commands themselves

- [ ] 3.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 3.3 Live: the work account, chosen once and still chosen tomorrow
      [dispatch: main]
  - Done criteria: on the owner's machine — choose from the account that
    lists eleven buckets, **quit the application entirely**, reopen, and
    select that connection. The chosen buckets are what is listed, and the
    screen says why. Then a connection with no choice, which must show
    everything.
  - Verification: what was seen, quoted

- [ ] 3.4 Reader-facing documents [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`,
    `docs/planned-changes.md`
  - Done criteria: §4.2's rows updated — including the `[S]` favourites line;
    a roadmap row; and the parked section on where a per-connection
    preference lives gets its outcome. **Counts by the script.**
  - Verification: the script's totals match the tables

- [ ] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Done criteria: the five questions answered here. Question 4 in
    `XONHO-0023`'s form, and it has a specific answer to give: this change
    decides which buckets reach the viewport, so — like `XONHO-0025` — it
    decides what is **probed**. Say whether a bucket outside the choice is
    still probed, and whether that is what was intended.
  - Verification: the recorded findings
