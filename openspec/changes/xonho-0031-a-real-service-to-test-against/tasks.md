# Tasks — XONHO-0031 a real service to test against

> The point of this change is that the owner stops being the only person who
> can accept a generic-S3 flow. So the measure of it is not "tests exist" — it
> is **which roadmap rows can stop saying *awaiting live acceptance***. Task
> 4.2 is where that gets answered, and it is allowed to answer "fewer than we
> hoped".
>
> **Routing.** All `[dispatch: main]`. The work is one new test module plus a
> harness change in a file this session is holding, and the judgement — what a
> flow proves, and what it must not be read as proving — is the whole content.
> `agy` remains this workspace's second-priority executor and earns nothing
> here.

## 1. A service the tests can start

- [x] 1.1 `s3s-fs` as a dev-dependency, and the audit still clean
      [dispatch: main]
  - Paths: `Cargo.toml`, `crates/caixonho-core/Cargo.toml`
  - Done criteria: pinned in `[workspace.dependencies]` beside the others, with
    a comment saying why a test dependency is allowed here at all — `XONHO-0017`
    trimmed this graph deliberately and the next reader deserves the reason.
  - **Verified: `cargo deny check advisories` over the merged graph answers
    `advisories ok`.** 372 crates in `caixonho-core`'s tree, up from 224.

- [x] 1.2 Start one, on a port the OS chooses [dispatch: main]
  - Paths: `crates/caixonho-core/tests/` (new)
  - Done criteria: a helper that binds `127.0.0.1:0`, serves an empty
    filesystem root under a temporary directory, hands back the base URL, and
    stops when dropped. Base domain `localhost`, so virtual-hosted addressing
    resolves.
  - **Port 0, never a constant.** Two tests at once on a fixed port is a
    flake that reads as a product bug.
  - **What cost an hour, and is worth having written down:** the base domain
    must carry the port — `localhost:54321`, not `localhost`. `s3s` matches a
    virtual host with `strip_suffix(base_domain)`, which the port defeats, so
    every bucket request silently fell back to path-style and the service
    answered a *different operation* with HTTP 200. What reached the caller was
    `Unexpected { detail: "the service answered HTTP 200" }` — a message that
    reads like a product defect and is not one. The listener is therefore bound
    before the service is built, because the port is the OS's to choose.
  - Verification: a test that starts one, lists nothing, and stops

## 2. The adapter over real HTTP

- [x] 2.1 Reach it the way the application does [dispatch: main]
  - Paths: `crates/caixonho-core/tests/`
  - Done criteria: a temporary AWS config file naming `endpoint_url` and static
    keys, then `Session::open` — not a hand-built `SdkConfig`. The tier exists
    to test the wiring, and hand-building the configuration tests around it.
  - Verification: one listing that works

- [x] 2.2 Listing, both shapes [dispatch: main]
  - Done criteria: a location read with the delimiter groups into folders and
    objects; the flat walk (`list_keys_under`) returns every key at every
    depth. The same fixture through both, so the *difference* is what is
    asserted rather than each in isolation.
  - Verification: `cargo test -p caixonho-core --test <name>`

- [x] 2.3 Pagination, past one page [dispatch: main]
  - Done criteria: more objects than one page, and the walk reports the total.
    `XONHO-0030` proved this against a scripted double; this proves the
    continuation token the *real* service mints round-trips.
  - **First draft of this test was a lie.** It seeded 25 objects, and the
    adapter sets no `max_keys`, so the service's default of 1000 fetched
    everything in one page — the test passed without ever touching a
    continuation token, while its name reported the requirement as covered.
    1001 objects now, with `pages > 1` asserted; ablated back to 25 and it goes
    red. This is the same failure as `XONHO-0025`'s missing test, caught this
    time before the commit rather than by the owner afterwards.
  - Verification: the test, and the ablation

- [x] 2.4 The conditional write [dispatch: main]
  - Done criteria: a write to a free key lands; a conditional write to a taken
    key comes back as the question, refused **by the service**. This is the
    guarantee `XONHO-0020` claims and the first time anything has checked that
    a real service makes it.
  - Verification: the test

