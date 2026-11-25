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
        assert_eq!(READS.len(), 37);
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
        let expected = [
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
}
