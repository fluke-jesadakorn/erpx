# Finance payables cross-owner bundle request

Recorded: 2026-11-10 (Asia/Bangkok)
Base: `5f1c0ae9d41218402675ff595cf73096504618e0`

## Current Finance-owned state

Closed and pushed:

- Native standalone payable create/edit/post/draft-void/detail, AP journal, Money snapshots, line revisions, replay/version/Company guards.
- Native posted void: exact reversing journal, independent approver, period/date/reason, replay, audit/outbox.
- Native payable attachments: `supplier_bill` / `bill_match_support` / `action` / `invoice-review` evidence sets bound through the Records/Files port in the posting unit.
- Gate: `finance.rust-supplier-bill-postgres.integration` 8/8.

Still open for `finance/payables`:

1. Matched supplier bills from Commerce fulfillment.
2. Procurement bill-match posting/void.
3. People Payroll expense-claim posting.
4. Browser acceptance for a company-level Finance invoice detail.

## Why Finance cannot close these alone

Finance's native source boundary intentionally rejects non-standalone payables:

```rust
// packages/server-finance/src/adapters/supplier_bill.rs
// only `kind='payable'` invoices with `source_type='standalone'` are writable here,
// so matched supplier bills and expense claims keep their existing owners.
```

The retained TypeScript posting paths read owner-owned snapshots through
`financeOwnedSnapshotSources()` in Commerce, Procurement and People Payroll.
Those are closed projections, not Finance tables. Finance must consume a public
owner port; it must not query or write those source tables directly.

## Required frozen bundles

### A. Commerce — matched fulfillment payable

Owner: Commerce (Common)
Writer files:

- `packages/server-commerce/src/public/finance_owned_snapshot.rs` (new public port; exact owned columns from `finance-owned-snapshot-sources.ts`).
- `packages/server-commerce/src/application/finance_owned_snapshot.rs` (read/lock the exact fulfillment, order, lines, settings, warehouse, goods valuation).
- `packages/server-commerce/src/adapters/finance_owned_snapshot_postgres.rs`.
- `packages/server-commerce/Cargo.toml` / `src/public/mod.rs` exports.

Contract:

- `FulfillmentPayableSnapshot { company, branch, department, fulfillment_id, fulfillment_version, order_id, order_version, party_id, currency, subtotal_minor, tax_minor, total_minor, invoice_date, due_date, debit_account_id, tax_account_id, payable_account_id, chart_id, goods_fx_source }`.
- `lock_finance_owned_sources(&mut PgConnection)`.
- `load_fulfillment_payable_snapshot(&mut PgConnection, company, fulfillment_id)`.
- No Finance table writes; no SQL/predicate supplied by the caller.
- Reject if the fulfillment is not posted, already invoiced to a different invoice, or the order is not `kind='purchase'`.

### B. Procurement — bill match payable

Owner: Procurement (Common)
Writer files:

- `packages/server-procurement/src/public/finance_owned_bill_match.rs` (new).
- `packages/server-procurement/src/application/finance_owned_bill_match.rs`.
- `packages/server-procurement/src/adapters/finance_owned_bill_match_postgres.rs`.
- `packages/server-procurement/Cargo.toml` / `src/public/mod.rs` exports.

Contract:

- `ProcurementBillMatchSnapshot { match_id, match_version, order_id, fulfillment_id, supplier_id, currency, chart_id, clearing_account_id, payable_account_id, tax_account_id, reference, invoice_date, due_date, expected_subtotal_minor, subtotal_minor, tax_minor, quantity_matches, variance_account_id, reviewed_by, review_reason, invoice_id, status }`.
- `lock_finance_owned_sources`, `load_bill_match_snapshot`.
- Reject unless the match is reviewed and the invoice link is absent/current.

### C. People Payroll — expense claim payable

Owner: People Payroll (Common/Distribute boundary)
Writer files:

- `packages/server-people_payroll/src/public/finance_owned_expense_claim.rs` (new).
- `packages/server-people_payroll/src/application/finance_owned_expense_claim.rs`.
- `packages/server-people_payroll/src/adapters/finance_owned_expense_claim_postgres.rs`.
- `packages/server-people_payroll/Cargo.toml` / `src/public/mod.rs` exports.

Contract:

- `ExpenseClaimPayableSnapshot { claim_id, claim_version, company, branch, department, claimant_person_id, supplier_party_id, currency, subtotal_minor, tax_minor, total_minor, invoice_date, due_date, debit_account_id, tax_account_id, payable_account_id, chart_id, approved_by }`.
- `lock_finance_owned_sources`, `load_expense_claim_payable_snapshot`.
- Reject unless the claim is approved, not already posted, and the claimant/party facts are current.

### D. Files — no new bundle

Finance already consumes `PgBusinessEvidenceFiles` / `prepare_business_evidence`
for payable review evidence. The Files owner only needs to confirm the
`supplier_bill` / `bill_match_support` policy version and set-binding contract
used by the posting unit.

### E. Work Projection / Account — Finance browser access

The available development account cannot open a company-level Finance invoice:

```text
/choose-work-role?companyId=acme-co&next=/holdings/acme/finance/documents/invoices/<id>
→ "No current role and package can open this page."
```

and the invoice detail returns 403 from the
`finance-product-presentation` `work-context` assertion
(`The original queued Work context changed`).

Required from the owner:

- A Finance-authorized development account, or a reviewed role/package grant for
  the company-level Finance assignment.
- Confirmation that the queued work-context revision used by
  `finance-product-presentation` matches the selected Finance assignment.

## Finance integration plan after bundles arrive

1. Freeze each owner bundle with exact SHA and evidence.
2. Finance adds one public source enum and a single `post_supplier_bill_from_source`
   path that accepts only those snapshots, in the same serializable Finance unit.
3. Preserve: Money snapshots, version/CAS, independent approver, replay,
   audit/outbox, Company isolation, no source-table writes.
4. Add PostgreSQL gates for each source: success, replay, stale version,
   foreign Company, already-invoiced, missing purchase, independent approver.
5. Add browser evidence on the company Finance invoice detail for:
   - matched bill post/void
   - expense claim post
   - payable attachment binding
6. Certify `finance/payables` and Finance as owner 6/41 only after all five
   workflows have native/frontend/legacy/gates/review/integration evidence.

## Coordination status

No Common frozen bundle, 6-subagent roster, reviewer/verification assignment,
or Common evidence has been received in this session. Per the objective, Finance
must not guess or edit those owner files without the agreed writer. This document
is the exact request list for the coordinator.
