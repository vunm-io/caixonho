## Purpose

Showing the buckets a connection can actually see, with the facts that matter
for choosing one, and staying honest when the answer is empty or the caller is
not allowed to ask.

## Requirements

### Requirement: Listing buckets for a connection

The system SHALL list the buckets visible to the active connection, presenting
each bucket's name and creation date.

A connection's buckets are not all returned by one operation. The system SHALL
also retrieve the account's directory buckets, which the general bucket listing
does not return, and SHALL present both kinds in one list rather than requiring
the user to know which operation their buckets answer to.

That retrieval is relieved in exactly one case, and in no other: where the
connection addresses a service that does not offer directory buckets at all.
That relief may not be reported as an account that holds none.

The list SHALL be produced without blocking interaction: the interface remains
responsive while the call is in flight, and the in-flight state is visible.

#### Scenario: Account with buckets

- **WHEN** the active connection can list buckets and the account has some
- **THEN** every bucket returned by the service is presented with its name and
  creation date

#### Scenario: Account with directory buckets

- **WHEN** the account contains directory buckets
- **THEN** they are presented in the same list as the account's ordinary
  buckets, each with its name and creation date

#### Scenario: Listing is slow

- **WHEN** the call has not yet returned
- **THEN** the interface stays responsive and shows that a listing is in
  progress

#### Scenario: Account with no buckets

- **WHEN** the account genuinely contains no buckets of either kind
- **THEN** the system states that the account has no buckets, and does not
  present this as an error or as a permission problem

### Requirement: Bucket region is reported when known

The system SHALL obtain each bucket's region as part of the bucket listing
whenever the service will report it, and SHALL present the region it reported.

The system SHALL present the region as unknown for any bucket the service
reports no region for, and SHALL distinguish "region not yet determined" from
any specific region rather than displaying a guessed or default value.

This holds for buckets that may exist in any region. It does not hold for a
directory bucket, which exists in one zone within one region by construction,
and for which "the region the listing was made against" is a fact rather than
a guess — see *A directory bucket's region is where its zone is*.

#### Scenario: The service reports a region

- **WHEN** the bucket listing is retrieved and the service reports a region for
  a bucket
- **THEN** that bucket is presented with the region the service reported

#### Scenario: Region is not known yet

- **WHEN** an ordinary bucket's region has not been determined
- **THEN** the bucket is presented with its region shown as unknown, not as the
  connection's own region

### Requirement: Listing denial is reported as such

WHEN the caller is not permitted to list buckets, the system SHALL report that
the listing itself was denied, naming the IAM action that would be required,
and SHALL NOT present the result as an empty account.

The two listings are permitted independently. WHEN one is denied and the other
succeeds, the system SHALL present what it did retrieve **and** state what it
was not allowed to ask for, naming that operation's own IAM action. It SHALL
NOT discard the successful half, and SHALL NOT present the outcome as a total
failure or as an account with nothing in it.

#### Scenario: Caller lacks the listing permission

- **WHEN** the service denies both bucket listings on authorization grounds
- **THEN** the system reports the denial and names the required IAM action, and
  the bucket list is not shown as empty

#### Scenario: Only one of the two listings is permitted

- **WHEN** one listing is denied on authorization grounds and the other returns
  buckets
- **THEN** the buckets that were returned are presented, and the system states
  which listing was refused and which IAM action it required

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

### Requirement: A directory bucket is presented as its own kind

Directory buckets differ from ordinary buckets in where they live and in what
can be done with them, and a name is not a reliable way for a user to tell them
apart. The system SHALL make plain, without the user reading a name for a
suffix, which of the buckets presented are directory buckets.

Where every bucket presented is of one kind, saying so **once** satisfies this;
a mark repeated on every row of a uniform list distinguishes nothing and is
read as decoration. Where more than one kind is present, each row SHALL carry
its own mark, because there the mark is what tells rows apart.

A directory bucket's name carries its zone (`<name>--<az-id>--x-s3`). Wherever
the system presents a bucket's name in full, it SHALL present the name the
service returned, unaltered — it is what any other tool, policy or console will
show — while making the part the user chose legible rather than lost inside a
suffix.

A surface too narrow to hold the whole name SHALL show the chosen part rather
than a truncation of it, because truncation cuts the half that distinguishes
one bucket from another and keeps the half that is identical across a zone. The
full name SHALL remain available on a surface that has room for it.

#### Scenario: Every bucket presented is a directory bucket

- **WHEN** the buckets presented are all directory buckets
- **THEN** the system says so once for the list, and does not repeat it on
  every row

#### Scenario: Both kinds are presented together

- **WHEN** the buckets presented include both kinds
- **THEN** each directory bucket carries its own mark, and it is identifiable
  without the user having to read its name for a suffix

#### Scenario: The name is still the service's name

- **WHEN** a directory bucket's name is presented in full
- **THEN** the name is the one the service returned, unaltered, and the zone it
  encodes is available rather than hidden

#### Scenario: The surface is narrower than the name

- **WHEN** a directory bucket is named on a surface too narrow for the whole
  name
- **THEN** the part the user chose is what is shown, not a truncation that cuts
  it, and the full name is still available elsewhere

### Requirement: A directory bucket's region is where its zone is

The system SHALL present a directory bucket's region as the region whose
listing returned it, rather than as unknown, when the service reports no region
for the bucket itself.

A directory bucket exists in one zone within one region. Presenting it as being
of unknown region would place every such bucket outside every region choice,
which is the opposite of what the region narrowing is for.

#### Scenario: The service reports no region for a directory bucket

- **WHEN** a directory bucket is returned by a listing made against a region
  and carries no region of its own
- **THEN** that bucket is presented as being in the region the listing was made
  against, and is reachable through that region's choice

### Requirement: Denials specific to directory buckets name their own action

The permissions governing directory buckets are distinct from those governing
ordinary buckets, and telling a user to obtain the wrong one costs them a
request to whoever grants it. The system SHALL report a refusal of the
directory-bucket listing, or of the session required to read a directory
bucket's contents, as its own cause, naming the action that was refused.

Such a refusal SHALL NOT be reported as a refusal of the general bucket
listing, and SHALL NOT be reported as an unrecognised failure.

#### Scenario: The directory-bucket listing is refused

- **WHEN** the service refuses the directory-bucket listing on authorization
  grounds
- **THEN** the system names that operation's own required action, and does not
  attribute the refusal to the general bucket listing

#### Scenario: The session for a directory bucket is refused

- **WHEN** opening a directory bucket is refused because the caller may not
  obtain a session for it
- **THEN** the system reports that as the cause, names the action it required,
  and does not present it as an unrecognised failure
