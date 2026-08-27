# Tasks — XONHO-0024 a folder you can make

> Core is TDD (`AGENTS.md` §7). The interesting tests here are not "a put was
> made" but the two refusals: a name that cannot be one, and a bucket kind
> that cannot hold one.

## 1. Making one

- [x] 1.1 A name that cannot be a folder is refused before any request
      [dispatch: main]
      - Done in `main` (2026-08-26) as `crates/caixonho-core/src/folder.rs`,
        red first, six tests. `key_for` both validates and builds the key, in
        one place, so what a folder key looks like is decided once.
      - The trim is a rule rather than a courtesy: a name typed with a stray
        space either side is the name the person meant, and a key with a space
        either side is invisible in every listing that shows it.
  - Paths: `crates/caixonho-core/src/store.rs` or a new small module
  - Done criteria: a pure function deciding whether a name may become a
    folder — non-empty, no `/`, and whatever else the key rules forbid. Red
    first. Tests: empty, `/` inside, leading and trailing whitespace, and a
    name that is fine. **Nothing reaches the service to find this out.**
  - Verification: `cargo test -p caixonho-core`

- [x] 1.2 The marker, on a general purpose bucket [dispatch: main]
      - Done in `main` (2026-08-26). `create_folder` is its own trait method
        rather than a `put_object` with an empty temporary file: `put_object`
        takes a path because it streams a file the user chose, and making a
        folder has nothing to do with a filesystem.
      - No `if_none_match`. A folder that already exists is a name collision
        the caller refuses by name, not a file to step aside from — and putting
        the marker twice writes the same zero bytes over the same key. So the
        guard is **absent by decision**, not omitted.
  - Paths: `crates/caixonho-core/src/store.rs`,
    `crates/caixonho-core/src/adapter.rs`,
    `crates/caixonho-core/src/session.rs`
  - Done criteria: `ObjectStore` gains create-folder; the adapter puts a
    zero-byte object at `<prefix><name>/`; `Session` gains its spawn. Red
    first, against `StoreDouble`. Tests: the key is the location's prefix
    plus the name plus exactly one `/`; a folder made at the bucket root has
    no leading `/`; a failure keeps its classified cause.
  - Verification: `cargo test -p caixonho-core`

- [x] 1.3 A directory bucket is refused with what does work [dispatch: main]
      - Done in `main` (2026-08-26). Both refusals — wrong kind, bad name —
        happen **before the store is even taken**, so neither can read as
        "something went wrong out there". `NotOnADirectoryBucket` is
        deliberately *not* an `Error`: nothing failed, the service is behaving
        exactly as documented, and reporting it as a failure would send someone
        to fix a bucket that is fine.
      - The assertion that carries it is on `folders_made()` being **empty** —
        a new counter on `StoreDouble`, because only that can tell "refused
        without asking" apart from "asked and was refused".
      - **Ablated**: removing the kind check turns
        `a_directory_bucket_is_refused_without_spending_a_request` red. (The
        first ablation attempt appeared to show nothing, and the fault was my
        test-name filter, not the test.)
  - Paths: `crates/caixonho-core/src/session.rs`
  - Done criteria: the decision is taken from the **bucket kind already in
    the listing**, before any request. Red first. Tests: a directory bucket
    yields the refusal and **no call reaches the store** (assert on the
    double's call count, not only on the returned value); a general purpose
    bucket puts. The refusal carries what to do instead, so the window has
    something to say rather than a bare "no".
  - Verification: `cargo test -p caixonho-core`

## 2. The window

- [x] 2.1 `New folder…`, its prompt, and the two answers [dispatch: main]
      - Done in `main` (2026-08-26). A strip under the listing, like the
        transfer's and the deletion's, and its own rather than shared: the
        wording is where the meaning is, and nothing here is destructive.
      - The kind is read from the account listing **by name over `rows`, not
        `shown`** — the question is asked from inside a bucket, where the
        account list may since have been narrowed by `XONHO-0025`, and a
        narrowing must not change what a bucket *is*.
      - Four window tests, including `XONHO-0019`'s discipline on the newest
        verb: a folder made into an account the user has left is dropped, not
        announced over the one they are looking at.
      - A made folder re-reads the location. A folder nobody can see is a
        folder nobody believes in.
      - **Every one of these was first written as an `inline_message` card,
        and the owner had to point at it.** The slot under the listing has one
        voice — `transfer_line` and `deletion_line` are both flat, full-width
        lines with text on the left and actions on the right — and this
        arrived as a bordered, shadowed card with an icon tile and the button
        inside it. Side by side with `bucket-06-name-taken`, a *failed upload*
        was getting a lighter treatment than a message where nothing is wrong.
        All three are strips now.
      - The lesson is not "use strips". It is that the screenshot harness
        wrote both images and **I read them and did not see it** — the same
        failure as the border cutting through text on 2026-08-25. A frame that
        is never compared against its neighbour is a frame nobody has judged.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: a button beside the five verbs, enabled only inside a
    bucket; a name prompt; the listing refreshed on success so the folder is
    there without a manual reload; the directory-bucket refusal shown as a
    sentence the user can act on, not as an error.
  - Verification: `cargo test -p caixonho-gui`

- [x] 2.2 The screenshot harness covers the new states [dispatch: main]
      - Done in `main` (2026-08-26): `bucket-13-new-folder-naming` and
        `bucket-14-new-folder-not-on-a-directory-bucket`, both pixel-distinct.
      - Driven through the real controls, which is a lesson taken straight
        from `XONHO-0025` earlier the same day — that change photographed two
        states no user could reach because it set state instead of using the
        controls, and the distinctness assertion cannot catch that.
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: the naming prompt, a refusal, and a made folder each get a
    frame, and each is **pixel-distinct** from every other — the assertion
    `XONHO-0009` added after twelve identical images got through.
  - Verification: `cargo test -p caixonho-gui`

## 3. Close-out

- [x] 3.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-26): fmt and clippy exit 0, 359 core + 74
        window green (8 + 1 ignored).
  - Verification: the commands themselves

