## ADDED Requirements

### Requirement: More than one transfer may be in flight

The system SHALL accept more transfers than it runs at once, SHALL run up to a
bounded number concurrently, and SHALL start a waiting transfer as a slot frees
— so that choosing several files is one act rather than a sequence of waits.

The bound SHALL exist and SHALL be small. Sending everything at once is how a
service starts refusing.

#### Scenario: More work than slots

- **WHEN** the user starts more transfers than the bound allows
- **THEN** the first fill the slots, the rest are listed as waiting, and each
  waiting one starts as an earlier one ends

#### Scenario: Every transfer is accounted for

- **WHEN** transfers are running and waiting
- **THEN** each is listed with what it is, where it is going, and its own
  progress — and the queue says how many have finished of how many there are

### Requirement: One failure does not stop the others

The system SHALL contain a failed transfer to itself: the remaining transfers
SHALL continue, and the failed one SHALL stay listed with the cause and the
means to try again — so that the twentieth file is not lost because the fourth
was.

#### Scenario: One of many fails

- **WHEN** one transfer fails while others are running or waiting
- **THEN** the others are unaffected, and the failed one is listed with its
  cause

#### Scenario: Retrying what failed

- **WHEN** the user retries a failed transfer
- **THEN** it re-enters the queue and is attempted again, without disturbing
  transfers already in flight

### Requirement: A queue can be acted on as a whole

The system SHALL let the user cancel a single transfer, cancel everything not
yet finished, retry everything that failed, and clear everything that has
finished — and clearing SHALL remove only what has finished.

#### Scenario: Cancelling everything

- **WHEN** the user cancels the queue
- **THEN** running transfers stop, waiting transfers never start, and both are
  reported as cancelled rather than as failed

#### Scenario: Clearing what is done

- **WHEN** the user clears finished transfers
- **THEN** those that finished leave the list, and those running, waiting or
  failed remain

#### Scenario: A queue with nothing left in it

- **WHEN** every transfer has finished and been cleared
- **THEN** the panel says nothing rather than showing an empty frame

### Requirement: A collision is answered per transfer

The system SHALL ask a collision question about the transfer it belongs to,
and SHALL NOT let an answer given for one transfer decide another — so that
"replace" chosen for one file cannot silently overwrite a second.

#### Scenario: Two transfers meet a taken key

- **WHEN** two transfers each find something already at their destination
- **THEN** each is asked about separately, and answering one leaves the other
  waiting for its own answer

#### Scenario: A transfer waiting on an answer holds no slot

- **WHEN** a transfer is waiting for the user to answer a collision
- **THEN** it is not occupying a concurrency slot, and other transfers may run
