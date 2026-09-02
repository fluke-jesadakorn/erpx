# docs/contracts — Archived Contracts Inventory (pruned on development)

> **Pruned from `development` on 2026-09-02 (`99a66de7`).** The 37 `*.proto` files and descriptors listed below are **reference-only** — they describe the Rust/Tonic era archived on `backup/rust-2026-09-02` (`rust-archive-2026-09-02`, `f97b74d6`, baseline `cc0887d`). Living contracts on this branch are REST/tRPC + Prisma/Drizzle under `src/app/` and (future) `docs/contracts/next/` — see `CONTRACT_POLICY.md`, `docs/TOC.md`, and `ADR-020`.

## What was there

- `contracts/proto/erpx/agent/service.proto` … `erpx/*/service.proto` — 37 proto files from `crates/proto/proto/erpx/**`
- `contracts/descriptor/erpx_descriptor_set.sha256` + `erpx_prior_descriptor_set.bin` (860K) + `erpx_prior_descriptor_set.provenance` + `erpx_prior_descriptor_set.sha256`
- Full list (37 files on backup):
  - `agent/service.proto`, `assets/service.proto`, `benefits/service.proto`, `brand/service.proto`, `catalog/active.proto`, `collaboration/service.proto`, `commercial/service.proto`, `common/context.proto`, `crm/service.proto`, `customer_360/service.proto`, `customer_authority/service.proto`, `ecommerce/service.proto`, `field_service/service.proto`, `finance/service.proto`, `identity_access/service.proto`, `inventory/service.proto`, `learning/service.proto`, `legal/service.proto`, `maintenance/service.proto`, `manufacturing/service.proto`, `marketing/service.proto`, `operations/service.proto`, `organization/service.proto`, `people_payroll/service.proto`, `planning/service.proto`, `platform_administration/service.proto`, `pos/service.proto`, `procurement/service.proto`, `projects/service.proto`, `quality/service.proto`, `sales/service.proto`, `support/service.proto`, `talent/service.proto` + shared `common/` etc. (see backup for exact 37)

## Recovery

```bash
git checkout backup/rust-2026-09-02
ls docs/contracts/proto/erpx/*/service.proto | wc -l   # 37
ls docs/contracts/descriptor/
git show rust-archive-2026-09-02 --stat | grep proto
git show f97b74d6 --stat | grep contracts
```

## Living replacement

- `docs/contracts/next/` — skeleton for OpenAPI / tRPC route table (empty until `src/app/` routes are materialized)
- `src/app/api/` — future REST handlers; each route updates `Plan/traceability.md` (living columns) and `CONTRACT_POLICY.md`
