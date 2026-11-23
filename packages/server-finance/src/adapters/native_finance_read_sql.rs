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

const READS: &[NativeReadStatement] = &[
    READ_ASSET_POLICY_ROW,
    READ_ASSET_POLICY_RUNS,
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
        assert_eq!(READS.len(), 16);
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
}
