//! Owner-owned caller-unit read plans for Commerce. Generated from the retired
//! TypeScript SELECT literals with the exact positional payloads; the mounted
//! `commerce/command-plan` route resolves the allow-listed kind here and returns
//! the statement for the caller to prepare and execute on its own open unit.
//!
//! This family covers the small owner-local Commerce setup-existence reads on
//! `commerce_warehouses` and `commerce_settings`. Reads carry no receipt/audit/
//! outbox ledger: the caller executes them on the same unit as the surrounding
//! command. No SQL is ever accepted from the wire and no TypeScript SQL remains
//! for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the number
/// of positional arguments the caller must send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCommerceReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `pmsRolloutCommerceFacts.inventory`
/// active-warehouse existence read. The caller binds `[company_id]`.
pub const RD_PMS_WAREHOUSE_ACTIVE: &str = "commerce_rd_pms_warehouse_active";
const SQL_PMS_WAREHOUSE_ACTIVE: &str =
    "SELECT 1 FROM commerce_warehouses WHERE company_id=? AND status='active' LIMIT 1";
const READ_PMS_WAREHOUSE_ACTIVE: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PMS_WAREHOUSE_ACTIVE,
    sql: SQL_PMS_WAREHOUSE_ACTIVE,
    bind_count: 1,
};

/// Native successor of the TypeScript `pmsRolloutCommerceFacts.commerce`
/// settings existence read. The caller binds `[company_id]`.
pub const RD_PMS_SETTINGS_EXISTS: &str = "commerce_rd_pms_settings_exists";
const SQL_PMS_SETTINGS_EXISTS: &str = "SELECT 1 FROM commerce_settings WHERE company_id=?";
const READ_PMS_SETTINGS_EXISTS: NativeCommerceReadStatement = NativeCommerceReadStatement {
    kind: RD_PMS_SETTINGS_EXISTS,
    sql: SQL_PMS_SETTINGS_EXISTS,
    bind_count: 1,
};

const STATEMENTS: &[NativeCommerceReadStatement] = &[
    READ_PMS_WAREHOUSE_ACTIVE,
    READ_PMS_SETTINGS_EXISTS,
];

/// Resolve an allow-listed owner read kind. Any unregistered kind fails closed.
pub fn read_statement(kind: &str) -> Option<&'static NativeCommerceReadStatement> {
    STATEMENTS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_existence_reads_are_owner_authored_select_only_and_single_bound() {
        let expected = [
            (RD_PMS_WAREHOUSE_ACTIVE, SQL_PMS_WAREHOUSE_ACTIVE),
            (RD_PMS_SETTINGS_EXISTS, SQL_PMS_SETTINGS_EXISTS),
        ];
        for (kind, sql) in expected {
            let statement = read_statement(kind).expect("registered owner read kind");
            assert_eq!(statement.kind, kind);
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, 1);
            assert!(sql.starts_with("SELECT "), "owner read must be SELECT-only: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!sql.contains(forbidden), "owner read must not mutate: {forbidden}");
            }
            assert!(
                sql.contains("commerce_"),
                "owner read must stay on owner-local Commerce tables: {sql}"
            );
            assert_eq!(sql.matches('?').count(), 1);
        }
        assert_eq!(STATEMENTS.len(), 2);
    }

    #[test]
    fn unknown_and_mismatched_read_kinds_fail_closed() {
        assert!(read_statement("SELECT 1 FROM commerce_settings").is_none());
        assert!(read_statement("").is_none());
        assert!(read_statement("commerce_rd_unknown").is_none());
        let statement = read_statement(RD_PMS_WAREHOUSE_ACTIVE).expect("warehouse read");
        assert!(statement.bind_count != 2);
    }
}
