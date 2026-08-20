## Purpose

Turning a user's chosen AWS profile into credentials caixonho can call S3 with,
and — when that fails — saying precisely which of the several indistinguishable
causes actually occurred, so the user fixes the right thing.

## Requirements

### Requirement: Profile discovery

The system SHALL list the connection profiles available on the machine by
reading the AWS shared configuration files (`~/.aws/config` and
`~/.aws/credentials`, or the paths named by `AWS_CONFIG_FILE` and
`AWS_SHARED_CREDENTIALS_FILE`), and SHALL present the default profile alongside
named ones.

Discovery SHALL NOT require any credential to be valid: a profile that exists
but cannot authenticate MUST still appear in the list.

#### Scenario: Named and default profiles are listed

- **WHEN** the shared config declares a default profile and two named profiles
- **THEN** the system lists all three, identifying which one is the default

#### Scenario: No configuration files exist

- **WHEN** neither shared configuration file exists
- **THEN** the system reports that no profiles were found, and does not present
  this as an authentication failure

#### Scenario: A listed profile has unusable credentials

- **WHEN** a profile is declared but its credentials are expired or incomplete
- **THEN** the profile still appears in the list, and the failure surfaces only
  when a connection to it is opened

### Requirement: Credential resolution

The system SHALL resolve a selected profile into usable credentials, supporting
static access keys, `role_arn` with `source_profile` chains, and IAM Identity
Center (SSO) profiles whose token is already present in the AWS CLI token cache.

Resolution SHALL respect the region configured for the profile, and SHALL treat
the absence of a region as a reportable configuration error rather than a
silent default.

#### Scenario: SSO profile with a valid cached token

- **WHEN** a connection is opened to an SSO profile whose cached token is still
  valid
- **THEN** credentials resolve without any browser interaction or external
  command

#### Scenario: SSO profile with an expired cached token

- **WHEN** the cached token for an SSO profile has expired
- **THEN** the system reports an expired-session condition naming the profile
  and its SSO session, distinct from a permission failure

#### Scenario: Assume-role chain

- **WHEN** the selected profile declares `role_arn` with a `source_profile`
- **THEN** the source profile's credentials are used to assume the role, and
  the resulting temporary credentials are used for subsequent calls

#### Scenario: Profile without a region

- **WHEN** the selected profile declares no region and none is set in the
  environment
- **THEN** the system reports a missing-region configuration error naming the
  profile

### Requirement: Credentials are never disclosed

The system SHALL NOT write access keys, secret keys, session tokens, SSO
tokens, or `Authorization` headers to logs, error messages, or any user-visible
surface, and SHALL NOT copy credential material into its own configuration
files.

#### Scenario: Failure message contains no secret

- **WHEN** any connection error is produced, for any cause
- **THEN** its message and its details contain no credential material

### Requirement: Failure causes are distinguished

The system SHALL classify a failed connection or call into distinct, separately
reportable causes: no credentials available, expired or invalid session, TLS
trust failure, network unreachable, access denied by policy, missing
configuration, and unexpected service error.

An access-denied result SHALL be reported **only** when the service actually
denied the request on authorization grounds. Expired sessions, wrong regions,
TLS failures, and network failures MUST NOT be reported as access denied.

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

### Requirement: Enterprise trust material is honoured

The system SHALL verify TLS using the operating system's trust store, so that
certificates issued by an enterprise certificate authority are accepted without
extra configuration. It SHALL also honour a certificate bundle named by
`AWS_CA_BUNDLE` or `SSL_CERT_FILE` when present.

Trust settings SHALL apply identically to service calls and to credential and
SSO calls.

The system SHALL NOT offer any option that disables certificate verification.

#### Scenario: Enterprise CA in the OS trust store

- **WHEN** the machine's trust store contains the CA that signed the
  intercepted connection
- **THEN** calls succeed without additional configuration

#### Scenario: Credential endpoint uses the same trust settings

- **WHEN** trust material is configured
- **THEN** it applies to token and credential endpoints as well as to S3
  endpoints

### Requirement: Switching profiles without restart

The system SHALL allow changing the active profile at runtime, and after a
switch SHALL show data belonging only to the newly selected profile.

#### Scenario: Switching to another profile

- **WHEN** the user selects a different profile while results from the previous
  one are displayed
- **THEN** the previous results are cleared or replaced, and no result from the
  previous profile remains visible as if it belonged to the new one

#### Scenario: Switching away from a failing profile

- **WHEN** the active profile is in an error state and the user selects a
  working profile
- **THEN** the error is cleared and the new profile's data is loaded

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
