## MODIFIED Requirements

### Requirement: Failure causes are distinguished

The system SHALL classify a failed connection or call into distinct, separately
reportable causes: no credentials available, expired or invalid session, TLS
trust failure, network unreachable, access denied by policy, missing
configuration, a bucket that lives in another region the service would not
name, and unexpected service error.

An access-denied result SHALL be reported **only** when the service actually
denied the request on authorization grounds. Expired sessions, wrong regions,
TLS failures, and network failures MUST NOT be reported as access denied.

A wrong region SHALL NOT reach the user as a failure at all where the service
names the right one — it is followed instead (`bucket-listing`, *A bucket in
another region is followed rather than refused*). The cause above exists for
the remaining case: the service says the bucket is elsewhere and declines to
say where, so there is nothing to follow. Naming it separately is what keeps
the previous paragraph honest, because a condition with no cause of its own
becomes "unexpected", and "unexpected" is what a user cannot act on.

A TLS trust failure SHALL be classified before the expired-session case,
because their underlying messages overlap.

An access-denied report SHALL name the IAM action that would have been required.

#### Scenario: Interception proxy without a trusted root

- **WHEN** a call fails because the presented certificate chain is not trusted
- **THEN** the system reports a certificate-trust failure and points at trust
  configuration, and does not report expired credentials or access denied

#### Scenario: Policy denies the call

- **WHEN** the service rejects a call on authorization grounds
- **THEN** the system reports access denied together with the IAM action that
  would be required

#### Scenario: Network is unreachable

- **WHEN** the endpoint cannot be reached at all
- **THEN** the system reports a network failure and offers a retry, and does
  not report a credential problem

#### Scenario: The bucket is elsewhere and the service will not say where

- **WHEN** a call is refused because the bucket belongs to another region and
  the service names no region
- **THEN** the system reports that cause on its own terms, and reports neither
  an unexpected service error nor access denied
