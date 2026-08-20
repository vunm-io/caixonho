## Purpose

Reading what a bucket contains: the prefixes that stand in for folders, the
objects that are its contents, and the honesty owed to a user about a hierarchy
that the service does not actually have.

## ADDED Requirements

### Requirement: A bucket's contents can be read one page at a time

The system SHALL list what a bucket holds under a given prefix, returning the
prefixes directly beneath it and the objects directly within it, and SHALL do
so in pages rather than gathering everything before presenting anything.

The system SHALL make it possible to tell, from a page, whether more remains,
and SHALL request the remainder only as it is needed.

#### Scenario: Opening a bucket

- **WHEN** the user opens a bucket
- **THEN** the first page of its contents is presented, and nothing waits on
  pages that have not been asked for

#### Scenario: More than one page

- **WHEN** a location holds more entries than one page carries
- **THEN** the interface says that more is still to come, and the next page is
  requested as the user reaches the end of what is shown

#### Scenario: Reading does not block the interface

- **WHEN** a page is being fetched
- **THEN** the window continues to respond, and what has already been read
  stays readable

### Requirement: Folders are inferred, and the inference is not disguised

S3 stores keys, not directories. The system SHALL derive folders from the
prefixes the service reports, and SHALL NOT present a derived folder as though
it were a stored object.

The system SHALL treat an entry whose key is exactly the current prefix as that
folder itself, and SHALL NOT present it as an entry within itself.

Where an object and a prefix share a name, the system SHALL present both, each
as what it is.

#### Scenario: A folder that no object stands behind

- **WHEN** a location contains only keys nested beneath a prefix, with no
  object at the prefix itself
- **THEN** the folder is presented and can be entered, and the columns that
  describe an object are empty for it rather than filled with substitutes

#### Scenario: A folder created by another tool

- **WHEN** a zero-length object whose key ends in a separator exists, as tools
  that offer "create folder" write
- **THEN** entering that folder does not present it as an entry inside itself

#### Scenario: An object named like a folder

- **WHEN** an object and a prefix share the same name at the same location
- **THEN** both are presented, one openable and one not, and neither is
  concealed by the other

### Requirement: The application states where it is

The system SHALL hold one location — a connection, a bucket and a prefix — as
the single answer to where the user is, and SHALL derive what it displays about
that location from it rather than maintaining a separate record.

The system SHALL present the trail from the bucket to the current prefix, and
SHALL allow any step of that trail to be returned to.

#### Scenario: Moving into a folder

- **WHEN** the user enters a folder
- **THEN** the displayed trail extends by that folder, and the contents shown
  are that folder's

#### Scenario: Returning to an ancestor

- **WHEN** the user selects an earlier step of the trail
- **THEN** the location becomes that step, and what is shown is that step's
  contents

### Requirement: A location can be given directly

The system SHALL accept a location entered as text, in the form the service's
own addressing uses, and SHALL go there.

WHEN the text does not name a location the system can go to, the system SHALL
say so and SHALL leave the current location as it was.

#### Scenario: Typing a path

- **WHEN** the user enters a bucket and prefix as text
- **THEN** the application goes to that location and lists it

#### Scenario: A bucket the account cannot enumerate

- **WHEN** the credentials may work inside a bucket but may not list the
  account's buckets, and the user enters that bucket by name
- **THEN** the bucket is opened and its contents listed, without a listing of
  the account being required first

#### Scenario: Text that names nowhere

- **WHEN** the entered text cannot be resolved to a location
- **THEN** the user is told, and the location already open is unchanged

### Requirement: An empty location and a refused location are never alike

WHEN listing a location fails because the credentials are not permitted to read
it, the system SHALL present that as a refusal, naming the cause, and SHALL NOT
present it as a location that holds nothing.

The system SHALL apply to a prefix the same distinction it applies to a bucket:
a refusal is a refusal, an expired session is an expired session, a network
failure is a network failure, and none of them is emptiness.

#### Scenario: A prefix the credentials cannot read

- **WHEN** listing a prefix is refused on authorization grounds
- **THEN** the refusal is presented with its cause, and the location is not
  drawn as empty

#### Scenario: A location that genuinely holds nothing

- **WHEN** a location is readable and contains no prefixes and no objects
- **THEN** it is presented as empty, and that is distinguishable from a refusal

#### Scenario: A failure that is neither

- **WHEN** listing fails because the session has expired, or the network could
  not be reached
- **THEN** that cause is presented as itself, and never as a refusal or as
  emptiness

### Requirement: What is known about a location follows the credentials

The system SHALL scope what it has observed about a location to the credentials
that observed it, and SHALL NOT carry an observation made with one connection
into another.

#### Scenario: Changing connection

- **WHEN** the user changes to a different connection
- **THEN** what was observed about locations under the previous connection is
  not presented as true of the new one
