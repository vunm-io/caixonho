## ADDED Requirements

### Requirement: Deleting is two deliberate acts

The system SHALL delete an object only through a confirmation that names the
exact key being deleted, presented as its own surface after an explicit
Delete action. The initial action SHALL NOT delete anything, and dismissing
the confirmation SHALL delete nothing.

The confirmation SHALL state the operation in plain destructive terms, and
SHALL NOT promise reversibility it has not established.

#### Scenario: The confirmation names the key

- **WHEN** the user invokes Delete on a selected object
- **THEN** nothing is deleted, and a confirmation appears naming that
  object's key and asking for the second act

#### Scenario: Declining deletes nothing

- **WHEN** the user dismisses the confirmation
- **THEN** the object is untouched and the confirmation is gone

#### Scenario: Confirming deletes exactly the named object

- **WHEN** the user confirms
- **THEN** the delete is issued for exactly the key the confirmation named,
  and the outcome is reported

### Requirement: The undo the service offers is surfaced, and only then

When the service's response to a delete states that a delete marker was
created, the system SHALL say so and SHALL offer to undo — removing that
marker by its version id, restoring the object. When the response states no
marker, the system SHALL NOT offer an undo and SHALL report the deletion as
permanent.

The undo offer SHALL be derived from the delete's own response, never from
an assumption about the bucket.

#### Scenario: A versioned bucket's delete carries its undo

- **WHEN** a delete's response reports a delete marker and its version id
- **THEN** the outcome says a marker was placed and offers Undo

#### Scenario: Undo restores the object

- **WHEN** the user invokes Undo on that outcome
- **THEN** the marker is removed by its version id and the object is listed
  again

#### Scenario: An unversioned delete is called what it is

- **WHEN** a delete's response reports no marker
- **THEN** the outcome states the object is gone, and no Undo is shown

#### Scenario: Undo refused is a classified refusal

- **WHEN** the credentials may not remove the marker
- **THEN** the failure names the permission it required, in the vocabulary
  every other refusal uses, and the outcome does not claim the object was
  restored

### Requirement: A delete that fails changes nothing on screen

A refused or failed delete SHALL be reported with its classified cause, and
the object's row SHALL remain. The listing SHALL be re-read from the service
after a successful delete, so what leaves the screen leaves because the
service said so.

#### Scenario: The delete is refused

- **WHEN** the credentials may not delete the object
- **THEN** the refusal names the permission it required, and the row remains

#### Scenario: The listing catches up after success

- **WHEN** a delete succeeds
- **THEN** the location is re-read and the row is gone from the fresh listing

### Requirement: Delete outcomes are logged without an inventory

The log SHALL record each delete and undo outcome — the bucket, whether a
marker was involved, and the classified cause on failure — and SHALL NOT
record the object's key, at any detail level, per the standing no-inventory
requirement.

#### Scenario: A logged delete

- **WHEN** a delete or an undo settles and is logged
- **THEN** the entry carries the bucket and the outcome, and no key in any
  representation
