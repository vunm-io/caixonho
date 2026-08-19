## Purpose

What the application records about its own behaviour so that a failure can be
explained after it has happened, by someone who was not watching when it did —
and the rules that keep that record from becoming a place secrets end up.

## ADDED Requirements

### Requirement: The application keeps a log on this machine

The system SHALL write a record of its own significant events to a file on the
machine it runs on, and SHALL be able to state where that file is.

The record SHALL be enough to explain a failure after the fact: which
connection was used, what was attempted, and the cause the system decided on.

#### Scenario: A failure is explained afterwards

- **WHEN** an operation has failed and the user reports it later
- **THEN** the log names the connection, what was attempted, and the cause the
  system settled on

#### Scenario: Finding the file

- **WHEN** the user is asked to send the log
- **THEN** the application can tell them the path to it without their needing to
  know the platform's conventions

### Requirement: No secret is ever written to the log

The system SHALL NOT write a secret access key, a session token, a password, a
presigned URL or an `Authorization` header to the log, at any level of detail,
for any event, whatever the cause.

A value that cannot be written SHALL NOT be written in a different encoding
either — as raw bytes, or escaped by the log's own format.

#### Scenario: A failure involving a credential

- **WHEN** any operation using a credential fails and the failure is logged
- **THEN** the log contains no secret access key and no session token, in any
  representation

#### Scenario: The most detailed setting

- **WHEN** logging is turned up to its most detailed level
- **THEN** the rule still holds: more detail never means more secret

### Requirement: Detail is a choice, and the default is modest

The system SHALL log at a level that records decisions rather than every step,
and SHALL allow that level to be raised for an investigation without rebuilding
the application.

Third-party diagnostics SHALL be quiet unless asked for: at their most detailed
they carry request material the user did not ask to have written down.

#### Scenario: Ordinary use

- **WHEN** the application runs without being asked for more detail
- **THEN** it records its own decisions, and the underlying libraries record
  only their warnings and failures

#### Scenario: Investigating a problem

- **WHEN** the user raises the level of detail
- **THEN** more is recorded, including from the underlying libraries

### Requirement: The log cannot grow without limit

The system SHALL bound what it keeps, so that a long-running or repeatedly
failing session cannot fill the disk.

#### Scenario: A session that fails repeatedly

- **WHEN** an operation fails over and over for a long time
- **THEN** the log is bounded, and what is kept is the most recent

### Requirement: Logging never takes the application down

WHEN the log cannot be opened or written — no location, no permission, a full
disk — the system SHALL continue to run without it.

#### Scenario: The log file cannot be created

- **WHEN** the log's location cannot be written to
- **THEN** the application starts and works, and the absence of a log is not
  presented as a failure of the thing the user was doing
