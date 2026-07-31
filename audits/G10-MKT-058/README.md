# G10-MKT-058 — Public Marketing form and preview transport

Date: 2026-07-26

## Scope delivered

The server HTTP boundary now exposes:

- `POST /public/marketing/forms/:form_id/submissions` for anonymous form submissions; and
- `GET /public/marketing/previews/:preview_id?token=...` for expiring preview reads.

Both routes require an active server-side tenant registry route and a valid custom-domain `Host`. The form route additionally requires an `Idempotency-Key`, loads the authoritative published form from tenant write state, checks that the active domain scope contains the form scope, creates a bounded service-attributed command context, and writes through the existing SQLx form command. The preview route hashes and checks the bearer token against write-side state, requires the active domain binding to match the preview template revision, and reads published content from the replacement tenant read database. Raw preview tokens, tenant credentials, and database references do not appear in responses or logs.

The worker executable now registers `marketing_content_preview_expiry` when selected by `ERPX_WORKER_HANDLER`. It routes the configured tenant/holding through `TenantDatabaseRouter`, uses the bounded SQLx expiry adapter, preserves deterministic retry identity, and runs through the existing restart-safe worker polling loop.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; 173 domain, 158 infrastructure with 4 ignored, 78 server, and 108 worker tests passed; worker has 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Worker-focused strict Clippy and tests — PASS; 108 passed, 1 ignored.
- Marketing content live proof — PASS; two disposable tenant databases exercised published-form loading, hashed preview authorization, active-domain binding lookup, replacement-read template loading, forced-RLS lifecycle/rebuild behavior, and exact cleanup. Both databases were dropped and residue was checked.
- `bun run lint` — PASS with four pre-existing warnings in `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections.
- `git diff --check` — PASS.

## Explicitly open

Preview issuance/revocation and domain create/challenge/verification/binding still lack public/authenticated transport RPCs and raw-token/challenge generation adapters. Anonymous routing currently receives the tenant reference through a public header and validates it against the server registry plus active domain binding; a control-plane domain directory and host-only tenant discovery remain open. Public preview/form access has no SSR/Dioxus UI, MinIO object rendering, semantic field validation, privacy retention/erasure, or public abuse/rate-limit policy yet. Durable worker retry/dead-letter persistence, Marketing application-layer ports, vendor adapters/secret management, campaigns/journeys/CRM attribution, remaining ERP contexts, migration/cutover, and Folio write retirement remain open.

No Folio behavior or Folio writes changed.
