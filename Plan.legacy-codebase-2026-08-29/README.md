# ERPX Requirements — Codebase-Derived Baseline (v2)

> **Source of truth:** this document set is generated from the current codebase at `/` on 2026-08-29.
> Legacy `Plan/` content has been archived to `Plan.legacy-2026-08-29/`. All requirements below trace to implemented artifacts: `crates/proto`, `crates/domain`, `crates/contexts`, `apps/console`, `apps/server`, `crates/migration`.

## How to use

- **Verification:** `cargo xtask verify` is the complete gate. See `README.md` at repo root.
- **Fast checks:** `cargo xtask check-affected` / `cargo xtask test-affected`.
- **Structure:** `Plan/01-architecture.md` (system), `Plan/02-bounded-contexts.md` + `Plan/contexts/*.md` (per-context), `Plan/03-data-and-persistence.md`, `Plan/04-interfaces.md`, `Plan/05-security-compliance.md`, `Plan/06-operations-and-deployment.md`, `Plan/glossary.md`, `Plan/traceability.md`.

## System at a glance

- **Web-first Rust monorepo** — Dioxus console on `127.0.0.1:3004`, Tonic gRPC `127.0.0.1:50051`, HTTP auth/health `127.0.0.1:50052`. Desktop/Mobile deferred (`verify-platforms`).
- **Modular monolith, DDD + Clean + Hexagonal + CQRS** — `crates/kernel` → `crates/domain` → `crates/contexts/<ctx>` → `crates/adapters/<ctx>-sqlx` → `apps/server` + `apps/worker`. Domain has zero framework/transport deps.
- **Protocols:** 35 bounded contexts expose gRPC services defined under `crates/proto/proto/erpx/*/service.proto` (≈571 RPCs across Command/Query/Stream services). See `Plan/04-interfaces.md`.
- **Persistence:** 7 SQL migrations (`crates/migration/migrations/0001..0007`) define control/tenant schemas; authoritative writes are normalized + outbox → replaceable read projections (ADR-002/014).
- **Isolation:** per-Holding physical isolation with optional per-Branch pair (`holding_pair` vs `holding_and_branch_pairs`), branch-scoped directory views (ADR-001/013).
- **RBAC:** universal expiring assignments, SoD decisions, break-glass, access reviews (ADR-004).
- **Console:** Dioxus routes `56` routes including `/work`, `/operations`, `/finance`, `/sales`, `/procurement`, `/inventory`, `/asset`, `/maintenance`, `/quality`, `/marketing`, `/people`, `/pos`, `/legal`, `/manufacturing`, `/planning`, `/project`, `/commerce`, `/talent`, `/learning`, `/benefits`, `/field-service`, `/ai`, `/platform/*`, `/identity/*`, `/reference/item/:id`.

## Bounded contexts (implemented)

- **agent** — services: AgentOrchestrationCommandService, AgentOrchestrationQueryService; RPCs: 30
- **assets** — services: AssetsCommandService, AssetsQueryService; RPCs: 8
- **benefits** — services: BenefitsCommandService, BenefitsQueryService; RPCs: 9
- **brand** — services: BrandCommandService, BrandQueryService; RPCs: 8
- **catalog** — services: —; RPCs: 0
- **collaboration** — services: CollaborationCommandService, CollaborationQueryService, CollaborationStreamService; RPCs: 26
- **commercial** — services: CommercialCommandService, CommercialQueryService; RPCs: 22
- **common** — services: —; RPCs: 0
- **crm** — services: CrmCommandService, CrmQueryService; RPCs: 10
- **customer_360** — services: Customer360CommandService, Customer360QueryService; RPCs: 3
- **customer_authority** — services: CustomerAuthorityCommandService, CustomerAuthorityQueryService; RPCs: 9
- **ecommerce** — services: EcommerceCommandService, EcommerceQueryService; RPCs: 26
- **field_service** — services: FieldServiceCommandService, FieldServiceQueryService; RPCs: 12
- **finance** — services: FinancePayablesCommandService, FinancePayablesQueryService, FinanceCommandService, FinanceQueryService; RPCs: 26
- **identity_access** — services: IdentityAccessCommandService, IdentityAccessQueryService; RPCs: 34
- **interface** — services: —; RPCs: 0
- **inventory** — services: InventoryCommandService, InventoryQueryService; RPCs: 7
- **learning** — services: LearningCommandService, LearningQueryService; RPCs: 9
- **legal** — services: LegalCommandService, LegalQueryService; RPCs: 12
- **maintenance** — services: MaintenanceCommandService, MaintenanceQueryService; RPCs: 10
- **manufacturing** — services: ManufacturingCommandService, ManufacturingQueryService; RPCs: 18
- **marketing** — services: MarketingCommandService, MarketingQueryService; RPCs: 18
- **operations** — services: OperationsCommandService, OperationsQueryService; RPCs: 11
- **organization** — services: OrganizationCommandService, OrganizationQueryService; RPCs: 9
- **people_payroll** — services: PeoplePayrollCommandService, PeoplePayrollQueryService; RPCs: 16
- **planning** — services: PlanningCommandService, PlanningQueryService; RPCs: 14
- **platform_administration** — services: PlatformAdministrationCommandService, PlatformAdministrationQueryService, DataPlaneStreamService; RPCs: 65
- **pos** — services: PointOfSaleCommandService, PointOfSaleQueryService; RPCs: 16
- **procurement** — services: ProcurementCommandService, ProcurementQueryService; RPCs: 16
- **projects** — services: ProjectsCommandService, ProjectsQueryService; RPCs: 23
- **quality** — services: QualityCommandService, QualityQueryService; RPCs: 7
- **reference** — services: ReferenceQueryService; RPCs: 3
- **sales** — services: SalesCommandService, SalesQueryService; RPCs: 13
- **support** — services: SupportCommandService, SupportQueryService; RPCs: 21
- **talent** — services: TalentCommandService, TalentQueryService; RPCs: 28

Domain modules: access, asset_maintenance, commercial, contracts, corporate_identity, cqrs, customer, data_plane, finance, finance_banking, finance_budget, finance_chart, finance_close, finance_subledgers, finance_tax, holding_administration, hr, legal, marketing, marketing_abuse, marketing_callback, marketing_callback_secret, marketing_content, marketing_crm, marketing_delivery, marketing_journey, operations, operations_completion, platform_completion, pos, pos_refund, procurement_inventory, quality, sales, support.

## Decisions (authoritative ADRs)

ADR-001 Holding isolation · ADR-002 CQRS · ADR-003 No long-term dual writes · ADR-004 Expiring RBAC · ADR-005 Audited global reads · ADR-006 Support capabilities · ADR-007 Stripe authority · ADR-008 Operations boundaries · ADR-009 Marketing/CRM boundaries · ADR-010 Dioxus target strategy · ADR-011 Customer data-plane agent · ADR-012 Three-layer corporate identity · ADR-013 Holding+branch DB & directory authority · ADR-014 Modular monolith · ADR-015 Retire n8n · ADR-016 Dioxus Native atomic cutover · ADR-017 Universal collaboration & human-approved AI · ADR-018 Open-source modular AI & hosted pricing.

See `Plan/01-architecture.md` and `adr/` for full texts.

## Change control

- Product Council owns baseline; Domain Owners approve business content; Architecture/Security/Privacy/Ops/UX approve controls.
- Every requirement change must update acceptance coverage and traceability in the same change.
- Retired IDs are never reused. Delivery status lives outside `Plan/`.
