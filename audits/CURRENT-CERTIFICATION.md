# ERPX Current Certification Index

This file is the source of truth for the current certification scope. Dated
directories under `audits/` and `CUTOVER.md` are historical evidence and do not
override this index.

## Current scope

- Target: Web-first local-development certification.
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

Status: **PASS for the Web-first local-development scope** on 2026-08-09
(Asia/Bangkok).

`cargo xtask verify` completed with exit status 0 after running the shared core
gate, disposable PostgreSQL acceptance, strict runtime catalog/heartbeat and
projection-state verification, and the HTTP/SSR/BFF smoke suite. The runtime
evidence covered 53 required worker processes and fresh heartbeats. A
still-running process or an HTTP 200 without this strict evidence is not a
future pass.

## Explicit exclusions

- Credentialed external Generic LDAP, AD DS, and UCS certification.
- Desktop/Mobile release certification.

These exclusions fail closed: no excluded capability may be represented as
ready by a synthetic local value.

## Web production status

Status: **NOT YET CERTIFIED**. The fail-closed production gate, security
profile, operational metrics, alert rules, runbooks, and versioned evidence
schema are implemented. A release becomes certified only after the gate runs
against its exact immutable HTTPS deployment with current browser, security,
backup/restore, object-recovery, and PostgreSQL PITR evidence. The local
development pass above is not accepted as production evidence.
