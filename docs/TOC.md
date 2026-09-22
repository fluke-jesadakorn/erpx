# ERPX Docs — TOC (single hub)

> Start at `README.md` → here. Replaces `.scratch/docs-seamless/prototype/TOC-seamless.md`.

- [Cross-department business scenarios — 2026-09-20](testing/cross-department-business-scenarios.md): Thai audit of 21 scenario groups, customer outcomes, 183-capability traceability including Legal, current integration/browser evidence, gaps and business UAT gates.

- [A–J implementation progress and readiness](testing/cross-department-implementation-progress.md): actual implementation gaps, scoped evidence and Company-admin readiness dashboard.
- [Isolated browser simulation](testing/isolated-browser-simulation.md): disposable databases, real password sign-in and independent role handoffs on loopback 3102.
- [Working demo company](testing/working-demo-company.md): resettable isolated ERPX environment, real owner services, independent roles and a guided laptop purchase-to-payment scenario.
- [Isolated recovery simulation](testing/recovery-simulation.md): source-linked business snapshot restore with per-table checksums; production RPO/RTO remain pending.
- [Shared stock reservation simulation](testing/shared-stock-reservation-simulation.md): Sales/POS availability, partial consume, release, expiry and concurrency evidence.
- [Company routing review](testing/routing-attention-simulation.md): named-source scans, permission-filtered pagination and audited routing repair without a recent-document cutoff.
- [Supplier payment simulation](testing/supplier-payment-simulation.md): source reservations, independent approval, manual export and bank/GL confirmation with partial outcomes.
- [Return/refund simulation S04](testing/return-refund-simulation.md): paid returns, independent refund approval, partial bank instructions, exception review and reconciliation.
- [Inventory transit simulation](testing/inventory-transit-simulation.md): approved dispatch, partial receiving, discrepancy review, preserved valuation and source-linked GL.
- [Supplier delivery promises](testing/supplier-delivery-promises.md): immutable supplier confirmations, split dates, actual-receipt allocation and unconfirmed shortfall follow-up.
- [Procurement RFQ simulation](testing/procurement-rfq-simulation.md): immutable supplier comparisons, independent awards and atomic draft-PO handoffs.
- [Quality hold simulation](testing/quality-hold-simulation.md): warehouse/product quarantine, independent inspection/release, stock/payment guards and scoped handoffs.
- [Cross-department implementation progress](testing/cross-department-implementation-progress.md): simulation-first development, verified changes and remaining phase gates.
- [Business acceptance manifest](status/business-acceptance-manifest.json): scenario ownership and independent automated/browser/provider/production gates; simulation first.
- [Acceptance evidence ledger](status/business-acceptance-evidence.json): durable, checksum-validated observations linked to capabilities; observations do not promote acceptance gates.
- [Capability delivery matrix](status/capability-delivery-matrix.md): all 183 catalog capabilities including Legal mapped to delivery stage, current evidence and remaining acceptance gap.
- [Non-Legal core delivery checkpoint — 2026-09-19](testing/non-legal-core-checkpoint-2026-09-19.md): acceptance evidence, known baseline failures and the next delivery boundary for Stages 1–2.

- [Pagination, deferred loading and Server Component audit — 2026-09-18](testing/list-performance-2026-09-18.md): implemented list changes, verification and remaining full-snapshot readers.

## AI workspace — 2026-09-18

- [Freeform Executive widgets and role-scoped AI context](testing/executive-freeform-2026-09-18.md): saved HTML/CSS, border resizing, styled charts and live organization-aware generation.

- [Executive implementation and acceptance](testing/executive-canvas-2026-09-18.md): delivered canvas, real provider/browser checks, access tests, metric coverage and remaining scope.

- [Executive — AI Live Canvas implementation plan](plan/2026-09-18-executive-live-canvas.md): English-only AI, sidebar entry, chat-to-widget reports, live data, C-level eligibility, private ownership, and named sharing.
- [Architecture comparison: 44 patterns and AI/UX design](plan/2026-09-18-ai-workspace-research.md).
- [Implemented scope, verification and remaining work](testing/ai-workspace-2026-09-18.md).

## Work/Home performance — 2026-09-17

- [Implementation, native Redis and acceptance evidence](testing/work-home-performance.md).

## Staff self-service — 2026-09-17

- [Implementation and interfaces](contracts/next/staff-self-service.md): feed-centred Home, personal requests, leave entitlement, overtime reviews and payroll-hours handoff.
- [Acceptance evidence](testing/staff-self-service.md): integration, desktop/mobile browser checks and public development availability.

