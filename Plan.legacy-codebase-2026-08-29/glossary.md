# Glossary (Codebase-Derived)

| Term | Meaning | Code ref |
|---|---|---|
| Holding | Top-level isolation boundary; owns a DB pair | `HoldingId`, `holding_registry`, ADR-001/013 |
| Company / Branch | Organizational nodes under a holding | `CompanyId`, `BranchId`, `Organization*Service` |
| AuthorityScope | Platform/Holding/Company/Branch/Subtree scope for authz | `crates/kernel::AuthorityScope` |
| DataClassification | Public < Internal < Confidential < Restricted; propagated via `stricter()` | `crates/kernel::DataClassification` |
| Bounded Context | DDD module owning aggregates & invariants | `crates/contexts/*`, `crates/domain/*` |
| Port / Adapter | Hexagonal boundary vs external tech | `crates/contexts/*/ports.rs` / `crates/adapters/*` |
| CQRS | Writes normalized + outbox; reads from rebuildable projections | ADR-002/014, `outbox_events`, `projection_*` |
| Outbox | Transactional event table claimed by workers | `erpx_control.outbox_events` |
| Projection | Replaceable read model with checkpoint | `projection_checkpoints`, `projection_event_receipts` |
| RBAC assignment | Time-boxed, parent-bounded grant | `role_assignments`, `crates/domain/src/access.rs` |
| Break-glass | Emergency time-boxed elevation, dual-control | `IdentityBreakGlassSession` |
| Collaboration | Thread/comment/reaction/following/mention/action-card on any record | `collaboration` service, `0003..0006` migrations |
| Field Suggestion | Human-approved AI field-level proposal | `agent` service: `Request/Apply/RejectFieldSuggestion` |
| Data-plane agent | Customer-hosted outbound agent | ADR-011, `apps/data-plane-agent` |
| PITR | Point-in-time recovery via WAL archive | `platform_pitr_*`, `wal-archive` |
