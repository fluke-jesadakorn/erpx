# Cross-Department Journeys — ASCII + Spec (Document-First)

Canonical spine: `projects/proj-2026-001` —dependsOn→ `procurement/req-2026-014` —fulfills→ `inventory/move-2026-883` —billedBy→ `finance/journal-2026-117` —authoredWith→ `people_payroll/assign-2026-042` —blocks→ `support/ticket-2026-509`. Detail per module in its deep file (`../@-and-ai/<ctx>.md`).

## Journey 1 — Revenue: `sales quote → order → finance invoice/payment`

Flow: `crm/opportunity-2026-011` → `sales/quote-2026-042` (notes=@projects/proj-2026-001 + @customer_360/note-001, AI ghost from @brand + @crm) → `sales/order-2026-042` → `finance/journal-2026-117` (billedBy) → `finance/payment-2026-08`.

```ansi
[90m[ sales/quote-2026-042 ]─────billedBy────▶[ finance/journal-2026-117 ][0m
  [32m@projects/proj-2026-001[0m  [33mAI: fill payment_terms_days=30 from @customer_360[0m  [36mListBacklinks: sales/quote-2026-042[0m
  Chips: [32m@crm/opp-2026-011[0m [32m@customer_360/note-001[0m  Ghost: [32m+30 days[0m  [90m→ Apply → Applied (digest a1b2)[0m
```

Fallback:
```
[ sales/quote-2026-042 ]--billedBy-->[ finance/journal-2026-117 ]
  chips: @projects/proj-2026-001 @crm/opp-2026-011  AI: payment_terms=30 ghost +new
  Backlinks on finance: sales/quote-2026-042  Status: Ready → Applied
```
Refs: `Plan/03-cross-cutting/@-and-ai/sales.md`, `finance.md`, `crm.md`, `customer_360.md`.

## Journey 2 — Supply: `projects → procurement → inventory`

Flow: `projects/proj-2026-001` node `WBS-01` —dependsOn→ `procurement/req-2026-014` (justification=@projects + @inventory shortage, AI fill) —fulfills→ `inventory/move-2026-883` —authoredWith→ `people_payroll/assign-2026-042`.

```ansi
[90m[ projects/proj-2026-001:WBS-01 ]──dependsOn──▶[ procurement/req-2026-014 ]──fulfills──▶[ inventory/move-2026-883 ][0m
  [32m@procurement/req-2026-014[0m                [32m@inventory/move-2026-883[0m  [33mAI rail: justification ghost from @projects + @inventory[0m
```

Fallback:
```
[ projects:WBS-01 ]--dependsOn-->[ procurement/req-2026-014 ]--fulfills-->[ inventory/move-2026-883 ]
  procurement justification: @projects/proj-2026-001 + shortage AI ghost
```
Refs: `projects.md`, `procurement.md`, `inventory.md`, `people_payroll.md`.

## Journey 3 — People+Ops: `talent hire → projects → people_payroll` + `support → quality → maintenance`

3a Hire: `talent/requisition-2026-009` → `talent/offer-2026-009` → `people_payroll/assign-2026-042` —authoredWith→ `projects/proj-2026-001`
3b Ops: `support/ticket-2026-509` —blocks→ `quality/lot-2026-331` —blocks→ `maintenance/wo-2026-771` (description=@assets/asset-042 + @field_service/dispatch-018, AI ghost) → `field_service/job-2026-018`.

```ansi
[90m[ talent/req-2026-009 ]──▶[ people_payroll/assign-2026-042 ]──authoredWith──▶[ projects/proj-2026-001 ][0m
[90m[ support/ticket-2026-509 ]──blocks──▶[ quality/lot-2026-331 ]──blocks──▶[ maintenance/wo-2026-771 ][0m
  [32m@assets/asset-042[0m [32m@field_service/job-2026-018[0m  [33mAI: description ghost from @assets + @field_service → Conflicted on stale @assets version → Re-request[0m
```

Fallback:
```
[ talent/req-2026-009 ]-->[ people_payroll/assign-2026-042 ]--authoredWith-->[ projects ]
[ support/ticket-2026-509 ]--blocks-->[ quality/lot-2026-331 ]--blocks-->[ maintenance/wo-2026-771 @assets @field_service ghost → Conflicted ]
```
Refs: `talent.md`, `people_payroll.md`, `projects.md`, `support.md`, `quality.md`, `maintenance.md`, `assets.md`, `field_service.md`.

## How to read

- Each box is a host record; arrows are ordered typed edges (`kind` attribute, cap 50, deduped).
- Picker: `@`-prefix → `ReferenceQueryService.Search` ranked; chip insert is inline `contenteditable`.
- AI: single `FieldSuggestionCard` per host, `Evidence` per pinned source + `Provenance` + `output_digest`; `Apply` re-validates `current_source_version`/`previous_value_digest` → `Conflicted` on drift (strict).

## Smoke (code-backed)
- Shared graph `erpx_collaboration.record_relations` covers spine `projects → procurement → inventory → sales → finance → assets/operations → support/field_service` via `ReferenceQueryService.Search/ListBacklinks` (AuthorityScope+purpose, Stale/Degraded tombstones).
- Field AI `RecordFieldPatch {owner}` published for `finance.journal`/`projects.task`/`support.ticket` proves owner-scoped `Queued→Ready→Applied/Conflicted` with `Evidence/Provenance/output_digest`; remaining 23 contexts reuse same lifecycle (spec-ahead in `editable_fields_spec_ahead.rs`).