## Hotel operations — 2026-09-17

- [PMS implementation, API and rollout](contracts/next/pms.md): guided staff reservations, the multi-hotel guest booking portal, secure cancellation, Front Office, housekeeping, receipts, Finance ownership, Night Audit, recovery and deferred SiteMinder integration.

## Point of sale — 2026-09-17

- [POS implementation and rollout](contracts/next/pos.md): retail, kitchen, stock reservations, accounting, Connect setup and acceptance.
- [ADR-029](adr/ADR-029-pos-fulfillment-and-direct-charges.md): fulfillment ownership and company direct charges.

## Brand and business documents — 2026-09-16

- [ERPX brand guide](brand/ERPX-brand-guide.md): master artwork, color, clear-space, size and icon guidance.
- [ADR-027](adr/ADR-027-workspace-and-document-branding.md): separates Product Brand, Workspace Brand and the Company legal issuer.

## Identity and access — 2026-09-15

- [HR/IT separation and Technology console — 2026-09-18](contracts/next/hr-it-technology.md): independent HR placement, applied package snapshots, direct Directory/GPO apply, independent ERPX role review, and UCS Samba write-back. [Verification](testing/hr-it-technology-2026-09-18.md).
- [People positions and lifecycle implementation](contracts/next/people-lifecycle.md): HR→IT preparation, revisioned access packages, individual reviews, effective cutoffs, browser verification and remaining UCS/GPO work.
- [UCS lab and People directory operations](contracts/next/ucs-lab.md) — isolated lab, verified UDM write-back, and deferred Windows acceptance.
- [IAM implementation and operations](contracts/next/identity-access.md): local accounts, AD, Entra ID, UCS, scoped custom roles, matrix, independent review and selective migration.
- [ADR-025](adr/ADR-025-multiple-identity-providers-and-scoped-replacement.md): multiple providers and Company-scoped identity replacement.
- [IAM acceptance](testing/identity-access.md): verification and live-provider prerequisites.
- [Local user management](contracts/next/user-management.md): custom attributes, manual/automatic groups, policy attachments and independently reviewed derived grants.
- [ADR-026](adr/ADR-026-reviewed-local-access-policies.md): local policy ownership and the per-beneficiary review boundary.

## Active business roles — 2026-09-14

- [Active work context implementation](contracts/next/active-work-context.md): one effective assignment, PostgreSQL revisions, atomic writes, role recovery, audit/job provenance and browser acceptance. Supersedes combined business-role permissions.

## People and spending requirements — 2026-09-14

- [People management, organization structure, messaging, and spending approvals](plan/2026-09-14-spending-approval-requirements.md): approved requirements for HR onboarding, six staff levels, required managers, reusable positions, an organization chart, placement-based channels, and spending approval templates. Application implementation includes organization setup, scoped HR access, live organization channels and versioned spending routes. Existing staff require explicit HR mapping.

## Browser acceptance

- [WCAG 2.2 AA color audit](plan/2026-09-15-wcag-color-audit.md): light/dark contrast findings, exact recommended corrections, browser coverage and remaining verification limits.

- [Access and error feedback](testing/access-feedback.md): reason-specific workspace, session, permission and recovery UI, with security and browser acceptance coverage.

- [Workplace design acceptance](testing/workplace-design.md): Home, Work, Inbox, shared themes and responsive regression verification.
- [UX/UI component inventory](ux-ui/component-inventory.md): generated route and component catalogue, state coverage signals, and repair severity model.
- [E2E specification and test cases](testing/e2e-specification.md): source-based Work/authentication cases, existing business coverage, fixture prerequisites and remaining browser gaps.

## Workplace simplification — 2026-09-08

- [Work Cards](plan/2026-09-08-work-cards.md): permitted shortcuts, personal pin order, shared summaries, scoped preferences and verification.
- [Shared workspace and employee services](plan/2026-09-08-shared-workspace.md): combined responsibilities, service teams, requests, purchasing handoff, handbook, AI preparation and verification.

- [Named workflow handoffs](plan/2026-09-10-flow-handoffs.md): route defaults, responsible staff, separate GL and settlement, compatibility and verification.
- [Expense workflow, AI providers and IT access](plan/2026-09-08-expenses-ai-it.md): v2 routing, draft recovery, provider encryption, document preparation, rollout and verification.
- [Navigation, Administration and daily-work delivery](plan/2026-09-08-workplace-simplification.md): canonical management surfaces, role boundaries, compatibility and verification artifacts.

