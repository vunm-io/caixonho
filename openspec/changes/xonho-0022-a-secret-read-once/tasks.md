# Tasks — XONHO-0022 a secret read once

> Small, and core, so TDD applies without exception (`AGENTS.md` §7). The
> tests that carry this change are the *counting* ones: "read once" is a
> claim about a number, and the double is what can hold that number.

## 1. The decorator

- [x] 1.1 `SecretStoreDouble` counts what it was asked [dispatch: main]
      - Done in `main` (2026-08-25). Reads are recorded **before** the
        refusal check: a call that was refused still reached the store, and
        a test about how often the store is consulted wants to know.
  - Paths: `crates/caixonho-core/src/credentials.rs`
  - Done criteria: the double records every `get(connection, field)` it
    serves, and exposes the count — so a test can assert *how many times*
    the store was consulted rather than only what came back. Existing tests
    unaffected.
  - Verification: `cargo test -p caixonho-core credentials::`

- [x] 1.2 `Remembering`, wrapping any `SecretStore` [dispatch: main]
      - Done in `main` (2026-08-25), red first: six tests against a
        `todo!()` body, all six listed below now green.
      - One addition beyond the plan, small and needed: a blanket
        `impl SecretStore for Arc<S>`, so one `Arc` can be handed to a
        `Remembering` *and* held for direct inspection — which is how the
        session test asserts what the inner store was asked while the
        session talks only to the outer one.
      - `put` writes first and forgets after: a write that failed leaves
        what was remembered still true, and forgetting early would cost a
        question for nothing.
  - Paths: `crates/caixonho-core/src/credentials.rs`
  - Done criteria: `get` memoizes by `(connection, field)` including the
    `None` answer; `put` and `delete` drop that key's entry; a poisoned lock
    is recovered rather than propagated, as everywhere else in this crate.
    Red first, and the red tests are:
    - two `get`s of the same key consult the inner store once;
    - two different keys consult it twice (one does not stand in for the
      other);
    - a `get` that answered `None` is not re-asked;
    - `put` then `get` returns the **new** value and consults the inner
      store again;
    - `delete` then `get` returns `None` from the inner store, not a
      remembered value;
    - an error from the inner store is **not** remembered — a keychain that
      was locked once must be askable again.
  - Verification: `cargo test -p caixonho-core credentials::`

## 2. The session uses it

- [x] 2.1 Wrap `Keyring`, and correct the type's documentation
      [dispatch: main]
      - Done in `main` (2026-08-25). `CredentialSecret`'s comment no longer
        claims the type exists only in transit, and says both what changed
        (duration) and what did not (these are plain `String`s, unwiped —
        so exposure class is the same either way).
      - A `#[cfg(test)]` belonging to `mod double` was briefly captured by
        the new struct during editing, which made `Remembering` "configured
        out" and produced three confusing errors. Noted because the symptom
        — *unresolved import for a type you can see* — reads as anything
        but an attribute one line out of place.
  - Paths: `crates/caixonho-core/src/session.rs`,
    `crates/caixonho-core/src/credentials.rs`
  - Done criteria: `Session::new` builds `Remembering::new(Keyring)`;
    `with_secret_store` keeps taking a bare store so tests inject what they
    like. `CredentialSecret`'s doc comment no longer claims the type exists
    only in transit between the form and the store — it is now held for the
    run, and the comment says so and says why.
  - Verification: `cargo test -p caixonho-core`

- [x] 2.2 The session-level assertion: one open, then another
      [dispatch: main]
      - Done in `main` (2026-08-25). Both opens fail at the network — this
        machine reaches nothing — but both reach the credential store
        first, which is the part under test.
      - Ablated to confirm the assertions bite: removing the memory lookup
        turns three tests red, at both the decorator and the session tier.
  - Paths: `crates/caixonho-core/src/session.rs`
  - Done criteria: a test opens the same stored connection twice through a
    `Session` built over a counting double wrapped in `Remembering`, and
    asserts the store was consulted once for the secret. A second test
    saves a new secret between the two opens and asserts the second open
    used it — the invalidation path, exercised where a real caller lives
    rather than only on the decorator.
  - Verification: `cargo test -p caixonho-core session::`

## 3. Reader-facing documents

