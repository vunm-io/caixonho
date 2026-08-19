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
| Region handling that follows `x-amz-bucket-region` instead of a misleading error | none | Probes use a client for the bucket's own region, but the listing path does not follow a redirect, and a wrong region is not even a distinct cause in the classifier |

## §4.2 Browsing — M1

| Requirement | State | Notes |
|---|---|---|
| Bucket list with region and creation date | done | `XONHO-0005` |
| Prefix navigation as folders (`ListObjectsV2`, `delimiter=/`), paginated and lazy | none | `XONHO-0006`. The app cannot open a bucket |
| Virtualized table, 100k+ rows at ~60fps | partial | The table is virtualized and was measured on a synthetic feed in M0. It has never rendered a real listing longer than three rows |
| Columns name, size, last modified, storage class, ETag; sortable, resizable, persisted | none | Bucket columns only; nothing is sortable, resizable or persisted |
| Breadcrumbs plus an editable path bar | none | |
| Client-side filter of loaded rows **and** server-side prefix search, saying which is happening | partial | Region narrowing exists. No name filter, no prefix search |
| Sort honesty — say when a sort covers only loaded rows | none | Nothing sorts yet, so nothing lies yet |

## §4.3 Permission awareness — M1, the headline feature

| Requirement | State | Notes |
|---|---|---|
| Capability observed, never declared; three-valued per scope | done | `ADR-0002`, `openspec/specs/capability-awareness/` |
| Cheap non-destructive probes | done | `ListObjectsV2` for one key |
| Never auto-probe write | done | Enforced by the model |
| Probe budget: viewport-only, debounced, bounded | done | |
| Dimming at bucket/prefix granularity, never per object | partial | Buckets only; there are no prefixes yet |
| Never render "denied" for a different cause | done | Allowlist mapping; a live misreport was fixed 2026-08-19 |
| KMS denial distinguished from an S3 denial | none | No object reads yet |
| Cache per `(profile, bucket, prefix)` with TTL | partial | Cached and invalidated on credential change; no TTL |
| A list-only bucket stays visible | done | |

## §7–8 Non-functional and security

| Requirement | State | Notes |
|---|---|---|
| Crash handling without telemetry — a local file, with a way to attach it to an issue | partial | `XONHO-0012` lands the log and the path to it; the crash hook itself is not written |
| Secrets redacted from all logs, asserted by a unit test | partial | Was vacuous while there were no logs. `XONHO-0012` makes it real |
| No telemetry | done | There is no network path out of this application other than to the endpoint the user chose |
| Dependencies audited in CI | none | The brief promises it; CI runs fmt, clippy and tests only |
| One self-contained binary per platform | partial | It builds on both; there is no installer and nothing is signed |

## §4.4–4.6 — M2 and later

Transfers (7 `[M]`), object operations (3 `[M]`) and quality of life (2 `[M]`)
are not started. They are M2+ and are not late.

## The count

Of the 24 `[M]` requirements in the three M1 areas: **10 done, 7 partial, 7 not
started.** The nearest gaps are now signing in to IAM Identity Center from the app, and
opening a bucket. Entering a credential closed on 2026-08-20.
