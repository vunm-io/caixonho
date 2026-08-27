## ADDED Requirements

### Requirement: Files dropped on the window are uploaded

The system SHALL accept files dropped onto the window and upload them to the
location on screen, each keeping its own name, and SHALL queue them as one act
rather than as a sequence the user must repeat.

#### Scenario: Several files dropped at once

- **WHEN** the user drops several files while inside a bucket
- **THEN** every one is taken on, each destined for the location on screen
  under its own name

#### Scenario: The window says a drop will land

- **WHEN** files are dragged over a place that will accept them
- **THEN** the window shows that it will, and where they will go

#### Scenario: Choosing several files through the picker

- **WHEN** the user chooses more than one file through `Upload…`
- **THEN** they are taken on exactly as a drop of the same files would be

### Requirement: A drop that cannot be honoured is refused out loud

The system SHALL refuse a drop it cannot act on and SHALL say why, rather than
accepting it and doing nothing — a drop that vanishes is indistinguishable
from an application that is broken.

#### Scenario: Dropped where there is no destination

- **WHEN** files are dropped while no bucket is open
- **THEN** nothing is uploaded and the window says a location is needed first

#### Scenario: A folder is dropped

- **WHEN** a folder is dropped rather than a file
- **THEN** it is refused with the reason, and nothing is uploaded — neither
  silently nothing, nor only the files at its top

### Requirement: The destination means a folder when there is more than one file

WHEN a single file is being sent, the destination SHALL be its full key,
editable. WHEN more than one is being sent, the destination SHALL be the
folder they share, and each file SHALL keep its own name.

The system SHALL make clear which of the two is being asked for.

#### Scenario: One file keeps a full destination

- **WHEN** one file is being sent
- **THEN** the destination offered is the whole key and may be changed
  entirely, including the file's name

#### Scenario: Many files share a folder

- **WHEN** several files are being sent
- **THEN** one folder is asked for, every file lands under it with its own
  name, and the screen says so before anything is sent

#### Scenario: A folder that cannot be one is refused

- **WHEN** the folder given for several files cannot name one
- **THEN** the refusal names the rule and nothing is sent
