## 1. Confirm the region assumption

- [x] 1.1 [dispatch: main] With the credential store unlocked, call `ListBuckets`
      twice against the development account — once with no parameters, once with
      an explicit page size — and record whether `BucketRegion` appears only in
      the second. This decides section 2.
      - Dispatched: main (2026-08-19) — confirmed; verified: two live calls on
        the development account. No parameters returned `Name`, `CreationDate`
        and `BucketArn` only; `--max-buckets 100` returned those plus
        `BucketRegion` (`ap-southeast-1`) for all three buckets. The documented
        rule — a region is reported when the request carries at least one valid
        parameter — holds on this account.
- [x] 1.2 [dispatch: main] If regions do not come back on the listing, switch
      section 2 to per-bucket location lookups and note the reason here. The
      specs are unaffected either way.
      - Not needed: 1.1 confirmed the listing carries regions, so section 2
        stands as planned and the per-bucket fallback is unused.

## 2. Regions on the listing (core)

- [x] 2.1 [dispatch: claude-subagent] Test first: the S3 double returns buckets
      with and without a region; assert the domain bucket carries the reported
      region, and carries unknown — never the connection's region — when the
      service reported none.
- [x] 2.2 [dispatch: claude-subagent] Send an explicit page size on the listing
      request so the service reports regions, keeping the existing pagination.
- [x] 2.3 [dispatch: claude-subagent] Map the reported region onto the domain
      bucket type; keep `aws-sdk-s3` types out of the GUI's reach.
      - Dispatched: claude-subagent (2026-08-19) — section 2 complete; verified:
        `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
        warnings` and `cargo test --workspace` re-run in the main session, all
        exit 0. 2.3 turned out to be already satisfied — `map_bucket` read the
        reported region correctly all along, but the parameterless request meant
        it never had one to read, so 2.2 was the whole defect.

## 3. The list probe (core)

- [x] 3.1 [dispatch: claude-subagent] Test first: a list probe against the
      double resolves to allowed on success, denied on an authorization denial,
      and leaves the capability untouched for expired session, rejected
      credentials, wrong region, network failure and throttling.
- [x] 3.2 [dispatch: claude-subagent] Add the probe to the S3 port and its
      double: list the bucket's contents with a maximum of one key.
- [x] 3.3 [dispatch: claude-subagent] Implement it in the adapter, classifying
      failures through the existing classifier rather than inspecting SDK errors
      again.
- [x] 3.4 [dispatch: claude-subagent] Route each probe through a client for the
      bucket's own region, built lazily and cached for the session, sharing the
      credentials provider and the HTTP client from `tls.rs`. Assert in a test
      that resolving credentials happens once, not once per region.
      - Dispatched: claude-subagent (2026-08-19) — section 3 complete; verified:
        `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
        warnings` and `cargo test --workspace` re-run in the main session, all
        exit 0, 75 core tests. `connection.rs` and `tls.rs` needed no change:
        the SDK config already carries a lazy identity cache, and a regional
        client copies the provider, the cache and the HTTP client from it. The
        single-resolution claim is asserted behaviourally, not by inspection —
        four regions probed, four resolutions recorded, one cache partition.

## 4. The capability store (core)

- [x] 4.1 [dispatch: claude-subagent] Test first: observations are keyed by
      credentials and scope; a switch of credentials discards them; a successful
      real operation records allowed without a probe.
- [x] 4.2 [dispatch: claude-subagent] Implement the store over the existing
      `Observation` model, leaving the three-valued type unchanged.
- [x] 4.3 [dispatch: claude-subagent] Wire profile switching and
      re-authentication to discard the store's contents.
      - Dispatched: claude-subagent (2026-08-19) — section 4 complete; verified:
        same three commands, all exit 0, 59 core tests. The discard hangs off
        `Session::open`, which is the only route to a fresh listing today and
        covers both switch and re-authentication. Noted for sections 6-7: a
        plain "refresh the list" control must not go through `Session::open`, or
        it will throw away the capability cache along with it.

## 5. The probe scheduler (core)

- [x] 5.1 [dispatch: claude-subagent] Test first: submitting a viewport probes
      only unobserved scopes, never the same scope twice concurrently, and never
      exceeds the in-flight cap; a scope already observed produces no request.
- [x] 5.2 [dispatch: claude-subagent] Implement the scheduler with a fixed
      in-flight budget and an in-flight set, exposing the set so the UI can show
      what is being probed.
- [x] 5.3 [dispatch: claude-subagent] Assert that a large viewport submission
      issues probes for the submitted rows only, and that rendering never waits
      on one.
      - Dispatched: claude-subagent (2026-08-19) — section 5 complete; verified:
        `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
        warnings` and `cargo test --workspace` re-run in the main session, all
        exit 0, 100 core tests. Each guard was mutation-checked red on its own
        test. One test was rewritten after it was found to hang rather than fail
        under mutation: `spawn_blocking` plus `tokio::time::timeout` never
        surfaced its panic, because the runtime waits for blocking tasks at
        shutdown; a plain thread with `recv_timeout` fails in five seconds.

## 6. Region selector (GUI)

- [x] 6.1 [dispatch: main] Offer the distinct regions among the listed buckets,
      plus "all regions", and a choice for buckets with no known region.
- [x] 6.2 [dispatch: main] Filter the rows already held; issue no request on
      selection.
- [x] 6.3 [dispatch: main] Keep the selection sensible across a profile switch:
      a region absent from the new account falls back to all regions rather than
      showing an empty table.
      - Dispatched: main (2026-08-19) — section 6 complete; verified: same three
        commands, all exit 0, 9 new tests in core. The choice and what it admits
        live in core so the CLI can narrow the same way; the window holds only
        which choice is selected. Mutation-checked: sweeping unstated-region
        buckets into a named region turns
        `a_bucket_of_unstated_region_is_not_swept_into_a_named_one` red.

## 7. Capability in the list (GUI)

Two things section 5 left for this section, discovered while building it:

- **Nothing signals that a probe settled.** The scheduler writes into the
  capability store and updates its in-flight set, but emits no notification, so
  7.2 either reads the state each frame or the scheduler gains a sink — a
  `Fn(Scope)` handed to it at construction and called as each probe finishes.
- **A probe's structured error is dropped** once it has been mapped to an
  observation. The IAM action a list denial needs is the constant `s3:ListBucket`,
  so 7.3 can name it without plumbing; showing the *cause* of a failure that was
  not a denial would need the error carried out of the scheduler.


- [ ] 7.1 [dispatch: main] Report the visible rows to the scheduler, debounced,
      as the user scrolls.
- [ ] 7.2 [dispatch: main] Present four states per row — probing, enterable,
      not enterable, and not yet known — with only observed denials rendering as
      not enterable.
- [ ] 7.3 [dispatch: main] Group or dim the buckets observed to be unlistable
      without removing them, and show the cause and the required IAM action on
      request.
- [ ] 7.4 [dispatch: main] Handle the all-denied account: every bucket visible,
      nothing presented as an empty or failed listing.

## 8. Verification and close-out

- [ ] 8.1 [dispatch: main] `cargo fmt --all`, then
      `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo test --workspace` green locally.
- [ ] 8.2 [dispatch: main] CI green on `windows-latest` and `macos-latest`.
- [ ] 8.3 [dispatch: main] Live check on the development account: regions
      populated and filterable, probes visible as they resolve, and a bucket the
      credentials cannot enter presented as such with its IAM action named.
      Record what was exercised here.
