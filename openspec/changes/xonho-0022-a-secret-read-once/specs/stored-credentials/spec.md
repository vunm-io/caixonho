## ADDED Requirements

### Requirement: The credential store is consulted once per credential, per run

The system SHALL read a stored credential's secret from the operating
system's credential store at most once per run, and SHALL use what it read
for every later use of that credential within the same run.

This is a bound on how often the store is *asked*, not a change to where
secrets live: the credential store remains the only place a secret is kept
between runs, and what the system holds while running SHALL NOT outlive the
process.

#### Scenario: The same connection is opened twice

- **WHEN** a connection backed by a stored credential is opened, and then
  opened again in the same run
- **THEN** the credential store is consulted once, and the second open uses
  what the first read

#### Scenario: Several connections

- **WHEN** two different stored credentials are each used
- **THEN** each is read once; reading one does not stand in for the other

#### Scenario: A new run

- **WHEN** the application is started again
- **THEN** nothing from the previous run is available, and the first use of
  a credential consults the store afresh

### Requirement: A credential that changes is not remembered as it was

When a stored credential's secret is written or removed, the system SHALL
discard anything it was holding for that credential, so that no later
operation is signed with a secret the user has replaced or withdrawn.

Discarding SHALL follow from the write or removal itself rather than
depending on each call site to remember, so a future path that saves or
forgets a credential cannot leave a stale secret behind by omission.

#### Scenario: A credential is edited

- **WHEN** a stored credential's secret is saved over an existing one, and
  the connection is then opened
- **THEN** the open uses the new secret

#### Scenario: A credential is forgotten

- **WHEN** a stored credential is forgotten, and something then tries to
  open that connection
- **THEN** the attempt finds no credential, exactly as though the
  application had just started
