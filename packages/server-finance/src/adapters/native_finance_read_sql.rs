//! Owner-owned caller-unit read plans for Finance. Generated from the retired
//! TypeScript SELECT literals with the exact positional payloads; the axum
//! command-plan route resolves the kind here and returns the statement for the
//! caller to prepare and execute on its own already-open serializable unit.
//!
//! Reads carry no receipt/audit/outbox ledger: the caller executes them on the
//! same unit as the surrounding command, so they observe uncommitted effects.
//! No SQL is ever accepted from the wire and no TypeScript SQL remains for
//! these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound, `bind_count` the positional
/// argument count and `scope` the routing scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
    pub scope: &'static str,
}

/// Native successor of the TypeScript `finance-expansion-records` asset-policy
/// single-row read. The owner plans this literal; the caller executes it on its
/// open unit with the same `[companyId, id]` binds.
pub const RD_ASSET_POLICY_ROW: &str = "finance_rd_asset_policy_row";
const SQL_RD_ASSET_POLICY_ROW: &str =
    "SELECT * FROM finance_asset_policy_editions WHERE company_id=? AND id=?";
const READ_ASSET_POLICY_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_POLICY_ROW,
    sql: SQL_RD_ASSET_POLICY_ROW,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `finance-expansion-records` asset-policy
/// runs array read. Same owner statement and `[companyId, id]` binds.
pub const RD_ASSET_POLICY_RUNS: &str = "finance_rd_asset_policy_runs";
const SQL_RD_ASSET_POLICY_RUNS: &str =
    "SELECT * FROM finance_asset_policy_runs WHERE company_id=? AND policy_id=? ORDER BY ordinal,id";
const READ_ASSET_POLICY_RUNS: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_POLICY_RUNS,
    sql: SQL_RD_ASSET_POLICY_RUNS,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `finance-expansion-records` instrument
/// edition single-row read. The owner plans this literal; the caller executes
/// it on its open unit with the same `[companyId, id]` binds.
pub const RD_INSTRUMENT_EDITION: &str = "finance_rd_instrument_edition";
const SQL_RD_INSTRUMENT_EDITION: &str =
    "SELECT * FROM finance_instrument_editions WHERE company_id=? AND id=?";
