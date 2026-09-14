## Context

Everything this needs already runs. `XONHO-0007` downloads one object and maps
its key to a filename under `ADR-0004`. `XONHO-0028` runs many transfers at a
bound with per-item outcomes, cancel and retry. `XONHO-0030` walks a prefix
flat — `spawn_walk_under`, written so a folder delete could count before it
asked — and selects many rows. `XONHO-0031` can drive the whole thing from the
window against a real S3 service.

What does not exist is the **act**: one gesture that becomes many transfers and
carries state of its own — the destination chosen once, and the collision
answer chosen once.

Three of the four decisions below were settled with the owner before this was
written; they are recorded here with the reasoning, not re-opened.

## Goals / Non-Goals

**Goals**

- A folder, or a selection, downloaded by one gesture with one destination.
- Prefix structure reproduced on disk, and never escaping the destination.
- One collision answer able to stand for the rest of that act.
- No new concurrency machinery: the queue that exists does the work.

**Non-Goals**

- **Uploading a folder.** The `[M]` row names both directions; this is one, and
  the row stays partial saying which half.
- **Multipart or ranged download.** Per-object size limits are unchanged.
- **A second queue in `caixonho-core`.** See the decision below.
- **Progress as a single bar for the act.** The queue reports per item and a
  finished-of-total count; an aggregate percentage needs byte totals nobody has
  before the walk ends, and `XONHO-0028` declined ETA for the same reason.

## Decisions

### The fan-out lives in the window, not in the core

The window walks the prefix through the existing `spawn_walk_under`, then calls
the existing per-object `spawn_download` once per key into the existing queue.
`caixonho-core` gains one pure function and no new spawn API.

Considered and rejected: `spawn_download_under(location, destination, …)` in
core, which would make the core usable from the CLI crate the roadmap puts at
M6. It was rejected because core would then need its own concurrency bound,
its own cancellation and its own retry — a **second queue** beside the one the
window already owns, which is the shape `ADR-0003` exists to prevent. When the
CLI arrives it can grow that API deliberately; nothing here forecloses it,
because the pure mapping — the part both would share — is the part being added
to core now.

The deciding argument is where the state belongs. "This answer stands for the
rest" is a fact about a **user's act**, not about a transfer. The layer that
owns acts is the one drawing the question.

### The mapping is extended from a name to a path, and `ADR-0004` is amended

`transfer::local_name(key) -> Mapped` maps a key's final segment. A sibling
`local_path(key, under) -> MappedPath` maps the part of the key below `under`,
segment by segment, each through the identical rules.

`ADR-0004` is **amended rather than replaced**: its decision — one deterministic
scheme, percent-encoding rather than substitution, a deterministic suffix for a
segment that cannot serve, and a report of what was done — is unchanged. What
changes is the unit, from a filename to a relative path, and the ADR must
answer a case it never had: a **middle** segment that cannot serve.

The answer is the same rule, and the reason it is safe is the same reason it
was safe for the last segment. The suffix is FNV-1a over the **whole key**, so
two keys differing anywhere still differ after mapping, and a segment like `..`
or an empty one becomes an ordinary directory name rather than a movement.

Reported outcome is the **worst** across segments — `Suffixed` beats
`Substituted` beats `Unchanged` — because the user is being told something was
done to this object, and the strongest thing done is the honest headline.

### The destination is asserted, not trusted

After the relative path is joined to the chosen directory, the result is
checked to be inside it, and the transfer fails with a stated cause if it is
not. The mapping is believed to make escape impossible; the assertion is there
because "believed impossible" is how directory traversal ships. It costs one
comparison per object.

### A download walks without a ceiling

`MOST_KEYS_GATHERED` (5,000) refuses a delete that would be unbounded and
irreversible. A download is neither: abandoning one costs the bytes fetched so
far, and nothing is lost. Fifty thousand keys is roughly five megabytes held
while the queue drains, which is not a reason to refuse a legitimate request.

`spawn_walk_under` reports `Tally::TooMany` above the ceiling today, so the
walk needs a form that does not — either a parameter or a sibling. The tasks
choose the smaller of the two once both are written out; what matters here is
that delete keeps its ceiling and download does not inherit it.

## Risks / Trade-offs

- **[The window grows a new kind of state — the act]** → It is one struct
  beside the queue: the keys of this act, its destination, its answer. The risk
  is it being forgotten on a connection switch, which is the defect
  `XONHO-0019` exists for; the act is dropped where the location is dropped,
  and a test says so.
- **[A collision answer that covers a batch can overwrite many files at once]**
  → That is the point, and the guard is that the user chose it explicitly, for
  that act, once. The spec forbids the answer outliving the act. What it must
  never become is a preference remembered across acts without being asked.
- **[The walk of a very large prefix is slow and silent]** → The act shows it
  is walking, and is cancellable while it walks, which the delete flow already
  does. Nothing is queued until the walk finishes, so cancelling costs nothing.
- **[Two acts running at once share one queue]** → They do, and their answers
  do not mix because the answer is held by the act rather than by the queue.
  Worth a test rather than a comment.

## Migration Plan

None. Nothing persisted changes, and the single-object download is untouched —
it becomes the one-key case of the same path, but its own entry point stays
exactly as it is so `XONHO-0007`'s behaviour cannot regress by refactor.

## Open Questions

- **Whether the act should offer a subfolder for its own name.** Downloading
  `daily/` into `~/Downloads` writes `~/Downloads/daily/…` because the walk is
  relative to the location the act began at, which is the bucket root. That is
  the behaviour the owner chose. What is not decided is downloading a
  *selection* of loose objects — those land directly in the destination, with
  no wrapper folder, and whether that should instead be wrapped is a question
  for the first person who downloads forty files into a full Downloads folder.
