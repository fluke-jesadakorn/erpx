import "server-only";
import {fieldServiceFinanceSourceReadLocks} from "@erpx/server-field_service/public/finance-record-sources";
import {serviceNoChargeTerminalPredicate} from "@erpx/server-finance/adapters/service-no-charge-sources";
import {controlledPaymentStatementSettled} from "@erpx/server-finance/adapters/bank-statement-settlement";
import {failedSupplierInstructionSettlements} from "@erpx/server-finance/adapters/supplier-instruction-settlement";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "@erpx/server-commercial/public/package-settlement-sources";

type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
type Source = readonly [table: string, pending: string];
// These are durable owner sources, deliberately independent of purchase-filtered
// Work queues, the acting person's row scope, list limits and the current date.
// Future contractual recognition, depreciation and uncertain bank instructions
// still need settlement. Configuration and immutable issued history do not.
const core: readonly Source[] = [
  ["finance_periods", "r.state<>'closed'"],
  ["finance_journals", "r.status NOT IN ('posted','reversed')"],
  ["finance_invoices", "r.status NOT IN ('paid','voided') OR r.outstanding_minor<>0"],
  ["finance_payments", "r.status NOT IN ('settled','voided') OR r.outstanding_minor<>0"],
  ["finance_bank_statements", "r.status<>'reconciled' AND NOT EXISTS(SELECT 1 FROM settled_finance_bank_statements settled WHERE settled.id=r.id AND settled.company_id=r.company_id AND settled.bank_account=r.bank_account)"],
  ["finance_bank_transactions", "r.match_status NOT IN ('matched','ignored')"],
  ["finance_bank_match_groups", "r.status NOT IN ('matched','reversed','rejected')"],
  ["finance_refund_obligations", "r.status<>'settled' OR r.outstanding_minor<>0"],
  ["finance_refund_instructions", "r.status NOT IN ('confirmed','cancelled')"],
  ["finance_supplier_batches", "r.status<>'cancelled' AND (r.status<>'exported' OR NOT EXISTS(SELECT 1 FROM finance_supplier_instructions i WHERE i.batch_id=r.id) OR EXISTS(SELECT 1 FROM finance_supplier_instructions i WHERE i.batch_id=r.id AND i.status NOT IN ('confirmed','cancelled') AND NOT EXISTS(SELECT 1 FROM settled_failed_supplier_instructions settled WHERE settled.id=i.id AND settled.company_id=r.company_id)))"],
  ["finance_assets", "r.status<>'retired' AND (r.status<>'in_service' OR r.cost_minor-r.residual_minor-r.accumulated_depreciation_minor>0)"],
  ["finance_asset_policy_editions", "r.status='draft'"],
  ["finance_asset_policy_runs", "r.status='draft'"],
  ["finance_budgets", "r.status IN ('draft','submitted')"],
  ["finance_plan_editions", "r.status='draft'"],
  ["finance_accrual_schedules", "r.status NOT IN ('completed','cancelled')"],
  ["finance_accrual_schedule_runs", "r.status NOT IN ('posted','rejected')"],
  ["finance_revenue_contracts", "r.status NOT IN ('completed','cancelled')"],
  ["finance_business_contracts", "r.status NOT IN ('rejected','cancelled','superseded')"],
  ["finance_business_billing_runs", "r.no_charge_status='draft' AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=r.id)"],
  ["finance_recognition_runs", "r.status='draft'"],
  ["finance_billing_corrections", "r.status='draft'"],
  ["finance_collection_cases", "r.status NOT IN ('test_delivered','cancelled')"],
  ["finance_due_drafts", "r.status NOT IN ('prepared','cancelled')"],
  ["finance_accounting_profiles", "r.status='draft'"],
  ["finance_tax_identities", "r.status='draft'"],
  ["finance_tax_entries", "r.status='prepared'"],
  ["finance_vat_corrections", "r.status='prepared'"],
  ["finance_tax_returns", "r.status NOT IN ('settled','filed')"],
  ["finance_statutory_packages", "r.status NOT IN ('filed','paid','superseded')"],
  ["finance_calculation_runs", "r.status IN ('prepared','reviewed','stale') AND COALESCE(r.explanation_json::jsonb->>'engine','') IN ('source-vat-register-v1','source-acquisition-v1')"],
  ["finance_withholding_assessments", "r.status IN ('draft','approved') OR (r.status='consumed' AND r.tax_minor>0 AND NOT EXISTS(SELECT 1 FROM finance_withholding_remittance_items i JOIN finance_withholding_remittances m ON m.id=i.remittance_id WHERE i.assessment_id=r.id AND m.status='confirmed'))"],
  ["finance_withholding_packages", "r.status IN ('draft','approved')"],
  ["finance_thai_document_changes", "r.status='draft'"],
  ["finance_statement_packages", "r.status='draft'"],
  ["finance_year_closings", "r.status='draft'"],
  ["finance_fx_revaluation_runs", "r.status='prepared'"],
  ["finance_ai_proposals", "r.state IN ('queued','running')"],
];
const advanced: readonly Source[] = [
  ["finance_leases", "r.status NOT IN ('terminated','completed')"],
  ["finance_loans", "r.status<>'repaid' OR r.outstanding_minor<>0"],
  ["finance_instrument_editions", "r.status NOT IN ('superseded','rejected','closed')"],
  ["finance_instrument_runs", "r.status='draft'"],
  ["finance_book_rules", "r.status='draft'"],
  ["finance_book_adjustments", "r.status='draft'"],
  ["finance_calculation_policies", "r.status='draft'"],
  ["finance_calculation_runs", "r.status IN ('prepared','reviewed','stale') AND COALESCE(r.explanation_json::jsonb->>'engine','') NOT IN ('source-vat-register-v1','source-acquisition-v1')"],
  ["finance_group_report_editions", "r.status='draft'"],
];
const sources = new WeakMap<SqlDatabase, string>();
const joinedTables = ["finance_accounts", "finance_charts", "finance_allocations", "finance_financial_events", "finance_journal_lines", "finance_supplier_instructions", "finance_revenue_obligations", "finance_withholding_remittance_items", "finance_withholding_remittances", "finance_ownership_interests", "finance_acquisitions", "finance_group_entities", "finance_intercompany_transactions", "finance_intercompany_document_pairs", "finance_consolidation_runs", "finance_consolidation_eliminations", "finance_ai_policies", "finance_ai_usage"];

