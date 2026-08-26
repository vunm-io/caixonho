## ADDED Requirements

### Requirement: The account listing can be narrowed

The system SHALL let the user narrow the account listing by bucket kind, by
name, and by whether access has been denied — and SHALL state how many
buckets are shown of how many were loaded, so that a narrowed list is never
mistaken for the whole account.

Narrowings SHALL compose: choosing two of them shows the buckets that satisfy
both.

#### Scenario: Narrowing by kind

- **WHEN** the user chooses to see only directory buckets, or only general
  purpose ones
- **THEN** only buckets of that kind are listed, and the count says how many
  of the loaded buckets are shown

#### Scenario: Narrowing by name

- **WHEN** the user types part of a bucket's name
- **THEN** only buckets whose name contains it are listed

#### Scenario: Narrowings compose

- **WHEN** a kind and a name are both chosen
- **THEN** the buckets shown are those matching both, and clearing one leaves
  the other in force

#### Scenario: Nothing matches

- **WHEN** a narrowing leaves no buckets
- **THEN** the listing says the account's buckets were hidden by the
  narrowing — distinct from an account that holds none

### Requirement: A bucket is only hidden as denied when a denial was observed

The system SHALL hide a bucket under the no-access narrowing only where an
authorization denial has actually been observed for it, and SHALL keep
showing every bucket whose access is unknown — so that a narrowing never
presents absence of evidence as a denial (`capability-awareness`, "Only a
denial may be presented as a denial").

#### Scenario: A bucket that has not been probed yet

- **WHEN** the no-access narrowing is on and a bucket's access is still
  unknown
- **THEN** that bucket is still listed

#### Scenario: A bucket denied for a reason that is not authorization

- **WHEN** a bucket's read failed because the session expired, the network
  was unreachable, or the region was wrong
- **THEN** the bucket is not hidden, because none of those is a denial

#### Scenario: A denial observed while the user is looking

- **WHEN** the no-access narrowing is on and a bucket's probe settles as
  denied
- **THEN** the listing reflects it, and the count changes with it
