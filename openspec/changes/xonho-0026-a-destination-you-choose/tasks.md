# Tasks — XONHO-0026 a destination you choose

> The core half is a validator and is TDD (`AGENTS.md` §7). The window half
> is a phase on a strip that is already a phase machine. The test that
> carries this one is **what is shown is what is sent** — everything else
> here is arranging a field.

## 1. What a destination may be

- [x] 1.1 `object_key` beside `folder::key_for` [dispatch: main]
      - Done in `main` (2026-08-26), red first, five tests. Three refusals,
        each with its own sentence.
      - The leading-`/` refusal is the one that was argued and is worth
        re-reading before anyone "helpfully" trims it: trimming would send a
        key the user did not type, and this change's own spec says what is
        shown is what is sent.
  - Paths: `crates/caixonho-core/src/folder.rs`
  - Done criteria: a pure function deciding whether a typed destination may
    name an object. Red first. Tests: empty; ends in `/`; starts with `/`; a
    plain name; a name with a prefix; a name whose *middle* has a `/`, which
    is fine and is the whole point. Each refusal is its own variant with its
    own sentence — "that will not work" tells someone to guess.
  - **Same module as the folder rules on purpose.** These two share the rules
    they share; two modules would drift on them.
  - Verification: `cargo test -p caixonho-core folder::`

## 2. The window

- [x] 2.1 The destination is shown, defaulted, and editable [dispatch: main]
      - Done in `main` (2026-08-26). `ChoosingDestination` is its own state
        rather than a `TransferPhase`, and not for tidiness: a `Transfer`
        holds a `Cancel` for a request that is already running, and nothing is
        running yet — making one here would mean inventing a cancel for a
        request that does not exist.
      - It carries **no `connection`**, unlike `Deletion` and `MakingFolder`.
        Those guard a *late answer* against a switched account
        (`XONHO-0019`); nothing has been sent here, so there is nothing late
        to guard, and `end_location` already drops it. Clippy found the unread
        field before the review did — the same shape as `XONHO-0008`'s unread
        `size`, and this time it really was surplus rather than a requirement
        going unmet.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: choosing a file puts the upload strip into a phase that
    shows the destination, pre-filled with `<prefix><file name>` — the exact
    string `app.rs:1150` composes today — with Send and Cancel. Tests: the
    default matches what the old line produced; a cleared-and-retyped
    destination is what gets sent.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.2 What is shown is what is sent [dispatch: main]
      - Done in `main` (2026-08-26); the `format!` at the old `app.rs:1150`
        is gone, not left beside the field.
      - **Ablated as the task demanded**: recomposing the key from the
        location and the file's basename turns
        `what_is_shown_is_what_is_sent` red — and also the refusal test,
        because a recomposed key is never refused.
      - **A limit found while writing it, and named rather than hidden.** The
        first version asserted on the *store* — the far side of the port,
        which is where the claim really lives. It could not work: the session
        runs uploads on its own tokio runtime, which a window test never
        drives, so a store-side assertion would have been empty whatever
        happened. **An assertion that cannot fail is worse than none, because
        it reads as proof.** It now asserts at the window's own seam, where
        `start_upload` hands the same string to `spawn_upload` and to the
        transfer it records, synchronously.
      - A `puts_asked` recorder was added to `StoreDouble` for that first
        version and **removed again** when it turned out unusable, rather than
        left as API for later.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the key handed to core is read **from the field**, and
    `app.rs:1150`'s `format!` is gone rather than left beside it. Test: edit
    the destination to something sharing no part with the default — different
    prefix *and* different file name — and assert the key core was asked for
    is exactly that. **Ablate it**: recompose the key from the location and
    confirm the test goes red. A test that only checks the prefix would pass
    a version that keeps re-deriving the name.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.3 A refused destination costs a sentence, not a request
      [dispatch: main]
      - Done in `main` (2026-08-26). The assertion that bites is
        `transfer.is_none()`, and it is **not** a phase check: `start_upload`
        records the transfer in the same breath as it spawns the request, so
        no transfer means nothing was asked for. The store-side count the task
        asked for would have been vacuous here for 2.2's reason, and a comment
        in the test says so where the next reader will meet it.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: each refusal shows its own reason and **nothing reaches
    the store** — asserted on the double's call count, not only on the phase.
    That is the assertion `XONHO-0024` learned to write: only a count tells
    "refused without asking" apart from "asked and was refused".
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.4 The screenshot harness covers the new phase [dispatch: main]
      - Done in `main` (2026-08-26): `bucket-15-upload-destination` and
        `bucket-16-upload-destination-refused`, pixel-distinct, driven through
        the field rather than by setting state behind it.
      - **Looked at beside their neighbours**, which is the part the task
        insisted on. The strip reads like `transfer_line` — label, field,
        reason, actions right — rather than like the card `XONHO-0024` first
        shipped. The refusal sits beside the field it is about, so the fix is
        where the mistake is.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the destination phase and a refused destination each get a
    frame, pixel-distinct, and **driven through the controls** — set the
    field's value, do not set the state behind it. `XONHO-0025` photographed
    two impossible states by doing the latter, and the distinctness assertion
    cannot catch that.
  - Verification: `cargo test -p caixonho-gui`, then **look at the images**
    beside their neighbours. The strip has to read like `transfer_line` and
    `deletion_line`, not like a card — `XONHO-0024` got that wrong and the
    owner had to point at it.

