## ADDED Requirements

### Requirement: A connection is built once per run

The system SHALL build a connection's client at most once per run for a
given connection, and SHALL reuse what it built when that connection is
selected again — so that resolving credentials, which may be arbitrarily
expensive, is paid for once rather than on every selection.

Reuse SHALL NOT change what the user is shown: everything a selection ends
today still ends, and everything it re-reads is still re-read.

#### Scenario: The same connection is selected twice

- **WHEN** a connection is selected, and later selected again in the same
  run
- **THEN** its credentials are resolved once, and the second selection uses
  what the first built

#### Scenario: Coming back to a connection after another

- **WHEN** the user selects one connection, then a second, then the first
  again
- **THEN** the first connection's client is reused rather than rebuilt, and
  the second's is still available to be reused in turn

#### Scenario: Reuse is invisible on screen

- **WHEN** a connection is selected whose client is reused
- **THEN** the listing is read afresh, and what was on screen for any other
  connection is gone — identical to a selection that built its client

### Requirement: A connection that could not be used is not reused

The system SHALL NOT reuse a client for a connection whose last attempt
failed to authenticate, and SHALL build it again — so that a retry after
fixing credentials, and a selection following a sign-in, both reach the
service with what is true now rather than with what failed before.

#### Scenario: Retry after a failure

- **WHEN** a connection failed to authenticate and the user retries it
- **THEN** its client is built again rather than reused

#### Scenario: After signing in

- **WHEN** a sign-in produces a session for a connection that had none
- **THEN** the next selection of that connection builds its client again,
  and the new session is what it uses
