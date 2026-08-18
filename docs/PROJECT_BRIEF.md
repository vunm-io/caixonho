# caixonho — project brief

The living spec. Started from the 2026-08-18 kickoff brief, updated as decisions
land; irreversible decisions get an ADR in `docs/adr/` and are only summarized here.

---

## 1. What this is

A **fast, native, cross-platform desktop S3 client** — the tool the author actually
wants to use instead of S3 Browser, Cyberduck, WinSCP-with-S3, or the AWS console;
those are some mix of feature-poor, slow, ugly, Windows-only, or paid.

Target feel: **Zed-like.** Instant startup, GPU-rendered, no perceptible lag when
scrolling a bucket with 100k objects, keyboard-driven, good-looking enough to enjoy.

Positioning in one line: *a file-explorer-grade S3 client that is honest about
permissions and fast on huge buckets.*

Personal side project, built in the open, public from day zero. The repo being
public and the project being *announced* are separate events: announcement happens
at M4, not before.

### Name

**caixonho**, from Vietnamese *"cái xô nhỏ"* — "the little pail". Renamed from
the launch name *caithung* ("cái thùng" — the bucket) on 2026-08-18, before any
announcement or crates.io publication. Verified unique on crates.io, GitHub and
as a web search token (2026-08-18). Crates: `caixonho-core`, `caixonho-gui`,
`caixonho-cli`; binary `caixonho`.

## 2. Non-goals (say no early, revisit later)

- Not a general multi-cloud file manager (no GCS/Azure Blob in v1 — but do not
  architect them out).
- Not a backup/sync daemon; no background scheduling in v1.
- Not a bucket-administration console: IAM editing, cost analysis, CloudTrail — out.
- **No telemetry, no analytics, no phone-home. Ever.** A stated product value.
- No account provisioning: the app consumes credentials, never creates them.

## 3. Platform plan

| Phase | Target |
|---|---|
| v1 | **Windows 11** and **macOS (Apple Silicon + Intel)** — first-class, shipped together |
| v2 | Linux (Ubuntu first), then a `.deb`/AppImage/Flatpak decision |
| v3 | **CLI** (`caixonho ...`) sharing the same core crate |
| v4 (maybe) | TUI for servers |

Windows is the primary daily driver and **must never be the lagging port**. Note
the renderer reality found during verification: GPUI on Windows renders through
Blade → **Vulkan**. Machines without a working Vulkan driver (VMs, RDP, old
enterprise Intel drivers) are a first-class deployment risk; the app must detect
renderer-init failure and explain it plainly instead of dying silently.

## 4. Feature requirements

Tags: **[M]** must-have v1 · **[S]** should-have v1 · **[L]** later.

### 4.1 Authentication and connections

- **[M]** AWS **IAM Identity Center (SSO)**, including `[sso-session]` profiles.
  - **[M]** Reuse tokens already cached by the AWS CLI (`~/.aws/sso/cache`).
  - **[M]** In-app login via the OIDC device flow (`RegisterClient` →
    `StartDeviceAuthorization` → `CreateToken`) so the AWS CLI is not a hard
    dependency. Both paths ship in v1: cached-token first, device flow next.
    Whether we *write back* to the CLI's token cache is open — verify the cache
    format contract before deciding (reading is safe; writing must be exact).
  - **[M]** Detect expired/invalid tokens and offer re-login inline.
- **[M]** Static credentials (access key + secret + optional session token) in the
  OS keychain — never in a plaintext config file.
- **[M]** Named profiles from `~/.aws/config` / `~/.aws/credentials`, including
  `role_arn` + `source_profile` chains and `mfa_serial` prompting.
- **[S]** Custom endpoint + path-style addressing (MinIO, R2, B2, Wasabi, Ceph).
  MinIO specifically arrives **early as test infrastructure** (M1): integration
  tests against MinIO-in-Docker make listing/transfer tests free. Officially
  *supporting* third-party services (per-service checksum and quirk handling —
  e.g. newer SDK default request checksums that some services mishandle) stays M5
  and needs a per-connection toggle.
- **[M]** Multiple simultaneous connections; switch profile/region live.
- **[M]** Region handling that does not surprise: follow `x-amz-bucket-region`
  redirects instead of reporting a misleading error.
- **[S]** **S3 Express One Zone / directory buckets**: `ListDirectoryBuckets`,
  zonal endpoints, `CreateSession` with silent refresh, `<name>--<az-id>--x-s3`
  naming. Almost no GUI client supports these — a real differentiator.

### 4.2 Browsing

