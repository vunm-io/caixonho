## ADDED Requirements

### Requirement: Signing in is something the user asks for

The system SHALL begin a sign-in only in response to an explicit act by the
user, and SHALL NOT begin one because a connection was selected, because a
listing failed, or because the application started.

The system SHALL open the verification page in the user's browser as part of a
sign-in the user asked for, and SHALL NOT open a browser at any other time.

#### Scenario: Selecting a connection does not sign in

- **WHEN** the user selects a connection whose session has expired
- **THEN** the connection is reported unavailable with its cause, no browser
  opens, and no request is made to the identity provider

#### Scenario: Startup does not sign in

- **WHEN** the application starts with connections whose sessions have all
  expired
- **THEN** no sign-in begins and no browser opens

### Requirement: A session can be obtained from within the application

The system SHALL obtain an IAM Identity Center session using the OIDC device
authorization flow, without requiring the AWS CLI or any other external program
to be installed.

The flow SHALL use the start URL, region and scopes declared by the connection's
`sso_session`, and SHALL fail with a stated cause when the profile does not
carry them rather than guessing a value.

#### Scenario: A session is obtained

- **WHEN** the user asks to sign in to a connection whose profile declares an
  `sso_session`, and completes the authorization in the browser
- **THEN** the system holds a valid session for that connection, and the
  connection becomes usable without the application being restarted

#### Scenario: The profile does not say where to sign in

- **WHEN** the user asks to sign in to a connection whose profile carries no
  `sso_session` name, start URL or region
- **THEN** the system reports that the profile does not declare where to sign
  in, naming what is missing, and no request is made to any provider

### Requirement: What is happening is visible while it is happening

WHILE a sign-in is in progress the system SHALL show the user code and the
verification address, SHALL state that it is waiting for the browser, and SHALL
offer a way to abandon the attempt.

The user code SHALL remain readable for as long as the attempt is alive, so a
user whose browser did not open, or who is completing the step on another
device, can still finish it.

#### Scenario: The browser did not open

- **WHEN** a sign-in is in progress and the browser did not open
- **THEN** the verification address and the user code are shown in the
  application in a form the user can read and copy

#### Scenario: Abandoning an attempt

- **WHEN** the user abandons a sign-in that is in progress
- **THEN** polling stops, no session is written, and the connection returns to
  the state it was in before

### Requirement: The session is written where the rest of the system reads it

The system SHALL write the obtained session to the AWS CLI token cache
(`~/.aws/sso/cache`) in the format that cache already uses, so that credential
resolution finds it by the path it already uses and no second credential path
exists.

A write SHALL be atomic and SHALL NOT leave a partial or malformed entry
behind, because that directory is shared with the AWS CLI and a corrupt entry
would break a tool this application does not own.

The token SHALL be treated as a secret: never logged in any spelling, never
written to the connections file, and never carried into any diagnostic output.

#### Scenario: A newly obtained session is usable immediately

- **WHEN** a sign-in completes
- **THEN** the session is present in the CLI token cache, and opening the
  connection resolves credentials through the existing provider chain without
  further interaction

#### Scenario: A failed write does not corrupt the cache

- **WHEN** writing the session fails part-way
- **THEN** no partial entry is left in the cache, any pre-existing entry is
  intact, and the failure is reported with its cause

#### Scenario: The token never appears in the log

- **WHEN** a sign-in completes with logging raised to its most verbose level
- **THEN** no access token, refresh token or client secret appears in the log
  in readable, byte-array or escaped form

### Requirement: Each way a sign-in fails is told apart

The system SHALL distinguish, and report by cause: an authorization the user
declined, an attempt that expired before it was completed, a network or
provider failure, and a profile that does not declare where to sign in.

A sign-in that fails SHALL leave the connection exactly as it was, and SHALL
NOT be reported as a permission failure of the account.

#### Scenario: The user declines

- **WHEN** the user declines the authorization in the browser
- **THEN** the system reports that the authorization was declined, and does not
  keep polling

#### Scenario: The attempt expires

- **WHEN** the device code expires before the user completes the authorization
- **THEN** the system reports that the attempt expired and offers to start
  another, rather than reporting a credential or permission problem

#### Scenario: Polling honours the interval the provider asked for

- **WHEN** the provider states a polling interval, or answers that polling is
  too frequent
- **THEN** the system waits at least the stated interval between attempts, and
  backs off further when told to
