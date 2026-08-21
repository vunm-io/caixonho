## 1. What a bucket is, and where the other ones live

- [x] 1.1 Give `Bucket` its kind [dispatch: main]
      - Done in `main` (2026-08-20); verified: `cargo test --workspace` green.
      - `BucketKind::{General, Directory}` and a `kind` field. Seven
        construction sites updated — one real (`map_bucket`) and six test
        helpers, which is the honest cost of a field with no default: every
        place that makes a bucket now has to say what it made.
  - Paths: `crates/caixonho-core/src/types.rs`
  - Done criteria: a `BucketKind` with `General` and `Directory`, and a `kind`
    field on `Bucket`. The doc comment says the kind comes from the operation
    that returned the bucket, not from its name — design.md explains why, and
    the next reader should not have to find that out by trying the other way.
  - Verification: `cargo build -p caixonho-core`

- [x] 1.2 Read a directory bucket's region from its ARN [dispatch: external-ok]
      - Dispatched: agy (2026-08-20) — completed; verified independently here,
        not taken on its report: `cargo test -p caixonho-core adapter::` 23
        passed, `cargo fmt --all --check` clean, `cargo clippy -p caixonho-core
        --all-targets -- -D warnings` clean, and `git diff --stat` shows one
        file changed and nothing else touched.
      - `directory_bucket_region(arn, listing_region) -> Region`. It requires
        six colon-separated fields and a leading `arn`, so a short or malformed
        string falls back rather than yielding a fragment — which is the test
        that mattered.
      - **Carries `#[allow(dead_code)]`**, because nothing calls it until 1.3.
        That attribute is scaffolding and must come off in 1.3; left standing,
        it is how a function nobody calls survives a `-D warnings` build.
  - Paths: `crates/caixonho-core/src/adapter.rs`
  - Done criteria: a function taking the bucket's optional ARN and the region
    the listing was made against, returning the region: the ARN's region field
    when the ARN parses, the listing's region otherwise. It never returns
    unknown, because a directory bucket that exists is in a region.
  - Done criteria (tests): a well-formed `arn:aws:s3express:<region>:...`
    yields that region; a missing ARN yields the listing's region; a malformed
    ARN yields the listing's region rather than a fragment of the string
  - Verification: `cargo test -p caixonho-core adapter::`

- [x] 1.3 Call `ListDirectoryBuckets`, through the paginator [dispatch: main]
      - Done in `main` (2026-08-20); verified: `cargo test -p caixonho-core
        adapter::` — 30 tests.
      - The guard test is not vacuous, and proving that took a second test.
        `a_custom_endpoint_is_never_asked_for_directory_buckets` passes an HTTP
        client that refuses everything, so returning an empty list *is* the
        proof nothing was sent; `an_aws_connection_is_asked_for_directory_buckets`
        is its mirror and fails through the same client, which is what shows
        the first test would notice if the guard stopped working.
      - The store now keeps the region it was opened in. `directory_bucket_region`
        needs a fallback that always exists, and `SdkConfig::region()` is
        optional — a fallback that can be absent is not one.
      - `#[allow(dead_code)]` from 1.2 removed, as that task's record required.
  - Paths: `crates/caixonho-core/src/adapter.rs`
  - Done criteria: a method on `S3ObjectStore` that lists directory buckets for
    the connection's own region, walking every page, mapping each entry to a
    `Bucket` with `kind: Directory` and the region from 1.2. It returns
    immediately with an empty result — making **no request** — when
    `self.config.endpoint_url().is_some()`, because directory buckets are an
    AWS construct and an S3-compatible endpoint answering `NotImplemented` is a
    failure this application would then have to explain away.
  - Done criteria (test): a test asserts no request is issued when a custom
    endpoint is configured
  - Verification: `cargo test -p caixonho-core adapter::`; `grep -n
    "endpoint_url" crates/caixonho-core/src/adapter.rs` shows the guard

## 2. Two listings, one list

- [x] 2.1 Make room in the listing outcome for what was refused
      [dispatch: main]
      - Done in `main` (2026-08-20); verified: `cargo test --workspace` — 248
        core + 24 window.
      - `AccountListing { buckets, refused }` in `types.rs`, and the port now
        returns it. `Outcome::Loaded` carries it, so the refusal reaches the
        window by the same road the buckets do rather than by a second one.
      - **Blast radius, recorded because it is the argument against doing this
        later**: the port trait, both test doubles, `session.rs`, `outcome.rs`
        and four sites in `app.rs`. Changing the shape of an answer costs every
        place that reads it; deferring it would have cost the same, plus a
        migration.
  - Paths: `crates/caixonho-core/src/outcome.rs`,
    `crates/caixonho-core/src/listing.rs`,
    `crates/caixonho-core/src/error.rs`
  - Done criteria: the bucket-listing result can carry buckets **and** a
    refusal — which listing was refused and which IAM action it required —
    rather than being one or the other. An outcome carrying only a refusal and
    no buckets stays distinguishable from an outcome carrying an empty account,
    which is the rule `bucket-listing` already holds and this change must not
    break.
  - Verification: `cargo test -p caixonho-core listing::`

