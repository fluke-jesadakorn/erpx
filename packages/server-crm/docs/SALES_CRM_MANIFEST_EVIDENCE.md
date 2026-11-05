# sales.crm manifest evidence (CRM owner note, not an enforcement change)

`packages/server-commercial` is read-only for this subagent; the enforcement
source is **not** modified here. This file is owner-local evidence only.

- The Rust commercial catalogue (`erpx-server-commercial`) defines package
  metadata in `packages/server-commercial/Cargo.toml` and reads
  `company_package_grants` via `PgCompanyPurchaseReader`
  (`packages/server-commercial/src/adapters/postgres_purchases.rs`).
- The TS purchase-enforcement layer (`packages/server-crm/src/adapters/purchases.ts`)
  calls `commercePackageEnforcementMode`, `checkPurchasedCapability`,
  `purchaseCommandBoundary` and `companyPurchaseReadLocks` from
  `@erpx/server-commercial/public/purchase-command-boundary` and enforces
  `sales.crm` inside `withCRMPurchasedCommand`. The Rust CRM slices in this
  turn **do not** replace that layer; purchase checks remain TS-owned until a
  Rust equivalent exists in `server-commercial` (coordinator-owned).
- No Rust manifest file was added inside `packages/server-crm` for
  `sales.crm` enforcement, because a manifest inside the CRM crate would
  duplicate the commercial catalogue source of truth and could drift from it.
  The questions for the coordinator (manifest location, accounts CREATE-only
  scope, `sales_recipient_eligibility` source, opportunities CREATE-only) are
  reported in the subagent handoff, not decided here.
