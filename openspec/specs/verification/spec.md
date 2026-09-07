# verification Specification

## Purpose
How this project proves that what it says about S3 is true of a real S3 implementation — over real HTTP, from the window's own controls — and where that proof stops, stated where the tests live so a green suite is never read as coverage it does not have. Created by archiving change xonho-0031-a-real-service-to-test-against.

## Requirements

### Requirement: The adapter is proven against a real service

The project SHALL exercise its S3 adapter against a running S3 implementation,
over real HTTP, and not only against test doubles.

A double answers **above** the adapter and a replayed response answers
**below** it. Neither can say whether the request the adapter builds means, to
a real service, what the project believes it means — and that is the layer
where a wrong parameter, a missing delimiter or a mishandled continuation token
lives.

The service SHALL be startable by the test itself, with nothing for a developer
to install and no daemon to run, so that a check nobody can run is not the
check this project relies on.

#### Scenario: A listing crosses the wire

- **WHEN** the adapter is asked for the contents of a location
- **THEN** the request reaches a real service and the folders and objects it
  answers with are the ones the adapter reports

#### Scenario: A guarantee the service makes, not the client

- **WHEN** an object is written to a key that is already taken, conditionally
- **THEN** the real service refuses it, and the adapter reports that refusal as
  the question the user is owed rather than as a failure

### Requirement: A flow is proven from the window to the service

The project SHALL exercise at least one complete flow per built capability from
the window's own controls through to a real service and back, so that what is
verified is the path a user takes rather than the units it is made of.

#### Scenario: Deleting several objects

- **WHEN** rows are ticked in the window and the delete is confirmed
- **THEN** the objects are gone from the real service, and the window's outcome
  matches what the service actually did

### Requirement: What the local service cannot prove is written down

The project SHALL state, where the tests live, which behaviours a local service
cannot exercise — so that a passing suite is never mistaken for coverage it
does not have.

This is a requirement rather than a courtesy. The behaviours it excludes are
the ones this application exists for, and a reader who assumes the suite covers
them will stop checking the one thing that most needs checking.

#### Scenario: Reading the suite

- **WHEN** someone reads what the integration tests cover
- **THEN** directory buckets, versioning and denials are named as **not**
  covered, each with the reason

#### Scenario: A behaviour becomes coverable

- **WHEN** a local service gains one of the excluded behaviours
- **THEN** the exclusion is removed rather than left standing as folklore