- [x] 2.2 Issue both listings together and join them [dispatch: main]
      - Done in `main` (2026-08-20); verified: `cargo test -p caixonho-core
        adapter::` — five tests, one per case.
      - `tokio::join!`, so the pair costs what the slower one costs.
      - The policy is a **free function**, `combine()`, not a branch inside the
        async call: it is the whole substance of this change, and out here
        every case is assertable without a network.
      - **A rule the tasks did not state, decided here**: only an authorization
        denial makes a partial result. A network failure or an expired session
        applies to both calls equally, so presenting the half that happened to
        arrive as the whole account would hide it — that case fails the whole
        listing, and a test says so.
      - The refused action is read off the classified error rather than passed
        in, so what is reported is what the classifier decided the call needed.
  - Paths: `crates/caixonho-core/src/adapter.rs`,
    `crates/caixonho-core/src/listing.rs`
  - Done criteria: both calls are in flight at once, not one after the other;
    the results are one list; a denial of either leaves the other's buckets
    intact and records the refusal from 2.1. Both denied is still a denial with
    nothing to show.
  - Done criteria (tests): general denied + directory returns buckets → those
    buckets, plus a refusal naming the general listing; directory denied +
    general returns buckets → the mirror; both denied → a denial, and the list
    is not presented as empty; both succeed → one list holding both kinds
  - Verification: `cargo test -p caixonho-core listing::`

- [x] 2.3 Name the action each refusal actually required [dispatch: main]
      - Done in `main` (2026-08-20); verified: `cargo test --workspace` — 250
        core + 32 window, and a live run against a bucket the caller may not
        open.
      - The listing half carries `s3express:ListAllMyDirectoryBuckets`, quoted
        from `aws-sdk-s3`'s operation docs, with a test that a directory
        refusal never reports `s3:ListAllMyBuckets`.
      - **The session half was left for later, and the owner hit it within the
        hour.** Opening a directory bucket the caller has no
        `s3express:CreateSession` on rendered *"unexpected error: the call
        failed without a reportable cause"* — the exact failure this project
        exists to prevent, on a cause with a name and a remedy. The claim that
        it "does not bite the verification account" was true of the buckets
        that had been opened, and false of the account.
      - **The shape was measured, not reasoned.** An `#[ignore]`d instrument
        (`this_machine_opening_a_directory_bucket`) printed it: `kind:
        Dispatch, code: None, status: None`, chain holding `unhandled error
        (accessdenied)`. The session is obtained inside the SDK before dispatch,
        so the refusal never becomes our response and `code_is(DENIED_CODES)`
        could never have matched it. That instrument now asserts, so the path
        cannot silently regress.
      - Rule 4 of `classify` now also reads a denial out of the chain, guarded
        on `answered_nothing()` — no code and no status — so a response that
        *did* arrive is still judged by its own code and never by text further
        down. Both halves have a test.
      - The action is `s3express:CreateSession` for a bucket the directory
        listing returned. Which buckets those are is **remembered** from that
        listing, never read off the `--x-s3` suffix. A bucket opened by name
        without a listing is absent from that set, and then the connection
        honestly does not know.
      - Consequence worth watching at 4.3: those buckets now probe as
        **Denied** rather than Unknown, so they carry the "No access" badge the
        access column already had.
      - **Then the owner ran it again and the permission named was still
        wrong** — `s3:ListBucket`, not the session. A second defect, hiding
        behind the first: `session.rs` built **two** stores per connection, one
        thrown away on the bucket listing and one installed for reads. What the
        listing learned about which buckets are directory buckets died with the
        first. The listing now goes through the store `open` installs, so a
        connection has one store and one memory.
      - The instrument was upgraded to catch exactly that: it lists **through
        the same store** before reading, and asserts the refusal names
        `s3express:CreateSession`. Run live against a refused bucket (names the
        session) and a permitted one (opens), so it can fail in both
        directions.
      - **Backticks removed from `Error::AccessDenied`'s own message and from
        the guidance beside it**, and the guidance no longer says "not allowed
        to list buckets" for every denial — that cause now covers the account
        listing, one bucket's contents, and a session, and only one of those is
        listing buckets. This is the complaint that opened the session, finally
        answered where it lives rather than one screen over.
  - Paths: `crates/caixonho-core/src/adapter.rs`,
    `crates/caixonho-core/src/classify.rs`
  - Done criteria: the directory listing's call context carries
    `s3express:ListAllMyDirectoryBuckets`, and the session obtained to read a
    directory bucket carries `s3express:CreateSession`. No new classifier
    branch and no matching on error-code strings: the existing mechanism, told
    which call it is describing. Both permission names are quoted from
    `aws-sdk-s3`'s own operation docs, not from memory.
  - Done criteria (test): a denial of the directory listing classifies as a
    denial naming `s3express:ListAllMyDirectoryBuckets`, and never as a denial
    of `s3:ListAllMyBuckets`
  - Verification: `cargo test -p caixonho-core classify::`

