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

- [ ] 3.1 requirements-status, and the parked vault question
      [dispatch: main]
  - Paths: `docs/requirements-status.md`, `docs/planned-changes.md`
  - Done criteria: the §4.1 keychain row notes that the store is consulted
    once per run; `planned-changes.md` gains the **encrypted-vault question**
    as its own section — the owner's proposal, the four candidate homes for
    the vault key with what each costs, why the cheap half was done first,
    and what evidence would decide it. No README change: nothing a user can
    see is different. **Counts by `scripts/count-requirements.sh`.**
  - Verification: the script's totals match the tables

## 4. Close-out

- [ ] 4.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [ ] 4.2 CI green on both targets, run id recorded here [dispatch: main]
  - Paths: none
  - Done criteria: all four jobs successful for the tip; run id here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 4.3 Live: the prompt count actually falls [dispatch: main]
  - Paths: none
  - Done criteria: on the owner's machine, with the app freshly built so no
    grant is recorded for the new binary: switch between two stored
    connections several times and count the prompts. Expected **one per
    connection**, not one per switch. What was seen written here — and if
    it is not one per connection, that is the finding, because it would
    mean the OS is asking for a reason this change does not address.
  - Verification: the count, and the log's `connection opened` lines showing
    more opens than prompts

- [ ] 4.4 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: this change
  - Done criteria: the five questions answered and recorded here, question
    2 read the wide way.
  - Verification: the recorded findings
