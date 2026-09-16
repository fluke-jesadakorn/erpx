# ERPX Docs — TOC (single hub)

> Start at `README.md` → here. Replaces `.scratch/docs-seamless/prototype/TOC-seamless.md`.

## Identity and access — 2026-09-15

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
