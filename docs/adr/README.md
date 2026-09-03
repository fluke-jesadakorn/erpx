# ADR — Index (21 decisions, 2026-09-03 era)

> **Single index for `docs/adr/`.** `docs/TOC.md:25` + `AGENTS.md:61` + `README.md:41` all count here.

## Status (2026-09-02 adjudication, wayfinder ticket 03)

- **Reconfirmed 15 ✅ — still authoritative on `development` (Next.js era):** `001 holding-isolation`, `003 no-long-term-dual-writes`, `004 expiring-rbac`, `005 global-read-auditing`, `006 support-capabilities`, `007 stripe-authority`, `008 operations-boundaries`, `009 marketing-crm-boundaries`, `011 customer-data-plane-agent`, `012 three-layer-corporate-identity`, `013 holding-branch-database-and-directory-authority`, `015 retire-n8n`, `017 universal-record-collaboration-and-human-approved-ai` (475L), `018 open-source-modular-ai-and-hosted-stepper-pricing`, `019 universal-record-mentions-and-per-module-field-ai`
- **Deprecated 3 — Archived (Dioxus/Rust era, see `backup/rust-2026-09-02`):** `010 dioxus-target-strategy`, `014 cqrs-bounded-context-modular-monolith`, `016 dioxus-native-atomic-cutover`
- **Superseded 1 → 020:** `002 cqrs and replaceable read models` → `ADR-020 nextjs-stack`
- **Living 2:** `020 nextjs-stack` (Next.js 16 App Router, React 19, Tailwind 4, Prisma/Drizzle TBD, REST/tRPC `src/app/api/**` replaces Tonic `:50051`) + `021 social-engagement-layer` (ERP-as-social-network: posts/feed/hashtags/follow, reactions signal-only)

All 20 have banners `> **Next.js era: Reconfirmed (2026-09-02)` / `> **Deprecated — Archived` / `> **Superseded by ADR-020` (added `fe6c286e`).

## Files (21)

```
ADR-001-holding-isolation.md (13L) — DB-per-Holding, opaque bindings
ADR-002-cqrs.md (13L) — Superseded by 020
ADR-003-no-long-term-dual-writes.md (13L)
ADR-004-expiring-rbac.md (13L)
ADR-005-global-read-auditing.md (13L)
ADR-006-support-capabilities.md (15L)
ADR-007-stripe-authority.md (13L) — Stripe authoritative
ADR-008-operations-boundaries.md (13L)
ADR-009-marketing-crm-boundaries.md (13L)
ADR-010-dioxus-target-strategy.md (13L) — Deprecated Dioxus
ADR-011-customer-data-plane-agent.md (17L)
ADR-012-three-layer-corporate-identity.md (59L)
ADR-013-holding-branch-database-and-directory-authority.md (134L)
ADR-014-cqrs-bounded-context-modular-monolith.md (85L) — Deprecated Rust monolith
ADR-015-retire-n8n-and-unify-local-runtime.md (35L)
ADR-016-dioxus-native-atomic-cutover.md (31L) — Deprecated Native
ADR-017-universal-record-collaboration-and-human-approved-ai.md (475L) — @ typed mentions + agent
ADR-018-open-source-modular-ai-and-hosted-stepper-pricing.md (72L)
ADR-019-universal-record-mentions-and-per-module-field-ai.md (25L)
ADR-020-nextjs-stack.md (30L) — living (2026-09-02)
ADR-021-social-engagement-layer.md — living (2026-09-03, speculative plan)
```

## Change rule

`CONTRACT_POLICY.md:13` — Breaking changes require a new ADR + `CUTOVER.md` entry. Edit rule `docs/plan/README.md:27 Q18=A` (`traceability.md` + code SHA).

## Recovery

`git checkout backup/rust-2026-09-02 -- docs/adr/` (19 files, no 020, no banners) vs `development` 20 files with banners at `f97b74d6`/`rust-archive-2026-09-02`/`cc0887d`
