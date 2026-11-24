import "server-only";
import { FINANCE_INVOICES_PROJECTION_SQL, FINANCE_PARTIES_PROJECTION_SQL } from "./finance-tail-dedupe-sql";
import { FINANCE_JOURNALS_PROJECTION_SQL } from "./finance-journals-projection-dedupe-sql";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  company_currency_scales: "SELECT \"company_id\",\"currency\",\"scale\",\"configured_by\",\"configured_at\" FROM company_currency_scales",
  finance_asset_disposals: "SELECT \"id\",\"asset_id\",\"company_id\",\"asset_version\",\"period_id\",\"entry_date\",\"proceeds_minor\",\"proceeds_account_id\",\"gain_account_id\",\"loss_account_id\",\"evidence_ref\",\"status\",\"journal_id\",\"created_by\",\"decided_by\",\"version\",\"created_at\",\"updated_at\",\"policy_source_hash\" FROM finance_asset_disposals",
  finance_assets: "SELECT \"id\",\"company_id\",\"asset_number\",\"name\",\"account_id\",\"cost_minor\",\"residual_minor\",\"useful_life_months\",\"in_service_date\",\"status\",\"accumulated_depreciation_minor\",\"version\",\"depreciation_account_id\",\"currency\",\"depreciation_expense_account_id\",\"activation_journal_id\",\"activated_by\",\"created_by\",\"created_at\",\"updated_at\",\"depreciation_convention\" FROM finance_assets",
  finance_invoices: FINANCE_INVOICES_PROJECTION_SQL,
  finance_journals: FINANCE_JOURNALS_PROJECTION_SQL,
  finance_parties: FINANCE_PARTIES_PROJECTION_SQL,
  finance_payments: "SELECT \"id\",\"company_id\",\"party_id\",\"kind\",\"payment_number\",\"payment_date\",\"currency\",\"amount_minor\",\"outstanding_minor\",\"bank_account\",\"status\",\"journal_id\",\"version\",\"created_by\",\"approved_by\",\"released_by\",\"created_at\",\"updated_at\",\"payment_slip_id\",\"fx_rate_micros\",\"base_amount_minor\",\"fx_journal_id\",\"withholding_assessment_id\",\"withheld_minor\" FROM finance_payments",
  finance_refund_obligations: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"return_id\",\"invoice_id\",\"party_id\",\"direction\",\"currency\",\"chart_id\",\"balance_account_id\",\"amount_minor\",\"outstanding_minor\",\"status\",\"beneficiary_name\",\"beneficiary_account\",\"approval_reason\",\"created_by\",\"approved_by\",\"approved_at\",\"version\",\"created_at\",\"updated_at\",\"base_amount_minor\",\"base_outstanding_minor\",\"source_credit_journal_id\",\"billing_correction_id\" FROM finance_refund_obligations",
} as const;
