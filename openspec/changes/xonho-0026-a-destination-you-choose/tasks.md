# Tasks — XONHO-0026 a destination you choose

> The core half is a validator and is TDD (`AGENTS.md` §7). The window half
> is a phase on a strip that is already a phase machine. The test that
> carries this one is **what is shown is what is sent** — everything else
> here is arranging a field.

## 1. What a destination may be

- [ ] 1.1 `object_key` beside `folder::key_for` [dispatch: main]
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

- [ ] 2.1 The destination is shown, defaulted, and editable [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: choosing a file puts the upload strip into a phase that
    shows the destination, pre-filled with `<prefix><file name>` — the exact
    string `app.rs:1150` composes today — with Send and Cancel. Tests: the
    default matches what the old line produced; a cleared-and-retyped
    destination is what gets sent.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.2 What is shown is what is sent [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the key handed to core is read **from the field**, and
    `app.rs:1150`'s `format!` is gone rather than left beside it. Test: edit
    the destination to something sharing no part with the default — different
    prefix *and* different file name — and assert the key core was asked for
    is exactly that. **Ablate it**: recompose the key from the location and
    confirm the test goes red. A test that only checks the prefix would pass
    a version that keeps re-deriving the name.
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.3 A refused destination costs a sentence, not a request
      [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: each refusal shows its own reason and **nothing reaches
    the store** — asserted on the double's call count, not only on the phase.
    That is the assertion `XONHO-0024` learned to write: only a count tells
    "refused without asking" apart from "asked and was refused".
  - Verification: `cargo test -p caixonho-gui`

- [ ] 2.4 The screenshot harness covers the new phase [dispatch: main]
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

- [ ] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Verification: the commands themselves

- [ ] 3.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 3.3 Live: a folder made the way `XONHO-0024` says to [dispatch: main]
  - Done criteria: on the owner's machine, on a **directory bucket**, upload
    a file to a typed destination two levels deep that does not exist yet.
    Expected: it lands, and the folders are there. **This is the check that
    closes `XONHO-0024`'s hole** — that change tells the user to do exactly
    this, so if it does not work the advice was wrong too. Then the same on a
    general purpose bucket, and one refused destination.
  - Verification: what was seen, quoted

- [ ] 3.4 Reader-facing documents [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`,
    `docs/planned-changes.md`
  - Done criteria: §4.4's upload row says the destination is chosen; a
    roadmap M2 row; and the parked note about choosing a key at upload time
    gets its outcome written under it. **Counts by the script.**
  - Verification: the script's totals match the tables

- [ ] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Done criteria: the five questions answered here, question 2 read the wide
    way — **including `XONHO-0024`'s own text**, which tells the user to
    upload into a path and was written when they could not. Question 4 asked
    as `XONHO-0023` learned it: what did this change do to the evidence?
  - Verification: the recorded findings