- [x] 3.1 requirements-status, and the parked vault question
      [dispatch: main]
      - Done in `main` (2026-08-25). The §4.1 row stays **done** and gains
        the once-per-run note — where a secret lives did not change, so the
        state should not either. No count movement, which the script
        confirms.
      - The vault section carries the four candidate homes with what each
        costs, and one thing worth more than the table: the requirement that
        an encrypted vault would actually break is **not** the obvious one.
        `stored-credentials` forbids *writing* secrets to files, and
        ciphertext is not plaintext — but `lib.rs`'s invariant says secrets
        reach the OS credential store *and nothing else*, and under a vault
        what reaches it is the key. Named now so the proposal that does it
        starts from there rather than discovering it late.
  - Paths: `docs/requirements-status.md`, `docs/planned-changes.md`
  - Done criteria: the §4.1 keychain row notes that the store is consulted
    once per run; `planned-changes.md` gains the **encrypted-vault question**
    as its own section — the owner's proposal, the four candidate homes for
    the vault key with what each costs, why the cheap half was done first,
    and what evidence would decide it. No README change: nothing a user can
    see is different. **Counts by `scripts/count-requirements.sh`.**
  - Verification: the script's totals match the tables

## 4. Close-out

- [x] 4.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-25): fmt checked, clippy zero at
        `-D warnings`, 342 core + 63 window green (8 + 1 ignored).
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [ ] 4.2 CI green on both targets, run id recorded here [dispatch: main]
  - Paths: none
  - Done criteria: all four jobs successful for the tip; run id here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [x] 4.3 Live: the prompt count actually falls [dispatch: main]
      - Done on the owner's machine, 2026-08-25, on a binary built minutes
        earlier so no grant existed for it. **Owner's count: one prompt at
        launch, one on the first connection click — then silence.** The log
        for the same window shows one stored connection opened **three**
        times and another **twice**: five opens, and the owner was asked
        about neither of the repeats.
      - The second-open timing is the same fact from the other side: the
        first open of that connection took 1.11s, the next two took 0.07s
        and 0.09s. What was slow was the keychain, once.
      - **The prompt at launch is worth a second look and is not this
        change's doing** — nothing here reads a secret at startup, so
        something on the startup path asks the store before any connection
        is chosen. Recorded rather than explained; it costs one prompt per
        run and is its own small investigation.
      - **What did not move, exactly as the review predicted:** profile
        connections still take 4.07–9.50s in the same window
        (`vunm` ×4, `r2-caixonho` ×3), against 0.06–0.09s for a remembered
        stored credential. That is `credential_process`, measured at 3.99s
        warm, and it is a path this change never touches. The prediction
        being right is the point: the claim was narrowed *before* the
        measurement, not after it.
  - Paths: none
  - Done criteria: on the owner's machine, with the app freshly built so no
    grant is recorded for the new binary: switch between two stored
    connections several times and count the prompts. Expected **one per
    connection**, not one per switch. What was seen written here — and if
    it is not one per connection, that is the finding, because it would
    mean the OS is asking for a reason this change does not address.
  - Verification: the count, and the log's `connection opened` lines showing
    more opens than prompts

- [x] 4.4 Close-out review per `AGENTS.md` [dispatch: main]
      - Run 2026-08-25, before the live check as with the last four.
      - **Q1: one departure, and it is a narrowing of the claim rather than
        of the work.** The proposal opened by describing the owner's
        complaint — prompts on every switch — and this change reduces
        *reads*, which reduces the OS's opportunities to ask but does not
        control whether it does. The measurement in `planned-changes.md`
        the same day found the 4-second irritant is a different path
        entirely (`credential_process`, which this never touches). Both
        facts are now written where a reader meets them; neither was
        absorbed.
      - **Q3: checked, and it came out clean** — `Keyring` is constructed
        in exactly one place and only inside `Remembering`, so no path
        reaches the keychain unmemoized. Grepped rather than assumed.
      - **Q4, the honest gap:** every assertion here is about the *double*
        being asked once. Whether the operating system therefore prompts
        less is a claim no unit test can make, which is what 4.3 exists for
        — and 4.3 is written so that "still one prompt per switch" is a
        recorded finding rather than a silent disappointment.
      - **Q2 both directions:** requirements-status §4.1 keeps its **done**
        state deliberately — where a secret lives did not change, so moving
        the row would have been the drift, not the fix. Rows either side
        untouched and still true. `CredentialSecret`'s doc was the one piece
        of prose this change made false, and correcting it was task 2.1
        rather than an afterthought.
      - **Q5:** the vault question is parked with its four options, its two
        real costs, and the requirement it would actually break — which is
        `lib.rs`'s invariant, not the spec sentence everyone would check
        first.
  - Paths: this change
  - Done criteria: the five questions answered and recorded here, question
    2 read the wide way.
  - Verification: the recorded findings
