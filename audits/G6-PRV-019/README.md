# G6-PRV-019 — authenticated projection rebuild intent

## Delivered behavior

Platform Operations now exposes an authenticated `StartProjectionRebuild`
command. It requires a valid command context, platform scope, actor
attribution matching the access token, and the dedicated
`platform.projection.rebuild` permission. The transport accepts only the
allowlisted `holding_projection_status` projection and a positive source
version. Candidate database references are validated as opaque server-side
references; URLs, credentials, and connection material are not returned.

The command records durable holding replacement-read state as `rebuilding`
through `HoldingProvisioner::begin_read_rebuild` and records an audited platform
action. Physical candidate provisioning, projection replay, verification, and
activation remain worker/composition-root responsibilities; this command cannot
directly switch an active read database.

## Files changed

- `crates/proto/proto/erpx/platform/v1/platform.proto`
- `apps/server/src/main.rs`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`

## Verification

```text
cargo fmt --all — PASS
cargo test -p erpx-server --bin erpx-server — PASS: 63 tests
cargo check -p erpx-console --no-default-features --features web — PASS
cargo check -p erpx-console --no-default-features --features desktop — PASS
```

Transport tests prove projection allow-listing, positive source-version
validation, opaque candidate-reference rejection, and the existing actor and
platform-scope boundaries. Existing disposable provisioning evidence
independently proves the downstream verified replacement-read state machine;
this packet does not claim worker execution or product-surface wiring is
complete.

## Open requirements and rollback

Worker job dispatch, candidate physical provisioning, replay verification,
activation UI, backup/restore adapters, and complete Platform Operations
actions remain open. Rollback removes the additive RPC, permission boundary,
audit action, and generated contract fields; it does not touch existing holding
switch state or active read routing.
