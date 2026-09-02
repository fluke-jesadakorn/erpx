# _shared-contract — Polymorphic @ + AI + RLS (shared, 80% of deep files)

> **Parallel file — new.** This file is the **single source** for the 150L template that was duplicated byte-identical across 31 deep files (`Plan/03-cross-cutting/@-and-ai/*.md` ~213L each). Every per-module deep file now keeps only its **delta** (RPC table where it differs, editable fields flag, 1-line business flow, route) and links here with `see _shared-contract.md`. Central contract is `Plan/01-vision-and-scope.md:31-53` (polymorphic @, Q2=C Q8-12) — this file is its per-module expansion. Do not duplicate this prose per-module again.

## Scope

Applies verbatim to every bounded context where a record can reference another — collaboration threads/comments/action cards **and** field values, AI suggestions, search. Current code implements this in **collaboration** (`CollaborationCommandService` typed mentions + `ReferenceQueryService`); field-level indexer expansion is declared required follow-up (see `Plan/01:31`). Per-module file header `Scope: Extends Plan/02-capabilities/<group>/<ctx>.md. Central polymorphic @ contract in Plan/01 applies verbatim.` points here.

## Invariants (generic, 3 bullets — from ADR-001/013/002)

- All writes are holding-isolated (ADR-001 `HoldingId` isolation boundary, ADR-013 holding/branch data pair) — a Holding chooses `ErpxManaged` or `CustomerDataPlane`, business units are logical contexts inside same pair; control plane stores only opaque references.
- Assignments/roles are time-bounded and break-glass dual-controlled (ADR-004 expiring `role_assignments`, ADR-015).
- State machines are append-only with tombstone moderation — edit is soft, moderation is tombstone, never hard-delete (ADR-002 outbox, ADR-017).

Per-module **domain invariants** stay in `Plan/02-capabilities/<group>/<ctx>.md:12` and in the per-module delta section below (e.g., `sales Quote→order→posting`, `pos Refund bounded`, `commercial Stripe-authoritative`, `assets draft→retired`) — do not overwrite them with these generic 3.

## Data owned (generic)

- Write: `<ctx> aggregate + outbox_events` — CQRS (ADR-002). A command atomically commits aggregate state, idempotency receipt, audit evidence, and outbox event. Workers project outbox events into physically separate, replaceable read DBs with checkpoints/idempotency/ordering/bounded retries/DLQ/replay.
- Read: rebuildable projections indexed for search; `record_reference_edges(host ContextId+id, target StableRecordRef/MentionTarget, kind, ordinal)` — append-only, deduped by target (`UNIQUE(host,target)`), soft cap 50 per host (`CHECK ordinal<50`), explicit reorder, backlink-indexed `(target_ctx,target_id)`.
- No cross-context table joins — collaborators via `ReferenceQueryService` + Typed Mentions only.

## Editable fields & schema binding (generic)

- Source of truth: `crates/integration-contracts/src/editable_fields.rs` + `catalog::ContextId` + `CURRENT_RECORD_POLICIES`.
- Binding: `RecordFieldPatch{source:RecordVersionRef, schema_version, changes:Vec<SuggestedFieldChange>}` validated against `record_editable_field_schema(record_type)`; `supports_agent_patches()==OwnerPublished && !fields.is_empty()` required; `MAX_FIELD_PATCH_OPERATIONS=64`, `MAX_FIELD_EVIDENCE_REFS=16`; per-field `StableFieldRef`, `previous_value_digest (SHA-256)`, `confidence_basis_points ≤10000`, `previous_value + proposed_value` typed by `EditableFieldDefinition` (required/max_text_bytes/min/max/choices/classification/risk).
- Per-module flag: `OwnerPublished` (only `sales: SALES_QUOTE_FIELDS`, `procurement: REQUISITION_FIELDS` so far) vs `OwnerUnsupported (spec-ahead)` (30 others) — see per-module delta.

## @ record mentions — per-module surfaces

- **Surfaces:** inline typed chips in every text field + picker `@`-prefix via `ReferenceQueryService.search` (ranked) + ordered chips (`reorder handles`) + `ListBacklinks` (paginated after 50) + thread/comments.
- **Tokenizer:** `@<context>/<id> → StableRecordRef` for record refs, `@user/@group/@agent-pool → PrincipalRef/MentionTarget` for principals; typed edges, append-only, ordered, deduped (`UNIQUE target`), soft cap 50 (Q9=C), reordering explicit.
- **Edge roles:** `dependsOn, blocks, billedBy, fulfills, authoredWith, quotedFrom` (Q8=A+B flat ordered list with role label attribute).
- **Multi-reference:** host may hold ≥3 edges so coverage is start→present (`_journeys.md` spine `projects/proj-2026-001 —dependsOn→ procurement/req-2026-014 —fulfills→ inventory/move-2026-883 —billedBy→ finance/journal-2026-117 —authoredWith→ people_payroll/assign-2026-042 —blocks→ support/ticket-2026-509`).
- **Resolve & degraded:** `ReferenceQueryService.resolve` with `AuthorityScope+purpose`; denied/missing → tombstone chip + `Global Read Audit` (ADR-005); freshness `Stale/Degraded/Offline` (Q16=A).

## Per-module field AI

