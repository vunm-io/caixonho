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

- [x] 2.1 [dispatch: claude-subagent] Test first: a connection opens from a
      named profile or from a stored credential, and everything above the
      connection behaves identically for both.
- [x] 2.2 [dispatch: claude-subagent] Introduce the source type in core and
      thread it through `Session::open`; leave the profile path's behaviour
      unchanged.

## 3. The credential store

- [x] 3.1 [dispatch: claude-subagent] Test first, against a double of the
      credential store: saving keeps the secret only in the store; loading
      returns it; forgetting deletes it; a refusing store is reported as its own
      cause and nothing is written elsewhere.
      - **Blocks 6.1 until 4.4 lands.** "Its own cause" is a new
        `Error::CredentialStore { connection, problem }`, and `caixonho-gui`
        matches `Error` exhaustively in two places — `unavailable_reason`
        (`app.rs:32`) and `failure_panel`'s guidance (`app.rs:380`). Both need
        an arm, which is 4.4's own work; core is green on its own
        (`cargo test -p caixonho-core`: 133 passed). Deliberately not a
        wildcard arm anywhere: the enum exists so that adding a cause forces
        every reader to decide what it means.
      - "Nothing is written elsewhere" is asserted against the very paths the
        session was handed, not only against the double
        (`a_saved_credential_never_reaches_the_aws_shared_files`).
- [x] 3.2 [dispatch: claude-subagent] Add `keyring` and implement the store
      behind a port, so the tests above never touch a real keychain.
      - `keyring` 4.x is not the 3.x crate with a bigger number: it is a thin
        shim over `keyring-core`, and its whole API arrives through the
        default `v1` feature — `Entry::new(service, username)`,
        `set_password` / `get_password` / `delete_credential`. Read from the
        vendored source rather than recalled. The default features are the
        right ones and no feature list is needed: `v1` pulls
        `apple-native-keyring-store` on macOS and `windows-native-keyring-store`
        on Windows, both target-gated. (`cargo add` prints
        `apple-native-keyring-store` as *disabled*, which is wrong for a
        target-gated optional dependency — the macOS build compiles it.)
      - What no test here covers: whether the real keychain stores, returns and
        refuses as expected. That needs a real keychain — 6.3.
- [x] 3.3 [dispatch: claude-subagent] Keep name, region and access key id as
      ordinary configuration; the secret and session token in the store only.
      - `StoredCredential` (name, region, access key id) and `CredentialSecret`
        (secret, token) are separate types, and the store only ever sees the
        second. Asserted both ways in
        `saving_puts_the_secret_in_the_store_and_the_rest_nowhere`.
      - **Nothing persists the configuration half yet**, so a stored connection
        does not survive a restart. The spec permits this — it says the
        non-secret half *may* be kept as configuration — but "a connection can
        be forgotten" reads oddly when every connection is forgotten at exit.
        Whoever closes 4.x either persists it or writes it up as its own
        change.
- [x] 3.4 [dispatch: claude-subagent] Assert that no error, log line or
      diagnostic from a stored-credential failure contains the secret — the
      existing redaction test is the place for it.
      - Two tests, because there are two directions. `classify.rs`'s existing
        `no_classification_ever_leaks_credential_material` gained the case a
        mistyped stored credential actually produces — a
        `SignatureDoesNotMatch` chain quoting what was signed with — and
        `credentials.rs` gained
        `no_credential_store_failure_ever_discloses_the_secret` for the leak
        route out of the keychain.
      - **Mutation-checked, and the first version of the test was wrong.**
        With the store's error stringified into the message, the readable-string
        assertion passed for `keyring::Error::BadEncoding` — the one variant
        whose documentation says it hands the payload back — because its
        `Debug` renders the secret as `[119, 74, 97, ...]`. The test now checks
        both spellings, and both mutations (`{error:?}` and `to_string()`) turn
        it red.

## 4. Entering and forgetting

