# Contract conformance evidence

Implemented in G0-003:

- Rust contract version `1` in `crates/domain/src/contracts.rs`.
- Tenant scope containment tests, including branch-restricted subtree → whole-company denial.
- Command idempotency-key and audit-purpose validation.
- Correlation/audit consistency validation.
- Event envelope serialization with event version, aggregate version, causation, correlation, scope, actor, and payload.
- Additive common protobuf context/error messages under `erpx.common.v1`.
- Additive command/query context fields in platform RPC requests.
- Compatibility policy in `erpx/CONTRACT_POLICY.md`.

Verification: `cargo test -p erpx-migration -p erpx-domain -p erpx-proto` passed with 16 tests; server and Dioxus web/desktop checks passed. Server-side presence/authentication and semantic enforcement of those contexts remains a G1-004/G2 gate.
