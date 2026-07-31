# G1-003-WORKER-COMPOSITION-009

## Delivered

The worker composition now registers named `customer_360`, `stripe_semantic`, and `customer_access_audit_summary` handlers, accepts injected future handlers, and runs a bounded number of independently identified polling tasks. The existing infrastructure loop remains the owner of outbox claiming, ordering, projection receipts/checkpoints, retry/backoff, dead-lettering, replay, and redacted correlation context.

The worker exposes an explicit readiness composition with `ready`, `not_probed`, and `unavailable` states. Heartbeats require a recent injected observation. Tenant routing requires active registry metadata plus matching reachable write/read/storage health. Projection freshness is ready only after `activated`; backup/PITR is ready only for a matching completed and verified run. Unknown or partial evidence fails closed.

## Verification

- `cargo fmt -p erpx-worker -p erpx-infrastructure -- --check` — pass
- `cargo check -p erpx-worker --tests` — pass
- `cargo check -p erpx-worker --all-targets` — pass
- `cargo clippy -p erpx-worker --tests -- -D warnings` — pass
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — pass
- `cargo clippy -p erpx-infrastructure --tests -- -D warnings` — pass
- `cargo test -p erpx-worker --lib -- --nocapture` — 6 passed, 1 ignored disposable-PostgreSQL test
- `cargo test -p erpx-infrastructure --lib -- --nocapture` — 26 passed, 3 ignored disposable-PostgreSQL tests
- `cargo test -p erpx-infrastructure --test sqlx_control_plane -- --nocapture` — 0 passed, 1 ignored disposable-PostgreSQL test

Focused worker tests cover handler registration/duplicate rejection, injected handler failure propagation, bounded heartbeat classification, fail-closed readiness aggregation, tenant health matching, projection switch states, and backup verification states. Existing infrastructure unit tests cover retry policy, redacted correlation context, and safe worker primitives. Live database restart/idempotency/claim-recovery tests remain ignored unless disposable PostgreSQL URLs are supplied.

## Boundaries

No live scheduler, tenant database routing, projection freshness query, backup/PITR executor, external service, server transport, protobuf, identity, commercial provider, domain contract, migration, or ledger work is claimed by this packet.

Rollback is limited to the worker composition/report additions and this evidence file; existing claiming, projector, checkpoint, DLQ, replay, and handler implementations remain intact.
