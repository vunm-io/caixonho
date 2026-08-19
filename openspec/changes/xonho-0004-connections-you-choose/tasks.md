## 1. Startup stops connecting

- [x] 1.1 [dispatch: main] Test first: opening the application resolves no
      credentials and issues no request, whatever profiles exist.
      - **No GUI test was written, and that is a gap, not a decision.** The
        assertion is about constructing a window, and this crate has no harness
        for that — its only tests are on pure functions. Writing one would mean
        a GPUI test context, a tokio runtime and a real trust stack for a claim
        about a line that is now absent. What *is* covered, in core: a session
        that has been asked for nothing has opened nothing
        (`a_session_with_no_connection_open_probes_nothing`,
        `a_session_that_has_opened_nothing_has_observed_nothing`). The GUI half
        rests on review and on 6.3.
- [x] 1.2 [dispatch: main] Remove the automatic profile selection from
      `CaixonhoApp::new`, and with it the comment that justified it.
- [x] 1.3 [dispatch: main] First screen: the connections, and an invitation to
      choose one — an empty state, not a blank content area.

## 2. A connection is a source

- [ ] 2.1 [dispatch: claude-subagent] Test first: a connection opens from a
      named profile or from a stored credential, and everything above the
      connection behaves identically for both.
- [ ] 2.2 [dispatch: claude-subagent] Introduce the source type in core and
      thread it through `Session::open`; leave the profile path's behaviour
      unchanged.

## 3. The credential store

- [ ] 3.1 [dispatch: claude-subagent] Test first, against a double of the
      credential store: saving keeps the secret only in the store; loading
      returns it; forgetting deletes it; a refusing store is reported as its own
      cause and nothing is written elsewhere.
- [ ] 3.2 [dispatch: claude-subagent] Add `keyring` and implement the store
      behind a port, so the tests above never touch a real keychain.
- [ ] 3.3 [dispatch: claude-subagent] Keep name, region and access key id as
      ordinary configuration; the secret and session token in the store only.
- [ ] 3.4 [dispatch: claude-subagent] Assert that no error, log line or
      diagnostic from a stored-credential failure contains the secret — the
      existing redaction test is the place for it.

## 4. Entering and forgetting

- [ ] 4.1 [dispatch: main] A form for name, region, access key id, secret and
      optional session token, reachable from the sidebar.
- [ ] 4.2 [dispatch: main] Saving makes the connection selectable immediately;
      the keychain is written off the render thread.
- [ ] 4.3 [dispatch: main] Forgetting a connection deletes what was stored and
      removes it from the list.
- [ ] 4.4 [dispatch: main] A store that refuses says so, and the form keeps what
      was typed apart from the secret.

## 5. Unavailable connections

- [ ] 5.1 [dispatch: main] A connection that cannot authenticate is marked in
      the sidebar with its cause.
- [ ] 5.2 [dispatch: main] It stays listed, and its failure is never rendered as
      an empty account.
- [ ] 5.3 [dispatch: main] The cause names what would make it usable, which for
      a spent SSO session is re-establishing it — the action itself is
      `XONHO-0011`.

## 6. Close-out

- [ ] 6.1 [dispatch: main] `cargo fmt --all`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo test --workspace` green.
- [ ] 6.2 [dispatch: main] CI green on both targets.
- [ ] 6.3 [dispatch: main] Live: enter a real key, list with it, forget it, and
      confirm the secret is in the keychain and in no file the app wrote.
- [ ] 6.4 [dispatch: main] Update `docs/requirements-status.md`, and run the
      close-out review in `AGENTS.md`.