## 3. The window

- [x] 3.1 Mark a directory bucket as one [dispatch: main]
      - Done in `main` (2026-08-20); verified: `cargo test --workspace` — 248
        core + 28 window.
      - A `Directory` badge in the name cell, in the capsule vocabulary
        `status_badge` already establishes, tinted `primary` rather than a
        semantic colour: the kind is a fact, not a warning. Carries
        `debug_selector("directory-badge")`.
      - It reads the bucket's `kind`, never its name — the suffix is what the
        badge exists to save the user from reading.
      - **Amended the same evening, on the owner seeing it.** The first version
        put the badge on every row, and on an account that is entirely
        directory buckets that is eight identical badges saying nothing any one
        row did not. The rule `render_access` states one column over —
        *"silence is the good news; a mark on every row is noise and the eye
        stops reading it"* — was already written in this file, and this change
        broke it. Now the badge appears only when the shown list holds more
        than one kind (`shown_kind()`), and a list that is all one kind says so
        **once**, beside the region picker. Four tests, including that the
        decision follows `shown` rather than `rows`, so a region choice that
        narrows to one kind counts as one kind.
  - Paths: `crates/caixonho-gui/src/views/buckets.rs`
  - Done criteria: a directory bucket is identifiable without reading its name
    for a suffix, in the vocabulary `docs/design-language.md` already
    establishes for badges and capsules rather than a new one. Carries a
    `debug_selector` per `XONHO-0015`.
  - Verification: `cargo test -p caixonho-gui`

- [x] 3.2 Keep the service's name, and make the chosen part legible
      [dispatch: main]
      - Done in `main` (2026-08-20); verified: `cargo test -p caixonho-gui
        format::` — 6 tests, and the split is asserted identical for a
        two-segment and a three-segment zone id, which is the rule that would
        otherwise be discovered by meeting the wrong account.
      - `split_zonal_name` splits at the **first** `--` and returns `None` when
        there is nothing to split, so a general bucket with dashes in its name
        is left alone.
      - **In the table** the whole name is shown, unaltered, with the zone half
        quietened. **In the sidebar the zone moves to the item's suffix**,
        because that rail is far narrower than a zonal name and truncation
        there cuts the half that distinguishes buckets while keeping the half
        that is identical on all of them — which is what the screenshot that
        prompted this showed.
      - **The sidebar was got wrong first, and the screen said so.** Putting
        the zone in `SidebarMenuItem::suffix` gave the suffix the width and
        squeezed the label to three letters — `mar`, `viet`, `viet` — which is
        worse than the truncation it was meant to fix, and it was the label,
        not the suffix, that mattered. The zone is now dropped from the rail
        entirely: it is identical on every bucket in it, and the full name is
        on the table row, which has the width for it.
      - **The bucket group now appears only while inside a bucket**, which is
        the purpose its own doc comment claims — keeping sight of the account
        while the main panel is given over to contents. At account level the
        table already lists every bucket with room for the full name, so the
        rail was repeating it in a third of the width. Raised by the owner as
        "listing buckets here doesn't seem sensible"; this is the narrow answer
        to it, and removing the group altogether is the wider one still open.
  - Paths: `crates/caixonho-gui/src/views/buckets.rs`,
    `crates/caixonho-gui/src/views/format.rs`
  - Done criteria: the full name the service returned is what is presented and
    what is copied — it is what every console, policy and other tool shows —
    while the part before `--` reads as the name and the zone reads as the
    zone. Nothing may parse the zone id by segment count: a local zone's is
    three segments (`usw2-lax1-az1`) where a plain availability zone's is two
    (`usw2-az1`), and the account this is verified against holds local-zone
    buckets.
  - Done criteria (test): a name whose zone id has three segments is split the
    same way as one with two
  - Verification: `cargo test -p caixonho-gui`

