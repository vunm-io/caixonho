# Requirements status

Every `[M]` requirement in `PROJECT_BRIEF.md` §4, and where it actually stands.
This file exists because the brief listed them and nothing checked the plan
against it: work was ordered by what was convenient for the one machine it was
being written on, and mandatory requirements sat unbuilt while optional polish
went ahead of them. A list nobody diffs against reality is a list that stops
being read.

**Update it when a change lands.** A proposal names the requirements it
delivers; this is where that claim is kept.

Legend: **done** — built and exercised. **partial** — some of it works, and the
gap is named. **none** — not started.

## §4.1 Authentication and connections — M1

| Requirement | State | Notes |
|---|---|---|
| IAM Identity Center (SSO), including `[sso-session]` profiles | partial | Resolves through the provider chain when a token is cached. `sso_session` is read only from the profile's own section, so a legacy inline profile or a `source_profile` chain loses the session name and with it the exact re-login command |
| Reuse tokens cached by the AWS CLI (`~/.aws/sso/cache`) | done | Exercised live, `XONHO-0003` |
| In-app login via the OIDC device flow | none | `XONHO-0011`. Until this lands the AWS CLI is a hard dependency, which the brief says it must not be |
| Detect expired/invalid tokens and offer re-login inline | partial | Detecting landed 2026-08-19, and a connection that cannot authenticate now reads as unavailable where it is chosen. The *offer* is still a sentence telling the user to run a command — `XONHO-0011` |
| Static credentials in the OS keychain | done | `XONHO-0004`. Entered in the app, secret in the keychain, the rest in the platform's config location; a stored connection can be forgotten |
| Named profiles, including `role_arn` + `source_profile` chains and `mfa_serial` prompting | partial | Profiles are discovered and chains resolve through the SDK. No MFA prompt |
| Multiple simultaneous connections; switch profile **and region** live | partial | One connection at a time; choosing one is an explicit act, which simultaneity will need. Switching region is not offered |
| Region handling that follows `x-amz-bucket-region` instead of a misleading error | partial | `XONHO-0018`. The listing path follows a 301 to the region the service names, remembers it for later pages, and corrects the bucket's row; a redirect that names nowhere is its own cause rather than "unexpected error". **Partial, not done, because it has only been exercised against a canned exchange this repository wrote** — no local rig emits a real region redirect, so a real account is what accepts it (`xonho-0018` task 4.3) |

## §4.2 Browsing — M1

| Requirement | State | Notes |
|---|---|---|
| Bucket list with region and creation date | done | `XONHO-0005` |
| Prefix navigation as folders (`ListObjectsV2`, `delimiter=/`), paginated and lazy | done | `XONHO-0006`. A page at a time, the next fetched on reaching the end; an empty location and a refused one are never drawn alike |
| Virtualized table, 100k+ rows at ~60fps | partial | Virtualized, measured once on a synthetic feed in M0, and now rendering real listings — but never a long one. The claim stands unmeasured |
| Columns name, size, last modified, storage class, ETag; sortable, resizable, persisted | partial | `XONHO-0006` renders name, size and last modified, and carries storage class and ETag across the port unrendered. Nothing is sortable, resizable or persisted |
| Breadcrumbs plus an editable path bar | done | `XONHO-0006`. The trail is read from the location rather than stored; the path bar is the mode it turns into, and is also how a bucket is opened when the account cannot be listed. `XONHO-0019`: the trail is shown only for the connection it was read on — a switch ends the location instead of leaving the previous account's bucket named. Still **done**: the requirement was built, and this repaired a defect in it rather than delivering it |
| Client-side filter of loaded rows **and** server-side prefix search, saying which is happening | partial | Region narrowing exists. No name filter, no prefix search |
| Sort honesty — say when a sort covers only loaded rows | none | Nothing sorts yet, so nothing lies yet |

## §4.3 Permission awareness — M1, the headline feature

| Requirement | State | Notes |
|---|---|---|
| Capability observed, never declared; three-valued per scope | done | `ADR-0002`, `openspec/specs/capability-awareness/` |
| Cheap non-destructive probes | done | `ListObjectsV2` for one key |
| Never auto-probe write | done | Enforced by the model |
| Probe budget: viewport-only, debounced, bounded | done | |
| Dimming at bucket/prefix granularity, never per object | partial | Prefixes now exist and `Scope::at` gives each one its own question, so the model reaches them. The rendering of a dimmed prefix is not built |
| Never render "denied" for a different cause | done | Allowlist mapping; a live misreport was fixed 2026-08-19 |
| KMS denial distinguished from an S3 denial | none | No object reads yet |
| Cache per `(profile, bucket, prefix)` with TTL | partial | Cached and invalidated on credential change; no TTL |
| A list-only bucket stays visible | done | |

## §7–8 Non-functional and security

