## Purpose

Showing the buckets a connection can actually see, with the facts that matter
for choosing one, and staying honest when the answer is empty or the caller is
not allowed to ask.

## Requirements

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

The system SHALL obtain each bucket's region as part of the bucket listing
whenever the service will report it, and SHALL present the region it reported.

The system SHALL present the region as unknown for any bucket the service
reports no region for, and SHALL distinguish "region not yet determined" from
any specific region rather than displaying a guessed or default value.

#### Scenario: The service reports a region

- **WHEN** the bucket listing is retrieved and the service reports a region for
  a bucket
- **THEN** that bucket is presented with the region the service reported

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

### Requirement: The bucket list can be narrowed to one region

The system SHALL offer a choice of region covering every region present among
the listed buckets, plus a choice that imposes no region restriction, and SHALL
show only the buckets in the chosen region.

The choice SHALL be applied to the buckets already retrieved, without issuing
another listing request.

Buckets whose region is unknown SHALL remain reachable through a choice of
their own, and SHALL NOT be silently absent from every region.

#### Scenario: A region is chosen

- **WHEN** the user chooses a region that some of the account's buckets are in
- **THEN** only the buckets in that region are shown, and no new listing request
  is made

#### Scenario: No region restriction

- **WHEN** the user chooses to impose no region restriction
- **THEN** every bucket in the account listing is shown, whatever its region

#### Scenario: Buckets without a known region

- **WHEN** the account contains buckets the service reported no region for
- **THEN** those buckets are reachable through a choice for unknown region, and
  are not attributed to any specific region
### Requirement: Buckets that cannot be entered are visibly distinct

The system SHALL present buckets whose contents it has observed to be
unlistable separately from the rest — dimmed, or grouped apart — and SHALL
state, on request, why the bucket cannot be entered and which IAM action would
be required.

The distinction SHALL rest on an observed denial only. A bucket that has not
been probed, or whose probe is still in flight, SHALL NOT be presented this way.

A bucket presented this way SHALL remain visible in the list.

#### Scenario: A bucket's contents cannot be listed

- **WHEN** the system has observed that the caller may not list a bucket's
  contents
- **THEN** that bucket is shown as distinct from the enterable ones and its
  cause and required IAM action are available

#### Scenario: A bucket has not been probed yet

- **WHEN** a bucket's list capability is unknown or its probe is in flight
- **THEN** the bucket is not presented as unenterable

#### Scenario: The account listing succeeded but nothing can be entered

- **WHEN** every bucket in the account has been observed to be unlistable
- **THEN** all of them remain visible, and the list is not presented as empty
  or as a failed listing
