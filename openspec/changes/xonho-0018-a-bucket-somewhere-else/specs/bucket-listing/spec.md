## MODIFIED Requirements

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

A region the listing reported is an answer, not a guarantee. WHERE a later call
is answered from a different region, the region the call was actually served
from SHALL replace the one the listing reported — see *A bucket in another
region is followed rather than refused*. What is presented is always the most
recently established fact, never the earliest.

#### Scenario: The service reports a region

- **WHEN** the bucket listing is retrieved and the service reports a region for
  a bucket
- **THEN** that bucket is presented with the region the service reported

#### Scenario: Region is not known yet

- **WHEN** an ordinary bucket's region has not been determined
- **THEN** the bucket is presented with its region shown as unknown, not as the
  connection's own region

#### Scenario: A later call contradicts the listing

- **WHEN** a bucket is read and the service serves that read from a different
  region than the listing reported
- **THEN** the bucket is presented as being in the region that served the read

## ADDED Requirements

### Requirement: A bucket in another region is followed rather than refused

A bucket that lives outside the connection's region is answered with a
redirect that names the region it belongs to. That is an answer, and the
system SHALL act on it: the read SHALL be reissued against the named region and
its result presented, rather than the redirect being reported as a failure.

The reissue SHALL happen **once**. A service that redirects a request already
addressed to the region it named is not going to be satisfied by a third
attempt, and following endlessly would turn a wrong region into a hang, which
is a worse failure than the one being fixed.

The region so discovered SHALL be remembered for that bucket for as long as the
connection lives, so that subsequent reads go to it directly rather than
learning the same fact again.

Being redirected SHALL NOT be read as evidence about permission. A redirect
says where a bucket is, and nothing whatever about what the caller may do
with it.

#### Scenario: The bucket is in another region

- **WHEN** reading a bucket is answered with a redirect naming the region the
  bucket is in
- **THEN** the read is reissued against that region and its contents are
  presented, and no failure is reported

#### Scenario: The named region redirects in turn

- **WHEN** the reissued read is itself redirected
- **THEN** the system reports the failure rather than following again

#### Scenario: A second read of the same bucket

- **WHEN** a bucket whose region was discovered by redirect is read again on
  the same connection
- **THEN** the read is addressed to the discovered region without being
  redirected a second time

### Requirement: A redirect that names no region is reported as itself

A service may answer that a bucket is elsewhere without saying where. The
system SHALL report that as its own cause, stating that the bucket lives in
another region and that the service did not name it, and pointing at the
connection's region as the thing to change.

It SHALL NOT be reported as an unexpected service error, which tells the user
nothing they can act on, and SHALL NOT be reported as access denied, which is
a different condition entirely.

#### Scenario: The service redirects without naming a region

- **WHEN** reading a bucket is answered with a redirect that carries no region
- **THEN** the system reports that the bucket is in another region the service
  did not name, and says that the connection's region is what to change
