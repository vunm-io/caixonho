## Why

The app connects to something before being asked. It opens, picks a profile of
its own accord and resolves that profile's credentials immediately — the code
says why: *"Open on the default profile when there is one, so the first screen
shows data rather than an instruction."* On a machine whose credentials come
from an external process that is a **seven second wait, twenty-six on the first
run of the day**, both measured, for work nobody requested.

The deeper problem is what it was waiting on. Credentials here arrive through a
password manager via `credential_process` — one developer's test scaffolding.
Nobody else will hold credentials that way, and the brief never said they would:
§4.1 has asked from the beginning for credentials the user enters, kept in the
OS keychain. Ordering the work around that local arrangement is how three
mandatory requirements went unbuilt while the list of buckets grew a region
column.

And a connection that cannot authenticate is currently offered exactly like one
that can, explaining itself only after the wait. A connection that cannot sign
in is not a connection.

## What Changes

- **Startup offers the connections and stops.** Nothing resolves, nothing is
  fetched, until a connection is chosen. The wait is removed rather than hidden.
- **Credentials can be entered in the app** — access key, secret, optional
  session token — and are kept in the OS keychain. Never in a config file, never
  in a log, never in the crash report.
- **A connection can be forgotten**, which removes what was stored for it.
- **A connection that cannot authenticate reads as unavailable**, with its cause,
  instead of being presented as usable.
- Connections from `~/.aws` and connections this app holds appear in one list.
  To someone connecting, both are just somewhere to connect; where the secret
  lives is a property of the connection, not a category of it.

Not in this change: signing in to IAM Identity Center from within the app. The
device flow is `XONHO-0011`; this change makes room for it by treating a
connection that cannot authenticate as a first-class state.

## Requirements this delivers

From `PROJECT_BRIEF.md` §4.1, and recorded in `docs/requirements-status.md`:

- **Static credentials (access key + secret + optional session token) in the OS
  keychain** — currently *none*. This is the whole of it.
- **Multiple simultaneous connections; switch profile live** — currently
  *partial*. This does not add simultaneity, but it makes choosing a connection
  an explicit act, which simultaneity later needs.
- **Detect expired/invalid tokens and offer re-login inline** — currently
  *partial*. The detection landed on 2026-08-19; this delivers the honest
  presentation of an unusable connection. The *offer* is `XONHO-0011`.

## Requirements it steps over, deliberately

Still unbuilt and mandatory, from `docs/requirements-status.md`:

- **In-app SSO sign-in (device flow)** — `XONHO-0011`, next. It is larger than
  this change and needs the unavailable-connection state this one introduces.
- **Prefix navigation** — `XONHO-0006`. It is what a person opens the app to do,
  and it stays ahead of transfers. It goes after this because a client nobody
  can give a credential to has nothing to browse.
- **Region handling that follows `x-amz-bucket-region`** and **MFA prompting**
  are untouched here and remain open.

## Capabilities

### New Capabilities

- `stored-credentials`: credentials the application itself holds — entering,
  keeping, retrieving and forgetting them, and the rules that keep them out of
  everything that is not the keychain.

### Modified Capabilities

- `connections`: connecting becomes something the user asks for rather than
  something startup does, and a connection that cannot authenticate becomes a
  state of its own rather than a listing that fails.

## Impact

- `caixonho-core`: a keychain-backed credential store, and a connection source
  that can be either a discovered profile or a stored credential. The provider
  chain is unchanged for profiles.
- `caixonho-gui`: startup renders the sidebar and an invitation instead of
  fetching; a form for entering a credential; an unavailable connection reads as
  unavailable in the list.
- One new dependency: `keyring` 4.1.6 (MIT OR Apache-2.0, published 2026-08-01,
  ~8.4M downloads), which wraps macOS Keychain and Windows Credential Manager —
  both v1 targets. Verified on crates.io rather than recalled.
