# ADR-019: Universal record mentions and per-module field AI

Universal `@<context>/<id>` record mentions and per-module human-approved field AI ship as cross-cutting capabilities.

**Context:** `@` existed only as principal/group/pool `MentionTarget` inside collaboration threads; field AI existed only as generic `RecordFieldPatch` in `agent-orchestration` with human-approved lifecycle. Product needs multi-record coverage per host (project → tasks → invoices) and AI that reads referenced records and ghost-fills the host form, in every module.

**Decision:** Make `@` polymorphic (record `StableRecordRef` + principal `MentionTarget`) with ordered, deduped, append-only typed edges, soft cap 50 per host, role-labeled edge kind (`blocks/dependsOn/billedBy/fulfills`), stored like `mention_events` and queryable via `ListBacklinks`/`ReferenceQueryService.search`. Make field AI per-module (`SuggestionCapability::RecordFieldPatch { owner }`) that proposes materialized `RecordFieldPatch` with pinned `FieldSuggestionEvidence` per source, `provenance` and `output_digest`, `Queued→Running→Ready→Applied/Rejected/Conflicted` with `current_source_version`/`current_payload_digest` re-validation on `Apply`. AI reads every source as the human author (`AuthorityScope`+purpose); unreadable sources flagged; offline chips show `Stale/Degraded/Offline`. Surfaces: inline typed chips inside fields + picker/search `@`-prefix + host detail backlink chips + single `FieldSuggestionCard` ghost/diff rail. Spec covers all 31 modules; implementation ships sequentially. No free-text mentions, no silent auto-apply, no system-global read, no auto-rebase.

**Consequences:** Supplementary `record_reference_edges` (or reused mention tables) plus field-level tokenization and search ranking; strictly additive to existing RLS/isolation, CQRS/outbox, and suggestion transfer protections.

## Addendum 2026-08-29 — Production-fidelity deep specs (31)

**Binding to editable schema:** AI suggestions are no longer open-ended. Every `RecordFieldPatch` is validated against `crates/integration-contracts/src/editable_fields.rs` (`record_editable_field_schema`, `RecordEditableFieldSchema`, `EditableFieldDefinition`, `supports_agent_patches`) and `catalog::ContextId`. `schema_version` is pinned per patch; `MAX_FIELD_PATCH_OPERATIONS=64`, `MAX_FIELD_EVIDENCE_REFS=16`, `previous_value_digest` (SHA-256) + `confidence_basis_points` bounded; unsupported record types fail closed (`OwnerUnsupported`).

**Persistence promise (concrete):** dedicated shared `record_reference_edges(host_ctx, host_id, target_ctx, target_id, kind, ordinal, created_by, created_at)` — append-only, `UNIQUE(host,target)`, soft cap 50 indexed, `ListBacklinks` on `(target)`. Extends `mention_events`/`opaque_mention_inbox` pattern; per-context tables not used.

**Form-level guarantee:** every Text field in every module can host inline typed chips (`contenteditable` chip spans `data-ref`/`data-kind`/`data-ordinal`); `@`-prefix triggers `ReferenceQueryService.Search` ranked picker; host detail shows ordered chips + backlinks. Ghost/diff preview is a single `FieldSuggestionCard` rail; `Apply` re-validates `current_source_version` + `current_payload_digest_sha256` → strict `Conflicted` (no auto-rebase, no force-apply, Q11=A).

**Cross-module spine:** canonical chain `projects → procurement/inventory → sales → finance → assets/operations → support/field_service` (role labels `dependsOn/blocks/billedBy/fulfills/authoredWith`) must be navigable end-to-end via `Search` + `ListBacklinks`. Spec-ahead for thin contexts (e.g., `catalog` with no services) defines intended contract but is flagged `spec-ahead — no implementation yet`.

**Verification:** Q15=C per-module Chromium E2E + disposable-DB harnesses (`cargo xtask test-affected` per owning crate + `reference`/`collaboration`/`agent-orchestration`) wired as code lands; spec now, no empty skeletons checked in.

