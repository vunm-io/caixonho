## 1. Carrying what the redirect said

- [x] 1.1 Capture `x-amz-bucket-region` onto the failure [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test -p caixonho-core
        classify::` — 28 pass, 4 of them new.
      - `redirect_region_in` is a free function over the response rather than
        an inline block, so the rule it encodes — 301 only, blank is nothing —
        is one thing with one name instead of a condition buried in an arm.
      - The tests go through `from_sdk` with a real `SdkError::ServiceError`,
        not through the private builder the other tests use. The rule under
        test lives in the *extraction*, so a hand-built failure would let a
        misspelled header name or a dropped status check pass unnoticed.
        `ServiceError::builder()` is public and needs no new dependency.
  - Paths: `crates/caixonho-core/src/classify.rs`
  - Done criteria: `SdkFailure` has `redirect_region: Option<String>` and a
    reader for it. `from_sdk` fills it in the `ServiceError` arm, from
    `context.raw().headers().get("x-amz-bucket-region")`, **only when the
    status is 301** — a header on any other response is not a redirect and must
    not be treated as one. An empty or whitespace-only value is `None`, because
    a header that says nothing is the same as no header and the caller must not
    have to know the difference.
  - Done criteria (tests): a synthetic 301 carrying the header yields the
    region; a 301 without it yields `None`; a 200-shaped failure carrying the
    header yields `None`.
  - Verification: `cargo test -p caixonho-core classify::`

- [x] 1.2 A cause for a redirect that names nowhere [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test -p caixonho-core
        classify::` and `capability::` green.
      - `Error::BucketElsewhere { bucket }`, placed as step 8 of `classify` —
        last of the specific causes, ahead of `Unexpected`.
      - `CallContext` gained `bucket: Option<&str>`. All four call sites in
        `adapter.rs` pass it explicitly rather than defaulting, and signing in
        (`sso_adapter.rs`) passes `None` because it is about a session.
      - A 301 on a call scoped to no bucket cannot state this cause, so it
        falls through to `Unexpected` keeping code and status. Tested, because
        it is a branch and not an impossibility.
      - **Found in passing: `capability.rs` held a fixture that had become a
        lie.** Its case named "a wrong-region redirect" and built
        `Error::Unexpected { detail: "PermanentRedirect (HTTP 301)" }` — the
        shape the classifier no longer produces. It would have gone on passing
        for ever while testing nothing it claimed to. Corrected to
        `BucketElsewhere`; the assertion it makes (a redirect is no evidence
        about permission) is unchanged and still holds.
  - Paths: `crates/caixonho-core/src/error.rs`,
    `crates/caixonho-core/src/classify.rs`
  - Done criteria: an `Error` variant for "the bucket is in another region and
    the service did not say which", carrying the bucket. `classify` returns it
    for a 301 with no usable region. Its message names the condition and says
    the connection's region is what to change; it does not say "unexpected" and
    does not mention permissions.
  - Done criteria (test): **a 301 that DOES carry a region must not classify
    as this cause** — that path is followed, not reported, and a classifier
    that answers the same way to both makes the follow unreachable. Assert it
    explicitly rather than trusting the ordering.
  - Verification: `cargo test -p caixonho-core classify::`

## 2. Following it

- [x] 2.1 Say which region served a page [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test --workspace`.
      - `page_at` takes `served_from` as a parameter: only the caller knows
        which region it addressed, and that function sees an answer rather
        than where it came from.
      - The five test call sites pass a named constant,
        `SERVED_FROM_THE_REGION_ASKED`, not a bare `None` — it would have sat
        beside the `more` cursor, also `None`, and two anonymous `None`s in a
        row tell a reader nothing.
      - **Limit, recorded rather than fixed:** `served_from` is `Some` only on
        the page that actually followed a redirect, which is what this change
        specifies. A bucket already known is addressed to its own region, so
        its later pages report `None` and carry no correction. Within a
        session that is enough, because the row was corrected the first time.
        It stops being enough if the bucket list is replaced by a fresh
        listing that restates the old region — the row would then be wrong
        with nothing arriving to correct it. Not changed here: the design was
        reviewed with this reading, and widening it is a decision, not a
        detail.
  - Paths: `crates/caixonho-core/src/types.rs`,
    `crates/caixonho-core/src/listing.rs`,
    `crates/caixonho-core/src/store.rs`
  - Done criteria: `Page` has `served_from: Option<Region>`, `None` where the
    page came from the region it was addressed to. All three construction sites
    updated, no defaulted field — the explicit cost is what stops a future site
    forgetting silently.
  - Verification: `cargo test -p caixonho-core`

- [x] 2.2 Follow the redirect, once, and remember it [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test -p caixonho-core
        adapter::` — 3 new tests over `StaticReplayClient`.
      - `elsewhere: Arc<Mutex<HashMap<String, String>>>` beside `directory`,
        same lock discipline: `region_learned_for` and `remember_region` each
        hold it for one operation and never across an await.
      - `read_page` hands back the SDK's own error instead of a cause, because
        the caller has to look at the failure before deciding it is one —
        classifying on the way past would throw the redirect away.
      - **The region is remembered only after a read that worked.** Storing it
        on the strength of the redirect alone would send every later page to a
        region this connection has never successfully reached.
      - Retries are disabled in the replaying config. These tests assert how
        many times the request went out, and with a retry policy in the way
        that number would be a fact about the retry policy instead of about
        the code under test.
      - The second request is asserted by its URI, not only by the result: a
        reissue that went back to the same region would still have been
        answered by the next scripted response and the result alone would have
        passed.
  - Paths: `crates/caixonho-core/src/adapter.rs`
  - Done criteria: `S3ObjectStore` keeps `HashMap<String, String>` of
    bucket → discovered region, beside the existing directory-bucket set and
    under the same lock discipline (never held across an await). `list_objects`
    asks `redirect_region()` before classifying: `Some` → reissue once through
    `client_for(Region::Known(..))`, record the region, and set
    `served_from`. A redirect on the reissue is reported, not followed. A
    bucket already in the map is addressed to its region from the start.
  - Done criteria (tests): with `StaticReplayClient` — a 301 carrying the
    header followed by a 200 yields the page and `served_from`; the second
    request is addressed to the named region (assert the request, not only the
    result); a second 301 yields a failure; a bucket already known is addressed
    correctly on the first request.
  - Verification: `cargo test -p caixonho-core adapter::`

- [x] 2.3 Add `test-util` for the replay client [dispatch: external-ok]
      - Done in `main` (2026-08-21); verified by measurement, not by
        argument. `cargo tree -p caixonho-core --edges features,no-dev` reports
        `__rustls,default-client,hyper-014,legacy-rustls-ring,rustls-aws-lc`
        both before and after — byte-identical, no `test-util`. With dev edges
        included the same crate reports that set plus `test-util`.
        `cargo build --release -p caixonho-gui` finished in 8.27s having
        pulled no new crate.
      - Follows the precedent already in the repository: `caixonho-gui` takes
        `gpui` with `test-support` the same way.
  - Paths: `Cargo.toml`, `crates/caixonho-core/Cargo.toml`
  - Done criteria: `aws-smithy-http-client` with `features = ["test-util"]`
    under `[dev-dependencies]` only. Confirm the release build does not gain
    it: `cargo tree -e features --no-default-features` comparison, or
    `cargo build --release -p caixonho-gui` unchanged in what it pulls. Note
    in the file **why** dev-only is safe here (resolver 2; not part of the
    `ADR-0001` frozen stack).
  - Verification: `cargo build --release -p caixonho-gui`; `cargo test -p caixonho-core`
  - Routing (2026-08-21): kept in `main` despite the `external-ok` tag. This
    change is a strictly sequential chain and `2.2` does not compile without
    this task, so an external executor buys no wall-clock parallelism —
    `cargo` holds a file lock on `target/`, so a second executor building here
    would serialise behind this session anyway, and the result still needs
    full local verification. The tag stays as planned; it is the routing, not
    the plan, that was decided otherwise.

## 3. The window

- [x] 3.1 Correct the region on the row that was wrong [dispatch: main]
      - Done in `main` (2026-08-21); verified: `cargo test -p caixonho-gui
        buckets::` — 2 new tests.
      - `BucketsDelegate::correct_region` matches **by name, not by index**:
        the account listing can be replaced while a read is in flight, and an
        index taken before that lands on whatever row inherited it. A bucket
        the list no longer holds is simply not found.
      - The narrowing is not re-applied, as designed.
      - `debug_selector` `bucket-region` on the region cell — the one cell in
        this table that another part of the application corrects after the
        fact.
      - **Asserted but not verified: the wiring.** The delegate is tested; that
        `apply_page` calls it with the right bucket is not, because a test that
        drives the window needs the seam `XONHO-0015` owes. Same wall as
        `XONHO-0009` task 6.3.
  - Paths: `crates/caixonho-gui/src/app.rs`,
    `crates/caixonho-gui/src/views/buckets.rs`
  - Done criteria: when a page arrives with `served_from = Some(region)`, that
    bucket's row shows that region. The region narrowing is **not** re-applied
    while the user is inside that bucket — the row is corrected, the view is
    not pulled away. A `debug_selector` names whatever is added, per the
    convention `XONHO-0016` followed.
  - Done criteria (test): the delegate updates the right row and leaves the
    others alone.
  - Verification: `cargo test -p caixonho-gui`

- [x] 3.2 State the unfollowable case where causes are stated [dispatch: main]
      - Done in `main` (2026-08-21), **out of the order this file lists**:
        the match on `Error` in `failure.rs` is exhaustive, so adding the cause
        in 1.2 stopped the GUI compiling until this was written. The compiler
        decided the order, not the document — and the exhaustive match doing
        exactly that is the design working.
      - Verified: `cargo test -p caixonho-gui failure::` — 2 new tests, one
        asserting the words and one asserting a bucket elsewhere does not mark
        the connection unusable. The connection authenticated; it is pointed
        somewhere else.
      - No backticks, and no permission vocabulary — asserted in the test
        rather than left to review.
  - Paths: `crates/caixonho-gui/src/views/failure.rs`
  - Done criteria: the new cause has guidance text naming what happened and
    what to change, in the vocabulary the other causes use, with no backticks
    (nothing here renders markdown — the lesson `XONHO-0016` recorded).
  - Verification: `cargo test -p caixonho-gui`

## 4. Close-out

- [x] 4.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` green [dispatch: main]
      - Done in `main` (2026-08-21); verified: all three exit zero.
        `cargo fmt --all` clean, clippy reports nothing at `-D warnings`,
        `cargo test --workspace` 263 core + 36 window.
  - Paths: whole workspace
  - Done criteria: all three exit zero
  - Verification: the commands themselves

- [ ] 4.2 CI green on both targets [dispatch: main]
  - Paths: none
  - Done criteria: the run for the tip shows `build (windows-latest)` and
    `build (macos-latest)` successful; the run id is recorded here
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 4.3 Live: a bucket outside the connection's region [dispatch: main]
  - Paths: none
  - Done criteria: on a real account, opening a bucket that lives in another
    region shows its contents rather than an error, and that bucket's row then
    reports the region that served it. What was seen is written here.
    **The unit tests do not settle this** — they replay a 301 this repository
    wrote, and no local rig emits a real one. Do not tick 2.2 harder than the
    canned exchange supports; this is where the change is actually accepted.
  - Verification: the log in the platform's log directory names the location
    and the region each call was served from

- [x] 4.4 Update the reader-facing documents in this change, not after it [dispatch: main]
      - Done in `main` (2026-08-21); verified: `scripts/count-requirements.sh`
        agrees with the tables, and a second implementation written
        independently in Python produced identical numbers.
      - **Deviation from the done criteria, deliberately.** The task says the
        §4.1 region row moves from **none** to *done*. It moved to
        **partial**. This file's own legend defines done as "built and
        exercised", and every other network-facing row it marks done cites a
        live exercise ("Exercised live, `XONHO-0003`"); the virtualized-table
        row is partial for precisely this reason — "the claim stands
        unmeasured". The follow has been exercised only against a canned 301
        this repository wrote, which task 4.3 says in as many words is not
        acceptance. Marking it done would be the exact failure the file exists
        to prevent, in the file whose stated purpose is to be diffed against
        reality. **It moves to done when 4.3 passes**, and the row says so.
      - Counting is now `scripts/count-requirements.sh`, committed rather than
        left in a scratchpad: the file already instructed "count it with a
        script" after drifting twice by hand, and an instruction whose tool
        does not exist is one the next person will skip too.
      - `docs/planned-changes.md` records one finding — a hand-built fixture
        in `capability.rs` that kept passing after it stopped testing the case
        it was named for, and the two cheap habits that make that findable.
  - Paths: `docs/requirements-status.md`, `README.md`,
    `docs/planned-changes.md`
  - Done criteria: the §4.1 region row moves from **none** to done.
    **Recount the summary line with a script, not by hand** — it drifted twice
    in one day during `XONHO-0006` and the total stayed right while the split
    went wrong, which is exactly what review does not catch.
  - Verification: the counted totals match the table rows

- [x] 4.5 Close-out review per `AGENTS.md` [dispatch: main]
      - Done in `main` (2026-08-21). The five answers:

      **1. Did we build what was asked, or what was convenient?** What was
      asked. Every scenario in the two delta specs has a test that would fail
      without it: *the bucket is in another region*, *the named region
      redirects in turn*, *a second read of the same bucket*
      (`adapter::tests`, over `StaticReplayClient`); *the service redirects
      without naming a region* (`classify::tests`); *a later call contradicts
      the listing* (`buckets::tests`); and *being redirected is not evidence
      about permission* (`capability::tests`). One departure, written down
      rather than silent: the proposal and task 4.4 both say the
      `requirements-status` row moves to **done**, and it moved to
      **partial** — see 4.4 for the argument. It moves to done when 4.3
      passes.

      **2. Do the reader-facing documents still tell the truth?** Yes, and two
      needed work to keep doing so. `README.md` gained the behaviour, since it
      is user-visible and most clients do not do it. `docs/roadmap.md` gained
      a row — the M1 table said these changes each land on their own and did
      not list this one. `docs/architecture.md` needed nothing, but for a
      reason worth stating: its claim that "a wrong region keeps its own
      cause" was **aspirational when written** — a wrong region became
      `Unexpected`, which is the absence of a cause — and this change is what
      made it literally true. `docs/design-language.md` is untouched; no
      surface changed shape, only the words in one cause and the value in one
      cell.

      **3. Did we leave rubbish?** No. Clippy is clean at `-D warnings`, which
      is what caught the one candidate: `redirect_region()` sat unused between
      1.1 and 2.2, and rather than silencing it the tasks were finished in the
      order that made it live — which is also why §1 and §2 landed in one
      commit instead of a commit that would not have compiled clean. Nothing
      is commented out, no `TODO` was added, and the counting script is not
      throwaway: it is the tool `requirements-status.md` already told its
      readers to use.

      **4. What is asserted but not verified?**
      - **The whole follow, against a real service.** Every test here replays
        a 301 this repository wrote. No local rig emits a real region
        redirect, so the header name, the status, and the SDK's behaviour on
        a genuine redirect are asserted from the crate sources and from
        canned exchanges — not observed. This is 4.3, and it is where the
        change is actually accepted.
      - **The window wiring.** `correct_region` is tested; that `apply_page`
        calls it with the right bucket is not, because driving the window in
        a test needs the seam `XONHO-0015` owes. The same wall
        `XONHO-0009` 6.3 is stopped at.
      - **Retries.** The replaying config disables them so request counts mean
        what they say. Whether the real retry policy would interact with a 301
        differently is not tested — it should not, since 301 is not a
        retryable status, but that is read from the SDK's classifier rather
        than observed.
      - **Concurrency.** The `elsewhere` map is exercised only from single
        sequential reads. Two reads of the same unknown bucket racing would
        both follow and both record the same region — harmless, and
        untested.

      **5. What is left, and where is it written?**
      - 4.3, live acceptance — the only task left in this change, owner's to
        run. `requirements-status.md` moves to **done** on its result.
      - The `served_from`-only-on-the-follow limit, in task 2.1: a bucket list
        replaced by a fresh listing can restate the old region with nothing
        arriving to correct it. A decision, not a detail, so it is recorded
        rather than quietly widened.
      - The fixture-staleness finding, in `docs/planned-changes.md`, with the
        two habits that make it findable at any future close-out that adds a
        cause.
  - Paths: none
  - Done criteria: the five questions answered in writing here, including what
    is asserted but not verified — at minimum, that the follow is proven only
    against a canned exchange until 4.3 says otherwise
  - Verification: the answers exist and name specifics, not reassurances
