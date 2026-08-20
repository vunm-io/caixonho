## ADDED Requirements

### Requirement: Connecting is something the user asks for

The system SHALL NOT resolve credentials, open a connection or request anything
from the service until the user has chosen a connection.

On startup the system SHALL present the connections available to choose from,
and SHALL make clear that nothing has been contacted yet.

#### Scenario: The application is opened

- **WHEN** the application starts, whatever profiles or stored credentials exist
- **THEN** no credential is resolved and no request is made, and the connections
  are presented for the user to choose from

#### Scenario: A connection is chosen

- **WHEN** the user chooses a connection
- **THEN** that connection's credentials are resolved and its buckets listed,
  and the interface says that work is in progress

### Requirement: A connection that cannot authenticate is not offered as usable

WHEN establishing a connection fails because its credentials could not be
resolved, were refused, or belong to a session that is no longer valid, the
system SHALL present that connection as unavailable, stating the cause and what
would make it usable.

The system SHALL keep it visible rather than hiding it, and SHALL NOT present
its failure as though the account had no buckets.

#### Scenario: A session that is no longer valid

- **WHEN** a connection's session is refused by the service
- **THEN** the connection is shown as unavailable with that cause, and the user
  is told what re-establishes it

#### Scenario: The connection remains listed

- **WHEN** a connection cannot authenticate
- **THEN** it stays in the list of connections, marked, rather than disappearing
  or appearing to hold nothing