- [x] 3.3 Say what was refused beside what was found [dispatch: main]
      - Done in `main` (2026-08-20); verified: `cargo test -p caixonho-gui
        failure::` — the wording is pure and asserted without a window.
      - A line above the table, not a panel in its place: buckets came back and
        they are real, so a failure panel would overstate what happened. It
        names the kind that is missing and the permission that would fix it.
        `debug_selector("listing-refused")`.
      - **A second case the task did not name, found while wiring it.** An
        account where the permitted listing returns *nothing* and the other was
        refused used to render "This account has no buckets. The listing
        succeeded" — false on both counts. That arm now branches on the
        refusal, and a refusal is what it says.
      - **The line overflowed the window and was fixed the same evening.** The
        sentence would not shrink below its own text, so it ran off the right
        edge instead of wrapping: the badge now keeps its width and the
        sentence takes what is left, with `min_w_0` so it may actually fold.
        The copy was cut to one clause as well — a line that needs the whole
        window is a line that will overflow the next narrower one.
      - The wording carries **no backticks**. Nothing here renders markdown, so
        a code span arrives as punctuation the reader has to look past — which
        is exactly the complaint that opened this session, one screen over.
  - Paths: `crates/caixonho-gui/src/views/buckets.rs`,
    `crates/caixonho-gui/src/views/failure.rs`
  - Done criteria: when one listing was refused and the other returned buckets,
    the buckets are the screen and the refusal is stated near them — not a
    panel replacing the list, and not absent. The wording names the action that
    was refused.
  - Verification: `cargo test -p caixonho-gui`

## 4. Close-out