## 3. Close-out

- [x] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-26): fmt and clippy exit 0, 364 core + 80
        window green (8 + 1 ignored).
  - Verification: the commands themselves

- [x] 3.2 CI green on both targets, run id recorded here [dispatch: main]
      - Run `32928254599` on `0de278a`: `build (windows-latest)`,
        `build (macos-latest)`, `dependency audit` and `rustfmt` all success.
      - That run's `caixonho-app-windows-latest` artifact (13.9 MB, expires
        2026-11-24) is the first build of this application anyone has tried to
        **run** on Windows — see the M0b note in `docs/roadmap.md`.
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [x] 3.3 Live: a folder made the way `XONHO-0024` says to [dispatch: main]
      - Done on the owner's machine, 2026-08-27, on a **directory bucket**: a
        file sent to a typed destination whose folder did not exist, and both
        the object and the folder were there.
      - **So `XONHO-0024`'s advice is now true.** That change tells the user a
        directory bucket keeps a folder only while something is in it and to
        upload into the path they want instead; until this landed, the
        application could not take its own advice. This sitting closes that
        loop, which is what the task was written to do.
      - **It failed the first time, and not for a reason in this change.** The
        object was written and the folder existed at the service — but the
        listing on screen was not re-read, so neither appeared until the user
        navigated away and back. That is `XONHO-0020`'s upload path, which
        predates `XONHO-0024`'s "a folder nobody can see is a folder nobody
        believes in" and never learned it. Fixed and tested there.
      - The lesson for this task's wording: it asked whether the folders "are
        there", and the owner reasonably read that as *on screen*. A live
        check that means "at the service" and a live check that means "in
        front of the user" are different checks, and this one needed both.
  - Done criteria: on the owner's machine, on a **directory bucket**, upload
    a file to a typed destination two levels deep that does not exist yet.
    Expected: it lands, and the folders are there. **This is the check that
    closes `XONHO-0024`'s hole** — that change tells the user to do exactly
    this, so if it does not work the advice was wrong too. Then the same on a
    general purpose bucket, and one refused destination.
  - Verification: what was seen, quoted

- [x] 3.4 Reader-facing documents [dispatch: main]
      - Done in `main` (2026-08-26). §4.4's upload row **stays partial** —
        one file, no folders, no queue — and now says the destination is
        chosen. A roadmap M2 row. The parked note reads *built* rather than
        *issued*. Counts by the script: unmoved.
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`,
    `docs/planned-changes.md`
  - Done criteria: §4.4's upload row says the destination is chosen; a
    roadmap M2 row; and the parked note about choosing a key at upload time
    gets its outcome written under it. **Counts by the script.**
  - Verification: the script's totals match the tables

- [x] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
      - Run 2026-08-26, before the live check.
      - **Q1: no departures.** The design said no bucket-kind branch and there
        is none — the feature that needed the kind is the one that cannot
        work.
      - **Q2, read the wide way, and `XONHO-0024`'s own text was the point.**
        That change tells the user to upload into the path they want. It was
        written when they could not, and it is true now. Re-read and left as
        it stands, which is the outcome this change was for. §4.4's row stays
        partial and names what is still missing; roadmap row added.
      - **Q3:** `puts_asked` was written and deleted the same hour;
        `ChoosingDestination::connection` was written and deleted. Nothing
        added that production does not call.
      - **Q4, and the honest answer is a limit rather than a gap.** The claim
        "what is shown is what is sent" is asserted one side of the port,
        because the other side is unreachable from a window test. Named in
        2.2, in the test's own doc comment, and here. What no test covers at
        all: that writing into a path really creates the directories on a
        *directory bucket*. That is documentation so far, and it is 3.3 —
        which is also `XONHO-0024`'s live check by proxy.
      - **Q5:** nothing discovered and left in a transcript.
  - Done criteria: the five questions answered here, question 2 read the wide
    way — **including `XONHO-0024`'s own text**, which tells the user to
    upload into a path and was written when they could not. Question 4 asked
    as `XONHO-0023` learned it: what did this change do to the evidence?
  - Verification: the recorded findings
