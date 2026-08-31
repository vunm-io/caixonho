## ADDED Requirements

### Requirement: The window is dressed in the owner's own design system

The application SHALL take its colour, surface, elevation, radius, typography
and spacing from **Đất Nặn**, the design system the owner maintains, rather
than from values chosen here.

It SHALL use that system's **app branch** — the cool neutral surfaces it
specifies for a dashboard — and not the warm paper it specifies for the web.
The system gives its own reason: a large warm field makes data hard to read,
and this application is almost entirely data.

#### Scenario: A value is needed that the system names

- **WHEN** a colour, size, radius or space is needed anywhere in the window
- **THEN** it comes from a token the system defines, and no element names a
  raw value of its own

#### Scenario: A value the system does not name

- **WHEN** the system has no token for something the window needs
- **THEN** it is raised with the owner rather than invented, as the system's
  own notes require

### Requirement: Clay and flat are placed where the system places them

The system divides every surface into two tiers and states the rule: *if the
user presses it, it is clay; if the user reads it for a long time, it is flat.*

The application SHALL follow that division. Clay SHALL be limited to what the
desktop kit limits it to — the navigation item currently open, buttons, chips
and badges — and everything the user reads SHALL be flat: no inset shadow, a
thin line, a light drop shadow.

Blocks SHALL NOT be nested inside blocks.

#### Scenario: A table of objects

- **WHEN** a location's contents are shown
- **THEN** the table is flat, and no row is a moulded block

#### Scenario: The row that is selected

- **WHEN** a row is selected
- **THEN** it is marked by a raised background and a thin accent border, and
  it is neither filled with the primary colour nor lifted off the surface

### Requirement: What the toolkit cannot reproduce is said, not approximated

The system has signatures a CSS page can express and this toolkit cannot —
elliptical blob radii, and the squish the system names as its motion signature.

The application SHALL NOT ship a poor imitation of either. Where a signature
cannot be reproduced, its absence SHALL be recorded in the project's own design
document, so that a reader comparing the window to the system finds the
difference explained rather than assumed to be a defect.

#### Scenario: Someone compares the window to the system

- **WHEN** a reader notices the window's buttons do not deform when pressed
- **THEN** the design document says so, and says why

### Requirement: One theme, until the system has two

The application SHALL offer exactly the modes the design system defines.

The system defines one light theme and records dark as a decision the owner has
deferred. The application SHALL therefore ship one light theme, and SHALL NOT
carry a dark theme built from values the system has never specified.

#### Scenario: The system gains a dark palette

- **WHEN** the design system defines dark surfaces
- **THEN** the application takes them from it, rather than having improvised
  them in the meantime
