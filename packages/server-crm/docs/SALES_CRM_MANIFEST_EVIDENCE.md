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

## Activity command integration boundaries

The native account/activity slices remain unmounted. Their repository tests do
not qualify current authority, package admission, notifications or the command
ledger. TypeScript remains the authoritative runtime writer.

- `activities.ts` scopes the actual lead to the selected Company, branch and
  department, requires `commerce.read` and `commerce.sales`, and uses current
  assignment and purchase checks in `purchases.ts`. The native activity actor's
  booleans and Company alone are not a substitute for an opaque selected
  `AuthorizedWorkContext` or for scoped lead/activity reads. Identity's public
  `require_selected_actor_current` revalidates such a context in its original
  serializable transaction, but itself grants no source read/write permission.
- Commercial's public `PgCompanyPurchaseReader::inspect` and `revalidate` offer
  transaction-bound purchase witnesses. Composition must admit `sales.crm` for
  new work, settlement, or exact-replay history as appropriate, and revalidate
  the witness with current authority and the captured actual CRM source before
  commit. The existing TypeScript CRM purchase boundary additionally captures
  account/contact/lead and linked Finance source witnesses; those cannot be
  replaced by a caller's source label.
- Sales currently exposes `salesRecipientEligibility` only as a TypeScript SQL
  fragment. It checks current assignment/principal/role and scope, active
  unmerged person, and active dated engagement. A bounded native Sales public
  port must check the actual Company, branch, department and owner in the same
  transaction and retain/revalidate the relevant Identity and People facts;
  CRM activity create and reassign consume it. Identity's bounded
  `sales_recipient` port and People's dated `workforce_reference` port now
  provide those separate facts; Sales composition is not yet implemented.
  This remains a Sales/Identity/People boundary, not a CRM-owned duplicate
  of their tables.
- Collaboration's existing TypeScript `prepareCrmActivityNotification` stages
  `crm-activity-notice-{key}` for the actual activity owner and version, with
  the open/outcome message, in the caller's transaction. Its public Rust
  notification ports are for other sources, not CRM. A bounded CRM activity
  notification port needs that same owner-derived source and immutable effect;
  CRM create, complete, cancel and reassign consume it.
- Workflow's public `PgCommandLedger`, `PgCommandAuthority` and
  `PgOwnerCommandEffects` support same-connection replay and receipt/audit/
  outbox composition. CRM must supply source-specific request, canonical
  input/result, scope, authorization and prepare/apply/retain effects, including
  notification and all final guards. The present slice does not do so.

The isolated activity PostgreSQL tests establish the shown source lock order and
event persistence only. They do not prove the cross-owner ports, complete CRM
workflow, production composition or frontend journey.

## 2026-11-09 owner source additions (unmounted)

- `create_lead` now creates the CRM individual account and contact when the
  caller does not select existing identities, verifies selected references by
  Company, persists the caller-selected department, and writes the immutable
  `create` case event with the trimmed Zod-shaped payload.
- `link-finance` now has an owner-local account link adapter with version,
  independent-reviewer, branch/department lead-scope and evidence checks. The
  composition must still supply the current Finance customer witness and
  `commerce.approve`; the adapter is not that authority.
- `prepare_crm_qualification` is a read-only preparation path and
  `apply_crm_qualification_retained` remains the write path. Qualification
  facts now retain the lead branch/department.
- The reviewed account-link source now locks and reads `public.crm_*` and
  retains role/session/search-path/relation-OID boundary facts.
- `canonical_lead_create_input` and `LeadCreateReceipt` exist for exact create
  event/receipt shape. Workflow request fingerprints still require the
  retained TypeScript property order.
- None of these additions is mounted, purchase-enforced, authorized by a real
  session, browser-verified or promoted. `crm/activities` still lacks a native
  workspace/Work/history read.
- `LeadCreatePlan` and `LeadQualifyCommandInput` serialize in the retained TS
  property order with absent optional keys omitted; use them for new Workflow
  request fingerprints rather than `LeadCreateInput`/`LeadChangeInput` (the
  latter retain null-valued optional fields for historical native receipts).
- `link-finance` has read-only `prepare_link_finance` plus
  `apply_link_finance`; its wire DTO already matches the TS property order.
