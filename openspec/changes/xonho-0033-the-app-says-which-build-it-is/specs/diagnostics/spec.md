## ADDED Requirements

### Requirement: The application states which build it is

The system SHALL state, in its own window and without being asked, which build
of itself is running — and SHALL do so precisely enough to tell two builds
apart that carry the same released version.

This capability exists so that a failure can be explained by someone who was
not watching when it happened. The first thing such a person needs is which
build failed, and it is the one fact the user cannot look up: a version is not
visible in a file listing, a bundle's own metadata is not where anyone thinks
to look, and two downloads of different releases can carry identical names.

The statement SHALL be visible in ordinary use rather than behind a menu, on
the same reasoning as the log's location: the moment it is wanted is a moment
something has already gone wrong, and searching for it then is the worst time.

A build made where the source revision cannot be determined SHALL say so,
rather than omit it or present a value it did not read.

#### Scenario: Reporting a problem

- **WHEN** a user is asked what build they are running
- **THEN** the answer is on screen, and identifies the build precisely enough
  that the person receiving the report can obtain the same one

#### Scenario: Two builds of the same release

- **WHEN** two builds are made from the same released version but different
  source revisions
- **THEN** what the window states differs between them

#### Scenario: Built outside a repository

- **WHEN** the application is built where the source revision cannot be read
- **THEN** it still states its version, and says the revision is unknown rather
  than claiming one

### Requirement: One source states the version

The system SHALL derive every version it presents — in its window, in
platform metadata, and in the names of the files it distributes — from a single
declared source.

A second place stating a version is a second place to be wrong, and the ways it
goes wrong are silent: platform metadata is read by the operating system rather
than by a person, and a file name is read long after anyone could check it.

#### Scenario: The version is raised

- **WHEN** the declared version is changed
- **THEN** what the window says, what the platform metadata carries, and what a
  distributed file is called all change with it, without any of them being
  edited separately

#### Scenario: A distributed file is identified

- **WHEN** someone holds a downloaded file and needs to know which release it is
- **THEN** its name says so, without their having to open it or compute a
  checksum
