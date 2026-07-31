# G4-006-SUPPORT-INTERSECTION-005 evidence

## Delivered behavior

- Added a transport-neutral `SupportIntersectionRequest` and fail-closed `SupportIntersectionDenial` evaluator.
- Checks active agent identity and device, exact session agent/customer-approved state, session expiry/revocation, approved permission and scope, typed-to-string scope binding, capability secret and immutable binding, capability mode/expiry/revocation, platform role, role validity, parent cascade, role support permission, domain policy, SoD, module entitlement/activation, billing state, and mutation semantics.
- Impersonation requires an explicitly represented user, active represented identity, matching current role, active parent cascade, containing scope, and requested permission. Missing represented authority is denied.
- Added stable `Scope::canonical_key()` for binding typed scope to existing support session/capability string fields.
- Hardened `RoleAssignment::effective_at` so delegated roles are inactive when their required parent is absent, mismatched, expired, revoked through the supplied cascade, or not top-level in the bounded one-parent API.
- Added `SupportIntersectionService` as an application facade over the pure domain evaluator.

## Files changed

- `crates/domain/src/support.rs`
- `crates/domain/src/access.rs`
- `crates/application/src/lib.rs`
- `audits/G4-006-SUPPORT-INTERSECTION-005/evidence/README.md`

No protobuf, server, infrastructure, migration, UI, Folio, or shared planning files were changed.

## Tests and exact results

- `rustfmt --edition 2024 --check crates/domain/src/support.rs crates/domain/src/access.rs crates/application/src/lib.rs` — passed.
- `cargo test -p erpx-domain --lib` — 51 passed, 0 failed.
- `cargo test -p erpx-application --lib` — 6 passed, 0 failed.
- `cargo clippy -p erpx-domain -p erpx-application --all-targets -- -D warnings` — passed.

Focused domain tests cover the complete allow path and denial/boundary behavior for identity, device, session approval/expiry/revocation, exact scope and permission, capability secret/binding/mode/expiry/revocation, platform role and parent cascade, policy, SoD, entitlement activation, billing read-only mutation, and represented-user missing/inactive/parent/scope/permission authority.

## Security review and unresolved integration

- The request type does not derive `Debug` or `Serialize`; bearer secret material cannot be logged or transported accidentally through this contract.
- The evaluator does not load identity, roles, entitlements, policy, or capability state. Main/server must resolve those from authoritative stores and must not allow transport input to construct trusted policy or role snapshots.
- The current bounded role API accepts one parent reference. A delegated parent with its own parent is denied unless Main supplies/extends a complete cascade resolver; this is intentionally fail-closed.
- `ModuleEntitlement.active` is the activation check available in the current domain contract. Company/branch allocation activation must be resolved before constructing the entitlement or added by the provisioning/billing integration.
- Main must invoke this evaluator before support diagnostics, tenant mutation, or impersonation, and must bind the resulting decision to the authenticated device/session and action audit. No transport integration was attempted here.

This packet does not close G4. Support capability issuance/revocation persistence, customer approval transport, support authorization interceptors, diagnostics transport, incident/support UI, and end-to-end customer-audited actions remain outside this packet.

## Integration order and rollback

Integrate after authoritative identity/session, role, commercial entitlement, support capability, and customer-approval readers are available; then wire Main/server authorization to `SupportIntersectionService::authorize` before action dispatch. Preserve the existing session/capability write paths.

Rollback removes only the additive evaluator, canonical scope helper, application facade, focused tests, and this evidence file. Existing support ticket/session/capability and universal access APIs remain otherwise intact.
