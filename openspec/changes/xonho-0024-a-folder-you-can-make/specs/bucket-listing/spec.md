## ADDED Requirements

### Requirement: A folder can be made where the user is standing

The system SHALL let the user make a folder at the location currently shown,
by giving it a name, and SHALL show it in that location once it exists — so
that organising objects does not require leaving the application.

The system SHALL refuse a name that cannot be one, saying which rule it
broke, before any request is sent.

#### Scenario: Making a folder in a general purpose bucket

- **WHEN** the user makes a folder named `reports` at a location in a general
  purpose bucket
- **THEN** a zero-byte object is written whose key is that location's prefix
  followed by `reports/`, and the folder appears in the listing

#### Scenario: A name that cannot be a folder

- **WHEN** the user gives a name that is empty, contains `/`, or already
  names something at that location
- **THEN** the attempt is refused with the reason, and nothing is sent to the
  service

#### Scenario: The folder is made where the user is, not at the root

- **WHEN** the user is inside a prefix and makes a folder
- **THEN** the new folder is created inside that prefix

### Requirement: An empty folder is not offered where it cannot exist

A directory bucket removes a directory as soon as it becomes empty, so a
folder made with nothing in it does not survive there. The system SHALL NOT
offer to make an empty folder on a directory bucket, and SHALL say why and
what does work instead — so that the application never reports as done
something the service has already undone.

#### Scenario: Asking for a folder on a directory bucket

- **WHEN** the user asks to make a folder at a location in a directory bucket
- **THEN** the system does not claim to have made one, states that a
  directory bucket keeps a folder only while something is in it, and offers
  the act that does create it

#### Scenario: The two kinds are told apart before anything is sent

- **WHEN** a location's bucket kind is already known from the listing
- **THEN** which of the two behaviours applies is decided from that kind, not
  from the service's refusal of an attempt