| Requirement | State | Notes |
|---|---|---|
| Crash handling without telemetry — a local file, with a way to attach it to an issue | partial | `XONHO-0012` landed the log, bounded and rolled daily, and the status bar names its directory. The crash hook itself is not written, so a panic still leaves nothing behind |
| Secrets redacted from all logs, asserted by a unit test | partial | Real rather than vacuous since `XONHO-0012`: no logging signature accepts a `CredentialSecret`, and the three-spelling test guards it. The gap is the AWS SDK — quiet by default, but `CAIXONHO_LOG` can raise it to levels that carry request and header material, and nothing redacts that |
| No telemetry | done | There is no network path out of this application other than to the endpoint the user chose |
| Dependencies audited in CI | done | `XONHO-0017`. A job of its own runs `cargo deny check advisories` on every push and pull request; a vulnerability fails the build. The four that existed were **removed** rather than accepted — two SDK crates were pulling a legacy TLS client this application replaces anyway — and the seven remaining warnings are accepted one by one with a reason and an expiry that `scripts/check-advisory-expiry.sh` enforces, because cargo-deny has no expiry of its own. **Done rather than partial** unlike the §4.1 region row: this requirement's exercise venue *is* CI, and CI runs it — there is no live behaviour left over for a real account to confirm |
| One self-contained binary per platform | partial | It builds on both; there is no installer and nothing is signed |

## §4.4 Transfers — M2

`XONHO-0007` opened this section on 2026-08-24; the rows exist from the day
the first one moved.

| Requirement | State | Notes |
|---|---|---|
| Upload/download files and folders, preserving prefix structure | partial | `XONHO-0007` down, `XONHO-0020` up: one object each way — download is whole-or-nothing (working path, promoted by rename) and "open" rides the same path through a bounded cache; upload sends one local file into the location on screen. Folders and prefix-structure preservation are not started, and neither is multipart, so uploads are capped at 5 GiB |
| Multipart upload with configurable part size/concurrency | none | Upload territory |
| Transfer queue panel: per-item and aggregate progress, throughput, ETA, pause, resume, cancel, retry, clear | partial | The narrowest honest slice: one transfer at a time, cumulative progress against stated size, cancel that leaves nothing behind. No queue, no aggregate, no pause/resume/retry — the panel is its own change |
| Retry with backoff + jitter; adaptive concurrency on throttle | none | Needs the queue |
| Drag and drop OS → app; app → OS after download | none | M0's API question still stands |
| Collision policy: overwrite / skip / keep both / ask — remembered per session | partial | Ask is the shipped default on **both** sides and the user chooses replace / keep both / abandon per collision; keep-both takes the first free ` (n)` name. The remote side (`XONHO-0020`) is the stronger of the two: the refusal comes from the service via `If-None-Match`, so there is no window in which another writer's object could be destroyed, where the local side is a filesystem check. "Remembered per session" waits for the queue, where remembering has something to attach to |
| Key↔filesystem safety: sanitize deterministically, report every collision | done | `XONHO-0007`, ADR-0004. Percent-encoding (injective, `%` itself encoded), reserved-device and overlong names suffixed by full-key FNV-1a, one scheme on every platform with no `cfg`; every substitution is reported in the window. Property-tested; same-segment and case collisions surface as the existing-file question at the destination |

## §4.5–4.6 — M3 and later

Object operations (3 `[M]`) and quality of life (2 `[M]`) are not started.
They are M3+ and are not late.

## The count

Of the 24 `[M]` requirements in the three M1 areas: **11 done, 10 partial, 3 not
started** — §4.1 has 2 done, 5 partial, 1 not started; §4.2 has 3, 3 and 1;
§4.3, the headline, has 6, 2 and 1. Outside M1, §7–8 stands at 2 done, 3
partial and nothing unstarted, and §4.4 — opened by `XONHO-0007` on
2026-08-24 — at 1 done, 3 partial, 3 not started of its 7.

The nearest gap is signing in to IAM Identity Center from the app — mandatory,
unbuilt, and stepped over by four changes running. Entering a credential and
opening a bucket both closed on 2026-08-20. It is worth saying why it keeps
being stepped over rather than letting the count imply neglect: `XONHO-0011` is
12/19 and every task left in it is a live check only the maintainer can run, so
the changes that went ahead of it went ahead of a queue, not of a decision.

Counted by `scripts/count-requirements.sh`, not from memory. That distinction
is not pedantry: this line has now drifted **twice** in one day — first reading
"10 done, 7 partial" against rows that said 9 and 8, and then, an hour after
being corrected, being rewritten by hand as "11, 8, 5" against rows that said
11, 9 and 4. Both times the total stayed right and the split went wrong, which
is exactly how it survives review. **Count it with a script.** A summary that
disagrees with what it summarises is worse than no summary, in a file whose
whole purpose is to be diffed against reality. The script exists so that
instruction costs nothing to follow.