## Global core implementation — 2026-09-07

- [Implementation status and full module inventory](plan/2026-09-07-global-core-status.md): implemented flows, test evidence and remaining production work.
- [Local data, accounts and acceptance journeys](local-seed.md): realistic fictional roster and connected development scenarios.
- [Acme organization test data](testing/acme-organization-data.md): 112 staff, 92 new logins, department coverage and independent spending participants.
- [ERP workflow research](plan/2026-09-06-production-workflows-research.md): primary sources and acceptance requirements.

## Mockup implementation plan — 2026-09-06

- [Enterprise UX/UI delivery](plan/2026-09-06-enterprise-ui.md): complete 31-module / 171-capability interface, live-versus-preview boundaries, local review, and verification.
- [Mockup → Next.js/Bun/Cloudflare implementation plan](plan/2026-09-06-mockup-to-fullstack.md): accepted architecture, current implementation checkpoint, 31-module rollout, Finance acceptance, R2 lifecycle and delivery gates.
- [ADR-022 Cloudflare runtime](adr/ADR-022-cloudflare-runtime.md): Workers/OpenNext, D1, R2, Queues and server-policy decision.
- [ADR-023 workspace routing](adr/ADR-023-three-level-workspace-routing.md): three-level Platform/Holding/Local authorization and immutable canonical slugs.
- [Superseded local stack research](plan/2026-09-06-local-stack-research.md): retained research history for the abandoned PostgreSQL/MinIO direction.

## Tree (pruned 2026-09-02)

```
README.md · AGENTS.md+CLAUDE.md · CUTOVER.md (2026-09-02 + 2026-07-31) · CONTEXT.md (46 terms) · CONTRACT_POLICY.md
docs/plan/ 31 contexts: README · 01-vision · MAP 31 flat · 02/<group>/<ctx>.md 31 one-pagers · 03/@-and-ai/ 31 deep 60L+_shared 109L · traceability 552 <details>+living route-table · ARCHIVE canonical
docs/adr/ 20 (15 Reconfirmed 3 Deprecated 1 Superseded 020 Next.js) · docs/TOC.md ←here · docs/README → ARCHIVE · docs/contracts/next/route-table.md living · docs/agents/ · docs/archive/ (delivery 768L + wayfinder 10 tickets + dioxus 22 dirs 3.9M + ops 68K)
Designed/mockup/ clickable prototype (feed+31 contexts, from docs) · audits/ stub→archive · src/app/ (layout/page) · .agents/skills/ 6 · .github/nextjs.yml living · fixtures/ops/ runbooks
```

## Sales — 1 hop

`docs/plan/02/03-finance-sales-pos/sales.md` 26L 13 RPCs · `03/@-and-ai/sales.md` 69L · `MAP.md` row `sales|03|POST /api/sales/quotes|@sales/quote-2026-042` · living `docs/contracts/next/route-table.md` · archived `backup:crates/proto/sales/service.proto`

`README.md` → `docs/TOC.md` → `MAP.md` search `sales` → `02/sales.md`+`03/sales.md`+`_shared-contract.md`

## Feed — 0 hops

`/feed` → Role Lane (needs-your-role) → Following (newest-first) → post → canonical record. Paradigm: `docs/plan/04-social-paradigm.md` (ADR-021).

## Walk (3 paths)

1. Newcomer: `README.md`→`docs/plan/01-vision`→`MAP.md` 31→`CONTEXT.md`→`src/app/page.tsx`→history `CUTOVER.md`+`ARCHIVE.md`→`git checkout backup/rust-2026-09-02`
2. Term: `glossary.md`→`CONTEXT.md`
3. History: `CUTOVER.md`+`ARCHIVE.md`→`backup` `rust-archive-2026-09-02` `cc0887d`

Links: `MAP.md` · `sales.md`→`@-and-ai/sales.md`+`_shared` · `route-table.md` · `ADR-020` · `ARCHIVE.md` · `src/app/page.tsx`

Notes: 31 deep 213→60L (6150→2156L) + MAP 31 + _shared 109L + Plan→docs/plan 13 READMEs removed; 7 contradictions resolved in `CUTOVER.md` `CONTRACT_POLICY.md` `traceability.md`.

- [Staff requests, levels, task approvals and routine schedules](contracts/next/staff-work.md) — interfaces, authorization, calendar rules and migration order.
- [Social + AI public beta](plan/2026-09-07-social-ai-public-beta.md) — primary research, approved conversation-to-completion scope, pilot hypotheses, and pending verification gates.

