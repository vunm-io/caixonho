## ADDED Requirements

### Requirement: Transfers are recorded as outcomes, never as an inventory

The log SHALL record what each transfer came to — the bucket, the byte counts,
whether it finished, was cancelled, or failed, and the classified cause on
failure — in the same shape listings are already recorded.

It SHALL NOT record the object's key or the local destination path, at any
detail level. A key names the user's own data and a destination names their
machine's layout; a log the application invites the user to send to a stranger
carries neither. This is the standing no-inventory practice stated as a
requirement, extended to transfers before the first transfer ships.

#### Scenario: A finished download

- **WHEN** a download completes and is logged
- **THEN** the entry carries the bucket, the size, and the outcome — and no
  object key and no destination path, in any representation

#### Scenario: A failed or cancelled transfer at full detail

- **WHEN** a transfer fails or is cancelled and logging is at its most
  detailed level
- **THEN** the cause is recorded and the rule still holds: more detail never
  means the key or the path appears
