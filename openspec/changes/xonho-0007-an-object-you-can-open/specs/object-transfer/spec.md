## ADDED Requirements

### Requirement: Downloading an object to a chosen destination

The system SHALL download a selected object's content to a destination
directory the user chooses, writing a file whose content is byte-identical to
the object.

The download SHALL NOT block interaction: the interface remains responsive
while the transfer is in flight.

#### Scenario: A successful download

- **WHEN** the user downloads an object to a directory they can write to
- **THEN** a file with the object's full content exists in that directory
  when the transfer reports itself finished, and not before

#### Scenario: The destination cannot be written

- **WHEN** the chosen destination refuses the write — permission, disk full,
  or a path the filesystem rejects
- **THEN** the failure is reported with its cause, and no partial file remains
  at the destination

#### Scenario: The object cannot be read

- **WHEN** the credentials may not read the object, or the service fails the
  read
- **THEN** the failure is reported with its classified cause, in the same
  vocabulary connection and listing failures already use

### Requirement: A partial download is never mistakable for the file

The system SHALL ensure the destination path never holds a partially written
file: content arrives at a working path and is moved to the final name only
when complete. A transfer that fails or is cancelled SHALL leave no file at
the final path, and SHALL NOT leave working files behind on the happy path.

#### Scenario: Interrupted mid-transfer

- **WHEN** a download fails or is cancelled after some bytes have arrived
- **THEN** the final path holds either nothing or a file from before the
  transfer began — never a truncated copy

### Requirement: Opening an object with the operating system

The system SHALL open an object on request by downloading it to a location the
application manages and handing the resulting file to the operating system's
default opener for it. The application SHALL NOT attempt to render the content
itself under this operation.

A failure to *open* after a successful download SHALL be reported as that —
the file exists and the report says where it is — never as a failed download.

#### Scenario: Opening a file kind the OS knows

- **WHEN** the user opens an object whose kind the machine has a default
  application for
- **THEN** the object is downloaded to an application-managed location and
  that application is asked to open it

#### Scenario: The OS has no opener for it

- **WHEN** the opener refuses or nothing is registered for the file's kind
- **THEN** the user is told the file was retrieved and where it is, and the
  event is not presented as a transfer failure

### Requirement: One transfer is visible and abandonable

The system SHALL present a transfer in flight: that it is running, and its
progress against the object's size when the service stated one. The user SHALL
be able to cancel it, and cancellation honors the partial-file rule.

#### Scenario: Progress against a stated size

- **WHEN** a download is in flight for an object whose size is known
- **THEN** the interface shows bytes transferred against that size

#### Scenario: Cancelled by the user

- **WHEN** the user cancels a download in flight
- **THEN** the transfer stops, the interface says so, and no file appears at
  the final destination

### Requirement: Keys map to filenames deterministically and losslessly in effect

S3 keys admit names filesystems refuse — reserved characters, trailing
separators, names differing only by case on case-insensitive volumes. The
system SHALL map keys to local names by a single deterministic scheme, SHALL
apply it identically on every platform the application ships for, and SHALL
report every substitution or collision the mapping performs. Two distinct keys
SHALL NOT silently produce one local file.

#### Scenario: A key a filesystem refuses

- **WHEN** an object's name carries characters the destination filesystem
  rejects
- **THEN** the file is written under the scheme's substituted name and the
  substitution is reported to the user

#### Scenario: Two keys, one candidate filename

- **WHEN** two downloads in the same destination would produce the same local
  name — by case folding or by substitution
- **THEN** both files exist under distinguishable names and the collision is
  reported, rather than the second write replacing the first

### Requirement: An existing local file is the user's decision

The system SHALL NOT overwrite an existing file at a download destination on
its own. When the final name is already taken, the user decides: replace,
keep both, or abandon.

#### Scenario: The name is already taken

- **WHEN** a download's final name already exists in the destination
- **THEN** nothing is written over it until the user chooses, and choosing
  keep-both produces a name that does not collide

### Requirement: Managed locations do not accumulate

Files downloaded only to be opened land in a location the application manages,
scoped to this application. The system SHALL bound what that location
accumulates across sessions: what an open leaves behind is a cache, not an
archive, and clearing it must never be the user's job.

#### Scenario: Opens across sessions

- **WHEN** the user has opened objects in earlier sessions
- **THEN** the managed location's contents from those sessions are bounded —
  reclaimed by the application, not left to grow until the disk objects
