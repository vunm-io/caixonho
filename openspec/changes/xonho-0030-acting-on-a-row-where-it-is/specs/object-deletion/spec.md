## ADDED Requirements

### Requirement: More than one object can be deleted at once

The system SHALL let the user select several rows and delete them together,
and SHALL run those deletes through the same bounded queue a transfer uses —
so that one refusal does not stop the rest and the user is told what became of
each.

#### Scenario: Several objects deleted together

- **WHEN** the user selects several objects and deletes them
- **THEN** each is deleted, and the outcome of each is reported separately

#### Scenario: One of many is refused

- **WHEN** one object in a selection cannot be deleted
- **THEN** the others are still deleted, and the one that failed keeps its own
  cause

### Requirement: A confirmation for more than one states how many

WHEN a delete would remove more than one object, the confirmation SHALL state
the **number of objects** it would remove, and SHALL NOT proceed until it is
confirmed — because a mistake here multiplies, which is the whole reason the
count is required rather than a list the eye skims.

WHEN a delete would remove exactly one object, the confirmation SHALL name
that object's key, as it does today: a count of one is weaker than a name.

#### Scenario: Deleting a selection

- **WHEN** the user confirms deleting several objects
- **THEN** the confirmation stated how many would go before anything was sent

#### Scenario: Deleting one object

- **WHEN** the selection is a single object
- **THEN** the confirmation names its exact key rather than saying "1 object"

### Requirement: Deleting a folder counts it first

A folder is not a thing the service holds; it is every object under a prefix.
The system SHALL count those objects **before** asking, SHALL state that count
in the confirmation, and SHALL NOT begin deleting while still counting — so
that nobody confirms a number the application has not yet finished working out.

#### Scenario: A folder is deleted

- **WHEN** the user deletes a folder
- **THEN** the objects under its prefix are counted, the confirmation states
  how many, and only confirming begins the deleting

#### Scenario: Counting is still in progress

- **WHEN** the count has not finished
- **THEN** the confirmation says so and cannot be confirmed yet

#### Scenario: A folder that turns out to hold nothing

- **WHEN** the prefix holds no objects
- **THEN** the user is told there is nothing to delete rather than shown a
  confirmation for zero

### Requirement: A bulk delete says that Undo is not offered

WHEN more than one object has been deleted, the system SHALL NOT offer Undo,
and SHALL say that it is not offered — because restoring many objects is many
markers and a partial restore, and an Undo that might half-work is worse than
none.

#### Scenario: After deleting several

- **WHEN** several objects have been deleted on a versioned bucket
- **THEN** no Undo is offered, and the outcome says that undoing a bulk delete
  is not something this application does

### Requirement: A destructive action is not reachable by a stray click

The system SHALL NOT place a delete control where a single accidental click
can reach it — no hover control and no double-click. Deleting SHALL be reached
deliberately, and the confirmation is a second step rather than the only one.

#### Scenario: The row's actions

- **WHEN** the user brings up a row's actions
- **THEN** deleting is there, having been reached on purpose