const READ_INSTRUMENT_EDITION: NativeReadStatement = NativeReadStatement {
    kind: RD_INSTRUMENT_EDITION,
    sql: SQL_RD_INSTRUMENT_EDITION,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `finance-expansion-records` instrument
/// runs array read. Same owner statement and `[companyId, id]` binds.
pub const RD_INSTRUMENT_RUNS: &str = "finance_rd_instrument_runs";
const SQL_RD_INSTRUMENT_RUNS: &str =
    "SELECT * FROM finance_instrument_runs WHERE company_id=? AND instrument_id=? ORDER BY ordinal,id";
const READ_INSTRUMENT_RUNS: NativeReadStatement = NativeReadStatement {
    kind: RD_INSTRUMENT_RUNS,
    sql: SQL_RD_INSTRUMENT_RUNS,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `finance-record-details` collection-case
/// row read. The owner plans this joined literal with its `[companyId, id]`
/// binds; the caller runs it on its open unit and takes the first row.
pub const RD_COLLECTION_CASE_ROW: &str = "finance_rd_collection_case_row";
const SQL_RD_COLLECTION_CASE_ROW: &str = "SELECT c.*,i.version current_invoice_version,CASE WHEN EXISTS(SELECT 1 FROM finance_collection_cases child WHERE child.parent_case_id=c.id) THEN 0 ELSE 1 END is_current FROM finance_collection_cases c JOIN finance_invoices i ON i.id=c.invoice_id AND i.company_id=c.company_id WHERE c.company_id=? AND c.id=?";
const READ_COLLECTION_CASE_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_COLLECTION_CASE_ROW,
    sql: SQL_RD_COLLECTION_CASE_ROW,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `finance-record-details` business-bill
/// row read. The owner plans this joined literal with its `[id, companyId]`
/// binds; the caller runs it on its open unit and takes the first row.
pub const RD_BUSINESS_BILL_ROW: &str = "finance_rd_business_bill_row";
const SQL_RD_BUSINESS_BILL_ROW: &str = "SELECT b.*,COALESCE(b.contract_content_json,c.content_json) effective_contract_content_json,i.invoice_number,i.version current_invoice_version,COALESCE(i.status,b.no_charge_status) invoice_status,CASE WHEN EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=b.id) THEN 0 ELSE 1 END is_current FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id AND c.company_id=b.company_id LEFT JOIN finance_invoices i ON i.id=b.invoice_id WHERE b.id=? AND b.company_id=?";
const READ_BUSINESS_BILL_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_BUSINESS_BILL_ROW,
    sql: SQL_RD_BUSINESS_BILL_ROW,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the `entrypoint-queries` period-end-date list. The
/// caller binds `[companyId]` on its open unit.
pub const RD_PERIOD_END_DATES: &str = "finance_rd_period_end_dates";
const SQL_RD_PERIOD_END_DATES: &str =
    "SELECT id,end_date FROM finance_periods WHERE company_id=? ORDER BY start_date DESC";
const READ_PERIOD_END_DATES: NativeReadStatement = NativeReadStatement {
    kind: RD_PERIOD_END_DATES,
    sql: SQL_RD_PERIOD_END_DATES,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the `entrypoint-queries` full period list. The caller
/// binds `[companyId]` on its open unit.
pub const RD_ALL_PERIODS: &str = "finance_rd_all_periods";
const SQL_RD_ALL_PERIODS: &str =
    "SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC";
const READ_ALL_PERIODS: NativeReadStatement = NativeReadStatement {
    kind: RD_ALL_PERIODS,
    sql: SQL_RD_ALL_PERIODS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the retired `supplier-payment-repository` batch row read.
/// The caller binds `[id, companyId]` on its open unit.
pub const RD_SUPPLIER_BATCH_ROW: &str = "finance_rd_supplier_batch_row";
const SQL_RD_SUPPLIER_BATCH_ROW: &str = "SELECT \"id\",\"company_id\",\"currency\",\"bank_account_id\",\"planned_date\",\"status\",\"created_by\",\"approved_by\",\"approval_reason\",\"exported_by\",\"export_command_id\",\"export_hash\",\"version\",\"created_at\",\"updated_at\" FROM finance_supplier_batches WHERE id=? AND company_id=?";
const READ_SUPPLIER_BATCH_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_SUPPLIER_BATCH_ROW,
    sql: SQL_RD_SUPPLIER_BATCH_ROW,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the retired supplier instruction row read.
pub const RD_SUPPLIER_INSTRUCTION_ROW: &str = "finance_rd_supplier_instruction_row";
const SQL_RD_SUPPLIER_INSTRUCTION_ROW: &str = "SELECT i.* FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE i.id=? AND b.company_id=?";
const READ_SUPPLIER_INSTRUCTION_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_SUPPLIER_INSTRUCTION_ROW,
    sql: SQL_RD_SUPPLIER_INSTRUCTION_ROW,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the capped supplier-batch console list read.
pub const RD_SUPPLIER_BATCH_LIST: &str = "finance_rd_supplier_batch_list";
const SQL_RD_SUPPLIER_BATCH_LIST: &str = concat!(
    "SELECT b.*,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status IN ('reserved','exported','unknown')) pending_count,",
    "(SELECT count(*) FROM finance_supplier_instructions i JOIN finance_invoices f ON f.id=i.invoice_id WHERE i.batch_id=b.id AND i.status='failed' AND f.outstanding_minor>0) failed_count,",
    "(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown') unknown_count ",
    "FROM finance_supplier_batches b WHERE b.company_id=? AND ",
    "(NOT ? OR (EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=b.id) AND NOT EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=b.id AND NOT EXISTS(SELECT 1 FROM (SELECT o.company_id,f.invoice_id id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id AND i.company_id=o.company_id AND i.kind='payable' WHERE o.kind='purchase') owned_source WHERE owned_source.id=owned_item.invoice_id AND owned_source.company_id=b.company_id)))) ",
    "ORDER BY b.updated_at DESC LIMIT 200"
);
const READ_SUPPLIER_BATCH_LIST: NativeReadStatement = NativeReadStatement {
    kind: RD_SUPPLIER_BATCH_LIST,
    sql: SQL_RD_SUPPLIER_BATCH_LIST,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the uncapped supplier-batch work list read.
pub const RD_SUPPLIER_BATCH_WORK_LIST: &str = "finance_rd_supplier_batch_work_list";
const SQL_RD_SUPPLIER_BATCH_WORK_LIST: &str = concat!(
    "SELECT b.*,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status IN ('reserved','exported','unknown')) pending_count,",
    "(SELECT count(*) FROM finance_supplier_instructions i JOIN finance_invoices f ON f.id=i.invoice_id WHERE i.batch_id=b.id AND i.status='failed' AND f.outstanding_minor>0) failed_count,",
    "(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown') unknown_count ",
    "FROM finance_supplier_batches b WHERE b.company_id=? AND ",
    "(NOT ? OR (EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=b.id) AND NOT EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=b.id AND NOT EXISTS(SELECT 1 FROM (SELECT o.company_id,f.invoice_id id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id AND i.company_id=o.company_id AND i.kind='payable' WHERE o.kind='purchase') owned_source WHERE owned_source.id=owned_item.invoice_id AND owned_source.company_id=b.company_id)))) ",
    "ORDER BY b.updated_at DESC"
);
const READ_SUPPLIER_BATCH_WORK_LIST: NativeReadStatement = NativeReadStatement {
    kind: RD_SUPPLIER_BATCH_WORK_LIST,
    sql: SQL_RD_SUPPLIER_BATCH_WORK_LIST,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the workspace instruction list for one batch.
pub const RD_SUPPLIER_BATCH_ITEMS: &str = "finance_rd_supplier_batch_items";
const SQL_RD_SUPPLIER_BATCH_ITEMS: &str = "SELECT s.*,i.invoice_number,i.currency FROM finance_supplier_instructions s JOIN finance_invoices i ON i.id=s.invoice_id WHERE s.batch_id=? ORDER BY s.id";
const READ_SUPPLIER_BATCH_ITEMS: NativeReadStatement = NativeReadStatement {
    kind: RD_SUPPLIER_BATCH_ITEMS,
    sql: SQL_RD_SUPPLIER_BATCH_ITEMS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the unbounded payable invoice picker read.
pub const RD_SUPPLIER_BATCH_INVOICES: &str = "finance_rd_supplier_batch_invoices";
const SQL_RD_SUPPLIER_BATCH_INVOICES: &str = "SELECT i.*,p.name party_name,i.outstanding_minor-COALESCE((SELECT SUM(s.amount_minor) FROM finance_supplier_instructions s WHERE s.invoice_id=i.id AND s.status IN ('reserved','exported','unknown')),0) available_minor FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? AND i.kind='payable' AND i.source_type IN ('standalone','commerce_fulfillment') AND i.status IN ('posted','partially_paid') AND (NOT ? OR EXISTS(SELECT 1 FROM (SELECT o.company_id,f.invoice_id id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id AND i.company_id=o.company_id AND i.kind='payable' WHERE o.kind='purchase') owner_invoice WHERE owner_invoice.id=i.id AND owner_invoice.company_id=i.company_id)) ORDER BY i.due_date,i.id LIMIT 200";
const READ_SUPPLIER_BATCH_INVOICES: NativeReadStatement = NativeReadStatement {
    kind: RD_SUPPLIER_BATCH_INVOICES,
    sql: SQL_RD_SUPPLIER_BATCH_INVOICES,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the invoiceId-narrowed payable picker read.
pub const RD_SUPPLIER_BATCH_INVOICE: &str = "finance_rd_supplier_batch_invoice";
const SQL_RD_SUPPLIER_BATCH_INVOICE: &str = "SELECT i.*,p.name party_name,i.outstanding_minor-COALESCE((SELECT SUM(s.amount_minor) FROM finance_supplier_instructions s WHERE s.invoice_id=i.id AND s.status IN ('reserved','exported','unknown')),0) available_minor FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? AND i.kind='payable' AND i.source_type IN ('standalone','commerce_fulfillment') AND i.status IN ('posted','partially_paid') AND (NOT ? OR EXISTS(SELECT 1 FROM (SELECT o.company_id,f.invoice_id id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id AND i.company_id=o.company_id AND i.kind='payable' WHERE o.kind='purchase') owner_invoice WHERE owner_invoice.id=i.id AND owner_invoice.company_id=i.company_id)) AND i.id=? ORDER BY i.due_date,i.id LIMIT 200";
const READ_SUPPLIER_BATCH_INVOICE: NativeReadStatement = NativeReadStatement {
    kind: RD_SUPPLIER_BATCH_INVOICE,
    sql: SQL_RD_SUPPLIER_BATCH_INVOICE,
    bind_count: 3,
    scope: "holding",
};

/// Native successor of the cash ledger list in the supplier workspace.
pub const RD_SUPPLIER_BATCH_ACCOUNTS: &str = "finance_rd_supplier_batch_accounts";
const SQL_RD_SUPPLIER_BATCH_ACCOUNTS: &str = "SELECT a.id,a.code,a.name,a.currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.category='asset' AND a.status='active' AND a.id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=a.company_id) AND c.status='active' AND a.currency=c.base_currency ORDER BY a.code";
const READ_SUPPLIER_BATCH_ACCOUNTS: NativeReadStatement = NativeReadStatement {
    kind: RD_SUPPLIER_BATCH_ACCOUNTS,
    sql: SQL_RD_SUPPLIER_BATCH_ACCOUNTS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the open-period list in the supplier workspace.
pub const RD_SUPPLIER_BATCH_PERIODS: &str = "finance_rd_supplier_batch_periods";
const SQL_RD_SUPPLIER_BATCH_PERIODS: &str = "SELECT id,name FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date";
const READ_SUPPLIER_BATCH_PERIODS: NativeReadStatement = NativeReadStatement {
    kind: RD_SUPPLIER_BATCH_PERIODS,
    sql: SQL_RD_SUPPLIER_BATCH_PERIODS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the candidate bank-debit read. `can_pay` is enforced by
/// the caller before this plan is requested; `source_only` is bound so an
/// included Supply buyer only sees debits matching an exported instruction.
pub const RD_SUPPLIER_BATCH_BANK: &str = "finance_rd_supplier_batch_bank";
const SQL_RD_SUPPLIER_BATCH_BANK: &str = "SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND s.bank_account=? AND t.match_status IN ('unmatched','proposed') AND (NOT ? OR (t.currency=? AND t.transaction_date>=? AND EXISTS(SELECT 1 FROM finance_supplier_instructions instruction WHERE instruction.batch_id=? AND instruction.status IN ('exported','unknown') AND t.amount_minor=-instruction.amount_minor))) ORDER BY t.transaction_date DESC LIMIT 200";
const READ_SUPPLIER_BATCH_BANK: NativeReadStatement = NativeReadStatement {
    kind: RD_SUPPLIER_BATCH_BANK,
    sql: SQL_RD_SUPPLIER_BATCH_BANK,
    bind_count: 6,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` approved-profile
/// point read. The caller binds `[companyId, chartId, date]` on its open unit.
pub const RD_TAX_PROFILE: &str = "finance_rd_tax_profile";
const SQL_RD_TAX_PROFILE: &str = "SELECT * FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=? AND status='approved' AND effective_date<=?";
const READ_TAX_PROFILE: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_PROFILE,
    sql: SQL_RD_TAX_PROFILE,
    bind_count: 3,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` PP30 package point
/// read. The caller binds `[id, companyId]` on its open unit.
pub const RD_TAX_PACKAGE: &str = "finance_rd_tax_package";
const SQL_RD_TAX_PACKAGE: &str = "SELECT * FROM finance_statutory_packages WHERE id=? AND company_id=? AND form_code='PP30' AND filing_month IS NOT NULL";
const READ_TAX_PACKAGE: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_PACKAGE,
    sql: SQL_RD_TAX_PACKAGE,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` VAT-correction point
/// read. The caller binds `[id, companyId]` on its open unit.
pub const RD_TAX_CORRECTION: &str = "finance_rd_tax_correction";
const SQL_RD_TAX_CORRECTION: &str =
    "SELECT * FROM finance_vat_corrections WHERE id=? AND company_id=?";
const READ_TAX_CORRECTION: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_CORRECTION,
    sql: SQL_RD_TAX_CORRECTION,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` tax-entry point read.
/// The caller binds `[id, companyId]` on its open unit.
pub const RD_TAX_ENTRY: &str = "finance_rd_tax_entry";
const SQL_RD_TAX_ENTRY: &str = "SELECT * FROM finance_tax_entries WHERE id=? AND company_id=?";
const READ_TAX_ENTRY: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_ENTRY,
    sql: SQL_RD_TAX_ENTRY,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` approved previous
/// entry point read. The caller binds `[id, companyId]` on its open unit.
pub const RD_TAX_PREVIOUS_ENTRY: &str = "finance_rd_tax_previous_entry";
const SQL_RD_TAX_PREVIOUS_ENTRY: &str =
    "SELECT * FROM finance_current_tax_entries WHERE id=? AND company_id=? AND status='approved'";
const READ_TAX_PREVIOUS_ENTRY: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_PREVIOUS_ENTRY,
    sql: SQL_RD_TAX_PREVIOUS_ENTRY,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` original invoice VAT
/// entry point read. The caller binds `[companyId, sourceId]` on its open unit.
pub const RD_TAX_ORIGINAL_ENTRY: &str = "finance_rd_tax_original_entry";
const SQL_RD_TAX_ORIGINAL_ENTRY: &str = "SELECT * FROM finance_current_tax_entries WHERE company_id=? AND source_type='invoice' AND source_id=? AND status='approved'";
const READ_TAX_ORIGINAL_ENTRY: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_ORIGINAL_ENTRY,
    sql: SQL_RD_TAX_ORIGINAL_ENTRY,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` pending-correction
/// family point read. The caller binds `[companyId, familyId]` on its open unit.
pub const RD_TAX_PENDING_FAMILY: &str = "finance_rd_tax_pending_family";
const SQL_RD_TAX_PENDING_FAMILY: &str = "SELECT 1 FROM finance_vat_corrections WHERE company_id=? AND family_source_id=? AND status='prepared'";
const READ_TAX_PENDING_FAMILY: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_PENDING_FAMILY,
    sql: SQL_RD_TAX_PENDING_FAMILY,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` approved-profile list
/// read. The caller binds `[companyId]` on its open unit.
pub const RD_TAX_PROFILES: &str = "finance_rd_tax_profiles";
const SQL_RD_TAX_PROFILES: &str =
    "SELECT * FROM finance_accounting_profiles WHERE company_id=? AND status='approved'";
const READ_TAX_PROFILES: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_PROFILES,
    sql: SQL_RD_TAX_PROFILES,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` chart existence point
/// read. The caller binds `[chartId, companyId]` on its open unit.
pub const RD_TAX_CHART: &str = "finance_rd_tax_chart";
const SQL_RD_TAX_CHART: &str = "SELECT 1 FROM finance_charts WHERE id=? AND company_id=?";
const READ_TAX_CHART: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_CHART,
    sql: SQL_RD_TAX_CHART,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` current-entry point
/// read. The caller binds `[companyId, sourceType, sourceId]` on its open unit.
pub const RD_TAX_CURRENT_ENTRY: &str = "finance_rd_tax_current_entry";
const SQL_RD_TAX_CURRENT_ENTRY: &str = "SELECT id FROM finance_current_tax_entries WHERE company_id=? AND source_type=? AND source_id=? AND status='approved'";
const READ_TAX_CURRENT_ENTRY: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_CURRENT_ENTRY,
    sql: SQL_RD_TAX_CURRENT_ENTRY,
    bind_count: 3,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` pending-correction
/// count read. The caller binds `[companyId, chartId, endDate]` on its open unit.
/// Only the read is native; the TS assertion guard and the surrounding lock
/// remain in the owner adapter.
pub const RD_TAX_PENDING_CORRECTION_COUNT: &str = "finance_rd_tax_pending_correction_count";
const SQL_RD_TAX_PENDING_CORRECTION_COUNT: &str = "SELECT COUNT(*) FROM finance_vat_corrections c JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.company_id=? AND c.chart_id=? AND c.status='prepared' AND t.accounting_date<=?";
const READ_TAX_PENDING_CORRECTION_COUNT: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_PENDING_CORRECTION_COUNT,
    sql: SQL_RD_TAX_PENDING_CORRECTION_COUNT,
    bind_count: 3,
    scope: "holding",
};

/// Native successor of the retired `finance-tax-register` close-profile point
/// read. The caller binds `[companyId, endDate]` on its open unit.
pub const RD_TAX_CLOSE_PROFILE: &str = "finance_rd_tax_close_profile";
const SQL_RD_TAX_CLOSE_PROFILE: &str = "SELECT id,statutory_chart_id,effective_date FROM finance_accounting_profiles WHERE company_id=? AND status='approved' AND effective_date<=?";
const READ_TAX_CLOSE_PROFILE: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_CLOSE_PROFILE,
    sql: SQL_RD_TAX_CLOSE_PROFILE,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the retired `finance-bank-automation` imported-file
/// duplicate probe. The owner plans the exact joined literal; the caller binds
/// `[companyId, bankAccountId, fileHash]` on its open unit and takes the first
/// row. No TypeScript SQL remains for this kind.
pub const RD_BANK_IMPORTED_FILE: &str = "finance_rd_bank_imported_file";
const SQL_RD_BANK_IMPORTED_FILE: &str = "SELECT s.statement_number FROM finance_bank_import_files f JOIN finance_bank_statements s ON s.id=f.statement_id AND s.company_id=f.company_id WHERE f.company_id=? AND f.bank_account_id=? AND f.file_hash=?";
const READ_BANK_IMPORTED_FILE: NativeReadStatement = NativeReadStatement {
    kind: RD_BANK_IMPORTED_FILE,
    sql: SQL_RD_BANK_IMPORTED_FILE,
    bind_count: 3,
    scope: "holding",
};

/// Native successors of the six static `finance-tax-identities` point and list
/// reads. Each plan is byte-identical to the retired TS literal; the caller
/// binds the same positional payload on its open unit and applies the existing
/// first-row / all-rows projection.
pub const RD_TAX_IDENTITY_ACTIVE_PARTY: &str = "finance_rd_tax_identity_active_party";
const SQL_RD_TAX_IDENTITY_ACTIVE_PARTY: &str = "SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND status='active' AND person_id IS NULL";
const READ_TAX_IDENTITY_ACTIVE_PARTY: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_IDENTITY_ACTIVE_PARTY,
    sql: SQL_RD_TAX_IDENTITY_ACTIVE_PARTY,
    bind_count: 2,
    scope: "holding",
};

pub const RD_TAX_IDENTITY_PREDECESSOR: &str = "finance_rd_tax_identity_predecessor";
const SQL_RD_TAX_IDENTITY_PREDECESSOR: &str = "SELECT 1 FROM finance_tax_identities WHERE id=? AND company_id=? AND party_id IS NOT DISTINCT FROM ?::text AND status='approved'";
const READ_TAX_IDENTITY_PREDECESSOR: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_IDENTITY_PREDECESSOR,
    sql: SQL_RD_TAX_IDENTITY_PREDECESSOR,
    bind_count: 3,
    scope: "holding",
};

pub const RD_TAX_IDENTITY_ROW: &str = "finance_rd_tax_identity_row";
const SQL_RD_TAX_IDENTITY_ROW: &str = "SELECT * FROM finance_tax_identities WHERE id=? AND company_id=?";
const READ_TAX_IDENTITY_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_IDENTITY_ROW,
    sql: SQL_RD_TAX_IDENTITY_ROW,
    bind_count: 2,
    scope: "holding",
};

pub const RD_TAX_IDENTITY_REVIEWED: &str = "finance_rd_tax_identity_reviewed";
const SQL_RD_TAX_IDENTITY_REVIEWED: &str = "SELECT * FROM finance_tax_identities WHERE id=? AND company_id=? AND party_id IS NOT DISTINCT FROM ?::text AND status='approved' AND effective_from<=? AND effective_to>=? AND rule_pack_version=?";
const READ_TAX_IDENTITY_REVIEWED: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_IDENTITY_REVIEWED,
    sql: SQL_RD_TAX_IDENTITY_REVIEWED,
    bind_count: 6,
    scope: "holding",
};

pub const RD_TAX_IDENTITY_LIST: &str = "finance_rd_tax_identity_list";
const SQL_RD_TAX_IDENTITY_LIST: &str = "SELECT id,party_id,legal_name,tax_id,address,branch_code,entity_kind,vat_registered,vat_registration_evidence,effective_from,effective_to,previous_identity_id,signer_name,signer_authority,signature_link_id,signature_sha256,rule_pack_version,evidence_ref,status,created_by,reviewed_by,version FROM finance_tax_identities WHERE company_id=? ORDER BY created_at DESC";
const READ_TAX_IDENTITY_LIST: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_IDENTITY_LIST,
    sql: SQL_RD_TAX_IDENTITY_LIST,
    bind_count: 1,
    scope: "holding",
};

pub const RD_TAX_IDENTITY_PARTY_LIST: &str = "finance_rd_tax_identity_party_list";
const SQL_RD_TAX_IDENTITY_PARTY_LIST: &str = "SELECT id,name,kind FROM finance_parties WHERE company_id=? AND status='active' AND person_id IS NULL ORDER BY name";
const READ_TAX_IDENTITY_PARTY_LIST: NativeReadStatement = NativeReadStatement {
    kind: RD_TAX_IDENTITY_PARTY_LIST,
    sql: SQL_RD_TAX_IDENTITY_PARTY_LIST,
    bind_count: 1,
    scope: "holding",
};

/// Native successors of the static `finance-assets` point and list reads.
/// Each plan is byte-identical to the retired TS literal; lock-bearing owner
/// statements and assertion guards stay in the caller's transaction.
pub const RD_ASSET_ACCOUNT_ROWS: &str = "finance_rd_asset_account_rows";
const SQL_RD_ASSET_ACCOUNT_ROWS: &str = "SELECT id,chart_id,category FROM finance_accounts WHERE company_id=? AND status='active' AND id IN (?,?,?)";
const READ_ASSET_ACCOUNT_ROWS: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_ACCOUNT_ROWS,
    sql: SQL_RD_ASSET_ACCOUNT_ROWS,
    bind_count: 4,
    scope: "holding",
};

pub const RD_ASSET_EVENT_LIST: &str = "finance_rd_asset_event_list";
const SQL_RD_ASSET_EVENT_LIST: &str = "SELECT * FROM finance_asset_events WHERE asset_id=? ORDER BY created_at,id";
const READ_ASSET_EVENT_LIST: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_EVENT_LIST,
    sql: SQL_RD_ASSET_EVENT_LIST,
    bind_count: 1,
    scope: "holding",
};

pub const RD_ASSET_DISPOSAL_LIST: &str = "finance_rd_asset_disposal_list";
const SQL_RD_ASSET_DISPOSAL_LIST: &str = "SELECT * FROM finance_asset_disposals WHERE asset_id=? ORDER BY created_at,id";
const READ_ASSET_DISPOSAL_LIST: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_DISPOSAL_LIST,
    sql: SQL_RD_ASSET_DISPOSAL_LIST,
    bind_count: 1,
    scope: "holding",
};

pub const RD_ASSET_REVIEWED_POLICY: &str = "finance_rd_asset_reviewed_policy";
const SQL_RD_ASSET_REVIEWED_POLICY: &str = "SELECT 1 FROM finance_asset_policy_editions WHERE asset_id=? AND status IN ('active','superseded')";
const READ_ASSET_REVIEWED_POLICY: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_REVIEWED_POLICY,
    sql: SQL_RD_ASSET_REVIEWED_POLICY,
    bind_count: 1,
    scope: "holding",
};

pub const RD_ASSET_DEPRECIATION_HISTORY: &str = "finance_rd_asset_depreciation_history";
const SQL_RD_ASSET_DEPRECIATION_HISTORY: &str = "SELECT ordinal,amount_minor FROM finance_asset_depreciations WHERE asset_id=? ORDER BY ordinal";
const READ_ASSET_DEPRECIATION_HISTORY: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_DEPRECIATION_HISTORY,
    sql: SQL_RD_ASSET_DEPRECIATION_HISTORY,
    bind_count: 1,
    scope: "holding",
};

pub const RD_ASSET_LATEST_DEPRECIATION: &str = "finance_rd_asset_latest_depreciation";
const SQL_RD_ASSET_LATEST_DEPRECIATION: &str = "SELECT MAX(entry_date) date FROM finance_asset_depreciations WHERE asset_id=?";
const READ_ASSET_LATEST_DEPRECIATION: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_LATEST_DEPRECIATION,
    sql: SQL_RD_ASSET_LATEST_DEPRECIATION,
    bind_count: 1,
    scope: "holding",
};

pub const RD_ASSET_RETAINED_POLICY_HASH: &str = "finance_rd_asset_retained_policy_hash";
const SQL_RD_ASSET_RETAINED_POLICY_HASH: &str = "SELECT policy_source_hash FROM finance_asset_disposals WHERE id=?";
const READ_ASSET_RETAINED_POLICY_HASH: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_RETAINED_POLICY_HASH,
    sql: SQL_RD_ASSET_RETAINED_POLICY_HASH,
    bind_count: 1,
    scope: "holding",
};

pub const RD_ASSET_DIMENSION_VALUES: &str = "finance_rd_asset_dimension_values";
const SQL_RD_ASSET_DIMENSION_VALUES: &str = "SELECT dimension_id,code,name FROM finance_dimension_values WHERE company_id=? AND status='active' ORDER BY code";
const READ_ASSET_DIMENSION_VALUES: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_DIMENSION_VALUES,
    sql: SQL_RD_ASSET_DIMENSION_VALUES,
    bind_count: 1,
    scope: "holding",
};

/// Native successors of the static `finance-reporting` point and list reads.
/// Lock-bearing revision statements and command assertions stay in the
/// caller's transaction; only the read plans cross the owner boundary.
pub const RD_REPORT_REVISION: &str = "finance_rd_report_revision";
const SQL_RD_REPORT_REVISION: &str = "SELECT revision FROM finance_reporting_revisions WHERE chart_id=?";
const READ_REPORT_REVISION: NativeReadStatement = NativeReadStatement {
    kind: RD_REPORT_REVISION,
    sql: SQL_RD_REPORT_REVISION,
    bind_count: 1,
    scope: "holding",
};

pub const RD_REPORT_PROFILE: &str = "finance_rd_report_profile";
const SQL_RD_REPORT_PROFILE: &str = "SELECT * FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=? AND status='approved' AND standard='TFRS for NPAEs' AND effective_date<=?";
const READ_REPORT_PROFILE: NativeReadStatement = NativeReadStatement {
    kind: RD_REPORT_PROFILE,
    sql: SQL_RD_REPORT_PROFILE,
    bind_count: 3,
    scope: "holding",
};

pub const RD_REPORT_PACKAGE_LIST: &str = "finance_rd_report_package_list";
const SQL_RD_REPORT_PACKAGE_LIST: &str = "SELECT id,start_date,end_date,comparative_start_date,comparative_end_date,edition,previous_package_id,status,version,prepared_by,reviewed_by,source_fingerprint,created_at FROM finance_statement_packages WHERE company_id=? AND chart_id=? ORDER BY end_date DESC,edition DESC";
const READ_REPORT_PACKAGE_LIST: NativeReadStatement = NativeReadStatement {
    kind: RD_REPORT_PACKAGE_LIST,
    sql: SQL_RD_REPORT_PACKAGE_LIST,
    bind_count: 2,
    scope: "holding",
};

pub const RD_REPORT_CLOSING_LIST: &str = "finance_rd_report_closing_list";
const SQL_RD_REPORT_CLOSING_LIST: &str = "SELECT * FROM finance_year_closings WHERE company_id=? AND chart_id=? ORDER BY end_date DESC,created_at DESC";
const READ_REPORT_CLOSING_LIST: NativeReadStatement = NativeReadStatement {
    kind: RD_REPORT_CLOSING_LIST,
    sql: SQL_RD_REPORT_CLOSING_LIST,
    bind_count: 2,
    scope: "holding",
};

pub const RD_REPORT_ACCOUNT_LIST: &str = "finance_rd_report_account_list";
const SQL_RD_REPORT_ACCOUNT_LIST: &str = "SELECT id,code,name,category,status FROM finance_accounts WHERE company_id=? AND chart_id=? ORDER BY code,id";
const READ_REPORT_ACCOUNT_LIST: NativeReadStatement = NativeReadStatement {
    kind: RD_REPORT_ACCOUNT_LIST,
    sql: SQL_RD_REPORT_ACCOUNT_LIST,
    bind_count: 2,
    scope: "holding",
};

pub const RD_REPORT_STATEMENT_HISTORY: &str = "finance_rd_report_statement_history";
const SQL_RD_REPORT_STATEMENT_HISTORY: &str = "SELECT * FROM finance_statement_events WHERE package_id=? ORDER BY package_version";
const READ_REPORT_STATEMENT_HISTORY: NativeReadStatement = NativeReadStatement {
    kind: RD_REPORT_STATEMENT_HISTORY,
    sql: SQL_RD_REPORT_STATEMENT_HISTORY,
    bind_count: 1,
    scope: "holding",
};

pub const RD_REPORT_PREVIOUS_PACKAGE: &str = "finance_rd_report_previous_package";
const SQL_RD_REPORT_PREVIOUS_PACKAGE: &str = "SELECT id FROM finance_statement_packages WHERE id=? AND company_id=? AND chart_id=? AND start_date=? AND end_date=? AND status='issued'";
const READ_REPORT_PREVIOUS_PACKAGE: NativeReadStatement = NativeReadStatement {
    kind: RD_REPORT_PREVIOUS_PACKAGE,
    sql: SQL_RD_REPORT_PREVIOUS_PACKAGE,
    bind_count: 5,
    scope: "holding",
};

pub const RD_REPORT_NEXT_EDITION: &str = "finance_rd_report_next_edition";
const SQL_RD_REPORT_NEXT_EDITION: &str = "SELECT COALESCE(MAX(edition),0)+1 edition FROM finance_statement_packages WHERE chart_id=? AND start_date=? AND end_date=?";
const READ_REPORT_NEXT_EDITION: NativeReadStatement = NativeReadStatement {
    kind: RD_REPORT_NEXT_EDITION,
    sql: SQL_RD_REPORT_NEXT_EDITION,
    bind_count: 3,
    scope: "holding",
};

/// Native successors of the static `finance-withholding` point/list reads.
/// Assertion guards and lock-bearing owner statements stay in the caller's
/// transaction; only these byte-identical read plans cross the boundary.
pub const RD_WITHHOLDING_BANK_ACCOUNT: &str = "finance_rd_withholding_bank_account";
const SQL_RD_WITHHOLDING_BANK_ACCOUNT: &str = "SELECT a.id,a.version,a.chart_id,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.status='active' AND a.control_kind='bank' AND a.currency='THB'";
const READ_WITHHOLDING_BANK_ACCOUNT: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_BANK_ACCOUNT,
    sql: SQL_RD_WITHHOLDING_BANK_ACCOUNT,
    bind_count: 2,
    scope: "holding",
};

pub const RD_WITHHOLDING_LIABILITY_ACCOUNT: &str = "finance_rd_withholding_liability_account";
const SQL_RD_WITHHOLDING_LIABILITY_ACCOUNT: &str = "SELECT id,version,chart_id FROM finance_accounts WHERE id=? AND company_id=? AND category='liability' AND control_kind='none' AND status='active' AND currency='THB'";
const READ_WITHHOLDING_LIABILITY_ACCOUNT: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_LIABILITY_ACCOUNT,
    sql: SQL_RD_WITHHOLDING_LIABILITY_ACCOUNT,
    bind_count: 2,
    scope: "holding",
};

pub const RD_WITHHOLDING_INVOICE: &str = "finance_rd_withholding_invoice";
const SQL_RD_WITHHOLDING_INVOICE: &str = "SELECT i.*,j.chart_id,j.status journal_status,j.version journal_version FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=?";
const READ_WITHHOLDING_INVOICE: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_INVOICE,
    sql: SQL_RD_WITHHOLDING_INVOICE,
    bind_count: 2,
    scope: "holding",
};

pub const RD_WITHHOLDING_CONFIGURED_OWNER: &str = "finance_rd_withholding_configured_owner";
const SQL_RD_WITHHOLDING_CONFIGURED_OWNER: &str = "SELECT 1 FROM finance_tax_identities WHERE company_id=? AND party_id IS NULL AND status='approved' AND effective_from<=? AND effective_to>=?";
const READ_WITHHOLDING_CONFIGURED_OWNER: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_CONFIGURED_OWNER,
    sql: SQL_RD_WITHHOLDING_CONFIGURED_OWNER,
    bind_count: 3,
    scope: "holding",
};

pub const RD_WITHHOLDING_APPROVED_ASSESSMENT: &str = "finance_rd_withholding_approved_assessment";
const SQL_RD_WITHHOLDING_APPROVED_ASSESSMENT: &str = "SELECT * FROM finance_withholding_assessments WHERE id=? AND company_id=? AND payment_id=? AND status='approved'";
const READ_WITHHOLDING_APPROVED_ASSESSMENT: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_APPROVED_ASSESSMENT,
    sql: SQL_RD_WITHHOLDING_APPROVED_ASSESSMENT,
    bind_count: 3,
    scope: "holding",
};

pub const RD_WITHHOLDING_CONSUMED_ASSESSMENT: &str = "finance_rd_withholding_consumed_assessment";
const SQL_RD_WITHHOLDING_CONSUMED_ASSESSMENT: &str = "SELECT id,version,form_kind FROM finance_withholding_assessments WHERE id=? AND company_id=? AND status='consumed'";
const READ_WITHHOLDING_CONSUMED_ASSESSMENT: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_CONSUMED_ASSESSMENT,
    sql: SQL_RD_WITHHOLDING_CONSUMED_ASSESSMENT,
    bind_count: 2,
    scope: "holding",
};

pub const RD_WITHHOLDING_PAYMENT_DOCUMENTS: &str = "finance_rd_withholding_payment_documents";
const SQL_RD_WITHHOLDING_PAYMENT_DOCUMENTS: &str = "SELECT id FROM finance_issued_thai_documents WHERE payment_id=? AND company_id=? AND document_kind='withholding_certificate'";
const READ_WITHHOLDING_PAYMENT_DOCUMENTS: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_PAYMENT_DOCUMENTS,
    sql: SQL_RD_WITHHOLDING_PAYMENT_DOCUMENTS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_WITHHOLDING_WORKSPACE_IDENTITIES: &str = "finance_rd_withholding_workspace_identities";
const SQL_RD_WITHHOLDING_WORKSPACE_IDENTITIES: &str = "SELECT id,party_id,legal_name,tax_id,status,version FROM finance_tax_identities WHERE company_id=? AND status='approved' ORDER BY legal_name";
const READ_WITHHOLDING_WORKSPACE_IDENTITIES: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_WORKSPACE_IDENTITIES,
    sql: SQL_RD_WITHHOLDING_WORKSPACE_IDENTITIES,
    bind_count: 1,
    scope: "holding",
};

/// Native successors of the static `finance-withholding-remittance` point and
/// list reads. Assertions and FOR UPDATE owner statements stay in the caller's
/// transaction; only these byte-identical read plans cross the boundary.
pub const RD_WITHHOLDING_REMIT_PACKAGE: &str = "finance_rd_withholding_remit_package";
const SQL_RD_WITHHOLDING_REMIT_PACKAGE: &str = "SELECT * FROM finance_withholding_packages WHERE id=? AND company_id=?";
const READ_WITHHOLDING_REMIT_PACKAGE: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_REMIT_PACKAGE,
    sql: SQL_RD_WITHHOLDING_REMIT_PACKAGE,
    bind_count: 2,
    scope: "holding",
};

pub const RD_WITHHOLDING_REMIT_DOCUMENT: &str = "finance_rd_withholding_remit_document";
const SQL_RD_WITHHOLDING_REMIT_DOCUMENT: &str = "SELECT snapshot_json,document_number,export_sha256 FROM finance_issued_thai_documents WHERE id=? AND company_id=?";
const READ_WITHHOLDING_REMIT_DOCUMENT: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_REMIT_DOCUMENT,
    sql: SQL_RD_WITHHOLDING_REMIT_DOCUMENT,
    bind_count: 2,
    scope: "holding",
};

pub const RD_WITHHOLDING_REMIT_JOURNAL_VALUE: &str = "finance_rd_withholding_remit_journal_value";
const SQL_RD_WITHHOLDING_REMIT_JOURNAL_VALUE: &str = "SELECT COALESCE(SUM(credit_minor-debit_minor),0) value FROM finance_journal_lines WHERE journal_id=? AND account_id=?";
const READ_WITHHOLDING_REMIT_JOURNAL_VALUE: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_REMIT_JOURNAL_VALUE,
    sql: SQL_RD_WITHHOLDING_REMIT_JOURNAL_VALUE,
    bind_count: 2,
    scope: "holding",
};

pub const RD_WITHHOLDING_REMIT_BANK_TXN: &str = "finance_rd_withholding_remit_bank_txn";
const SQL_RD_WITHHOLDING_REMIT_BANK_TXN: &str = "SELECT t.*,s.bank_account,a.chart_id,a.version account_version,a.control_kind,a.status account_status,c.base_currency FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id JOIN finance_accounts a ON a.id=s.bank_account JOIN finance_charts c ON c.id=a.chart_id WHERE t.id=? AND t.company_id=?";
const READ_WITHHOLDING_REMIT_BANK_TXN: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_REMIT_BANK_TXN,
    sql: SQL_RD_WITHHOLDING_REMIT_BANK_TXN,
    bind_count: 2,
    scope: "holding",
};

pub const RD_WITHHOLDING_REMIT_PACKAGE_EDITION: &str = "finance_rd_withholding_remit_package_edition";
const SQL_RD_WITHHOLDING_REMIT_PACKAGE_EDITION: &str = "SELECT id,edition FROM finance_withholding_packages WHERE company_id=? AND month=? AND form_kind=? ORDER BY edition DESC LIMIT 1";
const READ_WITHHOLDING_REMIT_PACKAGE_EDITION: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_REMIT_PACKAGE_EDITION,
    sql: SQL_RD_WITHHOLDING_REMIT_PACKAGE_EDITION,
    bind_count: 3,
    scope: "holding",
};

pub const RD_WITHHOLDING_REMIT_ROW: &str = "finance_rd_withholding_remit_row";
const SQL_RD_WITHHOLDING_REMIT_ROW: &str = "SELECT r.*,p.chart_id,p.month,p.form_kind,j.entry_date FROM finance_withholding_remittances r JOIN finance_withholding_packages p ON p.id=r.package_id JOIN finance_journals j ON j.id=r.journal_id WHERE r.id=? AND r.company_id=?";
const READ_WITHHOLDING_REMIT_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_REMIT_ROW,
    sql: SQL_RD_WITHHOLDING_REMIT_ROW,
    bind_count: 2,
    scope: "holding",
};

pub const RD_WITHHOLDING_REMIT_PACKAGE_LIST: &str = "finance_rd_withholding_remit_package_list";
const SQL_RD_WITHHOLDING_REMIT_PACKAGE_LIST: &str = "SELECT p.*,r.id remittance_id,r.version remittance_version,r.status remittance_status,r.bank_transaction_id FROM finance_withholding_packages p LEFT JOIN finance_withholding_remittances r ON r.package_id=p.id WHERE p.company_id=? ORDER BY p.month DESC,p.edition DESC";
const READ_WITHHOLDING_REMIT_PACKAGE_LIST: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_REMIT_PACKAGE_LIST,
    sql: SQL_RD_WITHHOLDING_REMIT_PACKAGE_LIST,
    bind_count: 1,
    scope: "holding",
};

pub const RD_WITHHOLDING_REMIT_BANK_CANDIDATES: &str = "finance_rd_withholding_remit_bank_candidates";
const SQL_RD_WITHHOLDING_REMIT_BANK_CANDIDATES: &str = "SELECT t.id,t.version,t.transaction_date,t.external_id,t.amount_minor,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id JOIN finance_accounts a ON a.id=s.bank_account WHERE t.company_id=? AND t.currency='THB' AND t.match_status IN ('unmatched','proposed') AND a.control_kind='bank' ORDER BY t.transaction_date DESC";
const READ_WITHHOLDING_REMIT_BANK_CANDIDATES: NativeReadStatement = NativeReadStatement {
    kind: RD_WITHHOLDING_REMIT_BANK_CANDIDATES,
    sql: SQL_RD_WITHHOLDING_REMIT_BANK_CANDIDATES,
    bind_count: 1,
    scope: "holding",
};

/// Native successors of the static `finance-planning` point/list reads.
/// Ledger/advisory-lock statements and dynamic journal projections stay in the
/// caller's transaction; only these byte-identical read plans cross the boundary.
pub const RD_PLAN_ACCOUNTS: &str = "finance_rd_plan_accounts";
const SQL_RD_PLAN_ACCOUNTS: &str = "SELECT id,code,name,category FROM finance_accounts WHERE chart_id=? AND company_id=? AND status='active'";
const READ_PLAN_ACCOUNTS: NativeReadStatement = NativeReadStatement {
    kind: RD_PLAN_ACCOUNTS,
    sql: SQL_RD_PLAN_ACCOUNTS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_PLAN_DIMENSION: &str = "finance_rd_plan_dimension";
const SQL_RD_PLAN_DIMENSION: &str = "SELECT 1 FROM finance_dimension_definitions d JOIN finance_dimension_values v ON v.dimension_id=d.id WHERE d.company_id=? AND d.code=? AND d.status='active' AND v.code=? AND v.status='active'";
const READ_PLAN_DIMENSION: NativeReadStatement = NativeReadStatement {
    kind: RD_PLAN_DIMENSION,
    sql: SQL_RD_PLAN_DIMENSION,
    bind_count: 3,
    scope: "holding",
};

pub const RD_PLAN_BANK_ACCOUNT: &str = "finance_rd_plan_bank_account";
const SQL_RD_PLAN_BANK_ACCOUNT: &str = "SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND control_kind='bank'";
const READ_PLAN_BANK_ACCOUNT: NativeReadStatement = NativeReadStatement {
    kind: RD_PLAN_BANK_ACCOUNT,
    sql: SQL_RD_PLAN_BANK_ACCOUNT,
    bind_count: 2,
    scope: "holding",
};

pub const RD_PLAN_EDITIONS: &str = "finance_rd_plan_editions";
const SQL_RD_PLAN_EDITIONS: &str = "SELECT * FROM finance_plan_editions WHERE company_id=? AND chart_id=? ORDER BY created_at DESC,id";
const READ_PLAN_EDITIONS: NativeReadStatement = NativeReadStatement {
    kind: RD_PLAN_EDITIONS,
    sql: SQL_RD_PLAN_EDITIONS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_PLAN_SNAPSHOTS: &str = "finance_rd_plan_snapshots";
const SQL_RD_PLAN_SNAPSHOTS: &str = "SELECT s.id,s.definition_id,s.start_date,s.end_date,s.source_hash,s.created_at,p.name FROM finance_report_snapshots s JOIN finance_plan_editions p ON p.id=s.definition_id WHERE s.company_id=? AND p.chart_id=? ORDER BY s.created_at DESC LIMIT 200";
const READ_PLAN_SNAPSHOTS: NativeReadStatement = NativeReadStatement {
    kind: RD_PLAN_SNAPSHOTS,
    sql: SQL_RD_PLAN_SNAPSHOTS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_PLAN_CATEGORY: &str = "finance_rd_plan_category";
const SQL_RD_PLAN_CATEGORY: &str = "SELECT category FROM finance_accounts WHERE id=?";
const READ_PLAN_CATEGORY: NativeReadStatement = NativeReadStatement {
    kind: RD_PLAN_CATEGORY,
    sql: SQL_RD_PLAN_CATEGORY,
    bind_count: 1,
    scope: "holding",
};

pub const RD_PLAN_EXPORT_ROW: &str = "finance_rd_plan_export_row";
const SQL_RD_PLAN_EXPORT_ROW: &str = "SELECT result_json,export_content,export_sha256 FROM finance_report_snapshots WHERE id=? AND company_id=?";
const READ_PLAN_EXPORT_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_PLAN_EXPORT_ROW,
    sql: SQL_RD_PLAN_EXPORT_ROW,
    bind_count: 2,
    scope: "holding",
};

pub const RD_PLAN_SOURCE_SNAPSHOT: &str = "finance_rd_plan_source_snapshot";
const SQL_RD_PLAN_SOURCE_SNAPSHOT: &str = "SELECT s.*,p.content_json,p.chart_id FROM finance_report_snapshots s JOIN finance_plan_editions p ON p.id=s.definition_id WHERE s.id=? AND s.company_id=? AND p.company_id=?";
const READ_PLAN_SOURCE_SNAPSHOT: NativeReadStatement = NativeReadStatement {
    kind: RD_PLAN_SOURCE_SNAPSHOT,
    sql: SQL_RD_PLAN_SOURCE_SNAPSHOT,
    bind_count: 3,
    scope: "holding",
};

/// Native successors of the static `finance-business-contracts` point/list
/// reads. Command assertion guards and owner-write statements stay in caller
/// transactions; only these byte-identical read plans cross the boundary.
pub const RD_CONTRACT_CUSTOMER: &str = "finance_rd_contract_customer";
const SQL_RD_CONTRACT_CUSTOMER: &str = "SELECT id,version FROM finance_parties WHERE id=? AND company_id=? AND status='active' AND kind='customer' AND currency=? AND person_id IS NULL";
const READ_CONTRACT_CUSTOMER: NativeReadStatement = NativeReadStatement {
    kind: RD_CONTRACT_CUSTOMER,
    sql: SQL_RD_CONTRACT_CUSTOMER,
    bind_count: 3,
    scope: "holding",
};

pub const RD_CONTRACT_DIMENSIONS: &str = "finance_rd_contract_dimensions";
const SQL_RD_CONTRACT_DIMENSIONS: &str = "SELECT id,code,required FROM finance_dimension_definitions WHERE company_id=? AND status='active' ORDER BY code";
const READ_CONTRACT_DIMENSIONS: NativeReadStatement = NativeReadStatement {
    kind: RD_CONTRACT_DIMENSIONS,
    sql: SQL_RD_CONTRACT_DIMENSIONS,
    bind_count: 1,
    scope: "holding",
};

pub const RD_CONTRACT_DIMENSION_VALUE: &str = "finance_rd_contract_dimension_value";
const SQL_RD_CONTRACT_DIMENSION_VALUE: &str = "SELECT 1 FROM finance_dimension_values WHERE company_id=? AND dimension_id=? AND code=? AND status='active'";
const READ_CONTRACT_DIMENSION_VALUE: NativeReadStatement = NativeReadStatement {
    kind: RD_CONTRACT_DIMENSION_VALUE,
    sql: SQL_RD_CONTRACT_DIMENSION_VALUE,
    bind_count: 3,
    scope: "holding",
};

pub const RD_CONTRACT_TAX_CODE: &str = "finance_rd_contract_tax_code";
const SQL_RD_CONTRACT_TAX_CODE: &str = "SELECT id,code,rate_micros,effective_from,effective_to FROM finance_tax_codes WHERE company_id=? AND status='active' AND (code=? OR id=?) AND rate_micros=70000 AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1";
const READ_CONTRACT_TAX_CODE: NativeReadStatement = NativeReadStatement {
    kind: RD_CONTRACT_TAX_CODE,
    sql: SQL_RD_CONTRACT_TAX_CODE,
    bind_count: 5,
    scope: "holding",
};

pub const RD_CONTRACT_BILLING_RUN: &str = "finance_rd_contract_billing_run";
const SQL_RD_CONTRACT_BILLING_RUN: &str = "SELECT b.*,i.status original_invoice_status,i.version original_invoice_version,i.journal_id original_journal_id,i.posted_at original_posted_at FROM finance_business_billing_runs b LEFT JOIN finance_invoices i ON i.id=b.invoice_id WHERE b.id=? AND b.company_id=? AND b.contract_id=?";
const READ_CONTRACT_BILLING_RUN: NativeReadStatement = NativeReadStatement {
    kind: RD_CONTRACT_BILLING_RUN,
    sql: SQL_RD_CONTRACT_BILLING_RUN,
    bind_count: 3,
    scope: "holding",
};

pub const RD_CONTRACT_BILLING_EXISTS: &str = "finance_rd_contract_billing_exists";
const SQL_RD_CONTRACT_BILLING_EXISTS: &str = "SELECT 1 FROM finance_business_billing_runs WHERE root_id=? AND billing_key=?";
const READ_CONTRACT_BILLING_EXISTS: NativeReadStatement = NativeReadStatement {
    kind: RD_CONTRACT_BILLING_EXISTS,
    sql: SQL_RD_CONTRACT_BILLING_EXISTS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_CONTRACT_BILL_SOURCE: &str = "finance_rd_contract_bill_source";
const SQL_RD_CONTRACT_BILL_SOURCE: &str = "SELECT b.*,COALESCE(b.contract_content_json,c.content_json) content_json FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id WHERE b.id=? AND b.company_id=?";
const READ_CONTRACT_BILL_SOURCE: NativeReadStatement = NativeReadStatement {
    kind: RD_CONTRACT_BILL_SOURCE,
    sql: SQL_RD_CONTRACT_BILL_SOURCE,
    bind_count: 2,
    scope: "holding",
};

pub const RD_CONTRACT_POSTED_CORRECTION: &str = "finance_rd_contract_posted_correction";
const SQL_RD_CONTRACT_POSTED_CORRECTION: &str = "SELECT 1 FROM finance_billing_corrections WHERE billing_run_id=? AND status='posted'";
const READ_CONTRACT_POSTED_CORRECTION: NativeReadStatement = NativeReadStatement {
    kind: RD_CONTRACT_POSTED_CORRECTION,
    sql: SQL_RD_CONTRACT_POSTED_CORRECTION,
    bind_count: 1,
    scope: "holding",
};

/// Native successors of the static `finance-invoice-read` point/list reads.
/// Owner-write helpers, assertion guards and dynamic field-invoice projections
/// stay in the caller's transaction; these plans are byte-identical reads.
pub const RD_INVOICE_REPORT_CHARTS: &str = "finance_rd_invoice_report_charts";
const SQL_RD_INVOICE_REPORT_CHARTS: &str = "SELECT id FROM finance_charts WHERE company_id=?";
const READ_INVOICE_REPORT_CHARTS: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE_REPORT_CHARTS,
    sql: SQL_RD_INVOICE_REPORT_CHARTS,
    bind_count: 1,
    scope: "holding",
};

pub const RD_INVOICE_ACCOUNTS: &str = "finance_rd_invoice_accounts";
const SQL_RD_INVOICE_ACCOUNTS: &str = "SELECT * FROM finance_accounts WHERE company_id=? AND chart_id=? ORDER BY code";
const READ_INVOICE_ACCOUNTS: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE_ACCOUNTS,
    sql: SQL_RD_INVOICE_ACCOUNTS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_INVOICE_LINE_HISTORY: &str = "finance_rd_invoice_line_history";
const SQL_RD_INVOICE_LINE_HISTORY: &str = "SELECT source_version,origin_kind,COUNT(*) line_count,SUM(subtotal_minor) subtotal_minor,SUM(tax_minor) tax_minor,MIN(created_at) created_at FROM finance_invoice_line_versions WHERE invoice_id=? AND company_id=? GROUP BY source_version,origin_kind ORDER BY source_version";
const READ_INVOICE_LINE_HISTORY: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE_LINE_HISTORY,
    sql: SQL_RD_INVOICE_LINE_HISTORY,
    bind_count: 2,
    scope: "holding",
};

pub const RD_INVOICE_THAI_DOCUMENTS: &str = "finance_rd_invoice_thai_documents";
const SQL_RD_INVOICE_THAI_DOCUMENTS: &str = "SELECT id,document_number,document_kind,edition,issue_date FROM finance_issued_thai_documents WHERE invoice_id=? AND company_id=? ORDER BY edition";
const READ_INVOICE_THAI_DOCUMENTS: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE_THAI_DOCUMENTS,
    sql: SQL_RD_INVOICE_THAI_DOCUMENTS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_INVOICE_PAYMENT_REVERSAL: &str = "finance_rd_invoice_payment_reversal";
const SQL_RD_INVOICE_PAYMENT_REVERSAL: &str = "SELECT * FROM finance_payment_reversals WHERE payment_id=?";
const READ_INVOICE_PAYMENT_REVERSAL: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE_PAYMENT_REVERSAL,
    sql: SQL_RD_INVOICE_PAYMENT_REVERSAL,
    bind_count: 1,
    scope: "holding",
};

pub const RD_INVOICE_OPEN_PERIODS: &str = "finance_rd_invoice_open_periods";
const SQL_RD_INVOICE_OPEN_PERIODS: &str = "SELECT * FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC";
const READ_INVOICE_OPEN_PERIODS: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE_OPEN_PERIODS,
    sql: SQL_RD_INVOICE_OPEN_PERIODS,
    bind_count: 1,
    scope: "holding",
};

pub const RD_INVOICE_STATEMENT_ROWS: &str = "finance_rd_invoice_statement_rows";
const SQL_RD_INVOICE_STATEMENT_ROWS: &str = "SELECT * FROM finance_bank_transactions WHERE statement_id=? AND company_id=? ORDER BY transaction_date,id LIMIT 31 OFFSET ?";
const READ_INVOICE_STATEMENT_ROWS: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE_STATEMENT_ROWS,
    sql: SQL_RD_INVOICE_STATEMENT_ROWS,
    bind_count: 3,
    scope: "holding",
};

pub const RD_INVOICE_OPEN_INVOICES: &str = "finance_rd_invoice_open_invoices";
const SQL_RD_INVOICE_OPEN_INVOICES: &str = "SELECT id,invoice_number,kind,outstanding_minor,status,version\n      FROM finance_invoices WHERE company_id=? AND party_id=? AND currency=? AND kind=? AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND source_type!='expense_claim' AND (?::text IS NULL OR id=?)\n      ORDER BY due_date,invoice_number";
const READ_INVOICE_OPEN_INVOICES: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE_OPEN_INVOICES,
    sql: SQL_RD_INVOICE_OPEN_INVOICES,
    bind_count: 6,
    scope: "holding",
};


/// Native successors of the second static `finance-invoice-read` point/list
/// family. Field-invoice ownership helpers and transaction guards stay in TS.
pub const RD_INVOICE2_INVOICES: &str = "finance_rd_invoice2_invoices";
const SQL_RD_INVOICE2_INVOICES: &str = "SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name\n    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id\n    WHERE i.company_id=? ORDER BY i.invoice_date DESC,i.invoice_number DESC LIMIT ?";
const READ_INVOICE2_INVOICES: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE2_INVOICES,
    sql: SQL_RD_INVOICE2_INVOICES,
    bind_count: 2,
    scope: "holding",
};

pub const RD_INVOICE2_INVOICE_SNAPSHOT: &str = "finance_rd_invoice2_invoice_snapshot";
const SQL_RD_INVOICE2_INVOICE_SNAPSHOT: &str = "SELECT i.*,p.code AS party_code,p.name AS party_name,\n    s.usd_units::text AS snapshot_usd_units,s.amount_units::text AS snapshot_source_units,s.fx_rate_id AS snapshot_rate_id\n    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id\n    LEFT JOIN money_transaction_snapshots s ON s.company_id=i.company_id AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total'\n    WHERE i.id=? AND i.company_id=?";
const READ_INVOICE2_INVOICE_SNAPSHOT: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE2_INVOICE_SNAPSHOT,
    sql: SQL_RD_INVOICE2_INVOICE_SNAPSHOT,
    bind_count: 2,
    scope: "holding",
};

pub const RD_INVOICE2_INVOICE_ALLOCATIONS: &str = "finance_rd_invoice2_invoice_allocations";
const SQL_RD_INVOICE2_INVOICE_ALLOCATIONS: &str = "SELECT a.id,a.payment_id,a.amount_minor,a.created_at,p.payment_number,p.status,p.payment_slip_id\n    FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id\n    WHERE a.invoice_id=? AND a.company_id=? ORDER BY a.created_at";
const READ_INVOICE2_INVOICE_ALLOCATIONS: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE2_INVOICE_ALLOCATIONS,
    sql: SQL_RD_INVOICE2_INVOICE_ALLOCATIONS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_INVOICE2_PAYMENTS: &str = "finance_rd_invoice2_payments";
const SQL_RD_INVOICE2_PAYMENTS: &str = "SELECT p.id,p.payment_number,p.kind,p.payment_date,p.currency,p.amount_minor,p.outstanding_minor,p.status,f.name AS party_name\n    FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id\n    WHERE p.company_id=? ORDER BY p.payment_date DESC,p.payment_number DESC LIMIT ?";
const READ_INVOICE2_PAYMENTS: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE2_PAYMENTS,
    sql: SQL_RD_INVOICE2_PAYMENTS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_INVOICE2_PAYMENT_ALLOCATIONS: &str = "finance_rd_invoice2_payment_allocations";
const SQL_RD_INVOICE2_PAYMENT_ALLOCATIONS: &str = "SELECT a.id,a.invoice_id,a.amount_minor,a.created_at,i.invoice_number,i.status\n    FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id\n    WHERE a.payment_id=? AND a.company_id=? ORDER BY a.created_at";
const READ_INVOICE2_PAYMENT_ALLOCATIONS: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE2_PAYMENT_ALLOCATIONS,
    sql: SQL_RD_INVOICE2_PAYMENT_ALLOCATIONS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_INVOICE2_BANK_CANDIDATES: &str = "finance_rd_invoice2_bank_candidates";
const SQL_RD_INVOICE2_BANK_CANDIDATES: &str = "SELECT t.id,t.transaction_date,t.external_id,t.description,t.amount_minor,t.currency,t.match_status,t.version,s.statement_number\n      FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id\n      WHERE t.company_id=? AND s.bank_account=? AND t.currency=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.transaction_date>=?\n      ORDER BY t.transaction_date DESC,t.external_id";
const READ_INVOICE2_BANK_CANDIDATES: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE2_BANK_CANDIDATES,
    sql: SQL_RD_INVOICE2_BANK_CANDIDATES,
    bind_count: 5,
    scope: "holding",
};

pub const RD_INVOICE2_BANK_ACCOUNTS: &str = "finance_rd_invoice2_bank_accounts";
const SQL_RD_INVOICE2_BANK_ACCOUNTS: &str = "SELECT a.id,a.code,a.name,a.category,a.currency,c.base_currency\n      FROM finance_accounts bank JOIN finance_charts c ON c.id=bank.chart_id JOIN finance_accounts a ON a.chart_id=c.id\n      WHERE bank.id=? AND bank.company_id=? AND a.company_id=bank.company_id AND a.status='active' AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense')\n      ORDER BY a.code";
const READ_INVOICE2_BANK_ACCOUNTS: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE2_BANK_ACCOUNTS,
    sql: SQL_RD_INVOICE2_BANK_ACCOUNTS,
    bind_count: 2,
    scope: "holding",
};

pub const RD_INVOICE2_BANK_STATEMENT: &str = "finance_rd_invoice2_bank_statement";
const SQL_RD_INVOICE2_BANK_STATEMENT: &str = "SELECT s.*,a.currency,a.code,a.name,c.base_currency FROM finance_bank_statements s JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=s.company_id JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=s.company_id WHERE s.id=? AND s.company_id=?";
const READ_INVOICE2_BANK_STATEMENT: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE2_BANK_STATEMENT,
    sql: SQL_RD_INVOICE2_BANK_STATEMENT,
    bind_count: 2,
    scope: "holding",
};


/// Native successors of the static `finance-ai` point/list reads. Owner-write
/// statements and assertion guards stay in caller transactions; these plans
/// are byte-identical reads with the exact positional payloads.
pub const RD_AI_POLICY: &str = "finance_rd_ai_policy";
const SQL_RD_AI_POLICY: &str = "SELECT * FROM finance_ai_policies WHERE company_id=?";
const READ_AI_POLICY: NativeReadStatement = NativeReadStatement {
    kind: RD_AI_POLICY,
    sql: SQL_RD_AI_POLICY,
    bind_count: 1,
    scope: "holding",
};

pub const RD_AI_PROPOSAL_EXACT: &str = "finance_rd_ai_proposal_exact";
const SQL_RD_AI_PROPOSAL_EXACT: &str = "SELECT * FROM finance_ai_proposals WHERE company_id=? AND kind=? AND source_id=? AND source_hash=? AND provider_json=? AND policy_version=?";
const READ_AI_PROPOSAL_EXACT: NativeReadStatement = NativeReadStatement {
    kind: RD_AI_PROPOSAL_EXACT,
    sql: SQL_RD_AI_PROPOSAL_EXACT,
    bind_count: 6,
    scope: "holding",
};

pub const RD_AI_CLAIMABLE: &str = "finance_rd_ai_claimable";
const SQL_RD_AI_CLAIMABLE: &str = "SELECT id FROM finance_ai_proposals WHERE attempts<3 AND (state='queued' OR (state='running' AND claim_until<?)) ORDER BY created_at,id LIMIT 2";
const READ_AI_CLAIMABLE: NativeReadStatement = NativeReadStatement {
    kind: RD_AI_CLAIMABLE,
    sql: SQL_RD_AI_CLAIMABLE,
    bind_count: 1,
    scope: "holding",
};

pub const RD_AI_EVENTS: &str = "finance_rd_ai_events";
const SQL_RD_AI_EVENTS: &str = "SELECT proposal_id,action,evidence_json,created_at FROM finance_ai_events WHERE company_id=? AND proposal_id IS NOT NULL ORDER BY created_at DESC LIMIT 120";
const READ_AI_EVENTS: NativeReadStatement = NativeReadStatement {
    kind: RD_AI_EVENTS,
    sql: SQL_RD_AI_EVENTS,
    bind_count: 1,
    scope: "holding",
};

pub const RD_AI_DUE_POLICIES: &str = "finance_rd_ai_due_policies";
const SQL_RD_AI_DUE_POLICIES: &str = "SELECT * FROM finance_ai_policies WHERE enabled AND next_scan_at<=? ORDER BY next_scan_at,company_id LIMIT 3";
const READ_AI_DUE_POLICIES: NativeReadStatement = NativeReadStatement {
    kind: RD_AI_DUE_POLICIES,
    sql: SQL_RD_AI_DUE_POLICIES,
    bind_count: 1,
    scope: "holding",
};

pub const RD_AI_BANK_CANDIDATES: &str = "finance_rd_ai_bank_candidates";
const SQL_RD_AI_BANK_CANDIDATES: &str = "SELECT id FROM finance_bank_transactions WHERE company_id=? AND match_status<>'matched' AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_bank_transactions.company_id AND p.source_id=finance_bank_transactions.id AND p.kind='bank_match' AND p.policy_version=? AND p.source_version=finance_bank_transactions.version) ORDER BY transaction_date,id LIMIT 20";
const READ_AI_BANK_CANDIDATES: NativeReadStatement = NativeReadStatement {
    kind: RD_AI_BANK_CANDIDATES,
    sql: SQL_RD_AI_BANK_CANDIDATES,
    bind_count: 2,
    scope: "holding",
};

pub const RD_AI_REPORT_CHARTS: &str = "finance_rd_ai_report_charts";
const SQL_RD_AI_REPORT_CHARTS: &str = "SELECT id FROM finance_charts WHERE company_id=? AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_charts.company_id AND p.source_id=finance_charts.id AND p.kind='report_explanation' AND p.policy_version=? AND p.source_query_json::jsonb->>'endDate'=?) ORDER BY id LIMIT 5";
const READ_AI_REPORT_CHARTS: NativeReadStatement = NativeReadStatement {
    kind: RD_AI_REPORT_CHARTS,
    sql: SQL_RD_AI_REPORT_CHARTS,
    bind_count: 3,
    scope: "holding",
};

pub const RD_AI_PROPOSAL_BY_SOURCE: &str = "finance_rd_ai_proposal_by_source";
const SQL_RD_AI_PROPOSAL_BY_SOURCE: &str = "SELECT id FROM finance_ai_proposals WHERE company_id=? AND kind=? AND source_id=? AND policy_version=? AND (?::boolean OR source_version=?) AND (?::text IS NULL OR source_query_json::jsonb->>'endDate'=?) LIMIT 1";
const READ_AI_PROPOSAL_BY_SOURCE: NativeReadStatement = NativeReadStatement {
    kind: RD_AI_PROPOSAL_BY_SOURCE,
    sql: SQL_RD_AI_PROPOSAL_BY_SOURCE,
    bind_count: 8,
    scope: "holding",
};


/// Native successors of the static `finance-asset-policies` point/list reads.
/// Every plan is byte-identical to the retired literal; owner writes and
/// assertion guards remain in the caller's transaction.
pub const RD_ASSET_POLICY_RULE: &str = "finance_rd_asset_policy_rule";
const SQL_RD_ASSET_POLICY_RULE: &str = "SELECT DISTINCT ON (secondary_chart_id) id,secondary_chart_id,effective_date FROM finance_book_rules WHERE company_id=? AND primary_chart_id=? AND status='approved' AND effective_date<=? ORDER BY secondary_chart_id,effective_date DESC";
const READ_ASSET_POLICY_RULE: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_POLICY_RULE,
    sql: SQL_RD_ASSET_POLICY_RULE,
    bind_count: 3,
    scope: "holding",
};

pub const RD_ASSET_POLICY_MAPPINGS: &str = "finance_rd_asset_policy_mappings";
const SQL_RD_ASSET_POLICY_MAPPINGS: &str = "SELECT primary_account_id,secondary_account_id FROM finance_book_account_mappings WHERE rule_id=? AND primary_account_id IN (?,?,?) ORDER BY primary_account_id";
const READ_ASSET_POLICY_MAPPINGS: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_POLICY_MAPPINGS,
    sql: SQL_RD_ASSET_POLICY_MAPPINGS,
    bind_count: 4,
    scope: "holding",
};

pub const RD_ASSET_POLICY_POSTED_RUNS: &str = "finance_rd_asset_policy_posted_runs";
const SQL_RD_ASSET_POLICY_POSTED_RUNS: &str = "SELECT * FROM finance_asset_policy_runs WHERE asset_id=? AND status='posted' ORDER BY ordinal";
const READ_ASSET_POLICY_POSTED_RUNS: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_POLICY_POSTED_RUNS,
    sql: SQL_RD_ASSET_POLICY_POSTED_RUNS,
    bind_count: 1,
    scope: "holding",
};

pub const RD_ASSET_POLICY_LEGACY: &str = "finance_rd_asset_policy_legacy";
const SQL_RD_ASSET_POLICY_LEGACY: &str = "SELECT 1 FROM finance_asset_policy_runs WHERE asset_id=? AND ((status='posted' AND ordinal>?) OR status='draft')";
const READ_ASSET_POLICY_LEGACY: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_POLICY_LEGACY,
    sql: SQL_RD_ASSET_POLICY_LEGACY,
    bind_count: 2,
    scope: "holding",
};

pub const RD_ASSET_POLICY_WORKSPACE_ASSETS: &str = "finance_rd_asset_policy_workspace_assets";
const SQL_RD_ASSET_POLICY_WORKSPACE_ASSETS: &str = "SELECT a.*,c.chart_id,(SELECT count(*) FROM finance_asset_depreciations old WHERE old.asset_id=a.id)+(SELECT count(*) FROM finance_asset_policy_runs run WHERE run.asset_id=a.id AND run.status='posted') completed_depreciation_months FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.company_id=? AND a.status='in_service' ORDER BY asset_number";
const READ_ASSET_POLICY_WORKSPACE_ASSETS: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_POLICY_WORKSPACE_ASSETS,
    sql: SQL_RD_ASSET_POLICY_WORKSPACE_ASSETS,
    bind_count: 1,
    scope: "holding",
};

pub const RD_ASSET_POLICY_WORKSPACE_BOOKS: &str = "finance_rd_asset_policy_workspace_books";
const SQL_RD_ASSET_POLICY_WORKSPACE_BOOKS: &str = "SELECT c.id,c.name,r.primary_chart_id FROM finance_charts c LEFT JOIN (SELECT DISTINCT secondary_chart_id,primary_chart_id FROM finance_book_rules WHERE status='approved') r ON r.secondary_chart_id=c.id WHERE c.company_id=? AND c.status='active' ORDER BY c.id";
const READ_ASSET_POLICY_WORKSPACE_BOOKS: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_POLICY_WORKSPACE_BOOKS,
    sql: SQL_RD_ASSET_POLICY_WORKSPACE_BOOKS,
    bind_count: 1,
    scope: "holding",
};

pub const RD_ASSET_POLICY_DISPOSAL: &str = "finance_rd_asset_policy_disposal";
const SQL_RD_ASSET_POLICY_DISPOSAL: &str = "SELECT * FROM finance_asset_policy_editions WHERE asset_id=? AND company_id=? AND status='active'";
const READ_ASSET_POLICY_DISPOSAL: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_POLICY_DISPOSAL,
    sql: SQL_RD_ASSET_POLICY_DISPOSAL,
    bind_count: 2,
    scope: "holding",
};

pub const RD_ASSET_POLICY_GAIN_LOSS: &str = "finance_rd_asset_policy_gain_loss";
const SQL_RD_ASSET_POLICY_GAIN_LOSS: &str = "SELECT m.primary_account_id,m.secondary_account_id,a.version,a.status FROM finance_book_account_mappings m JOIN finance_accounts a ON a.id=m.secondary_account_id WHERE m.rule_id=? AND m.primary_account_id IN (?,?) ORDER BY m.primary_account_id";
const READ_ASSET_POLICY_GAIN_LOSS: NativeReadStatement = NativeReadStatement {
    kind: RD_ASSET_POLICY_GAIN_LOSS,
    sql: SQL_RD_ASSET_POLICY_GAIN_LOSS,
    bind_count: 3,
    scope: "holding",
};


/// Native successors of the static `finance` point/list reads. Owner-write
/// helpers, locks and assertion guards stay in caller transactions.
pub const RD_FIN_COMMAND_EXISTS: &str = "finance_rd_fin_command_exists";
const SQL_RD_FIN_COMMAND_EXISTS: &str = "SELECT 1 FROM finance_commands WHERE command_id=?";
const READ_FIN_COMMAND_EXISTS: NativeReadStatement = NativeReadStatement {
    kind: RD_FIN_COMMAND_EXISTS,
    sql: SQL_RD_FIN_COMMAND_EXISTS,
    bind_count: 1,
    scope: "holding",
};

pub const RD_FIN_CHART_ROW: &str = "finance_rd_fin_chart_row";
const SQL_RD_FIN_CHART_ROW: &str = "SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=?";
const READ_FIN_CHART_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_FIN_CHART_ROW,
    sql: SQL_RD_FIN_CHART_ROW,
    bind_count: 2,
    scope: "holding",
};

pub const RD_FIN_ACCOUNT_ROW: &str = "finance_rd_fin_account_row";
const SQL_RD_FIN_ACCOUNT_ROW: &str = "SELECT id,status,version,is_control FROM finance_accounts WHERE id=? AND chart_id=? AND company_id=?";
const READ_FIN_ACCOUNT_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_FIN_ACCOUNT_ROW,
    sql: SQL_RD_FIN_ACCOUNT_ROW,
    bind_count: 3,
    scope: "holding",
};

pub const RD_FIN_ACCOUNT_BY_CODE: &str = "finance_rd_fin_account_by_code";
const SQL_RD_FIN_ACCOUNT_BY_CODE: &str = "SELECT * FROM finance_accounts WHERE chart_id=? AND company_id=? AND code=?";
const READ_FIN_ACCOUNT_BY_CODE: NativeReadStatement = NativeReadStatement {
    kind: RD_FIN_ACCOUNT_BY_CODE,
    sql: SQL_RD_FIN_ACCOUNT_BY_CODE,
    bind_count: 3,
    scope: "holding",
};

pub const RD_FIN_JOURNAL_DEBIT: &str = "finance_rd_fin_journal_debit";
const SQL_RD_FIN_JOURNAL_DEBIT: &str = "SELECT COALESCE(SUM(debit_minor),0) AS amount FROM finance_journal_lines WHERE journal_id=?";
const READ_FIN_JOURNAL_DEBIT: NativeReadStatement = NativeReadStatement {
    kind: RD_FIN_JOURNAL_DEBIT,
    sql: SQL_RD_FIN_JOURNAL_DEBIT,
    bind_count: 1,
    scope: "holding",
};

pub const RD_FIN_JOURNAL_ROW: &str = "finance_rd_fin_journal_row";
const SQL_RD_FIN_JOURNAL_ROW: &str = "SELECT * FROM finance_journals WHERE id=? AND company_id=?";
const READ_FIN_JOURNAL_ROW: NativeReadStatement = NativeReadStatement {
    kind: RD_FIN_JOURNAL_ROW,
    sql: SQL_RD_FIN_JOURNAL_ROW,
    bind_count: 2,
    scope: "holding",
};

pub const RD_FIN_JOURNAL_LINES: &str = "finance_rd_fin_journal_lines";
const SQL_RD_FIN_JOURNAL_LINES: &str = "SELECT id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal";
const READ_FIN_JOURNAL_LINES: NativeReadStatement = NativeReadStatement {
    kind: RD_FIN_JOURNAL_LINES,
    sql: SQL_RD_FIN_JOURNAL_LINES,
    bind_count: 1,
    scope: "holding",
};

pub const RD_FIN_OPEN_PERIOD: &str = "finance_rd_fin_open_period";
const SQL_RD_FIN_OPEN_PERIOD: &str = "SELECT * FROM finance_periods WHERE id=? AND company_id=? AND state='open'";
const READ_FIN_OPEN_PERIOD: NativeReadStatement = NativeReadStatement {
    kind: RD_FIN_OPEN_PERIOD,
    sql: SQL_RD_FIN_OPEN_PERIOD,
    bind_count: 2,
    scope: "holding",
};

const READS: &[NativeReadStatement] = &[
    READ_ASSET_POLICY_ROW,
    READ_ASSET_POLICY_RUNS,
    READ_INSTRUMENT_EDITION,
    READ_INSTRUMENT_RUNS,
    READ_COLLECTION_CASE_ROW,
    READ_BUSINESS_BILL_ROW,
    READ_PERIOD_END_DATES,
    READ_ALL_PERIODS,
    READ_SUPPLIER_BATCH_ROW,
    READ_SUPPLIER_INSTRUCTION_ROW,
    READ_SUPPLIER_BATCH_LIST,
    READ_SUPPLIER_BATCH_WORK_LIST,
    READ_SUPPLIER_BATCH_ITEMS,
    READ_SUPPLIER_BATCH_INVOICES,
    READ_SUPPLIER_BATCH_INVOICE,
    READ_SUPPLIER_BATCH_ACCOUNTS,
    READ_SUPPLIER_BATCH_PERIODS,
    READ_SUPPLIER_BATCH_BANK,
    READ_TAX_PROFILE,
    READ_TAX_PACKAGE,
    READ_TAX_CORRECTION,
    READ_TAX_ENTRY,
    READ_TAX_PREVIOUS_ENTRY,
    READ_TAX_ORIGINAL_ENTRY,
    READ_TAX_PENDING_FAMILY,
    READ_TAX_PROFILES,
    READ_TAX_CHART,
    READ_TAX_CURRENT_ENTRY,
    READ_TAX_PENDING_CORRECTION_COUNT,
    READ_TAX_CLOSE_PROFILE,
    READ_BANK_IMPORTED_FILE,
    READ_TAX_IDENTITY_ACTIVE_PARTY,
    READ_TAX_IDENTITY_PREDECESSOR,
    READ_TAX_IDENTITY_ROW,
    READ_TAX_IDENTITY_REVIEWED,
    READ_TAX_IDENTITY_LIST,
    READ_TAX_IDENTITY_PARTY_LIST,
    READ_ASSET_ACCOUNT_ROWS,
    READ_ASSET_EVENT_LIST,
    READ_ASSET_DISPOSAL_LIST,
    READ_ASSET_REVIEWED_POLICY,
    READ_ASSET_DEPRECIATION_HISTORY,
    READ_ASSET_LATEST_DEPRECIATION,
    READ_ASSET_RETAINED_POLICY_HASH,
    READ_ASSET_DIMENSION_VALUES,
    READ_REPORT_REVISION,
    READ_REPORT_PROFILE,
    READ_REPORT_PACKAGE_LIST,
    READ_REPORT_CLOSING_LIST,
    READ_REPORT_ACCOUNT_LIST,
    READ_REPORT_STATEMENT_HISTORY,
    READ_REPORT_PREVIOUS_PACKAGE,
    READ_REPORT_NEXT_EDITION,
    READ_WITHHOLDING_BANK_ACCOUNT,
    READ_WITHHOLDING_LIABILITY_ACCOUNT,
    READ_WITHHOLDING_INVOICE,
    READ_WITHHOLDING_CONFIGURED_OWNER,
    READ_WITHHOLDING_APPROVED_ASSESSMENT,
    READ_WITHHOLDING_CONSUMED_ASSESSMENT,
    READ_WITHHOLDING_PAYMENT_DOCUMENTS,
    READ_WITHHOLDING_WORKSPACE_IDENTITIES,
    READ_WITHHOLDING_REMIT_PACKAGE,
    READ_WITHHOLDING_REMIT_DOCUMENT,
    READ_WITHHOLDING_REMIT_JOURNAL_VALUE,
    READ_WITHHOLDING_REMIT_BANK_TXN,
    READ_WITHHOLDING_REMIT_PACKAGE_EDITION,
    READ_WITHHOLDING_REMIT_ROW,
    READ_WITHHOLDING_REMIT_PACKAGE_LIST,
    READ_WITHHOLDING_REMIT_BANK_CANDIDATES,
    READ_PLAN_ACCOUNTS,
    READ_PLAN_DIMENSION,
    READ_PLAN_BANK_ACCOUNT,
    READ_PLAN_EDITIONS,
    READ_PLAN_SNAPSHOTS,
    READ_PLAN_CATEGORY,
    READ_PLAN_EXPORT_ROW,
    READ_PLAN_SOURCE_SNAPSHOT,
    READ_CONTRACT_CUSTOMER,
    READ_CONTRACT_DIMENSIONS,
    READ_CONTRACT_DIMENSION_VALUE,
    READ_CONTRACT_TAX_CODE,
    READ_CONTRACT_BILLING_RUN,
    READ_CONTRACT_BILLING_EXISTS,
    READ_CONTRACT_BILL_SOURCE,
    READ_CONTRACT_POSTED_CORRECTION,
    READ_INVOICE_REPORT_CHARTS,
    READ_INVOICE_ACCOUNTS,
    READ_INVOICE_LINE_HISTORY,
    READ_INVOICE_THAI_DOCUMENTS,
    READ_INVOICE_PAYMENT_REVERSAL,
    READ_INVOICE_OPEN_PERIODS,
    READ_INVOICE_STATEMENT_ROWS,
    READ_INVOICE_OPEN_INVOICES,
    READ_INVOICE2_INVOICES,
    READ_INVOICE2_INVOICE_SNAPSHOT,
    READ_INVOICE2_INVOICE_ALLOCATIONS,
    READ_INVOICE2_PAYMENTS,
    READ_INVOICE2_PAYMENT_ALLOCATIONS,
    READ_INVOICE2_BANK_CANDIDATES,
    READ_INVOICE2_BANK_ACCOUNTS,
    READ_INVOICE2_BANK_STATEMENT,
    READ_AI_POLICY,
    READ_AI_PROPOSAL_EXACT,
    READ_AI_CLAIMABLE,
    READ_AI_EVENTS,
    READ_AI_DUE_POLICIES,
    READ_AI_BANK_CANDIDATES,
    READ_AI_REPORT_CHARTS,
    READ_AI_PROPOSAL_BY_SOURCE,
    READ_ASSET_POLICY_RULE,
    READ_ASSET_POLICY_MAPPINGS,
    READ_ASSET_POLICY_POSTED_RUNS,
    READ_ASSET_POLICY_LEGACY,
    READ_ASSET_POLICY_WORKSPACE_ASSETS,
    READ_ASSET_POLICY_WORKSPACE_BOOKS,
    READ_ASSET_POLICY_DISPOSAL,
    READ_ASSET_POLICY_GAIN_LOSS,
    READ_FIN_COMMAND_EXISTS,
    READ_FIN_CHART_ROW,
    READ_FIN_ACCOUNT_ROW,
    READ_FIN_ACCOUNT_BY_CODE,
    READ_FIN_JOURNAL_DEBIT,
    READ_FIN_JOURNAL_ROW,
    READ_FIN_JOURNAL_LINES,
    READ_FIN_OPEN_PERIOD,
];

/// Whether a discriminator is an owner read kind (never a write kind).
pub fn is_read(kind: &str) -> bool {
    read_statement(kind).is_some()
}

/// Resolve an owner read kind to its planned statement.
pub fn read_statement(kind: &str) -> Option<&'static NativeReadStatement> {
    READS.iter().find(|s| s.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_plan_is_holding_scoped_and_ledger_free() {
        for kind in [
            RD_ASSET_POLICY_ROW,
            RD_ASSET_POLICY_RUNS,
            RD_INSTRUMENT_EDITION,
            RD_INSTRUMENT_RUNS,
            RD_COLLECTION_CASE_ROW,
            RD_BUSINESS_BILL_ROW,
            RD_PERIOD_END_DATES,
            RD_ALL_PERIODS,
            RD_SUPPLIER_BATCH_ROW,
            RD_SUPPLIER_INSTRUCTION_ROW,
            RD_SUPPLIER_BATCH_LIST,
            RD_SUPPLIER_BATCH_WORK_LIST,
            RD_SUPPLIER_BATCH_ITEMS,
            RD_SUPPLIER_BATCH_INVOICES,
            RD_SUPPLIER_BATCH_INVOICE,
            RD_SUPPLIER_BATCH_ACCOUNTS,
            RD_SUPPLIER_BATCH_PERIODS,
            RD_SUPPLIER_BATCH_BANK,
            RD_TAX_PROFILE,
            RD_TAX_PACKAGE,
            RD_TAX_CORRECTION,
            RD_TAX_ENTRY,
            RD_TAX_PREVIOUS_ENTRY,
            RD_TAX_ORIGINAL_ENTRY,
            RD_TAX_PENDING_FAMILY,
            RD_TAX_PROFILES,
            RD_TAX_CHART,
            RD_TAX_CURRENT_ENTRY,
            RD_TAX_PENDING_CORRECTION_COUNT,
            RD_TAX_CLOSE_PROFILE,
            RD_BANK_IMPORTED_FILE,
            RD_TAX_IDENTITY_ACTIVE_PARTY,
            RD_TAX_IDENTITY_PREDECESSOR,
            RD_TAX_IDENTITY_ROW,
            RD_TAX_IDENTITY_REVIEWED,
            RD_TAX_IDENTITY_LIST,
            RD_TAX_IDENTITY_PARTY_LIST,
            RD_ASSET_ACCOUNT_ROWS,
            RD_ASSET_EVENT_LIST,
            RD_ASSET_DISPOSAL_LIST,
            RD_ASSET_REVIEWED_POLICY,
            RD_ASSET_DEPRECIATION_HISTORY,
            RD_ASSET_LATEST_DEPRECIATION,
            RD_ASSET_RETAINED_POLICY_HASH,
            RD_ASSET_DIMENSION_VALUES,
            RD_REPORT_REVISION,
            RD_REPORT_PROFILE,
            RD_REPORT_PACKAGE_LIST,
            RD_REPORT_CLOSING_LIST,
            RD_REPORT_ACCOUNT_LIST,
            RD_REPORT_STATEMENT_HISTORY,
            RD_REPORT_PREVIOUS_PACKAGE,
            RD_REPORT_NEXT_EDITION,
            RD_WITHHOLDING_BANK_ACCOUNT,
            RD_WITHHOLDING_LIABILITY_ACCOUNT,
            RD_WITHHOLDING_INVOICE,
            RD_WITHHOLDING_CONFIGURED_OWNER,
            RD_WITHHOLDING_APPROVED_ASSESSMENT,
            RD_WITHHOLDING_CONSUMED_ASSESSMENT,
            RD_WITHHOLDING_PAYMENT_DOCUMENTS,
            RD_WITHHOLDING_WORKSPACE_IDENTITIES,
            RD_WITHHOLDING_REMIT_PACKAGE,
            RD_WITHHOLDING_REMIT_DOCUMENT,
            RD_WITHHOLDING_REMIT_JOURNAL_VALUE,
            RD_WITHHOLDING_REMIT_BANK_TXN,
            RD_WITHHOLDING_REMIT_PACKAGE_EDITION,
            RD_WITHHOLDING_REMIT_ROW,
            RD_WITHHOLDING_REMIT_PACKAGE_LIST,
            RD_WITHHOLDING_REMIT_BANK_CANDIDATES,
            RD_PLAN_ACCOUNTS,
            RD_PLAN_DIMENSION,
            RD_PLAN_BANK_ACCOUNT,
            RD_PLAN_EDITIONS,
            RD_PLAN_SNAPSHOTS,
            RD_PLAN_CATEGORY,
            RD_PLAN_EXPORT_ROW,
            RD_PLAN_SOURCE_SNAPSHOT,
            RD_CONTRACT_CUSTOMER,
            RD_CONTRACT_DIMENSIONS,
            RD_CONTRACT_DIMENSION_VALUE,
            RD_CONTRACT_TAX_CODE,
            RD_CONTRACT_BILLING_RUN,
            RD_CONTRACT_BILLING_EXISTS,
            RD_CONTRACT_BILL_SOURCE,
            RD_CONTRACT_POSTED_CORRECTION,
            RD_INVOICE_REPORT_CHARTS,
            RD_INVOICE_ACCOUNTS,
            RD_INVOICE_LINE_HISTORY,
            RD_INVOICE_THAI_DOCUMENTS,
            RD_INVOICE_PAYMENT_REVERSAL,
            RD_INVOICE_OPEN_PERIODS,
            RD_INVOICE_STATEMENT_ROWS,
            RD_INVOICE_OPEN_INVOICES,
            RD_INVOICE2_INVOICES,
            RD_INVOICE2_INVOICE_SNAPSHOT,
            RD_INVOICE2_INVOICE_ALLOCATIONS,
            RD_INVOICE2_PAYMENTS,
            RD_INVOICE2_PAYMENT_ALLOCATIONS,
            RD_INVOICE2_BANK_CANDIDATES,
            RD_INVOICE2_BANK_ACCOUNTS,
            RD_INVOICE2_BANK_STATEMENT,
            RD_AI_POLICY,
            RD_AI_PROPOSAL_EXACT,
            RD_AI_CLAIMABLE,
            RD_AI_EVENTS,
            RD_AI_DUE_POLICIES,
            RD_AI_BANK_CANDIDATES,
            RD_AI_REPORT_CHARTS,
            RD_AI_PROPOSAL_BY_SOURCE,
            RD_ASSET_POLICY_RULE,
            RD_ASSET_POLICY_MAPPINGS,
            RD_ASSET_POLICY_POSTED_RUNS,
            RD_ASSET_POLICY_LEGACY,
            RD_ASSET_POLICY_WORKSPACE_ASSETS,
            RD_ASSET_POLICY_WORKSPACE_BOOKS,
            RD_ASSET_POLICY_DISPOSAL,
            RD_ASSET_POLICY_GAIN_LOSS,
            RD_FIN_COMMAND_EXISTS,
            RD_FIN_CHART_ROW,
            RD_FIN_ACCOUNT_ROW,
            RD_FIN_ACCOUNT_BY_CODE,
            RD_FIN_JOURNAL_DEBIT,
            RD_FIN_JOURNAL_ROW,
            RD_FIN_JOURNAL_LINES,
            RD_FIN_OPEN_PERIOD,
        ] {
            assert!(is_read(kind), "{kind}");
            let read = read_statement(kind).expect("read kind");
            let trimmed = read.sql.trim_start();
            assert!(
                trimmed.starts_with("SELECT") || trimmed.starts_with("WITH"),
                "{kind}"
            );
            assert!(!trimmed.starts_with("INSERT"), "{kind}");
            assert!(!trimmed.starts_with("UPDATE"), "{kind}");
            assert!(!trimmed.starts_with("DELETE"), "{kind}");
            assert_eq!(read.sql.matches('?').count(), read.bind_count, "{kind}");
            assert_eq!(read.scope, "holding", "{kind}");
        }
        assert_eq!(READS.len(), 125);
        assert!(!is_read("finance_stmt_1"));
        assert!(read_statement("finance_stmt_1").is_none());
    }

    /// Parity witness: each native plan is the exact statement the retired
    /// TypeScript literal bound, with the caller's unit binds intact.
    #[test]
    fn read_plans_match_retired_owner_statements() {
        let row = read_statement(RD_ASSET_POLICY_ROW).expect("read kind");
        assert_eq!(row.bind_count, 2);
        assert_eq!(row.sql, SQL_RD_ASSET_POLICY_ROW);
        assert!(row.sql.ends_with("WHERE company_id=? AND id=?"));

        let runs = read_statement(RD_ASSET_POLICY_RUNS).expect("read kind");
        assert_eq!(runs.bind_count, 2);
        assert_eq!(runs.sql, SQL_RD_ASSET_POLICY_RUNS);
        assert!(runs.sql.ends_with("WHERE company_id=? AND policy_id=? ORDER BY ordinal,id"));

        let edition = read_statement(RD_INSTRUMENT_EDITION).expect("read kind");
        assert_eq!(edition.bind_count, 2);
        assert_eq!(edition.sql, SQL_RD_INSTRUMENT_EDITION);
        assert!(edition.sql.ends_with("WHERE company_id=? AND id=?"));

        let instrument_runs = read_statement(RD_INSTRUMENT_RUNS).expect("read kind");
        assert_eq!(instrument_runs.bind_count, 2);
        assert_eq!(instrument_runs.sql, SQL_RD_INSTRUMENT_RUNS);
        assert!(instrument_runs.sql.ends_with("WHERE company_id=? AND instrument_id=? ORDER BY ordinal,id"));

        let case = read_statement(RD_COLLECTION_CASE_ROW).expect("read kind");
        assert_eq!(case.bind_count, 2);
        assert!(case.sql.starts_with("SELECT c.*,i.version current_invoice_version"));
        assert!(case.sql.contains(
            "CASE WHEN EXISTS(SELECT 1 FROM finance_collection_cases child WHERE child.parent_case_id=c.id) THEN 0 ELSE 1 END is_current"
        ));
        assert!(case.sql.ends_with("WHERE c.company_id=? AND c.id=?"));

        let bill = read_statement(RD_BUSINESS_BILL_ROW).expect("read kind");
        assert_eq!(bill.bind_count, 2);
        assert!(bill.sql.starts_with("SELECT b.*,COALESCE(b.contract_content_json,c.content_json) effective_contract_content_json"));
        assert!(bill.sql.contains(
            "COALESCE(i.status,b.no_charge_status) invoice_status"
        ));
        assert!(bill.sql.contains(
            "CASE WHEN EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=b.id) THEN 0 ELSE 1 END is_current"
        ));
        assert!(bill.sql.ends_with("WHERE b.id=? AND b.company_id=?"));

        let ends = read_statement(RD_PERIOD_END_DATES).expect("read kind");
        assert_eq!(ends.bind_count, 1);
        assert_eq!(
            ends.sql,
            "SELECT id,end_date FROM finance_periods WHERE company_id=? ORDER BY start_date DESC"
        );
        let periods = read_statement(RD_ALL_PERIODS).expect("read kind");
        assert_eq!(periods.bind_count, 1);
        assert_eq!(
            periods.sql,
            "SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC"
        );
    }

    /// The supplier-batch reads preserve the retired owner semantics: the
    /// purchase fence and bank reconciliation are bound, never inlined, and
    /// the work list drops the console row cap without weakening the Company.
    #[test]
    fn supplier_batch_read_plans_preserve_source_and_pay_fences() {
        for kind in [
            RD_SUPPLIER_BATCH_ROW,
            RD_SUPPLIER_INSTRUCTION_ROW,
            RD_SUPPLIER_BATCH_LIST,
            RD_SUPPLIER_BATCH_WORK_LIST,
            RD_SUPPLIER_BATCH_ITEMS,
            RD_SUPPLIER_BATCH_INVOICES,
            RD_SUPPLIER_BATCH_INVOICE,
            RD_SUPPLIER_BATCH_ACCOUNTS,
            RD_SUPPLIER_BATCH_PERIODS,
            RD_SUPPLIER_BATCH_BANK,
        ] {
            let read = read_statement(kind).expect("supplier read kind");
            assert_eq!(read.scope, "holding", "{kind}");
            assert_eq!(read.sql.matches('?').count(), read.bind_count, "{kind}");
        }
        // The instruction list is reached only after the batch was authorized
        // for the caller's Company; it is keyed by that exact batch id.
        assert!(read_statement(RD_SUPPLIER_BATCH_ITEMS)
            .expect("items")
            .sql
            .contains("WHERE s.batch_id=?"));
        for kind in [
            RD_SUPPLIER_BATCH_ROW,
            RD_SUPPLIER_INSTRUCTION_ROW,
            RD_SUPPLIER_BATCH_LIST,
            RD_SUPPLIER_BATCH_WORK_LIST,
            RD_SUPPLIER_BATCH_INVOICES,
            RD_SUPPLIER_BATCH_INVOICE,
            RD_SUPPLIER_BATCH_ACCOUNTS,
            RD_SUPPLIER_BATCH_PERIODS,
            RD_SUPPLIER_BATCH_BANK,
        ] {
            assert!(
                read_statement(kind).expect("supplier read kind").sql.contains("company_id"),
                "{kind}"
            );
        }

        let batch = read_statement(RD_SUPPLIER_BATCH_ROW).expect("row");
        assert_eq!(batch.bind_count, 2);
        assert!(batch.sql.ends_with("WHERE id=? AND company_id=?"));
        assert!(batch.sql.contains("\"approval_reason\""));

        let item = read_statement(RD_SUPPLIER_INSTRUCTION_ROW).expect("item");
        assert_eq!(item.bind_count, 2);
        assert!(item.sql.ends_with("WHERE i.id=? AND b.company_id=?"));

        let list = read_statement(RD_SUPPLIER_BATCH_LIST).expect("list");
        assert_eq!(list.bind_count, 2);
        assert!(list.sql.contains("owned_source"));
        assert!(list.sql.ends_with("ORDER BY b.updated_at DESC LIMIT 200"));

        let work = read_statement(RD_SUPPLIER_BATCH_WORK_LIST).expect("work");
        assert_eq!(work.bind_count, 2);
        assert_eq!(work.sql.replace(" LIMIT 200", ""), list.sql.replace(" LIMIT 200", ""));
        assert!(!work.sql.contains("LIMIT 200"));

        let invoices = read_statement(RD_SUPPLIER_BATCH_INVOICES).expect("invoices");
        assert_eq!(invoices.bind_count, 2);
        assert!(invoices.sql.contains("owner_invoice"));
        assert!(invoices.sql.contains("available_minor"));
        assert!(!invoices.sql.contains("i.id=?"));

        let invoice = read_statement(RD_SUPPLIER_BATCH_INVOICE).expect("invoice");
        assert_eq!(invoice.bind_count, 3);
        assert_eq!(
            invoice.sql.replace(" AND i.id=?", ""),
            invoices.sql
        );

        let accounts = read_statement(RD_SUPPLIER_BATCH_ACCOUNTS).expect("accounts");
        assert_eq!(accounts.bind_count, 1);
        assert!(accounts.sql.contains("cash_account_id"));

        let periods = read_statement(RD_SUPPLIER_BATCH_PERIODS).expect("periods");
        assert_eq!(periods.bind_count, 1);
        assert!(periods.sql.contains("state='open'"));

        let bank = read_statement(RD_SUPPLIER_BATCH_BANK).expect("bank");
        assert_eq!(bank.bind_count, 6);
        assert!(bank.sql.contains("t.amount_minor=-instruction.amount_minor"));
        assert!(bank.sql.contains("t.match_status IN ('unmatched','proposed')"));
        assert!(bank.sql.contains("NOT ?"));
    }

    /// Byte-for-byte witness against the retired TypeScript literals. The
    /// expected strings below are copied from `finance-tax-register.ts`; the
    /// owner plans must reproduce them exactly (no reformatting), with the
    /// caller's positional binds intact. Unknown kinds fail closed.
    #[test]
    fn tax_register_read_plans_are_byte_identical_to_retired_literals() {
        let expected: &[(&str, &str, usize)] = &[
            (
                RD_TAX_PROFILE,
                "SELECT * FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=? AND status='approved' AND effective_date<=?",
                3,
            ),
            (
                RD_TAX_PACKAGE,
                "SELECT * FROM finance_statutory_packages WHERE id=? AND company_id=? AND form_code='PP30' AND filing_month IS NOT NULL",
                2,
            ),
            (
                RD_TAX_CORRECTION,
                "SELECT * FROM finance_vat_corrections WHERE id=? AND company_id=?",
                2,
            ),
            (
                RD_TAX_ENTRY,
                "SELECT * FROM finance_tax_entries WHERE id=? AND company_id=?",
                2,
            ),
            (
                RD_TAX_PREVIOUS_ENTRY,
                "SELECT * FROM finance_current_tax_entries WHERE id=? AND company_id=? AND status='approved'",
                2,
            ),
            (
                RD_TAX_ORIGINAL_ENTRY,
                "SELECT * FROM finance_current_tax_entries WHERE company_id=? AND source_type='invoice' AND source_id=? AND status='approved'",
                2,
            ),
            (
                RD_TAX_PENDING_FAMILY,
                "SELECT 1 FROM finance_vat_corrections WHERE company_id=? AND family_source_id=? AND status='prepared'",
                2,
            ),
            (
                RD_TAX_PROFILES,
                "SELECT * FROM finance_accounting_profiles WHERE company_id=? AND status='approved'",
                1,
            ),
            (
                RD_TAX_CHART,
                "SELECT 1 FROM finance_charts WHERE id=? AND company_id=?",
                2,
            ),
            (
                RD_TAX_CURRENT_ENTRY,
                "SELECT id FROM finance_current_tax_entries WHERE company_id=? AND source_type=? AND source_id=? AND status='approved'",
                3,
            ),
            (
                RD_TAX_PENDING_CORRECTION_COUNT,
                "SELECT COUNT(*) FROM finance_vat_corrections c JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.company_id=? AND c.chart_id=? AND c.status='prepared' AND t.accounting_date<=?",
                3,
            ),
            (
                RD_TAX_CLOSE_PROFILE,
                "SELECT id,statutory_chart_id,effective_date FROM finance_accounting_profiles WHERE company_id=? AND status='approved' AND effective_date<=?",
                2,
            ),
        ];
        assert_eq!(expected.len(), 12);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered read"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(
                read.sql.matches('?').count(),
                *binds,
                "{kind}: positional binds"
            );
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
        }
        assert!(read_statement("finance_rd_tax_unknown").is_none());
        assert!(!is_read("SELECT * FROM finance_current_tax_entries"));
        assert!(read_statement("").is_none());
    }

    /// The bank imported-file probe is byte-identical to the retired TS literal
    /// and stays a read-only point lookup on the caller's unit.
    #[test]
    fn bank_imported_file_read_matches_retired_owner_statement() {
        let read = read_statement(RD_BANK_IMPORTED_FILE).expect("registered read");
        assert_eq!(read.kind, "finance_rd_bank_imported_file");
        assert_eq!(read.bind_count, 3);
        assert_eq!(read.scope, "holding");
        assert_eq!(
            read.sql,
            "SELECT s.statement_number FROM finance_bank_import_files f JOIN finance_bank_statements s ON s.id=f.statement_id AND s.company_id=f.company_id WHERE f.company_id=? AND f.bank_account_id=? AND f.file_hash=?"
        );
        assert!(!read.sql.contains("FOR UPDATE"));
        assert!(!read.sql.contains("INSERT"));
        assert!(!read.sql.contains("UPDATE"));
        assert!(is_read(RD_BANK_IMPORTED_FILE));
    }

    /// The tax-identity family plans the exact retired literals, with the
    /// caller's positional binds intact and no write or lock text.
    #[test]
    fn tax_identity_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (
                RD_TAX_IDENTITY_ACTIVE_PARTY,
                SQL_RD_TAX_IDENTITY_ACTIVE_PARTY,
                2usize,
            ),
            (
                RD_TAX_IDENTITY_PREDECESSOR,
                SQL_RD_TAX_IDENTITY_PREDECESSOR,
                3,
            ),
            (RD_TAX_IDENTITY_ROW, SQL_RD_TAX_IDENTITY_ROW, 2),
            (
                RD_TAX_IDENTITY_REVIEWED,
                SQL_RD_TAX_IDENTITY_REVIEWED,
                6,
            ),
            (RD_TAX_IDENTITY_LIST, SQL_RD_TAX_IDENTITY_LIST, 1),
            (
                RD_TAX_IDENTITY_PARTY_LIST,
                SQL_RD_TAX_IDENTITY_PARTY_LIST,
                1,
            ),
        ];
        assert_eq!(expected.len(), 6);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
            assert!(!read.sql.contains("UPDATE"), "{kind}");
        }
    }

    /// The finance-assets point/list family plans the exact retired literals;
    /// no lock or assertion text crosses the read-plan boundary.
    #[test]
    fn asset_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (RD_ASSET_ACCOUNT_ROWS, SQL_RD_ASSET_ACCOUNT_ROWS, 4),
            (RD_ASSET_EVENT_LIST, SQL_RD_ASSET_EVENT_LIST, 1),
            (RD_ASSET_DISPOSAL_LIST, SQL_RD_ASSET_DISPOSAL_LIST, 1),
            (RD_ASSET_REVIEWED_POLICY, SQL_RD_ASSET_REVIEWED_POLICY, 1),
            (
                RD_ASSET_DEPRECIATION_HISTORY,
                SQL_RD_ASSET_DEPRECIATION_HISTORY,
                1,
            ),
            (
                RD_ASSET_LATEST_DEPRECIATION,
                SQL_RD_ASSET_LATEST_DEPRECIATION,
                1,
            ),
            (
                RD_ASSET_RETAINED_POLICY_HASH,
                SQL_RD_ASSET_RETAINED_POLICY_HASH,
                1,
            ),
            (
                RD_ASSET_DIMENSION_VALUES,
                SQL_RD_ASSET_DIMENSION_VALUES,
                1,
            ),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
            assert!(!read.sql.contains("UPDATE"), "{kind}");
        }
    }

    /// The finance-reporting point/list family plans the exact retired
    /// literals; no lock or assertion text crosses the read-plan boundary.
    #[test]
    fn report_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (RD_REPORT_REVISION, SQL_RD_REPORT_REVISION, 1),
            (RD_REPORT_PROFILE, SQL_RD_REPORT_PROFILE, 3),
            (RD_REPORT_PACKAGE_LIST, SQL_RD_REPORT_PACKAGE_LIST, 2),
            (RD_REPORT_CLOSING_LIST, SQL_RD_REPORT_CLOSING_LIST, 2),
            (RD_REPORT_ACCOUNT_LIST, SQL_RD_REPORT_ACCOUNT_LIST, 2),
            (
                RD_REPORT_STATEMENT_HISTORY,
                SQL_RD_REPORT_STATEMENT_HISTORY,
                1,
            ),
            (
                RD_REPORT_PREVIOUS_PACKAGE,
                SQL_RD_REPORT_PREVIOUS_PACKAGE,
                5,
            ),
            (RD_REPORT_NEXT_EDITION, SQL_RD_REPORT_NEXT_EDITION, 3),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
            assert!(!read.sql.contains("UPDATE"), "{kind}");
        }
    }

    /// The finance-withholding point/list family plans the exact retired
    /// literals; assertion guards stay in the caller's command transaction.
    #[test]
    fn withholding_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (
                RD_WITHHOLDING_BANK_ACCOUNT,
                SQL_RD_WITHHOLDING_BANK_ACCOUNT,
                2,
            ),
            (
                RD_WITHHOLDING_LIABILITY_ACCOUNT,
                SQL_RD_WITHHOLDING_LIABILITY_ACCOUNT,
                2,
            ),
            (RD_WITHHOLDING_INVOICE, SQL_RD_WITHHOLDING_INVOICE, 2),
            (
                RD_WITHHOLDING_CONFIGURED_OWNER,
                SQL_RD_WITHHOLDING_CONFIGURED_OWNER,
                3,
            ),
            (
                RD_WITHHOLDING_APPROVED_ASSESSMENT,
                SQL_RD_WITHHOLDING_APPROVED_ASSESSMENT,
                3,
            ),
            (
                RD_WITHHOLDING_CONSUMED_ASSESSMENT,
                SQL_RD_WITHHOLDING_CONSUMED_ASSESSMENT,
                2,
            ),
            (
                RD_WITHHOLDING_PAYMENT_DOCUMENTS,
                SQL_RD_WITHHOLDING_PAYMENT_DOCUMENTS,
                2,
            ),
            (
                RD_WITHHOLDING_WORKSPACE_IDENTITIES,
                SQL_RD_WITHHOLDING_WORKSPACE_IDENTITIES,
                1,
            ),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
        }
    }

    /// The finance-withholding-remittance point/list family plans the exact
    /// retired literals; assertions and row locks remain in caller units.
    #[test]
    fn withholding_remit_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (
                RD_WITHHOLDING_REMIT_PACKAGE,
                SQL_RD_WITHHOLDING_REMIT_PACKAGE,
                2,
            ),
            (
                RD_WITHHOLDING_REMIT_DOCUMENT,
                SQL_RD_WITHHOLDING_REMIT_DOCUMENT,
                2,
            ),
            (
                RD_WITHHOLDING_REMIT_JOURNAL_VALUE,
                SQL_RD_WITHHOLDING_REMIT_JOURNAL_VALUE,
                2,
            ),
            (
                RD_WITHHOLDING_REMIT_BANK_TXN,
                SQL_RD_WITHHOLDING_REMIT_BANK_TXN,
                2,
            ),
            (
                RD_WITHHOLDING_REMIT_PACKAGE_EDITION,
                SQL_RD_WITHHOLDING_REMIT_PACKAGE_EDITION,
                3,
            ),
            (RD_WITHHOLDING_REMIT_ROW, SQL_RD_WITHHOLDING_REMIT_ROW, 2),
            (
                RD_WITHHOLDING_REMIT_PACKAGE_LIST,
                SQL_RD_WITHHOLDING_REMIT_PACKAGE_LIST,
                1,
            ),
            (
                RD_WITHHOLDING_REMIT_BANK_CANDIDATES,
                SQL_RD_WITHHOLDING_REMIT_BANK_CANDIDATES,
                1,
            ),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
        }
    }

    /// The finance-planning point/list family plans the exact retired
    /// literals; ledger and advisory-lock statements stay in caller units.
    #[test]
    fn planning_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (RD_PLAN_ACCOUNTS, SQL_RD_PLAN_ACCOUNTS, 2),
            (RD_PLAN_DIMENSION, SQL_RD_PLAN_DIMENSION, 3),
            (RD_PLAN_BANK_ACCOUNT, SQL_RD_PLAN_BANK_ACCOUNT, 2),
            (RD_PLAN_EDITIONS, SQL_RD_PLAN_EDITIONS, 2),
            (RD_PLAN_SNAPSHOTS, SQL_RD_PLAN_SNAPSHOTS, 2),
            (RD_PLAN_CATEGORY, SQL_RD_PLAN_CATEGORY, 1),
            (RD_PLAN_EXPORT_ROW, SQL_RD_PLAN_EXPORT_ROW, 2),
            (
                RD_PLAN_SOURCE_SNAPSHOT,
                SQL_RD_PLAN_SOURCE_SNAPSHOT,
                3,
            ),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
        }
    }

    /// The finance-business-contracts point/list family plans the exact
    /// retired literals; command guards stay in caller transactions.
    #[test]
    fn contract_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (RD_CONTRACT_CUSTOMER, SQL_RD_CONTRACT_CUSTOMER, 3),
            (RD_CONTRACT_DIMENSIONS, SQL_RD_CONTRACT_DIMENSIONS, 1),
            (
                RD_CONTRACT_DIMENSION_VALUE,
                SQL_RD_CONTRACT_DIMENSION_VALUE,
                3,
            ),
            (RD_CONTRACT_TAX_CODE, SQL_RD_CONTRACT_TAX_CODE, 5),
            (RD_CONTRACT_BILLING_RUN, SQL_RD_CONTRACT_BILLING_RUN, 3),
            (
                RD_CONTRACT_BILLING_EXISTS,
                SQL_RD_CONTRACT_BILLING_EXISTS,
                2,
            ),
            (RD_CONTRACT_BILL_SOURCE, SQL_RD_CONTRACT_BILL_SOURCE, 2),
            (
                RD_CONTRACT_POSTED_CORRECTION,
                SQL_RD_CONTRACT_POSTED_CORRECTION,
                1,
            ),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
        }
    }

    /// The finance-invoice-read point/list family plans the exact retired
    /// literals; field-invoice ownership helpers and locks stay in TS.
    #[test]
    fn invoice_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (RD_INVOICE_REPORT_CHARTS, SQL_RD_INVOICE_REPORT_CHARTS, 1),
            (RD_INVOICE_ACCOUNTS, SQL_RD_INVOICE_ACCOUNTS, 2),
            (RD_INVOICE_LINE_HISTORY, SQL_RD_INVOICE_LINE_HISTORY, 2),
            (
                RD_INVOICE_THAI_DOCUMENTS,
                SQL_RD_INVOICE_THAI_DOCUMENTS,
                2,
            ),
            (
                RD_INVOICE_PAYMENT_REVERSAL,
                SQL_RD_INVOICE_PAYMENT_REVERSAL,
                1,
            ),
            (RD_INVOICE_OPEN_PERIODS, SQL_RD_INVOICE_OPEN_PERIODS, 1),
            (
                RD_INVOICE_STATEMENT_ROWS,
                SQL_RD_INVOICE_STATEMENT_ROWS,
                3,
            ),
            (
                RD_INVOICE_OPEN_INVOICES,
                SQL_RD_INVOICE_OPEN_INVOICES,
                6,
            ),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
        }
    }

    /// The second finance-invoice-read point/list family plans the exact
    /// retired literals; ownership helpers and guards stay in caller units.
    #[test]
    fn invoice_reads_second_family_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (RD_INVOICE2_INVOICES, SQL_RD_INVOICE2_INVOICES, 2),
            (
                RD_INVOICE2_INVOICE_SNAPSHOT,
                SQL_RD_INVOICE2_INVOICE_SNAPSHOT,
                2,
            ),
            (
                RD_INVOICE2_INVOICE_ALLOCATIONS,
                SQL_RD_INVOICE2_INVOICE_ALLOCATIONS,
                2,
            ),
            (RD_INVOICE2_PAYMENTS, SQL_RD_INVOICE2_PAYMENTS, 2),
            (
                RD_INVOICE2_PAYMENT_ALLOCATIONS,
                SQL_RD_INVOICE2_PAYMENT_ALLOCATIONS,
                2,
            ),
            (
                RD_INVOICE2_BANK_CANDIDATES,
                SQL_RD_INVOICE2_BANK_CANDIDATES,
                5,
            ),
            (
                RD_INVOICE2_BANK_ACCOUNTS,
                SQL_RD_INVOICE2_BANK_ACCOUNTS,
                2,
            ),
            (
                RD_INVOICE2_BANK_STATEMENT,
                SQL_RD_INVOICE2_BANK_STATEMENT,
                2,
            ),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
        }
    }

    /// The finance-ai point/list family plans the exact retired literals;
    /// owner-write statements and assertion guards stay in caller units.
    #[test]
    fn ai_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (RD_AI_POLICY, SQL_RD_AI_POLICY, 1),
            (RD_AI_PROPOSAL_EXACT, SQL_RD_AI_PROPOSAL_EXACT, 6),
            (RD_AI_CLAIMABLE, SQL_RD_AI_CLAIMABLE, 1),
            (RD_AI_EVENTS, SQL_RD_AI_EVENTS, 1),
            (RD_AI_DUE_POLICIES, SQL_RD_AI_DUE_POLICIES, 1),
            (RD_AI_BANK_CANDIDATES, SQL_RD_AI_BANK_CANDIDATES, 2),
            (RD_AI_REPORT_CHARTS, SQL_RD_AI_REPORT_CHARTS, 3),
            (
                RD_AI_PROPOSAL_BY_SOURCE,
                SQL_RD_AI_PROPOSAL_BY_SOURCE,
                8,
            ),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
        }
    }

    /// The finance-asset-policies point/list family plans the exact retired
    /// literals; two plans carry nested SELECT 1 subqueries.
    #[test]
    fn asset_policy_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (RD_ASSET_POLICY_RULE, SQL_RD_ASSET_POLICY_RULE, 3),
            (RD_ASSET_POLICY_MAPPINGS, SQL_RD_ASSET_POLICY_MAPPINGS, 4),
            (
                RD_ASSET_POLICY_POSTED_RUNS,
                SQL_RD_ASSET_POLICY_POSTED_RUNS,
                1,
            ),
            (RD_ASSET_POLICY_LEGACY, SQL_RD_ASSET_POLICY_LEGACY, 2),
            (
                RD_ASSET_POLICY_WORKSPACE_ASSETS,
                SQL_RD_ASSET_POLICY_WORKSPACE_ASSETS,
                1,
            ),
            (
                RD_ASSET_POLICY_WORKSPACE_BOOKS,
                SQL_RD_ASSET_POLICY_WORKSPACE_BOOKS,
                1,
            ),
            (RD_ASSET_POLICY_DISPOSAL, SQL_RD_ASSET_POLICY_DISPOSAL, 2),
            (
                RD_ASSET_POLICY_GAIN_LOSS,
                SQL_RD_ASSET_POLICY_GAIN_LOSS,
                3,
            ),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
        }
    }

    /// The `finance` point/list family plans the exact retired literals;
    /// command guards, locks and owner writes stay in caller units.
    #[test]
    fn finance_reads_match_retired_owner_statements() {
        let expected: &[(&str, &str, usize)] = &[
            (RD_FIN_COMMAND_EXISTS, SQL_RD_FIN_COMMAND_EXISTS, 1),
            (RD_FIN_CHART_ROW, SQL_RD_FIN_CHART_ROW, 2),
            (RD_FIN_ACCOUNT_ROW, SQL_RD_FIN_ACCOUNT_ROW, 3),
            (RD_FIN_ACCOUNT_BY_CODE, SQL_RD_FIN_ACCOUNT_BY_CODE, 3),
            (RD_FIN_JOURNAL_DEBIT, SQL_RD_FIN_JOURNAL_DEBIT, 1),
            (RD_FIN_JOURNAL_ROW, SQL_RD_FIN_JOURNAL_ROW, 2),
            (RD_FIN_JOURNAL_LINES, SQL_RD_FIN_JOURNAL_LINES, 1),
            (RD_FIN_OPEN_PERIOD, SQL_RD_FIN_OPEN_PERIOD, 2),
        ];
        assert_eq!(expected.len(), 8);
        for (kind, sql, binds) in expected {
            let read = read_statement(kind).unwrap_or_else(|| panic!("{kind}: unregistered"));
            assert_eq!(read.kind, *kind, "{kind}: discriminator");
            assert_eq!(read.sql, *sql, "{kind}: byte-identical SQL");
            assert_eq!(read.bind_count, *binds, "{kind}: bind count");
            assert_eq!(read.sql.matches('?').count(), *binds, "{kind}: binds");
            assert_eq!(read.scope, "holding", "{kind}: scope");
            assert!(is_read(kind), "{kind}");
            assert!(!read.sql.contains("FOR UPDATE"), "{kind}");
            assert!(!read.sql.contains("INSERT"), "{kind}");
        }
    }
}