- **Capability:** `SuggestionCapability::RecordFieldPatch{owner: ContextId}` per module (plus existing `sales.quote.compose` / `finance.*`).
- **Suggestion:** materialized `RecordFieldPatch` from pinned refs into `FieldSuggestionEvidence::Record{record:RecordVersionRef}` + `FieldSuggestionProvenance{provider_kind,model_ref,prompt_digest}` + `output_digest_sha256`; evidence shows in host form ghost/diff.
- **Lifecycle:** `Queued → Running → Ready | Conflicted | Failed → Applied | Rejected` — human `Apply`/`Reject` explicit (inline chips + separate `FieldSuggestionCard` rail, Q13=B) — no silent autofill.
- **Versioning:** `Apply` re-validates `current_source_version + current_payload_digest_sha256`; drift → `Conflicted` (Q11=A), never auto-rebased.
- **Auth:** AI reads every source as the human author with `AuthorityScope+purpose`; omitted sources flagged; `Apply` re-checks (Q12=A).

## Forms & views — granular fields

- Host detail header: ordered chips with reorder handles, backlinks paginated after 50, thread/comments.
- Each editable field: kind-appropriate input validated per `EditableFieldDefinition` (required, max_text_bytes, minimum/maximum, choices, classification, risk); `@-chips` inline where allowed; picker `@`-prefixed via `ReferenceQueryService.search`.
- AI rail: `FieldSuggestionCard` per field — value, `Evidence` pills, `Provenance`, `output_digest`, `[Apply][Reject]`; RLS + expiring RBAC audited.

## Data & contracts

- **Write:** per-aggregate `+ outbox` holding-isolated; **Read:** rebuildable indexed for search; **Invariants preserved** (see § Invariants).
- **Example IDs:** `@<ctx>/example-28-01-2026-0001` (per-context, tombstone on missing/unauthorized).

## Tables & projections (code-derived)

- Writes: per-aggregate + `outbox_events` holding-isolated.
- Reads: `*` projection rebuildable, indexed `(target_ctx,target_id)` for backlinks; `record_reference_edges` `UNIQUE(host,target) CHECK ordinal<50`.

## Field-level detail

- Chips: `Stale/Degraded/Offline` (Q16=A); suggestions: evidence gap flagged.

## Flow walkthroughs (templates)

- **Flow1 Happy:** host with `AuthorityScope=Holding` create → chips ordered → outbox `ReferenceEdgesAppended` → `search`/`backlinks` → audit `correlationId`.
- **Flow2 Exception:** tombstone / `Stale` / `Conflicted` on version drift → re-request.
- **Flow3 AI-assisted:** user pins ≥3 refs (e.g., `@projects/proj-…, @procurement/req-…, @finance/inv-…`) → `RequestFieldSuggestion(owner=<ctx>)` `Queued→Running→Ready` ghost preview materialized from pinned versions → `Apply` only on version match else `Conflicted`.

## UI placement

- Host detail + `FieldSuggestionCard` rail + backlinks; RLS + expiring RBAC audited; every screen shows scope, read-only/support/impersonation, projection freshness (`Fresh/Stale/Offline/Denied`), denied/empty/loading/conflict/offline states.

## Out-of-scope / not fabricated

- Field-level indexer expansion declared required follow-up — docs do not fabricate field-level implementation as done (see `Plan/01:31 Q14=B`).

## Verification (Q15=C)

- E2E Chromium: type `@` → picker → chip persists → `ListBacklinks` ordered deduped multi-ref≥3 role-labeled; request AI N-refs → card diff evidence/provenance → `Apply` writes only when `current_source_version/digest` match else `Conflicted`; `Reject` clears.
- Harness: `cargo xtask test-affected` for `<ctx>+reference/collaboration/agent-orchestration`; disposable-DB outbox→projection convergence (archived on `backup/rust-2026-09-02`, living gate on `development` is `npm run lint+build` per `AGENTS.md`).

## Cross-module spine

`projects/proj-2026-001 —dependsOn→ procurement/req-2026-014 —fulfills→ inventory/move-2026-883 —billedBy→ finance/journal-2026-117 —authoredWith→ people_payroll/assign-2026-042 —blocks→ support/ticket-2026-509` (plus `field_service/assets`/`operations` legs) — E2E smoke for search/resolve/backlinks + AI multi-ref.

## Security & compliance

- RLS + expiring RBAC (ADR-004) + `Global Read Audit` (ADR-005) + no system-global read; Holding isolation (ADR-001/013); `HoldingId` canonical.

## ASCII UX — shared frame (25L)

```
ERPX Console
Sidebar: /work  /finance  /sales  /procurement  /inventory  /projects  (6)
List: /<ctx> + /work — items <ctx>-001/002/003
Workspace: Detail+Form — Title <CTX> detail #003
  Chips: @projects/proj-2026-001 @procurement/req-2026-014 @finance/journal-2026-117 @sales/quote-2026-042 (refs:4/50)
  Fields: <see per-module spec row> + inline @-chips + picker @-prefix
AI Rail: FieldSuggestionCard — Evidence pills, Provenance ollama/mistral, Backlinks @finance/... , [Apply][Reject] ghost diff, Status Ready|Conflicted digest a1b2
```

Per-module keeps only: **Route + 3 deltas: title CTX, list IDs, spec row** (e.g., `sales: quote.notes Text 2,048 Internal/Low` vs `finance: journal.memo OwnerUnsupported`). See per-module delta for exact.

## Notes

- Overflow → new ADR per `Plan/03-cross-cutting/@-and-ai/*.md:144`; preserve `Q18=A` (`traceability.md` + code SHA) per `Plan/README.md:28`.
- Implementation: `crates/proto/proto/erpx/<ctx>/service.proto` (archived) → `src/app/api/<ctx>/*` (TBD, see `docs/contracts/next/route-table.md` + `CONTRACT_POLICY.md`).