- [x] 2.5 Get, delete, and the round trip [dispatch: main]
  - Done criteria: bytes written come back byte-identical; a delete removes the
    key and the next listing does not show it.
  - Verification: the test

## 3. Flows from the window

- [ ] 3.1 Ungate the driving from the photographing [dispatch: main]
  - Paths: `crates/caixonho-gui/src/app.rs`
  - Done criteria: opening and driving a headless window works on every
    platform; only `capture_screenshot` stays `#[cfg(target_os = "macos")]`.
  - **This is the task the whole change turns on.** A flow that runs only on
    the owner's platform has not removed the owner from the loop.
  - Verification: `cargo test -p caixonho-gui`, and Windows green in CI

- [ ] 3.2 Browse: open a bucket and see what is in it [dispatch: main]
  - Done criteria: through `select_profile` and `open_bucket`, against the real
    service — the rows on screen are the objects the service holds.
  - Verification: the test

- [ ] 3.3 Upload, and the collision question [dispatch: main]
  - Done criteria: a local file uploaded through the window appears in the
    service; uploading it again raises the question rather than replacing.
  - Verification: the test

- [ ] 3.4 Download, and the bytes [dispatch: main]
  - Done criteria: the file on disk afterwards is byte-identical to the object.
  - Verification: the test

- [ ] 3.5 Delete several, and delete a folder [dispatch: main]
  - Done criteria: `XONHO-0030`'s two flows, end to end — tick rows, confirm
    the counted confirmation, and the objects are gone **from the service**;
    then a folder, counted first, and its whole subtree gone.
  - This is the live check the owner was asked for in `XONHO-0030` task 5.3,
    minus the parts only a directory bucket can answer.
  - Verification: the test

- [ ] 3.6 Preview [dispatch: main]
  - Done criteria: a text object previews its first page from a ranged read
    against the real service.
  - Verification: the test

## 4. What this does and does not buy

- [ ] 4.1 A test per exclusion, that fails when the reason stops holding
      [dispatch: main]
  - Done criteria: named tests recording that this service has no versioning
    (so no Undo), no IAM (so no denials), and no directory buckets. Where the
    reason is observable — a delete answering with no version id — assert it.
  - A comment becomes folklore the day the dependency changes. A test does not.
  - Verification: the tests, and the names reading as the exclusions

- [ ] 4.2 Re-state the roadmap honestly [dispatch: main]
  - Paths: `docs/roadmap.md`, `docs/requirements-status.md`
  - Done criteria: go row by row through the eight *awaiting live acceptance*
    rows and say, per row, what is now proven and what is still owed. A row
    whose flow is covered here says so; a row that still needs a directory
    bucket or a versioned bucket **keeps its wording** and names why.
  - **This is where the change is judged.** Overstating here is the failure
    mode, not understating.
  - Verification: the table read against the tests that exist

- [ ] 4.3 Say what it costs [dispatch: main]
  - Done criteria: the suite's wall-clock before and after, recorded here.
    Twenty whole-stack flows at a second each is a minute nobody chose.
  - Verification: `cargo test --workspace` timings

## 5. Close-out

- [ ] 5.1 `cargo fmt --all`, `cargo clippy --workspace --all-targets --
      -D warnings`, `cargo test --workspace` green [dispatch: main]
  - **And a gate check**: walk the file for macOS-gated items and confirm each
    reference sits inside one. Local clippy compiles one branch, so it is not
    evidence about the other target — `XONHO-0030` learned that from a red
    Windows build.
  - Verification: the commands, and the gate walk

- [ ] 5.2 CI green on both targets, run id recorded here [dispatch: main]
  - Verification: `gh run list --limit 1 --repo vunm-io/caixonho`

- [ ] 5.3 Close-out review per `AGENTS.md` [dispatch: main]
  - Question 4 has a known shape here: this change's whole product *is*
    evidence, so the question becomes what the new evidence is worth — and in
    particular whether any existing test is now redundant, or whether any of
    them was only ever passing because a double was kinder than a service.
  - Verification: the recorded findings