- [x] 3.2 CI green on both targets, run id recorded here [dispatch: main]
      - Run `32924996318` on `e714ad8`: `build (windows-latest)`,
        `build (macos-latest)`, `dependency audit` and `rustfmt` all success.
        Both changes landed in that one commit — see its message for why.
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [x] 3.3 Live: a folder on each kind of bucket [dispatch: main]
      - Done on the owner's machine, 2026-08-27. Both halves confirmed: a
        folder made on a general purpose bucket, and the refusal read on a
        directory one.
      - **And the assumption underneath this whole change was finally
        watched rather than read.** The close-out review named it as the
        unverified one: *"if AWS behaves differently from its own
        documentation, every test here is still green"*. The owner's delete
        sitting settled it — `test-folderr-2/` held one object, the object was
        deleted, and the next listing of that prefix returned **zero folders
        and zero objects**. The directory went with its last file, exactly as
        documented.
      - So the refusal this change offers is not merely defensible from a
        document; it is the behaviour of the bucket the owner uses daily.
  - Done criteria: on the owner's machine — make a folder on a **general
    purpose** bucket, leave the location, come back, and confirm it is still
    there; then ask for one on a **directory** bucket and read the refusal.
    Both written here. The first is the one that can surprise: if the folder
    is gone on return, the marker is not doing what this design says it does.
  - Verification: what was seen, quoted

- [x] 3.4 Reader-facing documents [dispatch: main]
      - Done in `main` (2026-08-26). §4.5's row **stays partial** — bulk and
        recursive delete are still unbuilt — and now says create-folder is two
        behaviours rather than one. A roadmap M3 row added. Counts by the
        script: unmoved, as expected.
  - Paths: `docs/requirements-status.md`, `docs/roadmap.md`
  - Done criteria: §4.5's create-folder row moves and says what it now does
    **and what it refuses**; the M3 roadmap table gains a row. Counts by
    `scripts/count-requirements.sh`.
  - Verification: the script's totals match the tables

- [x] 3.5 Close-out review per `AGENTS.md` [dispatch: main]
      - Run 2026-08-26, before the live check.
      - **Q1: no departures.** The proposal and spec were shaped by AWS's
        documentation *before* the design was written, which is why this
        carries two requirements. The one thing considered and rejected is in
        `design.md`: writing the marker on a directory bucket anyway would
        have worked — the object `reports/` is not an empty directory — and
        was rejected because it solves our display problem by writing rubbish
        into the user's bucket.
      - **Q2, amended after the fact:** the review passed this, and the owner
        then found the new strips rendering in a visual language nothing else
        in that slot uses. Question 2 asks whether the reader-facing
        *documents* still tell the truth; nothing asks whether a new state
        looks like its neighbours. `docs/design-language.md` is the document
        that would say so, and it was read for what it *forbids* rather than
        for what the slot already does. Worth a line there, and it is what
        `XONHO-0009` is for.
      - **Q2:** §4.5's row stays partial and names what is still missing;
        roadmap row added; rows either side re-read. `README.md` and
        `docs/design-language.md` say nothing this contradicts. The
        directory-bucket finding was already in `planned-changes.md` from
        planning time.
      - **Q3:** `create_folder` on the trait has one production caller and one
        adapter implementation; `folders_made()` on the double is used by
        three tests. `FolderPhase` has no unread field — the lesson from
        `XONHO-0008`, where an unread `size` was a spec requirement going
        unmet.
      - **Q4, in `XONHO-0023`'s form — what did this change do to the
        evidence?** It added a diagnostic that names the **bucket and not the
        key**: a folder's name is the user's own words about their own data,
        and a log they may send to a stranger has no business carrying it. The
        gaps:
        - the marker has never been written to a real bucket, so whether it
          survives a round trip is 3.3 and nothing else;
        - the directory-bucket behaviour is asserted from **documentation**,
          not observation. If AWS's behaviour differs from its own docs, every
          test here still passes. 3.3 asks for both kinds for that reason.
      - **Q5:** the observation worth more than this change is already parked:
        on a directory bucket the useful feature is **choosing the destination
        key at upload time**, not creating a folder. In `planned-changes.md`,
        unissued.
  - Done criteria: the five questions answered here, question 2 read the wide
    way, and question 4 asked in the form `XONHO-0023` learned it the hard
    way: **what did this change do to the evidence?**
  - Verification: the recorded findings
