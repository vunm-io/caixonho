## ADDED Requirements

### Requirement: Which buckets to show can be chosen and remembered

The system SHALL let the user choose which of a connection's buckets to list,
SHALL remember that choice for that connection across restarts, and SHALL
apply it alongside the narrowings already available — so that an account with
many buckets and few that matter can be reduced once rather than every
session.

A connection for which no choice has been recorded SHALL list every bucket.

#### Scenario: A choice made and kept

- **WHEN** the user chooses a subset of a connection's buckets, then restarts
  the application and selects that connection
- **THEN** the listing shows the chosen buckets

#### Scenario: A connection nobody has chosen for

- **WHEN** a connection has no recorded choice
- **THEN** every bucket the account lists is shown

#### Scenario: Choices belong to their own connection

- **WHEN** a choice is recorded for one connection and another is selected
- **THEN** the other connection's listing is unaffected by it

### Requirement: A remembered choice says it is in force

The system SHALL show that a listing is reduced by a remembered choice, and
SHALL offer to show every bucket without discarding the choice — so that a
filter set weeks ago and forgotten is never mistaken for a bucket that has
gone missing.

#### Scenario: The listing says it is a chosen subset

- **WHEN** a connection with a recorded choice is listed
- **THEN** the screen says the list is a chosen subset and how many buckets
  the account holds

#### Scenario: Showing everything without losing the choice

- **WHEN** the user asks to see every bucket
- **THEN** all are listed, and the recorded choice is still there to return to

### Requirement: A chosen bucket the account no longer has is not an error

The system SHALL treat a recorded choice as a wish about names, listing those
that exist and passing over those that do not, without failing and without
silently rewriting the choice — because a bucket may be absent for a session
and back the next, and a deletion made elsewhere is not this application's to
report as a fault.

#### Scenario: A chosen bucket has gone

- **WHEN** a connection's recorded choice names a bucket the account no
  longer lists
- **THEN** the remaining chosen buckets are listed, nothing is reported as
  failed, and the choice is left as it was
