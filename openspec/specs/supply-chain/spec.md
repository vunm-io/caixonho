# supply-chain Specification

## Purpose
TBD - created by archiving change xonho-0017-auditing-what-we-depend-on. Update Purpose after archive.

## Requirements

### Requirement: The dependency set is checked on every change

The project SHALL check the crates it depends on against a published advisory
database as part of continuous integration, on every push and every pull
request, so that a dependency problem is found by the project rather than
reported to it.

The check SHALL be a job of its own rather than a step inside the build. A
supply-chain problem and a compilation problem are different failures with
different owners and different remedies, and a red tick that could mean either
tells whoever reads it neither.

The check SHALL run against the same lockfile the build resolves from, so that
what is audited is what is compiled.

#### Scenario: A dependency acquires an advisory

- **WHEN** a crate in the lockfile becomes the subject of a published advisory
- **THEN** continuous integration reports it against the change that ran, and
  names the crate, the advisory and the version that resolves it

#### Scenario: The audit is distinguishable from the build

- **WHEN** the audit fails and the build succeeds
- **THEN** the two are reported separately, and the failure names the
  dependency set rather than the code

### Requirement: A known vulnerability stops the change

A vulnerability advisory against a crate in the lockfile SHALL fail the build.

It SHALL NOT be enough that the affected code is believed unreachable.
Reachability is a statement about the call graph on the day it is made, and it
is the kind of statement that stops being true without anybody editing the
sentence that asserts it. Where a vulnerable crate can be removed from the
build, removing it is the remedy; where it cannot, the exception below applies
and is dated.

#### Scenario: A vulnerable crate is in the build

- **WHEN** the lockfile resolves a crate version with a vulnerability advisory
  and no exception covers it
- **THEN** the audit fails and the change does not land

#### Scenario: A vulnerable crate is reachable only through an unused feature

- **WHEN** a vulnerable crate enters the build through a feature this project
  does not use
- **THEN** the feature is turned off and the crate leaves the build, rather
  than the advisory being recorded as accepted

### Requirement: An exception is an individual, reasoned, dated decision

Where an advisory cannot be resolved by removing or upgrading the crate, the
project SHALL record the exception in a policy file held in the repository,
and each exception SHALL name the specific advisory, why it is accepted, and
the date the acceptance expires.

A blanket exception SHALL NOT be used — neither a whole advisory class, nor a
whole crate, nor an unbounded acceptance. An exception with no expiry is
indistinguishable from having stopped looking, and a policy file full of them
satisfies the letter of this capability and none of its purpose.

An expired exception SHALL fail the build, so that the decision is taken again
by someone rather than inherited by silence.

#### Scenario: An advisory cannot be resolved today

- **WHEN** an advisory has no upgrade path and the crate cannot be dropped
- **THEN** the policy file names that advisory, states why it is accepted, and
  states when the acceptance expires

#### Scenario: An acceptance reaches its expiry

- **WHEN** the date recorded against an accepted advisory has passed
- **THEN** the audit fails until the decision is made again

#### Scenario: Someone reaches for a blanket ignore

- **WHEN** an exception would cover more than one named advisory
- **THEN** it is not written, and each advisory is decided on its own
