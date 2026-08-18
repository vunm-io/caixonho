## Purpose

Showing the buckets a connection can actually see, with the facts that matter
for choosing one, and staying honest when the answer is empty or the caller is
not allowed to ask.

## ADDED Requirements

### Requirement: Listing buckets for a connection

The system SHALL list the buckets visible to the active connection, presenting
each bucket's name and creation date.

The list SHALL be produced without blocking interaction: the interface remains
responsive while the call is in flight, and the in-flight state is visible.

#### Scenario: Account with buckets

- **WHEN** the active connection can list buckets and the account has some
- **THEN** every bucket returned by the service is presented with its name and
  creation date

#### Scenario: Listing is slow

- **WHEN** the call has not yet returned
- **THEN** the interface stays responsive and shows that a listing is in
  progress

#### Scenario: Account with no buckets

- **WHEN** the account genuinely contains no buckets
- **THEN** the system states that the account has no buckets, and does not
  present this as an error or as a permission problem

### Requirement: Bucket region is reported when known

The system SHALL present each bucket's region when it is known, and SHALL
distinguish "region not yet determined" from any specific region rather than
displaying a guessed or default value.

#### Scenario: Region is not known yet

- **WHEN** a bucket's region has not been determined
- **THEN** the bucket is presented with its region shown as unknown, not as the
  connection's own region

### Requirement: Listing denial is reported as such

WHEN the caller is not permitted to list buckets, the system SHALL report that
the listing itself was denied, naming the IAM action that would be required,
and SHALL NOT present the result as an empty account.

#### Scenario: Caller lacks the listing permission

- **WHEN** the service denies the bucket listing on authorization grounds
- **THEN** the system reports the denial and names the required IAM action, and
  the bucket list is not shown as empty

### Requirement: Failures are recoverable in place

WHEN listing fails for a recoverable reason, the system SHALL present the cause
in the terms established for connection failures and offer the matching next
action: retry for network failures, and re-authentication for expired sessions.

#### Scenario: Expired session during listing

- **WHEN** the listing fails because the session has expired
- **THEN** the system reports the expired session and offers re-authentication,
  without requiring a restart

#### Scenario: Retry after a network failure

- **WHEN** a listing failed because of a network failure and the user retries
- **THEN** the listing is attempted again on the same connection
