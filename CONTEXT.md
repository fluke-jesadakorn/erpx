# CONTEXT.md

Business language for ERPX. Code-derived, implementation-free.

## Identity & scope

- **Holding** — Top-level isolation boundary; owns a data pair. All business data is partitioned by Holding.
- **Company** — Child of a Holding; operational company within the holding.
- **Branch** — Child of a Company; physical/logical site. May have its own data pair (`holding_and_branch_pairs`).
- **AuthorityScope** — Scope of an act: Platform, Holding, Company, Branch, or Subtree.
- **Principal** — An actor that can be authorized (human or service).
- **Assignment** — Time-bounded grant of a role to a principal; always expires.
- **Break-Glass Session** — Emergency, time-boxed, dual-controlled elevation.

## Classification & audit

- **DataClassification** — Public < Internal < Confidential < Restricted; propagated via stricter-wins.
- **Audit Event** — Append-only record of who did what, when, for what purpose and correlation.
- **Global Read Audit** — Audited record of a cross-boundary read.

## Collaboration & reference

- **Referenceable** — Any record addressable at stable `/reference/item/:id` and linkable as `@<context>/<id>` (e.g., `@finance/inv-28-01-2026-1234`).
- **Typed Mention** — A mention stored as a typed edge, not free text — validated and auditable.
- **Collaboration Thread** — Thread of comments on a record, with reactions, following, and action cards.
- **Field Suggestion** — Human-approved AI proposal for a field value; must be explicitly applied or rejected.

- **Record Reference** — A stable handle to any business record (`<context>/<id>`) resolvable at `/reference/item/:id`.
- **Reference Edge** — A typed, ordered, deduped, append-only link from a host record to a target `Record Reference` or `Principal`, with optional role label (`blocks`, `dependsOn`, `billedBy`, `fulfills`); soft cap 50 per host; backlink-queryable.
- **Field Patch** — A materialized `RecordFieldPatch` proposing field values for a host, built from pinned source versions, with `Evidence` per source and `Provenance`.
- **Evidence** — Pinned `FieldSuggestionEvidence.record { record, version }` proving the source snapshot used for a patch.
- **Provenance** — Provider/model/prompt digest describing how a suggestion was produced.
- **Conflicted** — Patch state when host or source version/digest drifted since the suggestion was computed; requires re-request or explicit conflict acceptance.

## Commercial & platform

- **Commercial Catalog Revision** — Versioned commercial catalog mirrored from Stripe.
- **Entitlement** — Right to use a capability, allocation-booked.
- **Billing Lifecycle** — State machine of a billing attempt, Stripe-authoritative.

## Operations & compliance

- **Outbox Event** — Transactional event emitted with the write that produced it.
- **Projection** — Rebuildable read model derived from outbox events.
- **Holding Provisioning** — Platform workflow that reserves and activates a holding’s data pair.
- **PITR** — Point-in-time recovery via WAL archive.
