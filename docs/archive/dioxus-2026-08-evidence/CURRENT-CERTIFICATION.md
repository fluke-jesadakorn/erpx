# ERPX Current Certification Index

This file is the source of truth for the current certification scope. Dated
directories under `audits/` and `CUTOVER.md` are historical evidence and do not
override this index.

## Current scope

- Target: Dioxus Web v2 local-development certification.
- Final gate: `cargo xtask verify` with a clear zero exit status.
- CI parity: fetch locked dependencies, enforce dependency advisory/license/
  source policy, run `cargo xtask verify-core` offline, then run
  `cargo xtask verify-sqlx` against disposable PostgreSQL databases.
- Runtime evidence: recorded Server/Web/worker process catalog, fresh worker
  heartbeats, healthy projection-job state, HTTP readiness, SSR, login/session,
  CSRF, redirect, and browser-to-BFF credential-boundary smoke checks.
- Deferred manual gate: `cargo xtask verify-platforms` for Desktop/Mobile.
- Additive release gate: `cargo xtask verify-web-production` for an immutable
  Web revision and its external production evidence.

## Latest result

Status: **PASS for the Dioxus Web v2 local-development scope** on 2026-08-19
(Asia/Bangkok).

The complete serialized `cargo xtask verify` gate returned exit status 0. It
passed warnings-denied Clippy, the complete workspace test and doc-test suite,
Console feature checks, the 37-chunk Dioxus Web split release build,
architecture checks, disposable PostgreSQL acceptance, strict runtime
readiness for 95 required worker processes, and HTTP/SSR/BFF smoke checks.
During certification, the verifier's readiness route and removed-endpoint
method semantics were aligned with the locked typed-v2 router; a stale worker
was warm-restarted without resetting retained databases or objects before the
successful full rerun.

## Explicit exclusions

- Credentialed external Generic LDAP, AD DS, and UCS certification.
- macOS Native runtime release certification and iOS/Android runtime
  certification. The shared Native/Blitz graph does cross-compile for macOS,
  `aarch64-apple-ios`, and `aarch64-linux-android`; compile success is not
  runtime certification.

These exclusions fail closed: no excluded capability may be represented as
ready by a synthetic local value.

## Web production status

Status: **NOT YET CERTIFIED**. The fail-closed production gate, security
profile, operational metrics, alert rules, runbooks, and versioned evidence
schema are implemented. A release becomes certified only after the gate runs
against its exact immutable HTTPS deployment with current browser, security,
backup/restore, object-recovery, and PostgreSQL PITR evidence. The local
development pass above is not accepted as production evidence.

Typed command, authentication, and public-form Server Function adapters still
fail closed, and Native operation-specific generated gRPC client adapters
still return typed `Unavailable`. Those adapters plus the locked Web and macOS
performance/accessibility evidence are explicit production-release blockers.
