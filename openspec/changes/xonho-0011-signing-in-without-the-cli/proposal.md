## Why

The brief says the AWS CLI must not be a hard dependency. Today it is one. An
IAM Identity Center connection works only while a token someone else obtained
is still sitting in `~/.aws/sso/cache`, and when it expires the application
does the one thing this project exists to avoid: it explains the problem
accurately and then asks the user to go somewhere else to solve it. On a
machine without the CLI installed there is no somewhere else, and the
connection is simply dead.

This is the oldest unpaid debt in M1. `XONHO-0004`, `XONHO-0006` and
`XONHO-0009` each stepped over it, and the last proposal said so in writing —
"the second change in a row to do so". It is now the only mandatory requirement
in §4.1 that is both unstarted and unblocked by anything else.

## What Changes

- **A connection can sign itself in.** The OIDC device authorization flow,
  run from the application: register a client, start the authorization, open
  the verification page in the system browser, poll until the user finishes,
  and hold the resulting token.
- **The token is written where the CLI writes it.** `~/.aws/sso/cache`, in the
  format the CLI already uses, because reading that cache is already `done`
  (`XONHO-0003`) and the provider chain already resolves credentials from it.
  This change adds the missing half of a loop that otherwise works — it does
  not build a second credential path beside the existing one.
- **The sentence becomes a button.** A connection marked unavailable for an
  expired or missing session currently states which command to run. It gains
  the action itself, in the place the failure is already reported.
- **Signing in is something the user asks for**, never automatic. No connection
  opens a browser because it was selected, and none does so at startup.
- **A sign-in that fails says which way it failed** — declined, timed out,
  network, or a session whose configuration the profile does not carry — with
  the same discipline the rest of the application applies to failure causes.

## Requirements this delivers

From `PROJECT_BRIEF.md` §4.1, recorded in `docs/requirements-status.md`:

- **In-app login via the OIDC device flow** — currently *none*. This is the
  whole of it, and with it the brief's promise that the CLI is optional.
- **Detect expired/invalid tokens and offer re-login inline** — currently
  *partial*: detecting landed on 2026-08-19, the offer is still prose. This
  delivers the offer, which is the half that was missing.
- **IAM Identity Center (SSO), including `[sso-session]` profiles** — currently
  *partial*. Both declared shapes can now sign in: a profile pointing at an
  `[sso-session]` section, and a legacy profile carrying `sso_start_url` and
  `sso_region` itself. `source_profile` chains still lose the session, so this
  moves the row forward without closing it.

## Requirements it steps over, deliberately

Still unbuilt and mandatory, from `docs/requirements-status.md`:

- ~~**`sso_session` resolution for legacy inline profiles**~~ — **the
  exclusion was wrong, and is withdrawn (2026-08-20).** It was argued on shape:
  a config-parsing problem does not belong inside a change whose risk is a
  network protocol. What it was never checked against was a real machine. The
  owner ran the finished window against their own account and there was no
  sign-in button, because their only Identity Center profile declares
  `sso_start_url` and `sso_region` inline and the config file has no
  `[sso-session]` section at all. Excluding the legacy shape meant excluding
  the only shape present, and the change delivered nothing where it was meant
  to. Legacy inline profiles are therefore in scope. `source_profile` chains
  stay out — a different problem, and no profile here has one.
- **Region handling that follows `x-amz-bucket-region`** and **MFA prompting**
  remain untouched, as they were before this change.
- **Sortable, resizable and persisted columns**, **filter and prefix search**,
  and **sort honesty** are `[M]` in §4.2 and remain unbuilt. They are properties
  of a listing that works; this change is about being able to reach one at all.
- **The interface testing seam** (`XONHO-0015`) and the **local S3 rig**
  (`XONHO-0010`) are not requirements at all — they are how the project stops
  shipping defects the owner finds by looking. Both were candidates for this
  slot and both were passed over, because choosing them would have been the
  fourth consecutive change to leave a mandatory requirement unbuilt while
  doing something more convenient.

## Capabilities

### New Capabilities

- `sso-sign-in`: obtaining an IAM Identity Center session from within the
  application — the device authorization flow, what the user is shown while it
  is in progress, where the resulting token is written so the rest of the
  system finds it, and how each way it can fail is told apart.

### Modified Capabilities

- `connections`: **Credential resolution** stops requiring that an SSO token
  arrived from outside the application. **A connection that cannot
  authenticate is not offered as usable** gains the requirement that what
  would make it usable is offered as an action where the cause is reported,
  not only described.

## Impact

- **Dependencies**: none added. `aws-sdk-ssooidc` 1.108.0 and `aws-sdk-sso`
  1.106.0 are already in `Cargo.lock`, pulled in by `aws-config`'s `sso`
  feature; this change names the first as a direct dependency. Verified against
  the lockfile on 2026-08-20.
- **Code**: `caixonho-core` gains the flow and the token cache writer, behind a
  port like every other outside system it talks to, so the polling loop is
  testable against a double rather than against Identity Center.
  `caixonho-gui` gains the in-progress surface and the action on the
  unavailable-connection panel.
- **The filesystem**: the application starts *writing* to `~/.aws/sso/cache`,
  having only read it until now. That directory is shared with the AWS CLI,
  which makes the format a compatibility surface, and makes a malformed write
  something that breaks another tool rather than only this one.
- **Secrets**: an access token is a secret. It falls under the existing rules —
  never logged in any spelling, never written to the connections file — and the
  token cache is the one place it is allowed to land.
- **Network**: the first outbound calls this application makes that are not to
  S3, and the first that wait on a human. Timeouts and cancellation are
  therefore part of the change rather than an afterthought.
