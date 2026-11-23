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

const READS: &[NativeReadStatement] = &[
    READ_ASSET_POLICY_ROW,
    READ_ASSET_POLICY_RUNS,
    READ_COLLECTION_CASE_ROW,
    READ_BUSINESS_BILL_ROW,
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
        assert_eq!(READS.len(), 4);
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
    }
}
