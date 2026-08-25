# XONHO-0008 — A look without a download

## Why

The verbs are in: an object can be fetched, sent, opened and deleted. What
is still missing is the cheapest question a person asks a file browser —
*what is this?* — answered without paying for the whole object. The owner
asked for a view button in the same breath as the three verbs, and the brief
staged this change for it long ago: §4.5 `[S]`, *"preview without full
download (ranged first-N-KB for text/log/json/yaml/csv; images; markdown);
everything else gets explicit download-to-open"* — staged as `XONHO-0008`,
dependent on `XONHO-0007`, which has landed.

Planning measured the three facts the brief's line rests on, and one of them
bends the design:

- Ranged reads are one builder call (`GetObject.range`,
  `builders.rs:366`), and the ranged response names the whole object's size
  (`content_range`, `_get_object_output.rs:254`) — so "first 64 KiB of
  4.2 MiB" is honest, with both numbers from the service.
- The window can draw an image from in-memory bytes with what it already
  has: `gpui::Image { format, bytes }` and
  `From<Arc<Image>> for ImageSource` (`img.rs:113`). No new dependency.
- **A ranged image does not decode.** The first N KiB of a PNG or JPEG is a
  truncated file, and the decoder answers with an error, not a partial
  picture. So "ranged preview" is a text story only; an image previews by
  being fetched **whole**, which is honest exactly when the image is small —
  and most are.

So the change reads the brief's line the only way it can be built: text-like
kinds preview by their first page, images preview whole under a size gate,
and everything else — and everything over the gates — gets the explicit
download-to-open the brief already promised it.

## What Changes

- **A Preview action** beside the other object verbs, enabled on an object
  selection. Preview replaces the listing area with the preview and a Back
  control; the path bar stays, and Back returns to the listing exactly as it
  was asked for (a re-read, not a cached snapshot).
- **Text-like objects show their first page.** One ranged read of the first
  64 KiB, decoded as UTF-8 with honesty about what it is not: content that
  is not text (NUL bytes, invalid UTF-8 beyond a trailing truncated
  character) is said to be binary rather than rendered as noise. A truncated
  view says so in numbers: *first 64 KiB of 4.2 MiB* — both from the
  service's own response.
- **Images preview whole, under a gate.** A raster kind at or under 20 MiB
  is fetched into memory and drawn; over the gate, the preview says the size
  and offers Open instead. The gate is a constant, not a setting.
- **Every other kind says its honest thing**: no preview for this kind here,
  with Open as the way to look at it — the brief's own sentence, on screen.
- **Preview is read-only and leaves nothing behind**: no file is written, no
  cache entry, nothing to sweep. It is the one fetch path that never touches
  disk.
- **The log records preview outcomes** in the established shape — bucket,
  bytes fetched, outcome, cause — never the key.

### What is deliberately absent

- Markdown *rendering* (the brief lists markdown; this slice shows it as the
  text it is — a rendered view is presentation work that deserves its own
  argument, and a wrong renderer is worse than honest monospace).
- Syntax highlighting, search-in-preview, hex view for binaries, video or
  PDF preview (PDF stays with the OS opener per `XONHO-0007`'s decision).
- Previewing while the object is being downloaded, and any second request
  for "load more" — one page is the promise; more is Open or Download.

## Capabilities

### New Capabilities

- `object-preview`: answering *what is this* without a download — the
  ranged text page with its truncation honesty, the gated whole-image view,
  the binary and unsupported-kind refusals, and the no-disk guarantee.

### Modified Capabilities

None. `object-transfer` moves content to and from disk; preview
deliberately never touches disk, and the boundary is the capability line.

## Impact

- **`caixonho-core`**: `ObjectStore` grows a ranged read (`get_object_head`)
  returning the head's stream plus the object's full size from
  `content_range`; the transfer module gains the text/binary sniff (pure,
  tested); `diagnostics` gains the preview outcome. The whole-image path
  reuses `get_object` as-is.
- **`caixonho-gui`**: the Preview action, the preview surface with Back,
  and the three honest refusal states. Kind detection by extension, sniff by
  content — both pure functions in core.
- **Dependencies**: none. The image decode is gpui's own; the UTF-8 check is
  std.
- **Docs**: `README.md`, `docs/roadmap.md` (M3 table row),
  `docs/requirements-status.md` §4.5 preview row moves.
- **Ordering**: no constraint on other changes — `object-preview` is a new
  capability, so this change can archive on its own, unlike `XONHO-0020`
  whose delta still waits on `XONHO-0007`.
