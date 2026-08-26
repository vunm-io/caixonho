## ADDED Requirements

### Requirement: The destination of an upload is chosen, not derived

The system SHALL offer the destination key before an upload is sent, filled
in with the location the user is standing in followed by the local file's own
name, and SHALL allow it to be changed — so that a file can be placed
anywhere in the bucket without first navigating there, and so that a folder
can be brought into existence by writing into it.

The system SHALL send exactly the destination shown.

#### Scenario: The default is where the user is

- **WHEN** the user chooses a local file to upload from a location
- **THEN** the destination offered is that location's prefix followed by the
  file's own name

#### Scenario: A destination typed into a path that does not exist yet

- **WHEN** the user sends a file to a destination whose prefix has no objects
  under it
- **THEN** the object is written at that key, and the location now lists the
  folders the key implies

#### Scenario: What is shown is what is sent

- **WHEN** the user edits the destination and confirms
- **THEN** the key written is the one displayed, and no part of it is
  recomposed from the location or the file name

### Requirement: A destination that cannot be a key is refused before it is sent

The system SHALL refuse a destination that cannot name an object, saying
which rule it broke, and SHALL send nothing — so that a mistake costs a
sentence rather than a request and an unexplained failure.

#### Scenario: A destination naming no object

- **WHEN** the destination is empty, or ends in `/` so that it names a folder
  rather than an object
- **THEN** the upload is refused with the reason, and nothing is sent

#### Scenario: A destination that starts at the root

- **WHEN** the destination begins with `/`
- **THEN** it is refused, because a leading separator makes an object whose
  name begins with an empty folder — legal in S3, and never what was meant
