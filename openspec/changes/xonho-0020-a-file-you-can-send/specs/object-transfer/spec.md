## ADDED Requirements

### Requirement: Sending a local file to a location

The system SHALL upload a local file the user chooses into the location on
screen, creating an object whose content is byte-identical to the file and
whose key is the location's prefix followed by the file's own name.

The upload SHALL NOT block interaction, and its progress state SHALL be
visible for as long as it runs.

#### Scenario: A successful upload

- **WHEN** the user uploads a readable local file into a location the
  credentials may write
- **THEN** an object with that content exists under the location's prefix,
  and the interface says the upload finished

#### Scenario: The local file cannot be read

- **WHEN** the chosen file cannot be read — permission, or it disappeared
  between choosing and sending
- **THEN** the failure is reported with its cause, and nothing was created in
  the bucket

#### Scenario: The service refuses the write

- **WHEN** the credentials may not write to that location, or the service
  fails the request
- **THEN** the failure is reported with its classified cause, in the same
  vocabulary connection, listing and download failures already use

### Requirement: An existing object is never replaced without being asked

The system SHALL NOT replace an object that already exists at the target key
on its own initiative. The protection SHALL be enforced by the request
itself — a conditional write that the service refuses — and not by a
separate existence check performed before an unconditional write, so that no
window exists in which another writer's object could be destroyed.

When the target key is taken, the user decides: replace it, keep both under a
name derived beside it, or abandon. Replacing SHALL be a distinct act, and
SHALL be the only circumstance in which this application overwrites an
object.

#### Scenario: The key is already taken

- **WHEN** an upload targets a key that already holds an object
- **THEN** nothing is written over it, and the user is told the key is taken
  and asked what to do

#### Scenario: The user chooses to replace

- **WHEN** the user answers the taken-key question with replace
- **THEN** the object at that key is replaced, and this is the only way that
  happens

#### Scenario: The user chooses to keep both

- **WHEN** the user answers with keep both
- **THEN** an object is created under a key that was free, the chosen key is
  reported, and the existing object is untouched

#### Scenario: The endpoint refuses the condition itself

- **WHEN** the endpoint answers that it does not implement the conditional
  write, rather than answering the write
- **THEN** the upload does not proceed, and the user is told that this
  endpoint cannot guarantee an existing object is left alone — proceeding
  without the guarantee is then their explicit act, not a fallback the
  system takes on its own

### Requirement: An upload in flight can be abandoned

The system SHALL let the user cancel an upload in flight. A cancelled upload
SHALL be reported as cancelled rather than as a failure, and SHALL NOT leave
the user believing an object was created when the request did not complete.

#### Scenario: Cancelled by the user

- **WHEN** the user cancels an upload in flight
- **THEN** the transfer stops, the interface says it was cancelled, and the
  interface does not claim an object was created

### Requirement: A file the request cannot carry is refused before it is sent

A single write request has a size the service will not exceed. The system
SHALL establish the file's size before sending and SHALL refuse a file above
that limit up front, naming what would be needed to send it — rather than
transferring bytes toward a refusal that is certain.

#### Scenario: A file larger than one request allows

- **WHEN** the user chooses a file larger than a single write request can
  carry
- **THEN** the upload does not start, and the reason names the size limit and
  what lifts it

### Requirement: A key that is taken is chosen deterministically, and said

Deriving a free key beside a taken one SHALL be deterministic and SHALL
produce a key the service accepts. The derived key SHALL be reported to the
user, because the object they sent is not where they would look for it by
name.

#### Scenario: Keep both, twice

- **WHEN** the user keeps both against the same taken key on two occasions
- **THEN** neither upload replaces the other, and each derived key is
  reported