- **[M]** Bucket list with region, creation date, lazily-loaded hints.
- **[M]** Prefix navigation as folders: `ListObjectsV2` with `delimiter=/`,
  paginated and lazy — never "load the whole bucket, then render".
- **[M]** **Virtualized** table: 100k+ entries scroll at ~60fps without ballooning
  memory (this is the M0 spike).
- **[M]** Columns: name, size, last modified, storage class, ETag; sortable,
  resizable, persisted.
- **[M]** Breadcrumbs plus an editable path bar (`s3://bucket/prefix/`, paste to
  navigate).
- **[M]** Client-side filter of loaded rows **and** server-side `prefix` search,
  with the UI stating which is happening.
- **[M]** **Sort honesty** (same principle as filter honesty): S3 only returns
  keys in lexicographic order — there is no server-side sort by size or date.
  Sorting any other column therefore sorts *loaded rows only*; the UI must say
  "sorted within loaded" and offer an explicit, cancellable "scan all to sort".
- **[S]** Recursive search across a prefix: streamed, cancellable, visible
  objects-scanned count.
- **[S]** Object versions view (incl. delete markers).
- **[S]** "Compute folder size" as an explicit, cancellable action — never automatic.
- **[S]** Favorites / pinned buckets and prefixes; recent locations.
- **[L]** Dual-pane mode (local↔remote, remote↔remote).

### 4.3 Permission awareness (headline feature)

Buckets and prefixes the current credentials cannot read or write are visibly
distinct (dimmed + lock badge), and clicking them explains why.

- No API enumerates effective permissions ⇒ capability is **observed, not
  declared**: per-scope `Capability { list, read, write, delete }`, each
  `Unknown | Allowed | Denied`, defaulting to `Unknown` (seeded in
  `caixonho-core::capability`). UI additionally shows a `Probing` state so
  rows don't flicker between unknown and denied.
- Cheap non-destructive probes: `HeadBucket`, `ListObjectsV2 max-keys=1`,
  ranged `GetObject` (`bytes=0-0`) on a known key.
- **Never auto-probe write** — a write probe creates an object. Infer write from
  real failures or probe only on explicit user action.
- **Probe budget:** probes are lazy per viewport (only visible rows, debounced on
  scroll), run under their own small concurrency budget, and never block render.
  200 buckets in an account must not mean 200 startup requests.
- **Dimming granularity is bucket/prefix only.** Per-object permissions cannot be
  known without per-object probes, so caixonho does not pretend: objects are not
  dimmed; a failing object operation produces the rich error instead. (ADR when
  the model lands.)
- **The critical UX rule:** never render "denied" when the truth is an expired
  token, wrong region, network failure, or nonexistent bucket. Map error kinds
  explicitly; show the error code plus the IAM action that would be required
  (e.g. `s3:ListBucket` on `arn:aws:s3:::bucket/prefix/*`).
- **KMS-encrypted objects:** `s3:GetObject` can be allowed while `kms:Decrypt` is
  not; the resulting 403 must be distinguished from an S3 denial — no mainstream
  client does this, and it saves users hours of debugging the wrong policy.
- **[S]** Use `iam:SimulatePrincipalPolicy` when permitted; degrade silently to
  probing when not.
- Cache capability per `(profile, bucket, prefix)` with TTL; invalidate on
  profile switch and re-login.
- Dimmed is not hidden: a list-only bucket stays enterable at the list level.

### 4.4 Transfers

- **[M]** Upload/download files and folders, preserving prefix structure.
- **[M]** Multipart upload with configurable part size/concurrency;
  multipart/ranged download.
- **[M]** Transfer queue panel: per-item and aggregate progress, throughput, ETA,
  pause, resume, cancel, retry-failed, clear-completed.
- **[M]** Retry with exponential backoff + jitter; on `503 SlowDown` /
  throttling, **adaptive concurrency** — shed parallelism when throttled, ramp
  back when clean. (Many small files into one prefix is the classic trigger.)
- **[M]** Drag and drop OS → app. App → OS drag-out of *not-yet-downloaded*
  objects requires `IDataObject` + `CFSTR_FILEDESCRIPTOR` on Windows — assume
  unsupported by the framework until proven otherwise; M0 includes checking
  whether the API exists, and the fallback is drag-out after download-to-temp
  or an explicit "download to…" flow.
- **[M]** Collision policy: overwrite / skip / keep both / ask — remembered per
  session.
