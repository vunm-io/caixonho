# Documentation map

Four kinds of document, kept apart because they answer different questions and
go stale at different rates.

| Where | Answers | Authority |
|---|---|---|
| [`PROJECT_BRIEF.md`](PROJECT_BRIEF.md) | What the product is meant to become | Requirements. Aspirational by design — much of it is not built |
| [`../openspec/specs/`](../openspec/specs/) | What the software is contracted to do **today** | The behaviour contract. If the code and a spec disagree, one of them is a bug |
| [`adr/`](adr/) | Why a hard-to-reverse decision went the way it did | Binding until superseded by another ADR |
| [`planned-changes.md`](planned-changes.md) | What is next, in what order, and why cut that way | A staging list. Entries leave it when they become real changes |
| [`../openspec/changes/`](../openspec/changes/) | The change being built right now, and its finished siblings under `archive/` | Proposal, design, delta specs and task list per change |
| [`architecture.md`](architecture.md) | How the pieces fit at runtime, with diagrams | Explains the invariants; the invariants themselves are in `AGENTS.md` |
| [`roadmap.md`](roadmap.md) | What is built, what is next, and what this project will not do | Direction, not commitment on dates |
| [`requirements-status.md`](requirements-status.md) | Every mandatory requirement in the brief, and whether it is actually built | The diff between the brief and the binary; update it when a change lands |
| [`design-language.md`](design-language.md) | What the interface should look like and why, and the vocabulary it is built from | Binding on UI work — a screen that departs from it is a bug or an amendment |

The brief and the specs overlap on purpose and must not be read as the same
thing: the brief says what is wanted, the specs say what is owed. A feature in
the brief and absent from the specs is not built.

There is deliberately no separate "features" document. A feature is described
once, as a contract, in `openspec/specs/` — a second prose copy would be the
one that goes stale, and a reader would have no way to tell which of the two
was lying.

## How a change is made

Change management runs on [OpenSpec](https://github.com/Fission-AI/OpenSpec)
from M1 onward: explore → propose → specify → apply → archive. A change starts
as a directory under `openspec/changes/<task-id>-<slug>/` holding a proposal
(why), delta specs (what), a design (how) and a task list. When it lands, the
delta specs are merged into `openspec/specs/` and the directory moves to
`openspec/changes/archive/`.

Every commit carries the task ID of the change it belongs to, in the
Conventional Commit scope: `feat(XONHO-0005): …`.

## Task numbers

Project work uses `XONHO-NNNN`. Small housekeeping that is not project work
uses the owner's cross-repository `OPS-NNNN` sequence instead, so the project's
own numbers stay meaningful.

The sequence is not derivable from `openspec/changes/` alone — housekeeping
tasks never get a change directory — so this is the index:

| ID | What it covers | Where it lives |
|---|---|---|
| `XONHO-0001` | Renaming the project from *caithung* to *caixonho* | commit history |
| `XONHO-0002` | `scripts/mac-app.sh`, so the binary opens like a real macOS app | commit history |
| `XONHO-0003` | Connecting to an account and listing its buckets, with failure causes told apart | `openspec/changes/archive/` |
| `XONHO-0004` | Entering credentials in the app, the OS keychain, session lifetime | planned |
| `XONHO-0005` | A bucket list you can act on: regions, filtering, observed capability | `openspec/changes/archive/` |
| `XONHO-0006` | Opening a bucket and browsing objects by prefix | planned |
| `XONHO-0007` | Downloading objects | planned |
| `XONHO-0008` | Previewing text and images with ranged GETs | planned |
| `XONHO-0009` | The app shell, the palette and the four states | `openspec/changes/` |

Two slugs appear in the history and are retired. `THUNG-0001` and `THUNG-0002`
were written before the project was renamed; they are never reused, and the
work that was in flight as `THUNG-0002` was renumbered to `XONHO-0003` rather
than closed under a dead name. Numbers are never recycled, and a gap means a
number was spent, not skipped.

## For contributors

[`../AGENTS.md`](../AGENTS.md) is the working agreement for this repository —
crate boundaries, the invariants that must not be broken, commit and branch
conventions, and when an ADR is required. It is written for AI coding agents
and is just as binding on people; read it before opening a pull request.
