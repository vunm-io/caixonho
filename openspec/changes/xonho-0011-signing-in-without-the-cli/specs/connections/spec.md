## MODIFIED Requirements

### Requirement: Credential resolution

The system SHALL resolve a selected profile into usable credentials, supporting
static access keys, `role_arn` with `source_profile` chains, and IAM Identity
Center (SSO) profiles whose token is present in the AWS CLI token cache —
whether that token was obtained by this application or by another tool.

Resolution SHALL respect the region configured for the profile, and SHALL treat
the absence of a region as a reportable configuration error rather than a
silent default.

Resolution itself SHALL NOT obtain a session. Signing in is a separate act the
user asks for (`sso-sign-in`), and resolution reports the absence of a valid
token rather than repairing it.

#### Scenario: SSO profile with a valid cached token

- **WHEN** a connection is opened to an SSO profile whose cached token is still
  valid
- **THEN** credentials resolve without any browser interaction or external
  command

#### Scenario: SSO profile whose token this application obtained

- **WHEN** a connection is opened to an SSO profile whose token was obtained by
  this application's own sign-in
- **THEN** credentials resolve by the same path as a token left by the AWS CLI,
  with no separate handling

#### Scenario: SSO profile with an expired cached token

- **WHEN** the cached token for an SSO profile has expired
- **THEN** the system reports an expired-session condition naming the profile
  and its SSO session, distinct from a permission failure, and does not itself
  begin a sign-in

#### Scenario: Assume-role chain

- **WHEN** the selected profile declares `role_arn` with a `source_profile`
- **THEN** the source profile's credentials are used to assume the role, and
  the resulting temporary credentials are used for subsequent calls

#### Scenario: Profile without a region

- **WHEN** the selected profile declares no region and none is set in the
  environment
- **THEN** the system reports a missing-region configuration error naming the
  profile

### Requirement: A connection that cannot authenticate is not offered as usable

WHEN establishing a connection fails because its credentials could not be
resolved, were refused, or belong to a session that is no longer valid, the
system SHALL present that connection as unavailable, stating the cause and what
would make it usable.

The system SHALL keep it visible rather than hiding it, and SHALL NOT present
its failure as though the account had no buckets.

WHERE the cause is a session that expired or was never obtained, and the
profile declares where to sign in, the system SHALL offer signing in as an
action in the same place the cause is stated, rather than only naming a command
to run elsewhere.

WHERE the profile does not declare where to sign in, the system SHALL say so as
the cause, rather than offering an action that cannot succeed.

#### Scenario: A session that is no longer valid

- **WHEN** a connection's session is refused by the service
- **THEN** the connection is shown as unavailable with that cause, and the user
  is told what re-establishes it

#### Scenario: Re-establishing the session is offered where the cause is stated

- **WHEN** a connection is unavailable because its session expired, and its
  profile declares an `sso_session`
- **THEN** signing in is offered as an action beside the stated cause, and
  taking it begins a sign-in the user asked for

#### Scenario: An action that could not succeed is not offered

- **WHEN** a connection is unavailable because its session expired, and its
  profile declares no `sso_session`
- **THEN** the missing declaration is stated as the cause, and no sign-in action
  is offered

#### Scenario: The connection remains listed

- **WHEN** a connection cannot authenticate
- **THEN** it stays in the list of connections, marked, rather than disappearing
  or appearing to hold nothing
