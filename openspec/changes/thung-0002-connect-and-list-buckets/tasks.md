> Dispatch routing (passdown gate, 2026-08-19): all 29 tasks → `main` — TDD
> core is judgment-heavy, executor policy ranks the current Claude session
> first, and no task was planned external-ok. Re-gate if that changes.

## 1. Core scaffolding

- [x] 1.1 [dispatch: main] Add `aws-config` (with the `sso` feature), `aws-sdk-s3`,
      `rustls-platform-verifier`, `tokio` and `async-trait` to the workspace,
      pinned, and wire them into `caixonho-core`
- [x] 1.2 [dispatch: main] Add the crate's error type (`caixonho_core::Error`) with one variant
      per cause named in `specs/connections/spec.md`: no credentials, expired
      session, TLS trust, network, access denied (carrying the required IAM
      action), missing configuration, unexpected
- [x] 1.3 [dispatch: main] Add domain types `Profile`, `ConnectionId`, `Bucket` (name, creation
      date, `Region::Known | Unknown`) — no `aws-sdk-s3` type in any public
      signature

## 2. S3 port and its test double

- [x] 2.1 [dispatch: main] Define the async `ObjectStore` trait with `list_buckets`
- [x] 2.2 [dispatch: main] Write the hand-rolled test double: canned success, empty account, and
      one constructor per error variant
- [x] 2.3 [dispatch: main] Write the failing tests first for bucket mapping and for the
      empty-account case (`bucket-listing` spec, scenarios 1 and 3)

## 3. Credential resolution

- [x] 3.1 [dispatch: main] Implement profile discovery from the shared config files, honouring
      `AWS_CONFIG_FILE` / `AWS_SHARED_CREDENTIALS_FILE`, with tests over
      fixture files covering: named + default, no files, malformed entry
- [x] 3.2 [dispatch: main] Implement connection opening for a selected profile via the SDK
      provider chain (static keys, `role_arn` + `source_profile`, SSO cached
      token)
- [x] 3.3 [dispatch: main] Report a missing region as a configuration error rather than
      defaulting, with a test
- [x] 3.4 [dispatch: main] Build the shared HTTP client verifying against the OS trust store,
      honour `AWS_CA_BUNDLE` / `SSL_CERT_FILE`, and hand that same client to the
      `aws_config` loader so credential/SSO calls use it too
      - Built on `aws_smithy_http_client`'s `TrustStore` rather than
        `rustls-platform-verifier`, which the SDK's HTTP client cannot install
        through any supported API — see the amendment in `design.md`

## 4. Error classification

- [ ] 4.1 [dispatch: main] Implement classification at the adapter boundary in one module, in the
      fixed order TLS-trust → network → expired session → access denied →
      missing configuration → unexpected
- [ ] 4.2 [dispatch: main] Test each ordering hazard explicitly, at minimum: a TLS-trust failure
      is not reported as expired credentials, and an expired session is not
      reported as access denied
- [ ] 4.3 [dispatch: main] Test that no error's message or details contain credential material
      (`connections` spec, "Credentials are never disclosed")
- [ ] 4.4 [dispatch: main] Populate the required IAM action on access-denied results

## 5. AWS adapter

- [ ] 5.1 [dispatch: main] Implement `ObjectStore` over `aws-sdk-s3`, mapping `ListBuckets` to
      the domain type with region left `Unknown`
- [ ] 5.2 [dispatch: main] Route every SDK failure through the classifier from task 4.1

## 6. Runtime and bridge

- [ ] 6.1 [dispatch: main] Create one multi-thread tokio runtime at app startup and pass its
      `Handle` into core; remove the runtime the M0 spike creates inline
- [ ] 6.2 [dispatch: main] Model request outcomes as `Loading` / `Loaded` / `Failed`, each
      tagged with the `ConnectionId` it belongs to
- [ ] 6.3 [dispatch: main] Drop outcomes whose `ConnectionId` is no longer the active one, so a
      late response from a previous profile can never render as the new one's

## 7. GUI

- [ ] 7.1 [dispatch: main] Delete the synthetic feed and its generators; keep the virtualized
      table and the channel bridge
- [ ] 7.2 [dispatch: main] Add the profile picker, populated from task 3.1, showing which profile
      is active
- [ ] 7.3 [dispatch: main] Render the bucket list (name, creation date, region or "unknown")
      through the existing table
- [ ] 7.4 [dispatch: main] Render each error kind as its own message with the matching action:
      retry for network, re-login for expired session, trust guidance for TLS,
      required IAM action for access denied
- [ ] 7.5 [dispatch: main] Show the in-flight state, and keep the window responsive while a
      listing is running
- [ ] 7.6 [dispatch: main] Clear the previous profile's results on switch

## 8. Verification and close-out

- [ ] 8.1 [dispatch: main] `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo test --workspace` green locally
- [ ] 8.2 [dispatch: main] CI green on `windows-latest` and `macos-latest`
- [ ] 8.3 [dispatch: main] Manual live check against a real SSO profile and a static-key profile:
      list buckets, let the session expire (or revoke it) and confirm the
      expired-session path; record what was exercised in the change's notes
- [ ] 8.4 [dispatch: main] Update `AGENTS.md` "Current state" to M1 and note the M0 spike's
      retirement
