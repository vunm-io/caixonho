# caixonho

> From Vietnamese **"cái xô nhỏ"** — *the little pail* (that hauls big buckets).

A fast, native, cross-platform S3 client. GPU-rendered with [GPUI](https://gpui.rs)
(the framework behind Zed), written in Rust, built to feel instant: cold start under
half a second, buttery scrolling through prefixes with 100k+ objects, keyboard-first.

**Status: beta, milestone M2.** It connects, browses, transfers, deletes and
previews. It is not signed or notarized yet, so your operating system will
warn you the first time — see [Running a beta build](#running-a-beta-build).

## S3 Express One Zone, including Local Zones

Worth stating first, because it is the part almost nothing else does.

A **directory bucket** is not a general purpose bucket with a different name.
`ListBuckets` does not return them *at all*, so an account holding only
directory buckets looks empty to most clients. They authorize through
`s3express:CreateSession` rather than the object permissions you would expect.
They keep a real hierarchical namespace, so a folder exists only while
something is in it. And they can live in an **AWS Local Zone** as well as an
Availability Zone — `{base}--{zone-id}--x-s3`, where the zone id may be
`usw2-az1` or a Local Zone like `apse1-han1-az1`.

caixonho treats all of that as ordinary:

- **Lists directory buckets beside general purpose ones**, and says so when
  one listing was permitted and the other refused.
- **Names `s3express:CreateSession`** when a directory bucket refuses you —
  not the object permission it superficially resembles.
- **Knows a directory bucket cannot hold an empty folder.** `New folder…`
  refuses there, says why, and points at what does work, rather than writing a
  marker the service deletes a moment later.
- **Filters by kind**, so an account mixing both can be reduced to either.

Browsed, uploaded to, downloaded from, previewed and deleted in on real
**Local Zone** directory buckets in `apse1-han1-az1` (Hanoi) during
development — not inferred from documentation.

Working today:

- Reads the profiles in `~/.aws` and connects with the credentials they name —
  an SSO profile with a valid cached token, `credential_process`, or static keys.
- Takes a credential you type in, keeps the secret in the system keychain, and
  offers it again next time. Nothing goes into a config file, a log, or `~/.aws`.
- Contacts nothing until you choose a connection, and says so when one cannot
  sign in rather than offering it as though it worked.
- Lists that account's buckets with name, creation date and region, and narrows
  the list to one region.
- Opens a bucket that lives in another region without being told to. S3 answers
  a read addressed to the wrong region with a redirect naming the right one;
  caixonho follows it, remembers it, and corrects the region shown for that
  bucket — where most clients hand back the redirect as an error and leave you
  to work out which region to switch to.
- Lists **S3 Express One Zone directory buckets** too, which `ListBuckets` does
  not return at all — so an account holding only those is not shown as an
  account holding nothing. Where one listing is permitted and the other is not,
  it shows what came back and says what it was refused. Almost no GUI client
  does this.
- Says which buckets your credentials can actually open, and names the IAM action
  a refused one would need — including `s3express:CreateSession`, which is what
  a directory bucket is refused at rather than the listing permission it looks
  like. A bucket nobody has asked about yet says so, rather
  than guessing — and one still being checked says that, which is a different
  thing again.
- Tells failure causes apart — an expired session, rejected credentials, a
  network failure and a trust failure are each reported as themselves, with the
  action that fixes them, and none of them is ever reported as "access denied".
- Opens a bucket and walks its prefixes as folders, a page at a time, with a
  trail back out and a path you can type into. Typing a bucket's name opens it
  even when the credentials may not list the account — an ordinary way to be
  given access, and one most clients treat as a dead end.
- Never draws an empty folder and a refused one the same way, one level below
  where it already does that for buckets.
- **Takes files by drag and drop**, or several at once through the picker.
  Dropped files go to the folder on screen, each keeping its own name; when
  there is more than one, the destination you are asked for is the folder they
  share rather than a key each. A dropped *folder* is refused with a reason
  instead of half-uploaded.
- **Runs several transfers at once**, up to a small bound, with the rest
  queued — each with its own progress, its own outcome, and its own collision
  question. One failure does not stop the others, and a transfer waiting for
  you to answer does not hold a slot. Cancel one, cancel all, retry what
  failed, clear what finished. No pause or resume yet: a `PutObject` is one
  stream, and a Pause that restarted from zero would lie about the bytes
  already sent.
- **Chooses where an upload lands.** The destination is offered filled in and
  editable rather than derived — which is also how a folder is brought into
  existence on a directory bucket.
- **Makes a folder** on a general purpose bucket, as the zero-byte marker the
  console uses.
- **Narrows a long bucket list** by region, by kind, by name, and by whether
  you can actually open it — and can remember which buckets a connection shows
  so an account of eleven can be reduced to the two you use. A bucket whose
  access is simply unknown is never hidden as though it had been refused.
- **Sends a local file into the folder you are looking at — and never
  replaces an object without asking.** The no-clobber promise is made by the
  service, not by a check this app performs and hopes to win: the write is
  conditional, so a key that is already taken is refused by S3 itself and
  you are asked whether to replace it, keep both, or stop. Replacing is the
  only way an object here is ever overwritten. An endpoint that will not
  make that promise says so instead of quietly proceeding. Files above 5 GiB
  are refused up front — those need multipart, which is not built yet.
- **Deletes an object only after asking twice** — the action opens a
  confirmation naming the exact key, and only the confirmation deletes. On a
  bucket that keeps versions, the outcome says a delete marker was placed and
  offers **Undo**, which restores the object exactly; on one that does not,
  it says plainly that the object is gone. The Undo appears only when the
  service's own response proves it exists — never as decoration.
- **Previews an object without downloading it** — a text-like file shows its
  first page from one ranged read, saying honestly how much of the whole you
  are looking at; a small image is drawn whole; and everything else says
  plainly that there is no preview for its kind and points you at Open.
  Binary content wearing a text name is called binary instead of rendered as
  noise, and nothing on the preview path ever touches the disk.
- Downloads an object to a folder you choose, and **opens** one with whatever
  your machine already opens that kind of file with — the app renders no
  format itself; it downloads to a cache it owns
  (`~/Library/Caches/caixonho/open` on macOS,
  `%LOCALAPPDATA%\caixonho\cache\open` on Windows, swept on startup) and
  hands the file to the system. A download in flight shows its progress and
  can be cancelled; a cancelled or failed download never leaves a partial
  file under the real name, an existing file is never overwritten without
  asking, and a key no filesystem would accept is renamed deterministically
  and the rename is said out loud.
- Writes down what it decided and why, in your platform's own log location, and
  shows you that location in the status bar so a report can carry evidence
  instead of a description. The file is bounded and rolls daily; `CAIXONHO_LOG`
  turns the detail up for an investigation. No secret is ever written to it.

Not there yet: bulk and folder deletion, uploading folders, multipart for
large files, the transfer queue, rendered markdown preview, sorting or
searching a listing,
and signing in to IAM Identity Center from the app rather than through the AWS
CLI. Those are the next changes — see [`docs/planned-changes.md`](docs/planned-changes.md),
and [`docs/requirements-status.md`](docs/requirements-status.md) for every
requirement and whether it is actually built.

## Why another S3 client

The existing GUI clients are some combination of feature-poor, slow, ugly,
Windows-only, or paid. caixonho aims to be *a file-explorer-grade S3 client that is
honest about permissions and fast on huge buckets*:

- **Honest about permissions.** Buckets and prefixes your credentials cannot touch
  are dimmed with the reason attached — and an expired token, a wrong region or a
  network failure are never misreported as "access denied".
- **Honest about data.** When a filter or a sort only covers what has been loaded
  so far, the UI says so instead of quietly lying — most clients get this wrong.
- **Fast on purpose.** Virtualized everything; nothing network-shaped ever runs on
  the render thread.
- **No telemetry. Ever.** No analytics, no phone-home, no exceptions. Crash reports
  are written to a local file that *you* choose to attach to an issue.
- **The dependencies are audited on every change**, and a known vulnerability
  stops it. Where an advisory cannot be resolved, the exception is written down
  with the reason and the date it runs out — never a blanket ignore, and never
  one that outlives its own justification. This is an application you hand
  credentials to; what it links against is part of what it is.

## Platforms

| Phase | Target |
|---|---|
| v1 | Windows 11 + macOS (Apple Silicon & Intel), first-class together |
| v2 | Linux |
| v3 | CLI sharing the same core crate |

## Running a beta build

Tagged builds are attached to each
[release](https://github.com/vunm-io/caixonho/releases) and download without a
GitHub account. Newer ones appear as **Artifacts** on every green
[Actions run](https://github.com/vunm-io/caixonho/actions), and those *do* need
an account — artifacts are not served anonymously even from a public
repository.

They are **not signed or notarized**, so:

- **macOS** refuses the first launch with *"Apple could not verify…"*. That
  means unverified, not malicious. **System Settings → Privacy & Security →
  Open Anyway.**
- **Windows** shows SmartScreen's *"Windows protected your PC"* → **More
  info → Run anyway**.

Both warnings are doing their job, and caixonho reads AWS credentials — so
treat clicking through them as a decision, not a formality, and only for a
build you fetched yourself from this repository's own CI. Signed builds are
milestone M4.

The macOS artifact is a bare binary rather than an `.app`, so run it from a
terminal. `scripts/mac-app.sh` wraps a local build into one.

## Building

Requires the Rust toolchain pinned in `rust-toolchain.toml`, which rustup installs
on its own the first time you build.

```sh
cargo run -p caixonho-gui
```

It opens on the connections it can find and contacts none of them until you
choose one. With nothing configured it says so and does nothing else — it never
invents credentials.

## Documentation

[`docs/`](docs/README.md) maps the rest: what the product is meant to be, what it
is contracted to do today, the decisions that are hard to reverse, and what each
task number in the commit log refers to.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