- **[M]** **Key↔filesystem safety:** S3 keys may contain characters illegal on
  Windows (`: * ? " < > |` …), keys differing only by case collide on
  case-insensitive filesystems, keys may end in `/` or contain `//`. Downloads
  sanitize deterministically and **report every collision** — silent data loss is
  the one unforgivable file-manager bug. (ADR when the scheme lands.)
- **[S]** Resume interrupted multipart uploads (`ListMultipartUploads`,
  `ListParts`); janitor view for abandoned multipart uploads (they silently cost
  money — a trust feature).
- **[S]** One-way sync folder→prefix (size+mtime, optional checksum), dry-run
  first.
- **[S]** Bandwidth limit.
- **[L]** Server-side copy/move across buckets/accounts without round-tripping.

### 4.5 Object operations

- **[M]** Create "folder" (zero-byte marker); delete single/bulk/recursive with a
  confirmation stating the object count; rename via copy+delete with the UI
  saying it is not atomic.
- **[M]** Copy/move within and across buckets. **`CopyObject` caps at 5 GB** —
  larger objects must transparently use multipart `UploadPartCopy`, or every
  big-file move fails after the user confirmed it.
- **[M]** Properties panel: size, ETag, storage class, encryption, metadata,
  tags, owner, version id.
- **[S]** Edit user metadata/tags; change storage class; set `Content-Type`,
  `Content-Disposition`.
- **[S]** Presigned URLs with chosen TTL + clipboard; warn on long TTLs **and**
  on the harder truth: with SSO/assume-role credentials the URL dies when the
  session token expires, regardless of the TTL chosen.
- **[S]** Preview without full download (ranged first-N-KB for text/log/json/
  yaml/csv; images; markdown); everything else gets explicit "download to open".
- **[S]** Edit-and-save-back small text files.
- **[S]** Delete on a versioned bucket creates a delete marker ⇒ offer **Undo**
  (remove the marker) right after; hide it on unversioned buckets. Cheapest
  trust feature in the list.
- **[L]** Glacier restore; lifecycle viewer; read-only bucket-policy viewer;
  SSE-KMS key selection; Object Lock display.

### 4.6 Quality of life

- **[M]** Dark/light themes following the OS, manually switchable.
- **[M]** Keyboard-first: command palette (`Ctrl/Cmd+P`) covering every action,
  arrow/enter navigation, `Ctrl+C`/`Ctrl+V` objects, `Delete`, `F2`, `Ctrl+F`.
- **[S]** Multi-select, select-all-in-prefix, invert selection.
- **[S]** Persistent per-connection state: last prefix, sort, column widths,
  window geometry.
- **[S]** Operations/log panel exposing the actual API calls made — invaluable
  for debugging permissions; a differentiator versus opaque clients.
- **[L]** In-app updater; portable Windows build.

## 5. Technology

Decided and verified 2026-08-18 — these move fast, re-verify before shipping
identifiers:

- **UI: GPUI + gpui-component, tracked from git with pinned revisions
  ("stack B").** Full context, pin mechanics, and the M0 gates: **ADR-0001**.
  Key verified facts: crates.io `gpui` is frozen at 0.2.2 (2025-10-22) but does
  contain the Windows backend; Windows rendering is Blade→Vulkan;
  `gpui-component@main` requires git gpui and provides the virtualized
  `DataTable`; everything is Apache-2.0.
- **AWS:** `aws-sdk-s3` + `aws-config` (with the `sso` feature for cached-token
  reuse and `sso-session` support); `aws-sdk-ssooidc` for the in-app device flow.
- **Async bridge rule:** AWS is tokio; GPUI has its own executor. Tokio runs on
  background threads; results cross over channels; UI updates happen on GPUI's
  executor. **Nothing that touches the network may run on the render thread.**
  The M0 spike demonstrates the exact pattern.
- **TLS / corporate networks:** use `rustls-platform-verifier` (OS trust store —
  enterprise TLS-inspecting proxies work by construction), wired through the
  `aws_config` loader so it covers service *and* credential/SSO calls (the
  credential path is the classically forgotten one). Honor `AWS_CA_BUNDLE` and
  `SSL_CERT_FILE`; settings field for an extra CA bundle. **Never a
  "disable certificate verification" switch.** Classify trust errors *before*
  the generic credentials-expired path — the message strings overlap.
- **Other:** `keyring` (OS keychain); `directories`/`etcetera` + TOML config
  (no secrets in it); `tracing` with rolling files and **test-covered credential
  redaction**; `thiserror` in core / `anyhow` at edges; `mockall` or hand-rolled
  double for the S3 port; MinIO-in-Docker integration tests; `insta` snapshots;
  `cargo-deny` + `cargo-audit` in CI.

