# @ and AI — Per-Module Deep Specs (Production Fidelity)

31 files, one per bounded context. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` (Q2=C, Q13=A, Q15=B) applies verbatim; lightweight index in `docs/plan/02-capabilities/` (<80 lines, each points: `ASCII: see 03 deep file`).

Each deep file (200–215 lines, production-implementable) now contains:
- Purpose/actors & `AuthorityScope`+RBAC, real RPCs (from `crates/proto/proto/erpx/<ctx>/service.proto`), invariants, data owned (tables/projections + `record_reference_edges`)
- Editable-field table bound to `crates/integration-contracts/src/editable_fields.rs` — schema-bound, fail-closed
- `@` graph: inline typed chips in every Text field + `@`-prefix picker via `ReferenceQueryService.Search` + host ordered chips + `ListBacklinks`; polymorphic `StableRecordRef`/`MentionTarget`, ordered deduped append-only, soft cap 50, role labels
- Per-module field AI: `SuggestionCapability::RecordFieldPatch { owner }` → materialized `RecordFieldPatch` with `Evidence`/`Provenance`/`output_digest`, lifecycle `Queued→Running→Ready→Applied/Rejected/Conflicted`, ghost/diff rail, strict Conflicted
- Forms/views, business flows (happy + exception + AI with ≥3 refs) over canonical spine `projects → procurement/inventory → sales → finance → assets/operations → support/field_service`
- **ASCII UX (new):** per-context 2–3 frames (List + Detail/Form) in `ansi` + plain fallback with box chrome, chip colors, `Stale`/`tombstone`, and paired spec table (`Field | Kind | Required | @-able | AI | Validation/RBAC`). Cross-refs show spine propagation.
- Contracts & verification (Q15=C): outbox→`record_reference_edges` + `ReferenceQueryService` + `Agent*FieldSuggestion`; Chromium E2E + disposable-DB harnesses wired as code lands

Cross-department illustration: `_journeys.md` stitches 3 flagship journeys (Revenue `sales→finance`, Supply `projects→procurement→inventory`, People+Ops `talent→projects→people_payroll` + `support→quality→maintenance`) with ASCII sequences linking back to per-module mockups. Every deep file also carries its leg of the spine.

Groups mirror `02-capabilities` value streams (01 platform … 13 ai-collaboration). Spec covers all 31; implementation ships sequentially (Q14=A).

ADR: `docs/adr/ADR-019-universal-record-mentions-and-per-module-field-ai.md` (addendum 2026-08-29) · Glossary: `CONTEXT.md`
