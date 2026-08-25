## ADDED Requirements

### Requirement: A text-like object previews by its first page

The system SHALL preview a text-like object by fetching a bounded first
portion with a ranged read and rendering it as text. When the object is
larger than the fetched portion, the preview SHALL state both numbers — how
much is shown and how large the object is — taken from the service's own
response, not inferred.

#### Scenario: A small text object

- **WHEN** the user previews a text-like object no larger than the page
- **THEN** its full content is shown as text, and no truncation line appears

#### Scenario: A large log

- **WHEN** the user previews a text-like object larger than the page
- **THEN** the first portion is shown, and the preview states the portion's
  size against the object's full size

#### Scenario: The name said text, the bytes said otherwise

- **WHEN** the fetched portion is not decodable as text
- **THEN** the preview says the content is binary rather than rendering
  noise, and offers Open as the way to look at it

### Requirement: An image previews whole, and only under the gate

The system SHALL preview a raster image by fetching the whole object into
memory and drawing it, and SHALL do so only when the object's listed size is
at or under a fixed bound. Above the bound, the preview SHALL state the size
and offer Open instead of fetching.

A partial image SHALL never be drawn: a truncated raster file does not
decode, and nothing here attempts it.

#### Scenario: A small image

- **WHEN** the user previews an image at or under the bound
- **THEN** the whole image is fetched and drawn

#### Scenario: An image over the gate

- **WHEN** the user previews an image over the bound
- **THEN** nothing is fetched, and the preview states the size and offers
  Open

#### Scenario: The bytes do not decode

- **WHEN** a fetched image fails to decode
- **THEN** the preview says so and offers Open, and does not present the
  failure as a transfer error

### Requirement: Every other kind is refused honestly

For an object of a kind the preview does not serve, the system SHALL say
that there is no preview for this kind and SHALL offer Open — never a blank
surface, never an attempt that fails into noise.

#### Scenario: An unsupported kind

- **WHEN** the user previews an object that is neither text-like nor a
  supported image
- **THEN** the preview states there is no preview for this kind and offers
  Open

### Requirement: Preview never touches the disk

A preview SHALL be served entirely from memory: no file is written, no
cache entry is created, and closing the preview leaves no artifact on the
machine.

#### Scenario: After a preview

- **WHEN** the user previews any object and then leaves the preview
- **THEN** no file attributable to the preview exists on disk

### Requirement: The preview leaves as cleanly as it came

The preview SHALL replace the listing surface and SHALL return to the
listing on an explicit Back control. Leaving the location or switching
connection SHALL drop the preview with it; a preview from a connection the
user has left SHALL never be shown under another connection's name.

#### Scenario: Back returns to the listing

- **WHEN** the user leaves the preview with Back
- **THEN** the location's listing is shown again, re-read from the service

#### Scenario: A switch takes the preview with it

- **WHEN** the connection is switched while a preview is open or in flight
- **THEN** the preview is gone, and nothing from it renders under the new
  connection

### Requirement: Preview outcomes are logged without an inventory

The log SHALL record each preview's outcome — the bucket, the bytes
fetched, and the classified cause on failure — and SHALL NOT record the
object's key, at any detail level.

#### Scenario: A logged preview

- **WHEN** a preview settles and is logged
- **THEN** the entry carries the bucket and the outcome, and no key in any
  representation