- [Local Mac deployment and recovery](../ops/macos/README.md) — active runtime (ADR-024).
- [Development performance verification](testing/dev-performance-2026-09-22.md) — Work cache coverage, batched authorization, page timing and restart evidence.
- [ADR-030: Temporary Samba directory authority](adr/ADR-030-temporary-samba-directory-authority.md) — Alpine ARM64 isolation, identity continuity and recovery boundary.
- [Samba AD on the ERPX Mac](../ops/samba/README.md) — build, restore and runner operations for the temporary directory hosts.

- [ADR-028: Sandbox cost + Support billing](adr/ADR-028-sandbox-cost-plus-support-billing.md)
- [Billing Sandbox operations](contracts/next/billing.md)

- [People roster and staff change requests](contracts/next/people-roster.md) — weekly scheduling, swaps, permissions, and development examples.
- [Calendar and people planning](contracts/next/calendar.md) — combined source reads, privacy, appointments, announcements and lifecycle task planning.

- [Workplace UX, navigation and queues](contracts/next/workplace-ux.md) — shared presentation and navigation contracts.
- [UX and Retirement verification](plan/2026-09-17-ux-retirement-verification.md) — delivery, test results, browser coverage and development availability.

- [Legal implementation and acceptance boundary](contracts/next/legal.md) — working flows, security invariants, verification and explicit gaps against the full Legal plan.

- [Supplier invoice matching simulation](testing/supplier-invoice-matching.md) — receipt/PO variance, independent Finance review, AP/GL และ supplier returns.

- [Customer contact notes — scope, corrections and simulation](testing/customer-contact-notes.md)
- [Quality hold cancellation — independent correction](testing/quality-cancellation.md)
- [CRM customer follow-ups — assignment and outcome](testing/customer-followups.md)

- [CRM leads และ opportunities](testing/crm-leads-opportunities.md) — scoped pipeline และ approved Sales handoff; acceptance ยังแยก gate

- [Customer 360 scoped timeline](testing/customer-timeline.md) — source projection และ privacy boundary

- [Support private evidence](testing/support-evidence.md) — versioned private upload, scoped download และ frozen review evidence
- [Support sessions and capabilities](testing/support-sessions-capabilities.md) — time-bound independent approval และ allowlisted customer context
- [Asset and maintenance lifecycle](testing/asset-maintenance-lifecycle.md) — custody, condition, downtime, completion และ retirement evidence
- [Field service lifecycle](testing/field-service-lifecycle.md) — agreement, dispatch, reserved parts, visit evidence และ billing release
- [Cross-record collaboration](testing/collaboration-work.md) — private thread files, typed backlinks และ accountable action cards
- [Learning lifecycle](testing/learning-lifecycle.md) — reviewed catalog, assignments, results, completion และ eligibility handoff
- [Benefits lifecycle](testing/benefits-lifecycle.md) — reviewed plans, dependents, enrollment, provider acknowledgement และ payroll input
- [Talent lifecycle](testing/talent-lifecycle.md) — private candidates, reviewed requisitions/proposals/offers และ onboarding handoff
- [Agent governance](testing/agent-governance.md) — reviewed versions, routing pools, private sessions และ immutable invocation evidence
- [Marketing lifecycle](testing/marketing-lifecycle.md) — reviewed assets, consent/attribution, engagement และ CRM handoff
- [Planning and manufacturing lifecycle](testing/planning-manufacturing-lifecycle.md) — BOM/formula, shortage/capacity, production, QC, cost และ genealogy
- [E-commerce lifecycle](testing/ecommerce-lifecycle.md) — reviewed channels/listings, versioned carts, Sales checkout, shared reservation, fulfillment และ returns
- [Customer authority access](testing/customer-access.md) — source-bound approval, verified identity, revocation และ read audit
- [Customer duplicate reviews](testing/customer-duplicate-reviews.md) — independent identity decision และ canonical record evidence
- [Customer identity changes](testing/customer-identity-changes.md) — reviewed before/after snapshot และ guarded master update
- [Support incident simulation](testing/support-incidents.md) — shared cause, independent mitigation review และแยกผลลูกค้าแต่ละเคส
- [Support SLA simulation](testing/support-sla.md) — working calendar, immutable targets และ durable escalation
- [Support ticket simulation](testing/support-tickets.md) — source handoff และ independent customer confirmation review
