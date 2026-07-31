# G4-005 / G4-006 authoritative capability boundary

## Delivered behavior

- `SqlxControlPlane::authorize_support_capability` loads the capability, its
  access request, and ticket from the control write database.
- It validates the hash of the 256-bit bearer, all immutable binding fields,
  requested permission, issue/expiry half-open interval, active/revocation
  state, approved request, and non-terminal ticket state.
- `EntitlementService::check_entitlement` combines that result with the
  authenticated session, effective role chain, billing lifecycle, module
  allocation, approval amount/limit, and persisted segregation-of-duties
  decision.
- Customer support approval is bound to the authenticated actor and an active
  scoped `holding_owner` or `customer_security_head` assignment.

## Verification

- `cargo test -p erpx-infrastructure --test support_persistence support_persistence_enforces_idempotency_hash_only_secrets_and_immutable_bindings -- --ignored --nocapture` — PASS on disposable PostgreSQL database; database removed after the run.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS.
- `cargo fmt --all --check` — PASS.
- `bun run lint` — PASS with four pre-existing warnings and zero errors.
- `bunx tsc --noEmit` — PASS.
- Dioxus web and desktop checks — PASS.

## Open requirements

Universal enforcement at every tenant mutation handler, structured scope
containment, represented-user authorization, worker revocation/expiry
processing, and support UI are not claimed by this slice.
