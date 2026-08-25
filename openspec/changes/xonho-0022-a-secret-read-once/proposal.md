# XONHO-0022 — A secret read once

## Why

Selecting a connection reads its secret out of the OS credential store, and
nothing remembers the answer: `connection::open` calls `credentials::load`
on every open, so walking A → B → A is three reads of the keychain.
Measured, not assumed — there is no cache anywhere in `credentials.rs` or
`connection.rs`.

On macOS every one of those reads is a gate the OS may decide to ask about,
and the owner met the consequence directly on 2026-08-25: a fresh build has
no grant recorded, so *every switch* raised a password prompt. The grants
eventually stick and the prompts stop, but the shape of the problem is
worth naming — the application asks the operating system the same question
over and over, and the operating system is entitled to ask the user each
time.

The owner proposed the general form: hold the decrypted secret in memory for
the session, ask once, and ask again only after a restart. That is right,
and this change is the cheap half of it. **The expensive half — an
app-managed encrypted vault behind a single keychain item, so one grant
covers every connection rather than one per connection — is deliberately
not here**, and is recorded in `planned-changes.md` with the design question
that decides it. This change is what makes that decision an informed one:
if reading once per session is enough, the vault buys little; if it is not,
the vault has evidence behind it rather than a guess.

## What Changes

- **A secret is read from the credential store at most once per
  connection, per run.** The first open reads it; every later open of the
  same connection uses what was already read.
- **A write or a removal drops what was remembered**, so a credential that
  is edited or forgotten never leaves a stale secret behind to be signed
  with.
- **Nothing about where secrets live changes.** The credential store is
  still the only place they are kept between runs; the cache exists only
  while the process does, and holds nothing after it exits.
- **`CredentialSecret`'s own documentation is corrected.** It currently says
  the type "exists only between the form that produced it and the credential
  store, or between the credential store and the SDK client it signs for" —
  that stops being true the moment anything holds one, and a doc comment
  that describes a lifetime the code no longer has is the kind of quiet
  drift this repository has been caught by four times.

### What is deliberately absent

- The encrypted vault, and with it the "one grant for every connection"
  outcome. Named above, parked in `planned-changes.md`.
- Any expiry or TTL on what is remembered. A stored credential does not
  change unless this application changes it, and this application knows
  when it does — a timer would be inventing a reason to go and ask again.
- Zeroizing on drop. Worth saying plainly rather than leaving implied:
  `CredentialSecret` holds ordinary `String`s today and does not wipe them,
  so this change alters **how long** a secret is resident, not **how** it is
  held. Making the type wipe itself is a real improvement and a separate
  one, because it touches every construction site rather than one seam.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `stored-credentials`: gains a requirement about how often the credential
  store is consulted, and about a remembered secret being dropped when the
  credential it belongs to is written or forgotten. The existing "secrets
  live only in the OS credential store" requirement is untouched and
  unviolated — it forbids *writing* secrets to files, logs and reports, and
  a value held in memory while the program runs is neither.

## Impact

- **`caixonho-core`**: a caching decorator over the `SecretStore` port
  rather than a field on `Session` — `get` memoizes, `put` and `delete`
  invalidate, so a save or a forget cannot forget to say so. `Session::new`
  wraps `Keyring` in it. `CredentialSecret`'s doc comment is corrected.
- **`caixonho-gui`**: nothing. This is invisible except that the OS stops
  being asked.
- **Dependencies**: none.
- **Docs**: `docs/requirements-status.md` §4.1 keychain row gains a note;
  no README change, because nothing a user can see is different — the
  prompts they stop seeing are the OS's, not the app's.
