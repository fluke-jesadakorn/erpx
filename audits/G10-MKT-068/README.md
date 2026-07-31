# G10-MKT-068 — Multi-tenant Marketing delivery execution

## Delivered behavior

The Marketing delivery worker no longer requires one tenant, holding, or
provider account from environment variables. Its executable handler enumerates
active tenant/holding registrations through the control-plane registry, lists
active outbound provider accounts from authoritative control-plane state, and
processes a globally bounded batch across those bindings.

Each account pass routes a separate tenant write/read pair, constructs the
existing account-bound provider with the shared credential resolver, and runs
the existing durable delivery source. Claim UUIDs, lease ownership, consent
and suppression checks, retry scheduling, idempotency, permanent failure, and
dead-letter state therefore remain tenant-write authoritative. Persistence and
runtime failures are mapped to generic worker errors without exposing tenant,
database, account, credential, or provider details.

The control-plane account listing is bounded to 100 active accounts per
holding, ordered deterministically, and rejects an over-limit result. The
multi-tenant pass is bounded to 1,000 active holdings and a global delivery
batch per worker pass.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all workspace targets passed, with only
  the existing explicitly ignored disposable-database tests remaining
  ignored.
- Focused worker runtime-bound test — 1 PASS; worker target includes 126
  library tests, 1 binary runtime test, and 1 existing ignored database test.
- `cargo check -p erpx-infrastructure`, `cargo check -p erpx-worker`, and
  `cargo check -p erpx-server` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

## Scope and remaining work

This packet removes environment-only tenant/account selection and composes the
durable per-tenant delivery source across active bindings. A separate
control-plane scheduler job/lease/DLQ for the Marketing delivery handler,
vendor-native provider reconciliation, channel onboarding, live external
provider evidence, and the broader Marketing/ERP and migration gates remain
open. No disposable multi-tenant delivery database run or external provider
call is claimed by this packet.
