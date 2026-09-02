# 05 — Security & Compliance (Codebase-Derived)

**Source:** `crates/kernel`, `crates/domain/src/access.rs`, `crates/domain/src/*`, `crates/identity`, `adr/*`, `crates/migration` (audits), `apps/server/src/*`, `apps/console/src/*`, `CUTOVER.md`.

## 5.1 Authorities & trust

- **Decision hierarchy:** accepted ADR that supersedes wins → later ADR → accepted FSD → bounded-context cores & versioned contracts → accepted designs → migrations/tests/audits (non-weakening). Jurisdiction/sector packs scoped only to declared scope.
- **Isolation:** ADR-001/013 — per-holding physical DB pairs; branch mode explicit; directory views scoped; DSNs/credentials are provider-resolved profile keys, never caller-supplied.
- **Stripe authority:** ADR-007 — billing/commercial state authoritative in Stripe, mirrored via `stripe_event_inbox`, `billing_lifecycles`, `entitlement_allocation_books`.

## 5.2 Identity & access

- **Identifiers:** typed `Uuid` newtypes in `crates/kernel` (`HoldingId`, `CompanyId`, `BranchId`, `PrincipalId`, `AggregateId`, `RecordId`, `SessionId`, `CommandId`, `CorrelationId`, `EventId`, `PolicyId`, `QueryId`, `DataPlaneId`) — nil rejected.
- **Scopes:** `AuthorityScope::{Platform,Holding,Company,Branch,Subtree}` with `canonical_key()` for persistence/audit.
- **RBAC:** ADR-004 — all assignments are expiring (`valid_from/until`, `assignment_version`), parent-bounded, permission-narrowing; `role_assignments`, `role_review_*`, `role_expiry_notifications`, `sod_decisions`, `audit_events`. Break-glass sessions require fresh MFA, separate approver, time-boxed, fully audited.
- **Data classification:** `DataClassification::{Public,Internal,Confidential,Restricted}` with `stricter()` propagation and `is_permitted_by(ceiling)` enforcement.

## 5.3 Auditing

- ADR-005 — global reads audited (`global_read_audits`, `holding_platform_access_audit`), `audit_events` with `actor_ref`, `purpose` (max 256), `correlation_id`. Every fallback/login decision recorded including rejections.

## 5.4 Support & privacy

- ADR-006 — customer-approved support capabilities (`support_capabilities`, `support_access_requests`, `support_sessions`, `support_tickets`), SSA-wrapped access, expirations, SLA notifications.

## 5.5 Collaboration & AI

- ADR-017 — universal record collaboration (threads/comments/reactions/following/mentions/action cards) is human-approved; AI suggestions (`agent` field-suggestions) require explicit apply/reject.
- ADR-018 — open-source modular AI, hosted stepper pricing; `ai_provider` + `agent` contexts.

## 5.6 Operational safety

- `cargo xtask reset-dev` is loopback-only, schema/bucket allow-listed, target-printing, `--confirm RESET_ERPX_DEV` gated (see `CUTOVER.md`).
- `platform_pitr_*` chains, backup retention, wal-archive; development PITR is opt-in leased.
- No synthetic provider success; unconfigured providers remain disabled.
