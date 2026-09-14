## ADDED Requirements

### Requirement: A folder is downloaded as one act

The system SHALL let the user download a folder — every object under its
prefix, at every depth — as a single act rather than as a sequence of choices
made object by object.

The act SHALL ask once where its contents are to go. Asking per object is what
makes fetching a subtree impractical, and impracticality is the defect.

The act SHALL NOT be bounded by the ceiling that guards deleting a prefix. That
ceiling exists because an unbounded destructive act needs a bound; a download
destroys nothing, and abandoning one costs only the bytes already fetched.

#### Scenario: Fetching a subtree

- **WHEN** the user downloads a folder holding objects at more than one depth
- **THEN** every object under that prefix is fetched, and the user is asked for
  a destination once

#### Scenario: A prefix larger than a deletion may be

- **WHEN** the folder holds more objects than a bulk delete would accept
- **THEN** the download proceeds, rather than being refused

### Requirement: A selection is downloaded as one act

The system SHALL let the user download the rows they have chosen — objects,
folders, or a mixture — as a single act with one destination.

#### Scenario: Objects and folders together

- **WHEN** the selection holds both objects and folders
- **THEN** the objects and every object under the chosen folders are fetched
  as one act

### Requirement: A folder arrives as a folder

The system SHALL reproduce, under the chosen destination, the prefix structure
the objects had in the bucket, relative to the location the act began at.

A subtree flattened into one directory is not the subtree. It also creates
collisions that did not exist: two objects distinguished only by their prefix
would contend for a single local name.

The mapping SHALL NOT produce a path outside the chosen destination, whatever
the key contains.

#### Scenario: Depth is kept

- **WHEN** an object at `daily/monday.csv` is fetched from the bucket's root
- **THEN** it is written at `daily/monday.csv` under the chosen destination,
  and the directory is created

#### Scenario: A key that tries to climb

- **WHEN** a key contains a segment that would name a parent directory
- **THEN** the file is written inside the chosen destination under a name the
  scheme substitutes, and never above it

## MODIFIED Requirements

### Requirement: Keys map to filenames deterministically and losslessly in effect

S3 keys admit names filesystems refuse — reserved characters, trailing
separators, names differing only by case on case-insensitive volumes. The
system SHALL map keys to local **paths** by a single deterministic scheme,
SHALL apply it identically on every platform the application ships for, and
SHALL report every substitution or collision the mapping performs. Two distinct
keys SHALL NOT silently produce one local file.

Every segment of the key SHALL be mapped by the same rules, not only the last:
a segment that cannot serve as a directory name is subject to the same
substitution and the same deterministic suffix as one that cannot serve as a
file name.

#### Scenario: A key a filesystem refuses

- **WHEN** an object's name carries characters the destination filesystem
  rejects
- **THEN** the file is written under the scheme's substituted name and the
  substitution is reported to the user

#### Scenario: A directory a filesystem refuses

- **WHEN** a key's intermediate segment cannot serve as a directory name
- **THEN** that segment is substituted by the same scheme, and what is reported
  names the object rather than the segment

#### Scenario: Two keys, one candidate filename

- **WHEN** two downloads in the same destination would produce the same local
  name — by case folding or by substitution
- **THEN** both files exist under distinguishable names and the collision is
  reported, rather than the second write replacing the first

### Requirement: A collision is answered per transfer

The system SHALL ask a collision question about the transfer it belongs to,
and SHALL NOT let an answer given for one transfer decide an unrelated one —
so that "replace" chosen for one file cannot silently overwrite a second.

An answer MAY cover the remaining transfers **of the act it was asked within**,
and only when the user chooses that explicitly. The prohibition is on an answer
deciding a transfer the user did not have in mind; two hundred files fetched by
one gesture are one thing the user had in mind, and asking two hundred times is
a question nobody finishes answering.

An answer SHALL NOT outlive the act that carried it.

#### Scenario: Two transfers meet a taken key

- **WHEN** two transfers started separately each find something already at
  their destination
- **THEN** each is asked about separately, and answering one leaves the other
  waiting for its own answer

#### Scenario: An answer offered for the rest of one act

- **WHEN** a transfer belonging to a multi-object act meets a taken
  destination, and the user answers and asks that it stand for the rest
- **THEN** the remaining transfers of that act take the same answer without
  asking again, and transfers outside that act are unaffected

#### Scenario: A later act asks again

- **WHEN** a new download act meets a taken destination after an earlier act
  was answered for all of its transfers
- **THEN** the user is asked again, because the earlier answer belonged to the
  earlier act

#### Scenario: A transfer waiting on an answer holds no slot

- **WHEN** a transfer is waiting for the user to answer a collision
- **THEN** it is not occupying a concurrency slot, and other transfers may run
