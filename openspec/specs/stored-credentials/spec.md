## Purpose

Credentials the application holds on the user's behalf, so that connecting does
not require editing a file by hand or installing another tool first. What is
secret about them never leaves the operating system's own store.

## Requirements

### Requirement: A credential can be entered in the application

The system SHALL accept an access key id, a secret access key and an optional
session token, together with a name for the connection they belong to and the
region it uses, and SHALL make that connection available for use without any
change to files outside the system credential store.

#### Scenario: Entering a credential

- **WHEN** the user supplies an access key id, a secret access key and a region
- **THEN** a connection under the given name becomes available to select

#### Scenario: A temporary credential

- **WHEN** the credential includes a session token
- **THEN** the session token is stored and used with it

### Requirement: Secrets live only in the operating system credential store

The system SHALL keep the secret access key and the session token in the
operating system's credential store, and SHALL NOT write them to any
configuration file, log, crash report, error message or diagnostic output.

The system SHALL NOT write them to the AWS shared credentials file: a credential
entered here belongs to this application, and silently editing a file shared
with other tools is not something the user asked for.

#### Scenario: Everything except the secret is ordinary configuration

- **WHEN** a stored credential is saved
- **THEN** its name, region and access key id may be kept as ordinary
  configuration, and the secret access key and session token are in the
  credential store only

#### Scenario: A failure that mentions credentials

- **WHEN** any operation using a stored credential fails and is reported
- **THEN** the report contains no secret access key and no session token,
  whatever the cause

### Requirement: A stored credential can be forgotten

The system SHALL let the user remove a stored credential, and SHALL delete what
it holds in the credential store when it does.

A connection that has been forgotten SHALL NOT continue to be offered.

#### Scenario: Forgetting a connection

- **WHEN** the user removes a stored connection
- **THEN** its secret is deleted from the credential store and the connection is
  no longer offered

### Requirement: The credential store may be unavailable

WHEN the operating system's credential store cannot be reached — locked,
refused, or absent — the system SHALL report that as its own cause, and SHALL
NOT fall back to storing the secret anywhere else.

#### Scenario: The store refuses

- **WHEN** saving a credential fails because the credential store refused
- **THEN** the user is told the credential was not saved and why, and nothing is
  written elsewhere