- [x] 4.0 [dispatch: claude-subagent] Remember stored connections across
      restarts: the name, region and access key id in a configuration file in
      the platform's config location, never the secret. Forgetting deletes the
      keychain entry first and the configuration entry second, so a failure
      cannot leave an orphaned secret the app can no longer name.
      - Added during implementation. Without it a credential entered in the app
        disappears on restart while its secret stays in the keychain, which
        leaves secrets nobody can see or remove from the application.
      - `connections.rs`, behind a `ConnectionFile` port with a double beside
        the credential store's, so no test touches a real config directory or a
        real keychain. `directories` 6.0.0 resolves the location, read out of
        its own source rather than recalled: macOS
        `~/Library/Application Support/caixonho/connections.toml`, Windows
        `%APPDATA%\caixonho\config\connections.toml`, Linux
        `$XDG_CONFIG_HOME/caixonho/connections.toml`. The frontend loads them
        with `Session::stored_connections()` — synchronous, one small file, no
        keychain and no network, like `discover` beside it.
      - **One rule fixes both orders: the residue of a partial failure is
        always something the application can name and remove, never a secret it
        cannot see.** So remembering writes the configuration entry first and
        the secret second and takes the entry back out if the secret is
        refused; forgetting deletes the secret first and the entry second, and
        does not touch the file at all if the secret could not be deleted.
        Mutation-checked: reversing `forget` turns *two* tests red, from both
        sides — `a_credential_store_that_will_not_delete_stops_before_the_
        configuration_entry` and `a_configuration_that_cannot_be_written_does_
        not_leave_the_secret_undeleted`.
      - The rollback puts the *previous* list back rather than taking the new
        entry out, and the difference is not academic: the two diverge exactly
        when the credential replaced one of the same name — a key being rotated
        — and there the store still holds the previous secret, so removing the
        entry would strand it. Found by review after the first version passed
        every test; `a_replacement_the_store_refuses_leaves_the_connection_it_
        would_have_replaced` was written for it and was red before the fix.
      - **A file this cannot read is reported and left exactly as it is.** Not
        an empty list — a machine whose connections could not be read is not a
        machine with no connections, and saying the second invites the user to
        enter a credential on top of one already there. Not a panic either, and
        not a rewrite: replacing a file we failed to parse would discard every
        connection in it to save the one being written, so a failed read stops
        the write (`a_configuration_this_cannot_understand_is_never_replaced_
        by_one_it_can`). An absent file *is* an empty list — that is a first
        run. Writes go to a staged file and are renamed onto the real one, so a
        write that dies half way leaves the previous list intact.
      - **Mutation-checked, three spellings.** The file test follows
        `no_credential_store_failure_ever_discloses_the_secret` and adds one:
        a secret can reach the file as readable text, as bytes
        (`[119, 74, 97, ...]`), or *escaped by this file's own quoting*. All
        three were confirmed to bite — writing the secret in as a quoted value
        turned it red on the readable spelling, as a byte array on the second,
        and, with a secret made of characters the format escapes, on the third
        alone, where the first two sail past.
      - **Blocks 6.1.** "Its own cause" is a new
        `Error::Connections { problem, path }` with a
        `ConnectionsProblem` of `Unreadable` / `Malformed` / `NotWritable` /
        `NoLocation`, and `caixonho-gui` matches `Error` exhaustively in the
        same two places 3.1 named — `unavailable_reason` (`app.rs:34`) and
        `failure_panel`'s guidance (`app.rs:553`). Both need an arm, and 4.4
        was already ticked when this landed, so the two arms are outstanding
        work that nothing above will pick up on its own; core is green on its
        own (`cargo test -p caixonho-core`: 154 passed). The `caixonho-gui`
        crate was deliberately not edited from here — it was being worked in at
        the time. No wildcard arm anywhere, for 3.1's reason: the enum
        exists so that adding a cause forces every reader to decide what it
        means. Note that neither arm should mark a *connection* unavailable —
        a file that will not parse says nothing about whether any particular
        credential works.
      - What no test here covers: whether the real config directory is
        writable, and whether the path `directories` computes is the one the
        platform actually uses. That needs the three platforms — 6.2 and 6.3.
- [x] 4.1 [dispatch: main] A form for name, region, access key id, secret and
      optional session token, reachable from the sidebar.
- [x] 4.2 [dispatch: main] Saving makes the connection selectable immediately;
      the keychain is written off the render thread.
- [x] 4.3 [dispatch: main] Forgetting a connection deletes what was stored and
      removes it from the list.
      - Offered only for connections this application holds. A profile in
        `~/.aws` is not ours to remove, and offering to would be offering to
        edit a file shared with every other AWS tool on the machine.
      - The row disappears only once the store has actually let go: dropping it
        on a failure would leave a secret nobody can name, which is the same
        orphan 4.0 exists to prevent.
- [x] 4.4 [dispatch: main] A store that refuses says so, and the form keeps what
      was typed apart from the secret.
      - The form stays open on a refusal with everything still in it, including
        the secret: clearing it would punish the user for the keychain's
        refusal, and the secret is in the process's memory either way while the
        form is open.

## 5. Unavailable connections

- [x] 5.1 [dispatch: main] A connection that cannot authenticate is marked in
      the sidebar with its cause.
- [x] 5.2 [dispatch: main] It stays listed, and its failure is never rendered as
      an empty account.
      - Which failures mean the *connection* is unusable is a decision, not a
        rendering detail, so it is a function with tests of its own. A denial
        does not qualify — the connection worked and the permission did not, and
        marking it would send someone to fix a sign-in that is fine.
        Mutation-checked: treating a denial as unavailable turns
        `a_denial_does_not_make_the_connection_unusable` red.
- [x] 5.3 [dispatch: main] The cause names what would make it usable, which for
      a spent SSO session is re-establishing it — the action itself is
      `XONHO-0011`.
      - The advice moved out of the failure panel into a function of the cause,
        because the sidebar banner needs the same sentences. A cause with two
        surfaces and one wording cannot drift between them.

## 6. Close-out

- [x] 6.1 [dispatch: main] `cargo fmt --all`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo test --workspace` green.
      - All three exit 0 on 2026-08-20: 155 core tests, 7 GUI tests.
- [ ] 6.2 [dispatch: main] CI green on both targets.
- [ ] 6.3 [dispatch: main] Live: enter a real key, list with it, forget it, and
      confirm the secret is in the keychain and in no file the app wrote.
- [ ] 6.4 [dispatch: main] Update `docs/requirements-status.md`, and run the
      close-out review in `AGENTS.md`.