## 6. Architecture

```
caixonho/                    # cargo workspace
├── crates/
│   ├── caixonho-core/       # NO UI. async. all product logic; trait-based S3
│   │                        # port, testable without AWS.
│   ├── caixonho-gui/        # GPUI app: views, theming, keymap, state. Thin.
│   └── caixonho-cli/        # later (v3), same core
└── docs/
    ├── PROJECT_BRIEF.md     # this file
    └── adr/                 # one ADR per irreversible decision
```

Rules: `caixonho-gui` never depends on `aws-sdk-s3` (core re-exports domain
types); every long operation is cancellable and streams progress; concurrency
limits, retry policy and part sizes are core configuration, not UI constants.

## 7. Non-functional requirements

- Cold start **< 500 ms to an interactive window** — network I/O (bucket list)
  is explicitly outside that budget and arrives after first paint.
- Idle memory: single-digit MB above framework baseline; 100k listed objects
  must not mean 100k hydrated UI nodes.
- Scrolling and typing never block on network I/O.
- One self-contained binary per platform; no runtime install.
- Windows: installer + portable exe. macOS: signed/notarized `.dmg` eventually —
  v0.1 ships unsigned with Gatekeeper instructions (verify current Gatekeeper
  behavior before writing them); buy the Apple Developer account when there are
  real external users (decision deferred to M4).
- Graceful offline behavior; graceful **no-Vulkan behavior** on Windows (clear
  error, never a silent crash).
- Crash handling without telemetry: crash reports go to a **local file** with a
  "copy to clipboard for an issue" affordance. No network path exists.
- Accessibility: keyboard reachability for every action is v1; screen-reader
  support best-effort given framework maturity.

## 8. Security requirements

- Secrets in the OS keychain; config holds references only.
- Credentials, session tokens, presigned URLs, `Authorization` headers redacted
  from all logs, asserted by a unit test.
- No telemetry — stated prominently in the README.
- Destructive actions state the object count in their confirmation.
- Dependencies audited in CI; releases checksummed.

## 9. Milestones

| # | Goal | Done when |
|---|---|---|
| **M0a** | Stack builds | CI green (clippy, tests, release build) on windows-latest + macos-latest; artifacts uploaded |
| **M0b** | Stack is smooth | Spike run on real Windows 11 + macOS; ADR-0001 table filled; graceful-failure check on a no-Vulkan machine; drag-out API existence checked |
| **M1** | Read-only browser | SSO + static creds, bucket list, prefix navigation, virtualized table, permission dimming, dark/light; MinIO test rig |
| **M2** | Transfers | Up/download incl. folders, multipart, queue with progress/cancel/retry, drag-in |
| **M3** | Object ops | The *safe* subset first: create folder, delete with counted confirmation, read-only properties, presigned URLs |
| **M4** | Ship v0.1 | Installers for both platforms, README with GIF, CI green — **v0.1 = M1 + M2 + safe-M3**; copy/move/rename ship in v0.2. Public announcement |
| **M5** | Extras | S3-compatible endpoints (official support), directory buckets, sync, versions, full M3 |
| **M6** | Reach | Linux build, then the CLI crate |

## 10. Decision log

| Date | Decision | Where |
|---|---|---|
| 2026-08-18 | Name: **caixonho** | this file §1 |
| 2026-08-18 | UI stack: **GPUI + gpui-component from git, pinned ("stack B")** | ADR-0001 |
| 2026-08-18 | License: **dual MIT OR Apache-2.0** (matches upstream Apache-2.0; widest reuse) | LICENSE-* |
| 2026-08-18 | **Public repo from day zero**; announcement decoupled (M4). Decisive factor: free unmetered CI on public repos vs 2000 min/mo with 2×/10× Windows/macOS multipliers on private | this file §1 |
| 2026-08-18 | v0.1 scope = M1 + M2 + safe-M3 | §9 |
| 2026-08-18 | SSO: cached-token reuse **and** in-app device flow both in v1 | §4.1 |
| 2026-08-18 | MinIO early as test infra (M1); official S3-compatible support M5 | §4.1 |
| 2026-08-18 | macOS signing deferred; v0.1 unsigned | §7 |

## Open items

- Write-back to the AWS CLI SSO token cache: decide after verifying the format
  contract (M1).
- Filesystem-safe key mapping scheme → ADR before M2.
- Verify current macOS Gatekeeper behavior before writing v0.1 install docs (M4).
