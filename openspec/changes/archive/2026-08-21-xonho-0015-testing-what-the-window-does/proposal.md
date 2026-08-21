## Why

Two display defects reached the owner in August while the test suite stayed
green, and three separate places in this repository now point at the same
missing thing:

- `XONHO-0009` task **6.3** is blocked: `capture_screenshot` renders for real,
  but a screenshot is only worth judging if it is of a **real view**, and
  nothing here can build one in a test.
- `XONHO-0018` task **3.1** shipped with the wiring untested. `correct_region`
  has tests; that `apply_page` calls it with the right bucket does not, and
  that is the half a user would notice.
- The seed commit `21edc45` said so itself: *"Testing a real view still needs a
  seam — this app cannot be constructed without a tokio runtime, the machine's
  `~/.aws` and its keychain — and that goes through the planning gate."*

That commit already proved the **tier** exists: `#[gpui::test]`,
`TestAppContext`, `debug_selector`/`debug_bounds` and headless rendering are
on, and two component tests use them. What is missing is not the tooling. It is
one seam.

## What the inventory found

Read out of the crates rather than assumed, because assuming is what
`OPS-0028` exists to stop — and the answer is that **most of the seam is
already built**:

| Piece | Where it is | Verdict |
|---|---|---|
| Keychain double | `Session::secrets: Arc<dyn SecretStore>`, `with_secret_store` | Exists — but `pub(crate)` |
| Connections-file double | `Session::connections: Arc<dyn ConnectionFile>`, `with_connection_file` | Exists — but `pub(crate)` |
| An `ObjectStore` double | `store::double::StoreDouble`, used by 20 core tests | Exists — `#[cfg(test)]` only |
| A store slot to put it in | `Session::store: Arc<Mutex<Option<Arc<dyn ObjectStore>>>>` | Exists — no way in from outside |
| Trust material without the environment | `HttpStack::with_ca_bundle(None)` | **Already public** |
| Config paths without `~/.aws` | `ConfigPaths { config: None, credentials: None }` — both fields public | **Already public** |
| A window that can be driven | `TestAppContext`, `add_window_view`, `debug_bounds` | Already on (`21edc45`) |

So this change is small and specific. Two things are missing:

1. **A supported way to hand those doubles across the crate boundary.** The
   GUI crate cannot reach `pub(crate)` seams that core built for its own tests.
2. **A `CaixonhoApp` that is given its world instead of fetching it.**
   `CaixonhoApp::new` builds a tokio runtime, calls `ConfigPaths::from_env`,
   `discover`, `HttpStack::from_env` and `stored_connections` inline — so
   constructing it in a test reads the developer's own machine, and the result
   differs per machine.

## What Changes

- **`caixonho-core` gains a `test-support` feature** that makes its existing
  doubles and injectors reachable from outside the crate. Off by default, so
  the shipped binary is unchanged — the same arrangement `gpui` uses and that
  `caixonho-gui` already depends on it through.
- **`CaixonhoApp` is given its world.** The environment reading moves out of
  the constructor into what `main.rs` does before calling it. `main.rs`
  behaves exactly as it does today; a test can hand over a world it wrote.
- **The two recorded gaps get their tests**: the `apply_page` → row-correction
  wiring `XONHO-0018` left open, and the screenshot `XONHO-0009` 6.3 is
  blocked on.

No behaviour changes for a user. No breaking change outside this workspace.

## Capabilities

### New Capabilities

None. This changes how the application is **assembled**, not what it does.

### Modified Capabilities

None. Every requirement in `openspec/specs/` means exactly what it meant
before; more of them simply become checkable.

## Impact

**Requirements delivered.** `PROJECT_BRIEF.md` `[M]` requirements: **none**,
and that is stated plainly rather than argued around. This is the verification
tier, not a feature.

**`[M]` requirements still unbuilt ahead of it**, and why this one goes first:

| Unbuilt `[M]` | Why this change goes first anyway |
|---|---|
| In-app OIDC device-flow login (§4.1) | `XONHO-0011`, 12/19 — every remaining task is a live check only the owner can run |
| Region handling follows `x-amz-bucket-region` (§4.1) | `XONHO-0018`, 11/12 — same, task 4.3 |
| Dependencies audited in CI (§7–8) | `XONHO-0017` — built; its last task is a live check too |
| Sort honesty (§4.2) | Nothing sorts yet, so nothing lies yet |
| KMS denial distinguished from an S3 denial (§4.3) | Needs object reads, which do not exist |

The honest summary: **every mandatory row that is actionable is waiting on a
person, not on work.** Of what remains, this is the one that pays for itself —
it is the tier that would have caught the two display defects that reached the
owner in August, and it closes two verification gaps this repository has
already written down against itself.

It is also the cheapest it will ever be: the doubles exist, the tooling is on,
and the seed commit measured the cost at 31s build and 0.01s per test.

**Code.** `crates/caixonho-core/Cargo.toml` and `src/` (a `test-support`
feature over seams that already exist), `crates/caixonho-gui/src/app.rs` (the
constructor split), `crates/caixonho-gui/src/main.rs` (reads the world),
`crates/caixonho-gui/src/views/` (the tests).

**Dependencies.** None added. A feature flag over what is already compiled.
