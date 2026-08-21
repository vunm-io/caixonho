## 1. Carrying what the redirect said

- [ ] 1.1 Capture `x-amz-bucket-region` onto the failure [dispatch: main]
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

- [ ] 1.2 A cause for a redirect that names nowhere [dispatch: main]
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

- [ ] 2.1 Say which region served a page [dispatch: main]
  - Paths: `crates/caixonho-core/src/types.rs`,
    `crates/caixonho-core/src/listing.rs`,
    `crates/caixonho-core/src/store.rs`
  - Done criteria: `Page` has `served_from: Option<Region>`, `None` where the
    page came from the region it was addressed to. All three construction sites
    updated, no defaulted field — the explicit cost is what stops a future site
    forgetting silently.
  - Verification: `cargo test -p caixonho-core`

- [ ] 2.2 Follow the redirect, once, and remember it [dispatch: main]
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

- [ ] 2.3 Add `test-util` for the replay client [dispatch: external-ok]
  - Paths: `Cargo.toml`, `crates/caixonho-core/Cargo.toml`
  - Done criteria: `aws-smithy-http-client` with `features = ["test-util"]`
    under `[dev-dependencies]` only. Confirm the release build does not gain
    it: `cargo tree -e features --no-default-features` comparison, or
    `cargo build --release -p caixonho-gui` unchanged in what it pulls. Note
    in the file **why** dev-only is safe here (resolver 2; not part of the
    `ADR-0001` frozen stack).
  - Verification: `cargo build --release -p caixonho-gui`; `cargo test -p caixonho-core`

## 3. The window

- [ ] 3.1 Correct the region on the row that was wrong [dispatch: main]
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

- [ ] 3.2 State the unfollowable case where causes are stated [dispatch: main]
  - Paths: `crates/caixonho-gui/src/views/failure.rs`
  - Done criteria: the new cause has guidance text naming what happened and
    what to change, in the vocabulary the other causes use, with no backticks
    (nothing here renders markdown — the lesson `XONHO-0016` recorded).
  - Verification: `cargo test -p caixonho-gui`

## 4. Close-out

- [ ] 4.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` green [dispatch: main]
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

- [ ] 4.4 Update the reader-facing documents in this change, not after it [dispatch: main]
  - Paths: `docs/requirements-status.md`, `README.md`,
    `docs/planned-changes.md`
  - Done criteria: the §4.1 region row moves from **none** to done.
    **Recount the summary line with a script, not by hand** — it drifted twice
    in one day during `XONHO-0006` and the total stayed right while the split
    went wrong, which is exactly what review does not catch.
  - Verification: the counted totals match the table rows

- [ ] 4.5 Close-out review per `AGENTS.md` [dispatch: main]
  - Paths: none
  - Done criteria: the five questions answered in writing here, including what
    is asserted but not verified — at minimum, that the follow is proven only
    against a canned exchange until 4.3 says otherwise
  - Verification: the answers exist and name specifics, not reassurances
