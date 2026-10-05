# Architecture decisions

Reviewed 2026-10-03. Decisions retain their identifiers and original rationale; implementation acceptance is recorded separately. Current stack: ADR-020/024/032; configured file storage: ADR-035/036; Sandbox pricing: ADR-037; manual currencies and immutable money snapshots: ADR-038.

| Decision | Current disposition |
| --- | --- |
| [ADR-001: Holding isolation](ADR-001-holding-isolation.md) | Holding isolation retained; routing superseded by ADR-023; customer-hosted activation remains a separate architectural target |
| [ADR-002: CQRS and replaceable read models](ADR-002-cqrs.md) | Superseded by ADR-020 |
| [ADR-003: No long-term dual writes](ADR-003-no-long-term-dual-writes.md) | Retained decision; apply current owner contract |
| [ADR-004: Universal expiring RBAC](ADR-004-expiring-rbac.md) | Retained decision; apply current owner contract |
| [ADR-005: Audited global reads](ADR-005-global-read-auditing.md) | Retained decision; apply current owner contract |
| [ADR-006: Customer-approved support capabilities](ADR-006-support-capabilities.md) | Retained decision; apply current owner contract |
| [ADR-007: Stripe authority for commercial state](ADR-007-stripe-authority.md) | Stripe authority retained; offering narrowed to Sandbox by ADR-028/037 |
| [ADR-008: Operations context boundaries](ADR-008-operations-boundaries.md) | Retained decision; apply current owner contract |
| [ADR-009: Marketing and CRM boundaries](ADR-009-marketing-crm-boundaries.md) | Retained decision; apply current owner contract |
| [ADR-010: Shared Dioxus target strategy](ADR-010-dioxus-target-strategy.md) | Deprecated Rust/Dioxus |
| [ADR-011: Customer-hosted data plane and outbound agent](ADR-011-customer-data-plane-agent.md) | Architectural target; no current customer-agent runtime acceptance; directory boundary amended by ADR-025 |
| [ADR-012: Governed Three-Layer Corporate Identity](ADR-012-three-layer-corporate-identity.md) | Retained decision; apply current owner contract |
| [ADR-013: Holding-plus-branch database isolation and scoped directory authority](ADR-013-holding-branch-database-and-directory-authority.md) | Holding isolation retained; Rust physical branch-pair procedures archived; directory assumptions amended by ADR-025 |
| [ADR-014: CQRS bounded-context modular monolith](ADR-014-cqrs-bounded-context-modular-monolith.md) | Deprecated Rust monolith; ADR-020 |
| [ADR-015: Retire n8n as an ERPX runtime dependency](ADR-015-retire-n8n-and-unify-local-runtime.md) | Retained decision; apply current owner contract |
| [ADR-016: Dioxus Native/Blitz atomic cutover](ADR-016-dioxus-native-atomic-cutover.md) | Deprecated Rust/Dioxus |
| [ADR-017: Universal record collaboration and human-approved AI](ADR-017-universal-record-collaboration-and-human-approved-ai.md) | Retained decision; apply current owner contract |
| [ADR-018: Modular AI and separately reviewed hosted commercial offerings](ADR-018-open-source-modular-ai-and-hosted-stepper-pricing.md) | Product boundaries retained; prices superseded by ADR-028/037 |
| [ADR-019: Typed record mentions and human-reviewed field proposals](ADR-019-universal-record-mentions-and-per-module-field-ai.md) | Retained decision; apply current owner contract |
| [ADR-020: Next.js App Router stack](ADR-020-nextjs-stack.md) | Retained decision; apply current owner contract |
| [ADR-021: Social engagement layer (ERP as social network)](ADR-021-social-engagement-layer.md) | Retained decision; apply current owner contract |
| [ADR-022: Cloudflare runtime and persistence](ADR-022-cloudflare-runtime.md) | Runtime superseded by ADR-024 |
| [ADR-023: Three-level workspace authorization and canonical slug routing](ADR-023-three-level-workspace-routing.md) | Retained decision; apply current owner contract |
| [ADR-024: Local Node.js and PostgreSQL behind Cloudflare Tunnel](ADR-024-local-postgresql-runtime.md) | Current runtime; topology/storage amended by ADR-032/035 |
| [ADR-025: Multiple identity providers and scoped identity replacement](ADR-025-multiple-identity-providers-and-scoped-replacement.md) | Retained decision; apply current owner contract |
| [ADR-026: Reviewed local access policies](ADR-026-reviewed-local-access-policies.md) | Retained decision; apply current owner contract |
| [ADR-027: Separate product, workspace and legal document identity](ADR-027-workspace-and-document-branding.md) | Retained decision; apply current owner contract |
| [ADR-028: Sandbox billing: reviewed actual costs plus Support](ADR-028-sandbox-cost-plus-support-billing.md) | Historical Support billing retained; future offering ADR-037 |
| [ADR-029: POS separates bills, fulfillment and provider settlement](ADR-029-pos-fulfillment-and-direct-charges.md) | Retained decision; apply current owner contract |
| [ADR-030: Temporary Samba directory authority on Apple Silicon](ADR-030-temporary-samba-directory-authority.md) | Retained decision; apply current owner contract |
| [ADR-031: ERPX Holding and dual authority for ERPX Management](ADR-031-erpx-holding-and-dual-authority.md) | Retained decision; apply current owner contract |
| [ADR-032: Shared local PostgreSQL cluster for ERPX environments](ADR-032-shared-local-postgresql-cluster.md) | Retained decision; apply current owner contract |
| [ADR-033: Company scoped staff Inbox events and deliveries](ADR-033-company-scoped-staff-inbox-events.md) | Retained decision; apply current owner contract |
| [ADR-034: Inherited GPO rules and atomic Position moves](ADR-034-inherited-gpo-and-atomic-position-moves.md) | Retained decision; apply current owner contract |
| [ADR-035: MinIO for file content; PostgreSQL for configuration state](ADR-035-minio-object-and-configuration-boundary.md) | Retained decision; apply current owner contract |
| [ADR-036: Canonical file sources with policy-bound links](ADR-036-canonical-file-sources-and-policy-bound-links.md) | Retained decision; apply current owner contract |
| [ADR-037: Sandbox AI Gateway and rated Holding billing](ADR-037-sandbox-ai-gateway-and-rated-holding-billing.md) | Retained decision; apply current owner contract |
| [ADR-038: Company manual currencies and immutable money snapshots](ADR-038-company-manual-currency-and-money-snapshots.md) | Retained decision; apply current owner contract |
| [ADR-039: Atomic secondary book projection](ADR-039-finance-book-projection-boundary.md) | Deferred PostgreSQL projection, reviewed openings and independently owned noncontrol book adjustments; same functional currency initially |

Historical Rust terms and dated rollout evidence in older decisions apply to that archived implementation. Use [owner contracts](../contracts/next/README.md) and [catalogue workflows](../contracts/next/catalogue.md) for current coverage, not old phase or workspace counts.

[Contract policy](../../CONTRACT_POLICY.md) requires a new ADR and [cutover](../../CUTOVER.md) entry for a breaking interface. Unrelated documentation changes do not require schema/traceability changes. Cutover retains verified recovery refs for the removed Rust maps and plans.
