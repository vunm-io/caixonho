## Context

Everything here was read out of the crates and this repository on 2026-08-21,
not recalled — `OPS-0028` exists because this project has twice claimed a
library could not do something it could.

**What `CaixonhoApp::new` does today**, in one function: builds the child
views, builds a multi-thread tokio runtime, calls `ConfigPaths::from_env()`,
`caixonho_core::discover()`, `HttpStack::from_env()` and
`session.stored_connections()`, then wires four channels. Six of those read the
machine it runs on. A test that calls it reads the developer's `~/.aws` and
their keychain, and answers differently on each machine.

**What is already built**, and is the reason this change is small:

| Piece | Where | State |
|---|---|---|
| Keychain double | `Session::with_secret_store` | Exists, `pub(crate)` |
| Connections-file double | `Session::with_connection_file` | Exists, `pub(crate)` |
| `ObjectStore` double | `store::double::StoreDouble` | Exists, `#[cfg(test)]` |
| Store slot | `Session::store: Arc<Mutex<Option<Arc<dyn ObjectStore>>>>` | Exists, no way in |
| Trust material without env | `HttpStack::with_ca_bundle(None)` | Already `pub` |
| Config paths without `~/.aws` | `ConfigPaths { config: None, credentials: None }` | Fields already `pub` |
| Window driving | `#[gpui::test]`, `add_window_view`, `debug_bounds` | On since `21edc45` |

**One constraint that decides the shape of the screenshot work**, read from
`gpui_platform/src/gpui_platform.rs:85`:

```rust
pub fn current_headless_renderer() -> Option<Box<dyn gpui::PlatformHeadlessRenderer>> {
    #[cfg(target_os = "macos")] { Some(Box::new(MetalHeadlessRenderer::new())) }
    #[cfg(not(target_os = "macos"))] { None }
}
```

Headless rendering is **macOS-only**, by construction, in the pinned revision.
Windows is this project's primary daily driver and a first-class target that
`AGENTS.md` says must never lag. So a screenshot proves something about one of
the two platforms, and `debug_bounds` — which needs no renderer and already
runs green on both — proves it about both.

## Goals / Non-Goals

**Goals:** a real view can be constructed in a test without the machine's
environment; the two gaps already written down against this repository get
tests.

**Non-Goals:**

- A screenshot-comparison suite. Golden images are a maintenance burden this
  project has not earned yet, and on one platform out of two they would prove
  nothing about the other.
- Testing every view. The seam is the change; a sweep is what the seam makes
  possible afterwards, one change at a time.
- Removing `CaixonhoApp::new`. `main.rs` keeps working exactly as it does.

## Decisions

**A `test-support` feature on `caixonho-core`, not `pub` seams.** Making
`with_secret_store` public would put a test-only API in the shipped surface for
ever, and the close-out review in `AGENTS.md` names exactly that — *"API kept
'for later' is how a crate acquires functions nobody dares remove."* A feature
gate says what the API is for, in the one place a reader looks.

*It is also the arrangement already in this tree*: `gpui` is taken with
`test-support` under `caixonho-gui`'s `[dev-dependencies]`, and
`aws-smithy-http-client` with `test-util` under `caixonho-core`'s. This is the
third instance of a pattern, not a new one.

**The application is given its world; it does not fetch it.** A `World` struct
carries what `new` reads today — the runtime, the session, the discovered
profiles, the remembered connections, and the two startup errors — and
`main.rs` is what builds it from the environment.

*Alternative considered:* a trait for "the environment", with a real
implementation and a test one. Rejected — a trait whose only job is to return
five values is a bigger surface than the five values, and it would have to be
implemented twice to say the same thing once.

*Alternative considered:* leave `new` alone and let tests set `AWS_CONFIG_FILE`
and friends to a fixture directory. Rejected — it makes every test that touches
the window mutate process-global state, which is exactly the shape that cannot
be run in parallel, and it still cannot replace the keychain.

**`debug_bounds` is the primary instrument; a screenshot is a second
opinion.** The one measured limit above decides this. A layout assertion runs
on both targets and says what went wrong; a screenshot runs on one and says
that something did. `XONHO-0009` task 6.3 is satisfied by rendering a real view
and capturing it **on macOS**, and the task will say that rather than implying
coverage it does not have.

**The first tests are the two gaps already on record**, not a sweep: the
`apply_page` → row-correction wiring `XONHO-0018` left open, and 6.3. A seam
justified by tests nobody had asked for is a seam justified by itself.

## Risks / Trade-offs

**A feature-gated API only compiles when someone turns it on.** It would be
easy to let `test-support` rot. It does not rot here for a specific reason:
`caixonho-gui`'s dev-dependency enables it, and CI runs
`cargo clippy --workspace --all-targets -- -D warnings` on both targets, so it
is compiled on every push.

**Splitting the constructor moves real startup logic.** The environment reading
is not deleted, it is relocated, and `main.rs` is not covered by tests. The
mitigation is that what moves is a sequence of calls with no branching of its
own, and what stays is everything that decides anything.

**A screenshot test that only runs on macOS can drift on Windows unseen.** It
is a genuine gap and the reason `debug_bounds` leads. Stated in the task rather
than left for someone to discover from a green tick.

**The runtime in a test.** `CaixonhoApp` holds a `tokio::runtime::Runtime` so
spawned work outlives the call. A test must supply one, and a current-thread
runtime is enough — but it is a real object with real threads, and a test that
forgets to keep it alive will see work silently not happen. The `World` owning
it is what makes that hard to get wrong.
