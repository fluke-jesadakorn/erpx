//! Owner-owned caller-unit read plans for People Payroll. Generated from the
//! retired TypeScript SELECT literal with its exact positional payload; the
//! axum `command-plan` route resolves the kind here and returns the statement
//! for the caller to prepare and execute on its own already-open unit.
//!
//! Reads carry no receipt/audit/outbox ledger: the caller runs them on the same
//! unit as the surrounding work, so they observe uncommitted effects. No SQL is
//! ever accepted from the wire and no TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the
/// positional argument count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `records-record-details-owned-facts`
/// posted-payroll-run supplement rows read. The owner plans this literal; the
/// caller executes it on its open unit with the same
/// `[companyId, payMonth, currency, offset]` binds.
pub const RD_RECORD_SUPPLEMENT_ROWS: &str = "people_payroll_rd_record_supplement_rows";
const SQL_RECORD_SUPPLEMENT_ROWS: &str = "SELECT r.id,r.period_start,r.period_end,r.pay_date,r.currency,r.status,COUNT(i.id) headcount,SUM(i.gross_minor) gross_minor,SUM(i.deductions_minor) deductions_minor,SUM(i.net_minor) net_minor FROM hr_payroll_runs r JOIN hr_payroll_items i ON i.run_id=r.id WHERE r.company_id=? AND r.status='posted' AND substring(r.pay_date FROM 1 FOR 7)=? AND r.currency=? GROUP BY r.id ORDER BY r.pay_date DESC,r.id LIMIT 31 OFFSET ?";
const READ_RECORD_SUPPLEMENT_ROWS: NativeReadStatement = NativeReadStatement {
    kind: RD_RECORD_SUPPLEMENT_ROWS,
    sql: SQL_RECORD_SUPPLEMENT_ROWS,
    bind_count: 4,
};

const READS: &[NativeReadStatement] = &[READ_RECORD_SUPPLEMENT_ROWS];

/// Whether a kind is an allow-listed owner read (never a write command).
pub fn is_read(kind: &str) -> bool {
    read_statement(kind).is_some()
}

/// Resolve an owner read kind to its planned statement.
pub fn read_statement(kind: &str) -> Option<&'static NativeReadStatement> {
    READS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_plan_is_ledger_free_and_bounded() {
        let read = read_statement(RD_RECORD_SUPPLEMENT_ROWS).expect("read kind");
        assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
        assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
        assert_eq!(read.bind_count, 4);
        assert_eq!(read.sql.matches('?').count(), 4);
        assert!(is_read(RD_RECORD_SUPPLEMENT_ROWS));
        assert!(!is_read("people_payroll_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }
}
