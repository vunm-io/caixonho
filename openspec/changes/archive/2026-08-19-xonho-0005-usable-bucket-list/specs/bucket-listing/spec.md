## MODIFIED Requirements

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

## ADDED Requirements

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
