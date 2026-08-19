## Purpose

What the app knows about what the current credentials are allowed to do, how it
comes to know it, and what it is allowed to say. S3 exposes no API that
enumerates effective permissions, so every claim here is evidence the app has
observed, and everything else stays visibly unsettled.

## Requirements

### Requirement: Capability is observed, never declared

The system SHALL treat every capability on every scope as unknown until it has
observed evidence for it, and SHALL move it out of unknown only on the strength
of a completed operation or probe against that scope with the current
credentials.

The system SHALL NOT infer one scope's capability from another's, and SHALL NOT
infer capability from the shape of a policy, a role name, or an account.

#### Scenario: No evidence has been gathered

- **WHEN** a scope has not been probed and no operation has been attempted on it
- **THEN** its capability is unknown, and it is not presented as either allowed
  or denied

#### Scenario: An operation succeeds

- **WHEN** an operation completes successfully against a scope
- **THEN** that operation's capability on that scope is recorded as allowed,
  without a separate probe

### Requirement: Probing is non-destructive

The system SHALL only probe with operations that cannot create, modify or
delete data.

The system SHALL NOT probe write or delete capability automatically. Write and
delete capability SHALL move out of unknown only through an operation the user
asked for.

#### Scenario: Probing whether a bucket's contents can be listed

- **WHEN** the system probes list capability on a bucket
- **THEN** it uses a request that returns at most a minimal result and creates
  nothing

#### Scenario: Write capability is never probed

- **WHEN** a scope's write capability is unknown and no user-initiated write has
  occurred
- **THEN** the system leaves it unknown rather than probing it

### Requirement: Probing is lazy, budgeted and non-blocking

The system SHALL probe only the scopes the user is currently looking at, SHALL
limit how many probes are in flight at once, and SHALL NOT delay rendering on
any probe.

An account with many buckets SHALL NOT produce one probe per bucket when the
list is opened.

#### Scenario: A large account is opened

- **WHEN** the bucket list is opened for an account holding far more buckets
  than fit on screen
- **THEN** probes are issued for the visible rows only, and the rest stay
  unprobed until they are shown

#### Scenario: Results arrive while the user reads the list

- **WHEN** probes are still in flight
- **THEN** the list is already rendered and remains interactive, and rows
  settle as their own results arrive

### Requirement: A pending probe is distinct from no evidence

The system SHALL present a scope whose probe is in flight as being probed, and
SHALL NOT present it as unknown, allowed or denied while the probe is running.

#### Scenario: Probe in flight

- **WHEN** a scope's probe has been issued and has not returned
- **THEN** the scope is presented as being probed, and its presentation does not
  change again until the result arrives

### Requirement: Only a denial may be presented as a denial

WHEN an operation or probe fails, the system SHALL record and present a denial
only if the failure was an authorization denial. Expired sessions, rejected
credentials, unreachable networks, wrong regions, missing buckets and trust
failures SHALL each keep their own cause and SHALL NOT be recorded as denied.

WHEN a denial is presented, the system SHALL name the IAM action that would be
required.

#### Scenario: The session expired during a probe

- **WHEN** a probe fails because the session has expired
- **THEN** the scope's capability is left unchanged and the expired session is
  reported as such, not as a denial

#### Scenario: An authorization denial

- **WHEN** a probe fails with an authorization denial
- **THEN** the capability is recorded as denied and the required IAM action is
  named

### Requirement: Observations are scoped to the credentials that produced them

The system SHALL retain observations per set of credentials and per scope, and
SHALL discard them when the credentials change — including switching profile
and re-authenticating.

#### Scenario: Switching profile

- **WHEN** the user switches to another profile
- **THEN** observations gathered under the previous profile are discarded, and
  scopes return to unknown until observed again under the new credentials
