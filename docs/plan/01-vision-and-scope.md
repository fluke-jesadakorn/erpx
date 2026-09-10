# 01 — Vision and Scope

## Vision

One ERP codebase that an enterprise can run locally, self-host, or consume as hosted — without forking business logic. Open-source first, commercial overlay second.

## Scope (implemented)

31 bounded contexts across 13 value-stream groups (Q1=A, Q8=B). All capabilities below are implemented today — docs describe what code does (Q2=A):

- Platform/Governance · Customer & Support · Finance/Sales/POS · Supply-Chain/Assets/Ops · People/Legal · Marketing/CRM · Manufacturing · Planning · Projects · E-commerce · Talent/Learning/Benefits · Field Service · AI & Collaboration

No aspirational contexts added.

## What’s open vs commercial

- **Open (this repo):** all 31 contexts as business plans + Next.js implementation in `src/app/` (route handlers, Prisma/Drizzle). Full Rust implementation (`crates/*`, `apps/*`) is archived on `backup/rust-2026-09-02` (`rust-archive-2026-09-02`, `f97b74d6`); this branch is docs-only + Next.js scaffold.
- **Commercial overlay (hosted only):** Stripe-billed billing/entitlement (`commercial` context) — authoritative in Stripe (ADR-007), mirrored locally when present. Not required for local dev or self-hosted.

## Licensing & deployment tiers

- License: see repo `LICENSE` (open-source).
- Tiers on this branch: **Local dev** (`bun run dev` on `http://localhost:3100`) → **Cloudflare dev** (OpenNext Workers + D1 + R2 + Queues) → **Hosted** (operator-managed providers). Rust tiers are archived on `backup/rust-2026-09-02`.

## Ownership & change control

- Product Council owns baseline; Domain Owners approve business content; Architecture/Security/Privacy/Ops/UX approve controls (preserved from prior baseline).
- Community PRs welcome (Q18=A). Every `docs/plan/` change must update `traceability.md` and cite code SHA.
- Retired IDs never reused.

## Universal referenceability — `@`-mentions (Q13=A, Q14=B, Q15=B)

**Central contract:** Any record in any context is **Referenceable**: addressable at stable `/reference/item/:id` via `ReferenceQueryService` and linkable as `@<context>/<id>` (examples: `@finance/inv-28-01-2026-1234`, `@it/28-01-2026-5678`). Mentions are stored as **Typed Mentions** (`0006_global_collaboration_typed_mentions.sql`), not free text — edges are typed, validated, and auditable.

**Where it must work (Q14=B):** Everywhere a record can reference another — collaboration threads/comments/action cards **and** field values, AI suggestions, and search. Current code implements this in **collaboration** (CollaborationCommandService typed mentions + `ReferenceQueryService`); field-level linking and indexer expansion are declared as required follow-up to honor the promise. Docs do not fabricate field-level implementation as done.

**ID patterns:** Per-context examples live in each capability page’s “Referenceable” row. Missing/unauthorized targets render as tombstone; viewer `AuthorityScope` enforced at read time (ADR-005 audited reads).


### Deep contract (per-module, Q2=C + Q8–Q12)

- **Polymorphic `@`:** tokenizer emits `StableRecordRef` for `@<context>/<id>` record refs and `PrincipalRef`/`MentionTarget` for `@user/@group/@agent-pool`; both are typed edges, append-only, ordered, deduped (unique target), soft cap 50 (Q9=C), reordering explicit.
- **Multi-reference coverage:** a host (e.g., `projects/proj-…`, `sales/quote-…`, `finance/inv-…`) may hold many edges so people/AI see start→present coverage; backlinks (`ListBacklinks`) and search (`ReferenceQueryService.search`) are the read paths; pagination where needed.
- **Edge kinds:** role-labeled per host module where semantics warrant (`blocks`, `dependsOn`, `billedBy`, `fulfills`, `authoredWith`); flat ordered list is the primitive, role label is an attribute — Q8 “I want all” = A+B, no free-text graph.
- **Field-level chips + picker:** every text field in every module can host inline typed chips; every picker/search resolves `@`-prefixed queries; host detail pages surface ordered chips (Q3=B+C, Q14=B for deep).
- **AI per-module (Q4=all, Q5=A, Q10=A):** one `RecordFieldPatch { owner: ContextId }` capability per module (plus existing `sales.quote.compose`/`finance.*`); suggestion materializes values from all pinned refs into `FieldSuggestionEvidence`, shows ghost/diff preview in the host form, and requires explicit human `Apply`/`Reject` (inline chips + separate AI suggestion rail, Q13=B). No silent autofill.
- **Versioning:** `Apply` re-validates `current_source_version` + `current_payload_digest_sha256`; drift → `Conflicted` (Q11=A), never auto-rebased.
- **Auth:** AI reads every source as the human author with `AuthorityScope`+purpose; omitted sources flagged; `Apply` re-checks (Q12=A).
- **Degraded:** chips show `Stale/Degraded/Offline`, suggestions show evidence gap (Q16=A).


**Canonical cross-module spine (deep specs):** `projects/proj-2026-001` —dependsOn→ `procurement/req-2026-014` —fulfills→ `inventory/move-2026-883` —billedBy→ `finance/journal-2026-117` —authoredWith→ `people_payroll/assign-2026-042` —blocks→ `support/ticket-2026-509` (and `field_service`/`assets`/`operations` legs). This chain is the E2E smoke for search/resolve/backlinks and for AI multi-ref read-and-fill; every deep file references it.
Implementation ships sequentially (Q14=A) but spec covers all 31; see `docs/plan/03-cross-cutting/@-and-ai/<context>.md` per module.

## Engagement paradigm (social)

ERPX reads like a social network and audits like an ERP: every record is a **Post** in a newest-first **Feed** (Role Lane → Following → Hashtags → System), classified by scoped **#hashtags**, linked by typed **@**, followed without access change. Canonical: `docs/plan/04-social-paradigm.md` (ADR-021). Reactions are signals only; decisions stay human-proofed (ADR-017 §4).

## Architecture appendix

For deep technical detail (DDD/Clean/Hex, CQRS/outbox, holding isolation, RBAC, PITR) see `docs/adr/` (20 ADRs — ADR-020 is the Next.js stack) and `docs/plan/LEGACY.md` (pruned baselines archived on `backup/rust-2026-09-02`). Architecture is not duplicated here.

## Verification

Each capability page links to its business spec + archived proto (`docs/contracts/proto/` on backup) and living Next.js route in `src/app/` (once materialized). Full traceability in `docs/plan/traceability.md` — archived Rust table (banner) + living `src/app` route table (TBD). Verify this branch with `npm run lint` + `npm run build`; Rust gate `cargo xtask verify` is on backup.
