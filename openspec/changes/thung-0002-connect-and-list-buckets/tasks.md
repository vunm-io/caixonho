## 1. Core scaffolding

- [ ] 1.1 Add `aws-config` (with the `sso` feature), `aws-sdk-s3`,
      `rustls-platform-verifier`, `tokio` and `async-trait` to the workspace,
      pinned, and wire them into `caixonho-core`
- [ ] 1.2 Add the crate's error type (`caixonho_core::Error`) with one variant
      per cause named in `specs/connections/spec.md`: no credentials, expired
      session, TLS trust, network, access denied (carrying the required IAM
      action), missing configuration, unexpected
- [ ] 1.3 Add domain types `Profile`, `ConnectionId`, `Bucket` (name, creation
      date, `Region::Known | Unknown`) — no `aws-sdk-s3` type in any public
      signature

## 2. S3 port and its test double

- [ ] 2.1 Define the async `ObjectStore` trait with `list_buckets`
- [ ] 2.2 Write the hand-rolled test double: canned success, empty account, and
      one constructor per error variant
- [ ] 2.3 Write the failing tests first for bucket mapping and for the
      empty-account case (`bucket-listing` spec, scenarios 1 and 3)

## 3. Credential resolution

- [ ] 3.1 Implement profile discovery from the shared config files, honouring
      `AWS_CONFIG_FILE` / `AWS_SHARED_CREDENTIALS_FILE`, with tests over
      fixture files covering: named + default, no files, malformed entry
- [ ] 3.2 Implement connection opening for a selected profile via the SDK
      provider chain (static keys, `role_arn` + `source_profile`, SSO cached
      token)
- [ ] 3.3 Report a missing region as a configuration error rather than
      defaulting, with a test
- [ ] 3.4 Build the shared HTTP client with `rustls-platform-verifier`, honour
      `AWS_CA_BUNDLE` / `SSL_CERT_FILE`, and hand that same client to the
      `aws_config` loader so credential/SSO calls use it too

## 4. Error classification

- [ ] 4.1 Implement classification at the adapter boundary in one module, in the
      fixed order TLS-trust → network → expired session → access denied →
      missing configuration → unexpected
- [ ] 4.2 Test each ordering hazard explicitly, at minimum: a TLS-trust failure
      is not reported as expired credentials, and an expired session is not
      reported as access denied
- [ ] 4.3 Test that no error's message or details contain credential material
      (`connections` spec, "Credentials are never disclosed")
- [ ] 4.4 Populate the required IAM action on access-denied results

## 5. AWS adapter

- [ ] 5.1 Implement `ObjectStore` over `aws-sdk-s3`, mapping `ListBuckets` to
      the domain type with region left `Unknown`
- [ ] 5.2 Route every SDK failure through the classifier from task 4.1

## 6. Runtime and bridge

- [ ] 6.1 Create one multi-thread tokio runtime at app startup and pass its
      `Handle` into core; remove the runtime the M0 spike creates inline
- [ ] 6.2 Model request outcomes as `Loading` / `Loaded` / `Failed`, each
      tagged with the `ConnectionId` it belongs to
- [ ] 6.3 Drop outcomes whose `ConnectionId` is no longer the active one, so a
      late response from a previous profile can never render as the new one's

## 7. GUI

- [ ] 7.1 Delete the synthetic feed and its generators; keep the virtualized
      table and the channel bridge
- [ ] 7.2 Add the profile picker, populated from task 3.1, showing which profile
      is active
- [ ] 7.3 Render the bucket list (name, creation date, region or "unknown")
      through the existing table
- [ ] 7.4 Render each error kind as its own message with the matching action:
      retry for network, re-login for expired session, trust guidance for TLS,
      required IAM action for access denied
- [ ] 7.5 Show the in-flight state, and keep the window responsive while a
      listing is running
- [ ] 7.6 Clear the previous profile's results on switch

## 8. Verification and close-out

- [ ] 8.1 `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo test --workspace` green locally
- [ ] 8.2 CI green on `windows-latest` and `macos-latest`
- [ ] 8.3 Manual live check against a real SSO profile and a static-key profile:
      list buckets, let the session expire (or revoke it) and confirm the
      expired-session path; record what was exercised in the change's notes
- [ ] 8.4 Update `AGENTS.md` "Current state" to M1 and note the M0 spike's
      retirement