/** Acquire before any owner read/snapshot, including tables currently empty.
 * A naked transaction or an earlier serializable snapshot cannot mint this port. */
export async function lockFinancePackageObligationSources(owner: PostgresDatabase, lock: PackageSettlementSourceLock) {
  const { database, companyId } = packageSettlementSource(owner, lock);
  const tables = [...new Set([...core, ...advanced].map(([table]) => table).concat(joinedTables))].sort();
  await database.prepare("LOCK TABLE command_receipts IN SHARE MODE").run();
  await database.batch(fieldServiceFinanceSourceReadLocks(database));
  await database.prepare("LOCK TABLE money_transaction_snapshots IN SHARE MODE").run();
  await database.prepare(`LOCK TABLE ${tables.join(",")} IN SHARE MODE`).run();
  
  sources.set(database, companyId);
}

/** Both base and add-on withdrawal retain original financial/Legal/file history.
 * This port never cancels work, removes a source or changes an owner transaction. */
export async function financePackageObligations(owner: PostgresDatabase, database: SqlDatabase, input: Input) {
  owner.assertActiveTransaction(database);
  if (sources.get(database) !== input.companyId) throw new Error("FINANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
  if (input.moduleId !== "finance" || !["finance", "finance-advanced"].includes(input.packageId) || input.packageVersion !== 1 || !Number.isSafeInteger(input.now) || input.now < 0)
    throw new Error("FINANCE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  const full = input.packageId === "finance";
  const noCharge=full?await serviceNoChargeTerminalPredicate(database,"r"):"FALSE";
  const reads = (full ? [...core, ...advanced] : advanced).map(([table, pending]) =>
    `SELECT '${table}' kind,r.id,to_jsonb(r)::text fact FROM ${table} r,scope WHERE r.company_id=scope.company_id AND (${table==="finance_invoices"?`NOT (${noCharge}) AND (${pending})`:pending})`);
  // Secondary-book and intercompany settlement also depends on the base. Legacy
  // group records carry entity/snapshot identity rather than a Company column.
  reads.push(`SELECT 'finance_ownership_interests',r.id,to_jsonb(r)::text FROM finance_ownership_interests r,scope WHERE r.status='draft' AND (r.company_id=scope.company_id OR EXISTS(SELECT 1 FROM finance_group_entities e WHERE e.id IN (r.investor_entity_id,r.investee_entity_id) AND e.company_id=scope.company_id))`,
    `SELECT 'finance_acquisitions',r.id,to_jsonb(r)::text FROM finance_acquisitions r,scope WHERE r.status IN ('draft','reviewed') AND (r.company_id=scope.company_id OR EXISTS(SELECT 1 FROM finance_group_entities e WHERE e.id=r.investee_entity_id AND e.company_id=scope.company_id))`,
    `SELECT 'finance_intercompany_transactions',r.id,to_jsonb(r)::text FROM finance_intercompany_transactions r,scope WHERE scope.company_id IN (r.source_company_id,r.target_company_id) AND r.status NOT IN ('settled','eliminated')`,
    `SELECT 'finance_intercompany_document_pairs',r.id,to_jsonb(r)::text FROM finance_intercompany_document_pairs r,scope WHERE scope.company_id IN (r.company_id,r.source_company_id,r.target_company_id) AND r.status='draft'`,
    `SELECT 'finance_consolidation_runs',r.id,to_jsonb(r)::text FROM finance_consolidation_runs r,scope WHERE r.status<>'published' AND (CASE WHEN jsonb_typeof(r.source_snapshot_json::jsonb->'companies')='array' THEN jsonb_array_length(r.source_snapshot_json::jsonb->'companies')=0 OR EXISTS(SELECT 1 FROM jsonb_array_elements(r.source_snapshot_json::jsonb->'companies') company WHERE company->>'companyId'=scope.company_id OR jsonb_typeof(company->'companyId') IS DISTINCT FROM 'string') ELSE TRUE END)`);
  if (!full) reads.push(`SELECT 'finance_due_drafts',r.id,to_jsonb(r)::text FROM finance_due_drafts r,scope WHERE r.company_id=scope.company_id AND r.kind='instrument_payment' AND r.status NOT IN ('prepared','cancelled')`);
  if (full) reads.push(
    `SELECT 'finance_supplier_instructions',r.id,to_jsonb(r)::text FROM finance_supplier_instructions r JOIN finance_supplier_batches b ON b.id=r.batch_id,scope WHERE b.company_id=scope.company_id AND r.status NOT IN ('confirmed','cancelled') AND NOT EXISTS(SELECT 1 FROM settled_failed_supplier_instructions settled WHERE settled.id=r.id AND settled.company_id=b.company_id)`,
    `SELECT 'finance_revenue_obligations',r.id,to_jsonb(r)::text FROM finance_revenue_obligations r JOIN finance_revenue_contracts c ON c.id=r.contract_id,scope WHERE c.company_id=scope.company_id AND c.status<>'cancelled' AND (r.status<>'complete' OR r.recognized_minor<r.allocated_minor)`,
    `SELECT 'finance_posting_integrity',r.id,to_jsonb(r)::text FROM finance_journals r LEFT JOIN finance_journal_lines l ON l.journal_id=r.id,scope WHERE r.company_id=scope.company_id AND r.status IN ('posted','reversed') GROUP BY r.id HAVING count(l.id)<2 OR sum(l.debit_minor-l.credit_minor)<>0 OR sum(l.base_debit_minor-l.base_credit_minor)<>0`,
    `SELECT 'finance_missing_vat_invoice',r.id,to_jsonb(r)::text FROM finance_invoices r JOIN finance_journals j ON j.id=r.journal_id,scope WHERE r.company_id=scope.company_id AND r.tax_account_id IS NOT NULL AND j.status IN ('posted','reversed') AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='invoice' AND t.source_id=r.id AND t.status='approved')`,
    `SELECT 'finance_unfiled_vat_entry',r.id,to_jsonb(r)::text FROM finance_current_tax_entries r,scope WHERE r.company_id=scope.company_id AND r.status='approved' AND NOT EXISTS(SELECT 1 FROM finance_statutory_packages p WHERE p.company_id=r.company_id AND p.chart_id=r.chart_id AND p.filing_month=r.claim_month AND p.form_code='PP30' AND p.status IN ('filed','paid'))`,
    `SELECT 'finance_pending_recognition',concat(b.id,':',item->>'key'),item::text FROM finance_business_billing_runs b JOIN finance_invoices i ON i.id=b.invoice_id CROSS JOIN LATERAL jsonb_array_elements(b.result_json::jsonb->'items') item,scope WHERE b.company_id=scope.company_id AND i.status IN ('posted','partially_paid','paid') AND (item->>'amount')::numeric>0 AND NOT EXISTS(SELECT 1 FROM finance_billing_corrections c WHERE c.billing_run_id=b.id AND c.status='posted') AND NOT EXISTS(SELECT 1 FROM finance_recognition_runs r WHERE r.billing_run_id=b.id AND r.item_key=item->>'key' AND r.status IN ('draft','posted'))`,
    `SELECT 'finance_ai_policies',r.company_id,to_jsonb(r)::text FROM finance_ai_policies r,scope WHERE r.company_id=scope.company_id AND r.enabled`,
    `SELECT 'finance_ai_usage',r.id,to_jsonb(r)::text FROM finance_ai_usage r,scope WHERE r.company_id=scope.company_id AND r.state='reserved'`);
  const settlementSources=full?`,settled_finance_bank_statements AS MATERIALIZED(
    SELECT r.id,r.company_id,r.bank_account FROM finance_bank_statements r JOIN scope ON scope.company_id=r.company_id
    WHERE (${controlledPaymentStatementSettled})),
    settled_failed_supplier_instructions AS MATERIALIZED(SELECT settled.* FROM (${failedSupplierInstructionSettlements}) settled JOIN scope ON scope.company_id=settled.company_id)`:"";
  const facts = (await database.prepare(`WITH scope AS (SELECT ?::text company_id)${settlementSources} ${reads.join(" UNION ALL ")} ORDER BY kind,id,fact`).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "finance", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageId: input.packageId, packageVersion: input.packageVersion, facts })).digest("hex"), outstanding: facts.length };
}
