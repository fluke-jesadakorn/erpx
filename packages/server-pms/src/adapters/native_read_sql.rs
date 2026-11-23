//! Owner-owned caller-unit read plans for PMS. Generated from the retired
//! TypeScript SELECT literal with its exact positional payload; the mounted
//! axum `pms/command-plan` route resolves the kind here and returns the
//! statement for the caller to prepare and run on its own already-open unit.
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

/// Native successor of the TypeScript `readPmsInventoryCommand` read. The owner
/// plans this literal; the caller executes it on its open unit with the same
/// `[propertyId, propertyId]` binds.
pub const PMS_RD_INVENTORY_COMMAND: &str = "pms_rd_inventory_command";
/// Native successors of the owner-local booking-change idempotency reads. They
/// carry no receipt/audit/outbox ledger and touch no refund or lock path; the
/// caller runs them on its already-open booking transaction.
pub const PMS_RD_BOOKING_CHANGE: &str = "pms_rd_booking_change";
pub const PMS_RD_BOOKING_CHANGE_PENDING: &str = "pms_rd_booking_change_pending";
pub const PMS_RD_BOOKING_PAYMENT: &str = "pms_rd_booking_payment";
/// Native successors of the owner-local Finance posting journal lookups. Fixed
/// SQL with positional binds, no ledger/receipt/outbox and no lock/refund path;
/// the caller runs them on its already-open unit.
pub const PMS_RD_POSTING_CHARGE: &str = "pms_rd_posting_charge";
pub const PMS_RD_POSTING_BY_RECORD: &str = "pms_rd_posting_by_record";
const SQL_INVENTORY_COMMAND: &str = "SELECT id,kind,data_json,version,updated_at FROM pms_records \
WHERE property_id=? AND kind IN ('room_type','room','rate','block','item','credit') \
UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records \
WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house') \
ORDER BY id";
const READ_INVENTORY_COMMAND: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_INVENTORY_COMMAND,
    sql: SQL_INVENTORY_COMMAND,
    bind_count: 2,
};

/// `SELECT request_hash,state FROM pms_booking_changes WHERE id=? AND reservation_id=?`
const SQL_BOOKING_CHANGE: &str = "SELECT request_hash,state FROM pms_booking_changes WHERE id=? AND reservation_id=?";
const READ_BOOKING_CHANGE: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_CHANGE,
    sql: SQL_BOOKING_CHANGE,
    bind_count: 2,
};
/// `SELECT id FROM pms_booking_changes WHERE reservation_id=? AND state='payment_pending' AND expires_at>? LIMIT 1`
const SQL_BOOKING_CHANGE_PENDING: &str = "SELECT id FROM pms_booking_changes WHERE reservation_id=? AND state='payment_pending' AND expires_at>? LIMIT 1";
const READ_BOOKING_CHANGE_PENDING: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_CHANGE_PENDING,
    sql: SQL_BOOKING_CHANGE_PENDING,
    bind_count: 2,
};
/// `SELECT id FROM pms_booking_payments WHERE proposal_id=?`
const SQL_BOOKING_PAYMENT: &str = "SELECT id FROM pms_booking_payments WHERE proposal_id=?";
const READ_BOOKING_PAYMENT: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_PAYMENT,
    sql: SQL_BOOKING_PAYMENT,
    bind_count: 1,
};

/// `SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='entry' AND data_json::jsonb->>'guestAccountId'=?) AND source_key LIKE '%:charge' ORDER BY business_date,source_key LIMIT 1`
const SQL_POSTING_CHARGE: &str = "SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='entry' AND data_json::jsonb->>'guestAccountId'=?) AND source_key LIKE '%:charge' ORDER BY business_date,source_key LIMIT 1";
const READ_POSTING_CHARGE: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_POSTING_CHARGE,
    sql: SQL_POSTING_CHARGE,
    bind_count: 3,
};
/// `SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id=? ORDER BY source_key LIMIT 1`
const SQL_POSTING_BY_RECORD: &str = "SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id=? ORDER BY source_key LIMIT 1";
const READ_POSTING_BY_RECORD: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_POSTING_BY_RECORD,
    sql: SQL_POSTING_BY_RECORD,
    bind_count: 2,
};

const READS: &[NativeReadStatement] = &[READ_INVENTORY_COMMAND, READ_BOOKING_CHANGE, READ_BOOKING_CHANGE_PENDING, READ_BOOKING_PAYMENT, READ_POSTING_CHARGE, READ_POSTING_BY_RECORD];

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
        let read = read_statement(PMS_RD_INVENTORY_COMMAND).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"));
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE FROM"));
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert!(is_read(PMS_RD_INVENTORY_COMMAND));
        assert!(!is_read("pms_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }

    #[test]
    fn read_sql_matches_retired_typescript_literal() {
        let read = read_statement(PMS_RD_INVENTORY_COMMAND).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT id,kind,data_json,version,updated_at FROM pms_records \
WHERE property_id=? AND kind IN ('room_type','room','rate','block','item','credit') \
UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records \
WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house') \
ORDER BY id"
        );
    }

    #[test]
    fn booking_change_reads_are_fixed_ledger_free_selects() {
        let change = read_statement(PMS_RD_BOOKING_CHANGE).expect("booking change read");
        let pending = read_statement(PMS_RD_BOOKING_CHANGE_PENDING).expect("pending change read");
        let payment = read_statement(PMS_RD_BOOKING_PAYMENT).expect("booking payment read");
        assert_eq!(change.sql, "SELECT request_hash,state FROM pms_booking_changes WHERE id=? AND reservation_id=?");
        assert_eq!(pending.sql, "SELECT id FROM pms_booking_changes WHERE reservation_id=? AND state='payment_pending' AND expires_at>? LIMIT 1");
        assert_eq!(payment.sql, "SELECT id FROM pms_booking_payments WHERE proposal_id=?");
        for read in [change, pending, payment] {
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            assert!(is_read(read.kind));
        }
        assert_eq!(change.bind_count, 2);
        assert_eq!(pending.bind_count, 2);
        assert_eq!(payment.bind_count, 1);
    }

    #[test]
    fn posting_journal_reads_are_fixed_ledger_free_selects() {
        let charge = read_statement(PMS_RD_POSTING_CHARGE).expect("posting charge read");
        let by_record = read_statement(PMS_RD_POSTING_BY_RECORD).expect("posting by record read");
        assert_eq!(charge.sql, "SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='entry' AND data_json::jsonb->>'guestAccountId'=?) AND source_key LIKE '%:charge' ORDER BY business_date,source_key LIMIT 1");
        assert_eq!(by_record.sql, "SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id=? ORDER BY source_key LIMIT 1");
        for read in [charge, by_record] {
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            assert!(is_read(read.kind));
        }
        assert_eq!(charge.bind_count, 3);
        assert_eq!(by_record.bind_count, 2);
    }
}
