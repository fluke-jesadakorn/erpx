# G6-PRV-012 — authenticated Platform Operations projection status transport

## Delivered behavior

Added `ListProjectionStatuses` to the Platform Operations gRPC service. The
handler requires an authenticated session, platform scope, the existing
platform holding-read permission, bounded optional holding/holding/projection
filters, and a bounded continuation cursor. It records a platform audit event
with actor and correlation attribution before returning typed status rows.

The transport uses an opaque hex-encoded cursor containing only the validated
status ordering fields. Responses contain scheduler state and bounded error
metadata only; payloads, claim IDs, worker IDs, credentials, and physical
database references are not exposed.

## Files changed

- `crates/proto/proto/erpx/platform/v1/platform.proto`
- `apps/server/src/main.rs`
- `audits/G6-PRV-012/README.md`

No database migration, Folio route, environment file, or runtime service state
changed. The subsequent G6-PRV-013 packet moves this handler to the physically
separate rebuildable control-read status adapter; scheduler-to-read publishing
remains a follow-up dependency.

## Verification

From `erpx/`:

```text
cargo fmt --all --check — PASS
cargo check -p erpx-server — PASS
cargo test -p erpx-server --bin erpx-server — PASS: 61 tests
cargo clippy -p erpx-server --all-targets -- -D warnings — PASS
cargo clippy --workspace --all-targets -- -D warnings — PASS
cargo test --workspace — PASS: all active tests
cargo check -p erpx-console --no-default-features --features web — PASS
cargo check -p erpx-console --no-default-features --features desktop — PASS
```

Security tests cover default/max page bounds, cursor round-trip opacity,
malformed cursor rejection, and filter-bound pagination behavior in the
infrastructure adapter. Folio `bun run lint` and `bunx tsc --noEmit` also pass;
lint retains four existing warnings in `infra/scripts/start-n8n.js`. Folio
remains on port 3004 and PostgreSQL on 5432.

## Unfinished work and rollback

Projection rebuild/replace actions, scheduler-to-read publishing, live
freshness producers, and the complete Platform Operations UI/data journey
remain open. Rollback removes only the additive protobuf messages/RPC, server
handler/helpers/tests, and this audit; no schema rollback is required.
