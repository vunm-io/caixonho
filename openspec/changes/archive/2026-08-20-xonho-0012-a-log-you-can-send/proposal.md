## Why

Something went wrong on a real machine and the only evidence was a screenshot.
A key the owner believed correct came back refused, and answering *why* meant
adding a diagnostic by hand, running it, and deleting it again — a route nobody
who is not building the app can take, and one that is unavailable the moment
the interesting failure is intermittent.

The application writes nothing down. Everything it learns about a failure — the
cause the classifier settled on, which credential source was used, what the
service actually said — exists for as long as the panel is on screen and is then
gone.

This is also the last piece of an invariant that has been half-written since the
beginning: *"secrets never touch the config file or the logs — log redaction is
covered by tests once logging lands."* Logging is landing.

## What Changes

- The application writes a log to a file in the platform's own log location,
  and can say where that file is so a user can send it.
- The log records what the application decided and why: a connection opened, a
  listing failed with this cause, a probe settled this way. It is not a trace of
  every function entered.
- **No secret is ever written to it**, asserted by tests rather than by
  intention — the same three-spelling check that already guards the credential
  store, because a secret can reach a file as readable text, as bytes, or as
  something a format escaped.
- The AWS SDK's own diagnostics can be turned up for a specific investigation,
  and are quiet by default: at their most detailed they carry request material
  that has no business being written down unasked.
- The file is bounded. A log that can fill a disk is a bug of its own.

## Requirements this delivers

From `PROJECT_BRIEF.md` §7 and §8, recorded in `docs/requirements-status.md`:

- **Crash handling without telemetry — reports go to a local file with a "copy
  for an issue" affordance.** This delivers the local file and the path to it;
  the crash hook itself stays open.
- **Credentials, session tokens, presigned URLs and `Authorization` headers
  redacted from all logs, asserted by a unit test.** Currently vacuous, because
  there are no logs. It stops being vacuous here.

## Requirements it steps over, deliberately

- **In-app SSO sign-in** (`XONHO-0011`) and **prefix navigation**
  (`XONHO-0006`) both remain unbuilt and mandatory. This goes first because it
  is small, and because both of them will fail in ways that are easier to
  diagnose with a log than without one.

## Capabilities

### New Capabilities

- `diagnostics`: what the application records about its own behaviour, where it
  puts it, and what may never appear in it.

### Modified Capabilities

None.

## Impact

- `caixonho-core`: a logging module; events at the points where a cause is
  decided.
- `caixonho-gui`: says where the log is, so it can be found without knowing the
  platform's conventions.
- `tracing` is already in the dependency tree via the AWS SDK; this adds
  `tracing-subscriber` and a file writer.