- [x] 4.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
      - 2026-08-21: all three exit zero. 250 core + 32 window, 4 ignored (the
        three that were already there, plus this change's live instrument).
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [x] 4.2 CI green on both targets [dispatch: main]
      - Run **32456173681** on `ab9b728`, 2026-08-21: `build (windows-latest)`
        success, `build (macos-latest)` success, `rustfmt` success.
      - The run is for the tip of the four commits this change landed as. It
        was the last task open, and it was open only because nothing had been
        pushed — the work itself was finished and verified the night before.
  - Paths: none
  - Done criteria: the run for the tip shows `build (windows-latest)` and
    `build (macos-latest)` successful; the run id is recorded here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [x] 4.3 Live: an account whose only buckets are directory buckets
      [dispatch: main]
      - **Demonstrated 2026-08-20 → 21, on the account this change was
        planned around.** The connection that showed an access-denied panel
        that morning listed **eight** directory buckets, all in one local zone,
        each attributed to the connection's region rather than to unknown. Then
        one of them opened and was walked four prefixes deep, which is the
        whole zonal path working end to end: `CreateSession` obtained by the
        SDK, a zonal endpoint resolved, `ListObjectsV2` answered. The
        platform's log names the connection and every location listed.
      - **The rest closed over the next two hours**, each piece seen on screen
        by the owner: the list says once that every bucket in it is a directory
        bucket; the refusal of the general listing is stated above the table
        with the permission it needs; a bucket the caller may not open reports
        `s3express:CreateSession` rather than "unexpected error", and those
        buckets now carry "No access" from the probe before anyone clicks them.
      - Two defects were found this way and only this way — a refusal arriving
        as a mystery, and the permission named wrongly once it stopped being
        one. Both are recorded under task 2.3. Neither would have been caught
        by the unit tests that were green throughout.
      - The AWS CLI being off `PATH` is `XONHO-0011`'s task 5.3, not this
        one's; this change never depended on the CLI.
      - Nothing identifying that account — bucket name, account id, zone id —
        is recorded here or anywhere else in this repository. An earlier draft
        of this change put the zone id in `design.md` and in task 3.2 against
        the rule stated in `proposal.md`; both were removed the same day.
  - Paths: none
  - Done criteria: the connection that shows an access-denied panel today
    presents its directory buckets, each marked as one and attributed to its
    region; the refusal of the general listing is stated rather than hidden;
    one bucket opens and its contents (or its emptiness) are shown. What was
    seen is written here — **without** any bucket name, account id or zone id
    from that account, per the knowledge boundary this change was planned
    under.
  - Verification: the log in the platform's log directory names the connection
    and the outcome of each of the two listings

- [x] 4.4 Update the reader-facing documents in this change, not after it
      [dispatch: main]
      - `planned-changes.md`: the "absent by design" section is replaced by
        what they turned into, **keeping the diagnosis** — the next reader
        still needs to know why `ListBuckets` returns none of them. Four
        findings recorded there: three of the four parts are the SDK's, a
        session refusal does not arrive as a denial, `ListObjectsV2` on a
        directory bucket omits `KeyCount` and `IsTruncated`, and a zone id is
        not two segments. The two deferred mechanisms and the rejected
        connection-level switch are carried there with their reasoning.
      - `roadmap.md`: directory buckets out of M5, `XONHO-0016` into the M1
        table as landed, with a paragraph saying it was pulled forward and why.
      - `README.md`: directory buckets in the feature list, and
        `s3express:CreateSession` named beside the permission it is mistaken
        for.
      - `architecture.md`: the listing in the sequence diagram is two calls,
        and can come back as one of each.
      - **`requirements-status.md` is deliberately unchanged, and this task's
        own instruction to update it was wrong.** That file tracks the `[M]`
        requirements of the three M1 areas — its count line says so. Directory
        buckets are `[S]`, so there is no row to mark and no total to recount.
        Adding one would change what the file is for. The task said to recount
        with a script; there was nothing to recount.
  - Paths: `docs/planned-changes.md`, `docs/roadmap.md`,
    `docs/requirements-status.md`, `README.md`
  - Done criteria: the "Directory buckets are absent by design" section is
    replaced by the fact that they are present, keeping the diagnosis so the
    next reader does not re-derive it; `roadmap.md` records the `[S]` as
    landed and says it came ahead of M5 deliberately;
    `requirements-status.md` marks the §4.1 directory-bucket row **done**.
    **Recount the summary line with a script, not by hand** — it drifted twice
    in one day during `XONHO-0006`. `planned-changes.md` also gains the two
    mechanisms `design.md` records as deferred — remembering a refusal against
    the credentials that earned it, and narrowing the list by kind — so they
    are carried rather than lost.
  - Verification: the counted totals match the table rows

- [x] 4.5 Close-out review per `AGENTS.md` [dispatch: main]
      **1. Built what was asked, or what was convenient?** Asked. But the
      review caught **two places where the spec had come to overstate what was
      built**, and both are now amended in the delta rather than left as silent
      departures:
      - "SHALL mark a directory bucket wherever buckets are presented" — the
        owner was right that eight identical badges say nothing, so a uniform
        list now says it once. The requirement says that, with the mixed-list
        case kept as its own scenario.
      - "the name the service returned, unaltered" — true wherever a name is
        shown in full, not true of a 220px sidebar rail. The requirement now
        distinguishes the two and says what a narrow surface must do instead.

      **2. Do the reader-facing documents still tell the truth?** They do now —
      see 4.4. One thing found that is **not this change's**:
      `docs/design-language.md` line 132 carries the owner's employer's name in
      an ASCII sidebar mockup, in a public repository. Raised with the owner;
      left alone pending their answer rather than edited in passing.

      **3. Did we leave rubbish?** No. `cargo clippy --workspace --all-targets
      -- -D warnings` is clean, which is what would catch an unused constant or
      an API nothing calls; the tree holds no `allow(dead_code)`, `TODO` or
      `FIXME`. The `#[ignore]`d instrument stays deliberately: it asserts, it
      is documented with its own command line, and it is the only thing that
      covers the path two defects hid in.

      **4. What is asserted but not verified?** Named rather than glossed:
      - **The paginator walk has never seen a second page.** The verification
        account has eight directory buckets, one page. Nothing proves the loop
        continues correctly.
      - **The window's rendering has no window test.** The badge, the notice
        and the name split are covered as pure functions and through
        `shown_kind`, but nothing asserts what is actually drawn. The
        `debug_selector`s are in place for when `XONHO-0015`'s seam lands.
      - **The store-sharing fix is covered only live.** That a connection uses
        one store — the defect that made the permission name wrong — is proven
        by the instrument against a real account, not by a unit test.
      - **`ListObjectsV2` against a *general* bucket denied in the chain**
        would now be reported as needing `s3express:CreateSession` if that
        bucket were in the remembered directory set. It cannot be, since only
        the directory listing fills that set — but the reasoning, not a test,
        is what holds that.

      **5. What is left, and where is it written?** The two deferred mechanisms
      are in `docs/planned-changes.md` with their reasoning and the rejected
      alternative. Task 4.2 (CI) is the only task still open, and it is open
      because nothing has been pushed yet.
