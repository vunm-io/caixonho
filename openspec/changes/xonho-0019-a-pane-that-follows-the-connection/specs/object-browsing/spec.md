## MODIFIED Requirements

### Requirement: The application states where it is

The system SHALL hold one location — a connection, a bucket and a prefix — as
the single answer to where the user is, and SHALL derive what it displays about
that location from it rather than maintaining a separate record.

The system SHALL present the trail from the bucket to the current prefix, and
SHALL allow any step of that trail to be returned to.

The connection is part of the location and not a fact kept beside it.
Accordingly, WHEN the selected connection changes, the system SHALL end the
current location, and SHALL show nothing derived from it — no trail, no path
text, no contents — for the connection now selected. The system SHALL NOT
present a position belonging to a connection that is not the selected one,
including while the newly selected connection is still being listed.

#### Scenario: Moving into a folder

- **WHEN** the user enters a folder
- **THEN** the displayed trail extends by that folder, and the contents shown
  are that folder's

#### Scenario: Returning to an ancestor

- **WHEN** the user selects an earlier step of the trail
- **THEN** the location becomes that step, and what is shown is that step's
  contents

#### Scenario: Switching connections while inside a bucket

- **WHEN** a location is open on one connection and the user selects a
  different connection
- **THEN** the location ends, and the trail, the path text and the contents of
  the bucket that was open are no longer shown

#### Scenario: The previous position does not linger during a listing

- **WHEN** the newly selected connection has not finished listing its account
- **THEN** what is shown is that connection's own pending or empty state, and
  never the previous connection's bucket or prefix

#### Scenario: A position is never attributed to the wrong connection

- **WHEN** two connections each hold a bucket of the same name and the user
  switches between them
- **THEN** a trail is shown only for the selected connection's bucket, and
  entering it lists that connection's contents rather than the other's
