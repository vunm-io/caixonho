## Context

See `proposal.md`. What shapes the approach:

- `Session::open` resolves credentials through the AWS provider chain for a
  named profile. Nothing else in core knows where a credential came from, which
  is the property to preserve.
- `CaixonhoApp::new` selects a profile and lists it. That call is the wait.
- The GUI already has a sidebar of connections from `XONHO-0009`, which is where
  a stored credential and an unavailable connection both belong.
- `keyring` 4.1.6 wraps macOS Keychain and Windows Credential Manager, the two
  v1 targets. Verified on crates.io, not recalled.

## Goals / Non-Goals

**Goals:**

- Opening the app costs nothing.
- A credential can be given to the app without another tool.
- A connection that cannot authenticate says so instead of behaving like one
  that can.

**Non-Goals:**

- The SSO device flow. `XONHO-0011`.
- Several connections open at once. This makes choosing explicit, which that
  will need, but does not deliver it.
- Editing a credential in place: forget it and enter it again. Editing means a
  partial-update path through the credential store for no benefit yet.

## Decisions

### A connection is a source, not a profile

Core gains one type describing where a connection's credentials come from: a
named profile from `~/.aws`, or a credential this app stores. Everything above
it — listing, probing, the capability store — keeps taking a connection and
stays unaware of the difference.

The alternative, a second path parallel to the profile path, would double every
call site and guarantee they drift.

### Stored credentials use a static provider, not a written file

A stored credential is handed to the SDK as static credentials for that client.
It is never written to `~/.aws/credentials`. Writing there would be the shortest
route and is refused: that file is shared with every other AWS tool on the
machine, and editing it silently on the user's behalf is a side effect nobody
asked for. The spec says so rather than leaving it to taste.

### The keychain holds the secret; everything else is ordinary configuration

Name, region and access key id are not secret and are kept as configuration. The
secret access key and session token go to the credential store under a key
derived from the connection's name. This keeps the config file readable and
diffable, and means losing the config loses no secret.

### Startup renders; it does not fetch

`CaixonhoApp::new` stops selecting a profile. The first screen is the sidebar
and an invitation to choose. The comment being deleted — "so the first screen
shows data rather than an instruction" — was the whole defect: it traded a
sentence of text for seven seconds of a window that looks frozen.

### Unavailable is a state of the connection, not an outcome of a listing

Today a failure is the outcome of a listing attempt, so the failure panel is
where the content would be. A connection that cannot authenticate is a fact
about the connection, so it belongs in the sidebar row as well — a mark and a
cause — with the panel still explaining it when that connection is selected.

## Risks / Trade-offs

- **A keychain prompt can block on some systems** → the store is reached from
  the runtime, never from the render thread, exactly as network calls are.
- **`keyring` is one more dependency in a security path** → it is thin, widely
  used, and dual-licensed to match. The alternative is per-platform FFI in this
  repository, which is more code in the same security path with fewer eyes.
- **A stored credential can go stale like any other** → it fails the same way,
  through the same classifier, and reads as unavailable like the rest.
- **Nothing here can be verified by a test double alone.** Whether the keychain
  actually refuses to disclose, and whether a real listing works from a typed
  key, needs a real keychain and a real endpoint. The MinIO rig the brief asks
  for at M1 is the second half of that, and stays outstanding.
