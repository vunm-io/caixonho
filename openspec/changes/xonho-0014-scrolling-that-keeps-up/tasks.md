> `design.md` is deliberately absent. Its own instruction makes it conditional,
> and the condition is not met: this change **measures before it decides**, so a
> design written now would record choices nobody has the data for. The one
> decision already made — a curve keyed on pixels per second rather than per
> event — is argued in `proposal.md`, and the thresholds it needs are what
> section 1 exists to find.

## 1. Find out what the device actually sends

- [ ] 1.1 [dispatch: main] Record `(dy, dt, precise?)` for every scroll event
      through the log, not through stderr.
  - Paths: `crates/caixonho-gui/src/scroll.rs`,
    `crates/caixonho-core/src/diagnostics.rs`
  - Done criteria: scrolling the window appends one line per event to the file
    in the platform's log directory, carrying the delta, the milliseconds since
    the previous event, and whether the event was precise. The status bar
    already names that directory (`XONHO-0012`), so whoever ran the instance
    can find it.
  - **Why this and not `eprintln`**: two earlier rounds of measurement produced
    nothing at all, because the output went to stderr and the owner runs their
    own instance from their own terminal — the events landed in a scrollback
    nobody was reading. This is the same failure `XONHO-0012` was written to
    end, and using it is the whole reason this attempt should work.
  - Verification: scroll, then read the log file

- [ ] 1.2 [dispatch: main] Measure the owner's own mouse, and the trackpad,
      and write the distributions down in this file.
  - Paths: this file
  - Done criteria: for each device, the range of `dy` per event, the range of
    `dt` between events, and whether events arrive as precise or as lines.
    Enough to answer the open question directly: does the mouse fall under the
    12 px/event threshold, or does it send lines too sparsely for the streak to
    build?
  - Verification: the numbers are in this file, per device

- [ ] 1.3 [dispatch: main] Say which suspicion the numbers support, in writing,
      before changing any curve.
  - Paths: this file
  - Done criteria: one of the two recorded hypotheses is confirmed or both are
    refuted, with the numbers cited. A third explanation is a legitimate
    outcome and would change what the rest of this change does.
  - Verification: the answer is in this file

## 2. One curve for every precise device

- [ ] 2.1 [dispatch: main] Replace the per-event curve with one keyed on
      pixels per second.
  - Paths: `crates/caixonho-gui/src/scroll.rs`
  - Done criteria: acceleration is computed from `dy / dt` with `dt` clamped to
    a sane band, so a device that chops the same physical gesture into more
    events is not penalised for it. Starting points to measure against, from
    the original note: boost from roughly 800–1000 px/s, maximum around
    4000 px/s — **starting points, not decisions**; section 1's numbers settle
    them.
  - Verification: `cargo test --workspace`

- [ ] 2.2 [dispatch: main] Leave the `Lines` streak path alone.
  - Paths: `crates/caixonho-gui/src/scroll.rs`
  - Done criteria: a genuine notched wheel behaves exactly as it does today;
    the diff touches the precise path only.
  - Verification: `cargo test --workspace`

- [ ] 2.3 [dispatch: main] Unit-test the curve as a pure function.
  - Paths: `crates/caixonho-gui/src/scroll.rs`
  - Done criteria: given a velocity, the multiplier is asserted at the floor,
    at the ceiling, and between — no window required. The curve is arithmetic,
    which is the one part of this that does not need a person to judge it.
  - Verification: `cargo test --workspace`

## 3. Close-out

- [ ] 3.1 [dispatch: main] Feel-test both devices, and record the verdict.
  - Paths: this file
  - Done criteria: the owner scrolls with the mouse and with the trackpad and
    says whether each is right. This is the acceptance criterion and there is
    no substitute for it — scroll feel is not a thing a test can judge.
  - Verification: the owner's own words, in this file

- [ ] 3.2 [dispatch: main] Remove the measurement instrumentation.
  - Paths: `crates/caixonho-gui/src/scroll.rs`,
    `crates/caixonho-core/src/diagnostics.rs`
  - Done criteria: the per-event logging added in 1.1 is gone. A diagnostic
    that outlives its investigation is exactly the rubbish the close-out review
    in `AGENTS.md` looks for — and this one would write a line per scroll
    event, forever, into a bounded log that has better things to hold.
  - Verification: scroll, and see that the log stays quiet

- [ ] 3.3 [dispatch: main] `cargo fmt --all`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo test --workspace` green, and CI
      green on both targets.
  - Paths: whole workspace
  - Done criteria: all four
  - Verification: the commands, and `gh run list --limit 1`

- [ ] 3.4 [dispatch: main] Run the close-out review in `AGENTS.md` and write
      the five answers here.
  - Paths: this file
  - Done criteria: five answers in writing, including what is asserted but not
    verified — which here will include every input device nobody owns.
  - Verification: the answers are in this file
