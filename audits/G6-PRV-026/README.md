# G6-PRV-026 — Authenticated holding lifecycle transitions

## Delivered

- `HoldingProvisioner::transition_holding_state` serializes registry and
  provisioning state under one control-write transaction and rejects
  inconsistent or unsupported transitions.
- Activation is allowed only from provisioning, recovery, or suspended state
  and requires reachable write/read database health plus reachable storage
  health. It increments both lifecycle versions and clears the provisioning
  error marker.
- Suspension is an active-to-suspended terminal command for the current
  operational session; recovery is allowed from active or suspended state.
- Full, suspended, and recovery states are reflected in the registry used by
  holding routing and backup-worker claims. Only active holdings receive new
  backup work; suspended and recovery holdings wait until activation completes.
- Platform Operations exposes authenticated `ActivateHolding`, `SuspendHolding`,
  and `EnterHoldingRecovery` commands with actor binding, platform scope,
  dedicated permissions, safe receipts, and audit records.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-proto -p erpx-infrastructure -p erpx-server` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS; 81 tests.
- `cargo clippy -p erpx-infrastructure -p erpx-server --all-targets -- -D warnings` — PASS.
- `bash audits/G2-IAM-008/verify_authorization_routes.sh` — PASS.
- Lifecycle transition matrix unit coverage — PASS.

## Deliberate boundary

This packet does not claim owner seeding, activation-time organization
initialization, recovery restore orchestration, backup retention cleanup,
integration/settings/audit browsing, or the complete G6 gate. Live disposable
database health and storage probes remain deployment evidence rather than
local unit-test evidence.

## Rollback

Revert only the G6-PRV-026 files listed in the integration ledger. Preserve all
unrelated Folio and ERPX worktree changes.
