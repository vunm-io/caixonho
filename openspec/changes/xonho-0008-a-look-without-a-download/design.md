# Design — a look without a download

## Context

Every piece of machinery this needs exists: a streaming read port, the
session spawn contract, the outcome/diagnostics shapes, the stale-connection
discipline, and a window that already swaps its content area between modes
(form, confirmation, sign-in). Preview is the first *viewer* surface, and
the design's job is mostly to keep it honest at the edges — truncation,
binary masquerading as text, images too large to justify, and staleness.

## Goals / Non-Goals

**Goals**

- One ranged page for text, one gated whole-fetch for images, honest
  refusals for the rest.
- No disk, ever, on this path.
- The numbers on screen come from the service's response.

**Non-Goals**

- Markdown rendering, syntax highlighting, hex view, load-more, search.
- PDF and video (the OS opener owns them, per `XONHO-0007`).
- Any caching of previews, in memory or on disk, across invocations.

## Decisions

### The measured facts, and the one that bends the plan

`GetObject.range` is one builder call (`builders.rs:366`); the ranged
response carries `content_range` (`_get_object_output.rs:254`), whose total
is the honest "of 4.2 MiB" figure. `gpui::Image { format, bytes }` converts
straight into an `ImageSource` (`img.rs:113`), so drawing from memory costs
no dependency.

And the fact that bends it: **the first N KiB of a raster file does not
decode** — image decoders answer truncation with an error, not a partial
picture. So the brief's one line becomes two paths: text previews by range,
images preview whole under a 20 MiB gate (a constant; a knob would be a
setting about honesty). Anything over the gate, and any kind outside both
paths, gets the brief's own fallback sentence: download to open.

### Kind by extension, truth by content

The extension chooses the *path* (text-like set: txt, log, json, yaml, yml,
csv, md, toml, xml; raster set: png, jpg, jpeg, gif, webp, bmp), because it
is the only signal available before any bytes move. The *content* then gets
the last word on the text path: a NUL byte or invalid UTF-8 (beyond one
trailing truncated character — a ranged cut can split a multibyte character,
and that split must not condemn the file) means the preview says *binary*
instead of rendering noise. Both functions are pure, in core, tested against
the awkward shapes: UTF-8 split at the boundary, a BOM, an empty object, an
extensionless name.

### One page means one request

64 KiB, fetched with `range: bytes=0-65535`, rendered monospace. No
load-more: the promise is a look, and the moment a look becomes a pager it
grows state, scroll anchoring, and a second request policy — Open and
Download exist. The truncation line renders exactly when `content_range`'s
total exceeds the fetched length.

### The preview is a mode of the location surface

`preview: Option<Preview>` on the window; `contents()` renders it in place
of the table when present, path bar intact, with Back. Entering a preview
does not end the location — Back re-reads the listing through the existing
`go_to`, the same refresh the deletion strip uses. The stale rules follow
`XONHO-0019`/`XONHO-0021`: the preview carries its connection, `go_to` to a
different location drops it, `end_location` drops it, and an arriving result
for a preview no longer on screen is dropped silently.

### Fetch through the existing port, gather in memory, bounded

Text: a new port method `get_object_head(bucket, key, bytes)` returning the
same `ObjectContent` shape plus the total from `content_range` — ranged is a
different request, not a parameter dressing on `get_object`, and the double
scripts it separately. Images: the existing `get_object`, gathered into a
`Vec<u8>` whose capacity is checked against the gate *before* the fetch (the
listed size) and whose growth is checked *during* it (the stream is not
trusted to match the listing). The gather stops with an honest error if the
stream exceeds the gate — trust the bytes, not the metadata.

### No disk, structurally

The preview path calls no writer, no cache dir, no temp file. That is not a
discipline note but an architecture one: everything it fetches lives in the
`Preview` state and dies with it. The spec's scenario is checked by
construction — there is no code path to write from.

## Risks / Trade-offs

- **[64 KiB may split a UTF-8 character]** → the decoder tolerates exactly
  one truncated tail character; the sniff is otherwise strict.
- **[A 20 MiB image in a `Vec<u8>`]** → transient and bounded; dropped with
  the preview. The gate exists precisely so this cannot be 2 GiB.
- **[gpui decode happens at render]** → decode failure surfaces as the
  refusal state, not a crash: the `Image` conversion is checked before the
  state is set where the API allows, and the refusal state covers the rest.
- **[Preview of a just-deleted object]** → the fetch fails with the
  service's answer, classified and shown — the same vocabulary as
  everything else.

## Open Questions

None. The three load-bearing facts were measured before this document was
written, and the one product cut they force (ranged text vs whole small
images) is recorded above with its reason.
