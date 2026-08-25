# Design — a secret read once

## Context

`Session` holds `secrets: Arc<dyn SecretStore>` and every path that touches a
secret — `connection::open`, `spawn_save_credential`,
`spawn_forget_credential` — goes through that one handle. That is the whole
reason this change is small: there is already exactly one seam, and it is a
port with an injection point (`with_secret_store`) and a double
(`SecretStoreDouble`).

## Goals / Non-Goals

**Goals**

- One read per credential per run.
- Invalidation that cannot be forgotten by a future call site.
- No change to where secrets live between runs.

**Non-Goals**

- The encrypted vault (parked; see `planned-changes.md`).
- Zeroizing `CredentialSecret` on drop — real, separate, and touching every
  construction site rather than one seam.
- Any TTL.

## Decisions

### A decorator on the port, not a field on `Session`

`Remembering` wraps another `SecretStore`: `get` memoizes, `put` and
`delete` drop the entry. `Session::new` wraps `Keyring` in it.

The alternative — a `HashMap` on `Session`, consulted before calling
`credentials::load` — was declined for one reason, and it is the reason this
repository keeps choosing structure over discipline: with a map on the
session, every future path that saves or forgets a credential has to
*remember* to invalidate. With the decorator, saving **is** invalidating,
because `credentials::save` already calls `put` through the same handle. A
path that forgets to invalidate cannot be written, rather than merely being
discouraged.

It also lands the tests where the evidence is: `SecretStoreDouble` can count
reads, so "read once" becomes an assertion about a number rather than a
claim about behaviour nobody measured.

### Negative answers are remembered too

`credentials::load` asks for both fields, and a long-lived credential has no
session token — so half of every load is a question whose answer is "there
is nothing". Remembering `None` is safe here for a specific reason rather
than by convenience: these items have exactly one writer, this application,
and every write goes through `put` on this same decorator. Nothing can
appear behind our back that we would then fail to notice.

### The cache is keyed by what the store is keyed by

`(connection, field)` — the same pair `SecretStore::get` takes. Not by
connection alone: the two fields are separate items with separate answers,
and collapsing them would make a present session token indistinguishable
from an absent one.

### What this does not fix, stated so nobody expects it

The first read per connection per run still reaches the OS, and the OS may
still ask. On a fresh build with no grant recorded, the owner will still see
one prompt per connection — down from one per switch, but not zero. Zero is
the vault's promise, not this change's, and the difference is exactly what
`planned-changes.md` needs the evidence for.

## Risks / Trade-offs

- **[A secret is resident longer]** → true, and the honest framing is that
  it changes duration rather than exposure class: `CredentialSecret` holds
  plain `String`s that are not wiped today, so a secret is already readable
  in the process image while in use. Named in the proposal as absent work
  rather than hidden.
- **[A secret changed outside this application goes unseen]** → only this
  application writes these items, and it writes through the decorator. A
  user editing the item by hand in Keychain Access would be missed until
  restart; that is a real gap and a small one, and saying so beats
  discovering it.
- **[The decorator is one more layer between core and the keychain]** →
  it is thirty lines with its own tests, and it removes a repeated call the
  reader currently has to notice for themselves.

## Open Questions

None. The seam, the invalidation points and the absence of any existing
cache were all read from the code before this was written.
