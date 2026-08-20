## Why

The M0 spike built two scroll-acceleration paths and only one of them works
for the person using the application. Discrete wheel notches (`Lines`) get a
streak accelerator, verified with synthetic events. Precise deltas (`Pixels`)
get a stateless velocity curve keyed on pixels *per event*, and the owner's
feel-test passed it on a trackpad.

The owner's actual mouse still scrolls with no acceleration at all.

The suspicion on record — untested, and that matters — is that the mouse emits
*precise* deltas of only a few pixels per event, as a Magic Mouse or a
smooth-scrolling driver does, landing under the curve's 12 px/event threshold
and taking the native path unboosted. A second possibility is not excluded:
that its notches are too far apart for the `Lines` streak to ever accumulate.

Recorded on 2026-08-19 and unnumbered since — it sat in the workspace inbox
with "a new ticket uses `XONHO-NNNN`" and nobody issued one.

## What Changes

This change begins by **measuring**, and its shape follows from that: nobody
can specify a curve for a device whose numbers have never been read. Two
attempts to read them have already failed, and the reason is worth carrying:

- The instrumented build wrote through `eprintln` to stderr. The owner ran
  their own instance from their own terminal, so the output went to that
  terminal and vanished with its scrollback. Two rounds of measurement
  produced zero events for whoever was analysing them.

**That failure is already fixed by something else.** `XONHO-0012` gave this
application a log at a fixed location and a status bar that names it — which
is exactly the remedy the note asked for, written independently and never
connected to it. The measurement this change needs is now cheap: record
`(dy, dt, precise?)` through `diagnostics`, and read the file afterwards from
whichever instance was running.

Then, and only then:

- Replace the pixels-*per-event* curve with one keyed on **pixels per second**
  — `dy / dt` between precise events, with `dt` clamped to a sane band. One
  curve then covers every precise device regardless of how finely it chops its
  events: a 120 Hz trackpad, a Magic Mouse, a driver that smooths on the
  application's behalf. Momentum tails decay through the same curve rather
  than needing their own.
- Keep the `Lines` streak path unchanged for genuine notched wheels.

## Requirements this delivers

**None from `PROJECT_BRIEF.md` §4, and saying so is the point.** No `[M]`
requirement covers scroll feel. What this serves is the promise §1 opens with —
"scrolling a bucket with 100k objects" — and the **M0b** milestone, "the stack
is smooth", which the roadmap records as done bar one cell.

So this is not a requirement being delivered late. It is a defect in something
already called done, on the only input device the owner actually uses.

## Requirements it steps over, deliberately

Everything. `XONHO-0011` (in-app SSO) and `XONHO-0006` (browsing) are both
`[M]` and both ahead of this. This change is small and its first half is
measurement, but it does not jump the queue: it is worth doing when the person
running the application is being irritated by it daily, and worth deferring
otherwise.

## Capabilities

### New Capabilities

None. Scroll feel is a property of the window, not a contract the core owes
anyone — nothing here belongs in `openspec/specs/`.

### Modified Capabilities

None.

## Impact

- `caixonho-gui/src/scroll.rs`: the velocity curve and its thresholds.
- `caixonho-core/src/diagnostics.rs`: possibly one event for the measurement
  phase, which should be removed again when the curve is settled — a
  diagnostic that outlives its investigation is the rubbish the close-out
  review looks for.
- No new dependencies.
