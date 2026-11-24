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
/// Native successors of the owner-local pending charge-correction lookups. Fixed
/// SQL with positional binds, no ledger/receipt/outbox and no lock/refund path;
/// the caller runs them on its already-open unit.
pub const PMS_RD_CHARGE_CORRECTION_PENDING: &str = "pms_rd_charge_correction_pending";
pub const PMS_RD_CHARGE_CORRECTION_PENDING_COUNT: &str = "pms_rd_charge_correction_pending_count";
/// Native successor of the owner-local `readPmsGuestCommandRecord` fixed SQL.
/// No ledger/receipt/outbox and no lock/refund path; the caller runs it on its
/// already-open guest command unit with the same `[propertyId, guestId]` binds.
pub const PMS_RD_GUEST_COMMAND_RECORD: &str = "pms_rd_guest_command_record";
/// Native successors of the owner-local `pms_documents` point reads by id. Fixed
/// SQL with one positional bind, no ledger/receipt/outbox and no lock/refund
/// path; the caller runs them on its already-open unit.
pub const PMS_RD_DOCUMENT_BY_ID: &str = "pms_rd_document_by_id";
pub const PMS_RD_DOCUMENT_SNAPSHOT: &str = "pms_rd_document_snapshot";
/// Native successor of the owner-local hospitality day-control `pms_properties`
/// config point read. Fixed SQL with two positional binds, no ledger/receipt/outbox
/// and no lock/refund path; the caller runs it on its already-open unit.
pub const PMS_RD_PROPERTY_CONFIG: &str = "pms_rd_property_config";
/// Native successors of the owner-local hospitality day-control count reads for
/// pending hotel transfers and unposted booking receipts. Fixed SQL with no
/// ledger/receipt/outbox and no lock/refund path; the caller runs them on its
/// already-open unit.
pub const PMS_RD_PENDING_HOTEL_TRANSFERS: &str = "pms_rd_pending_hotel_transfers";
pub const PMS_RD_UNPOSTED_BOOKING_RECEIPTS: &str = "pms_rd_unposted_booking_receipts";
/// Native successor of the owner-local hospitality day-control unposted counter
/// receipt count read. Fixed SQL with four positional binds, no
/// ledger/receipt/outbox and no lock/refund path; the caller runs it on its
/// already-open unit.
pub const PMS_RD_UNPOSTED_COUNTER_RECEIPTS: &str = "pms_rd_unposted_counter_receipts";
/// Native successor of the owner-local hospitality-setup property list read.
/// Fixed SQL with two positional binds, no ledger/receipt/outbox and no
/// lock/refund path; the caller runs it on its already-open unit.
pub const PMS_RD_HOSPITALITY_SETUP_PROPERTIES: &str = "pms_rd_hospitality_setup_properties";
/// Native successors of the owner-local attachment read-source image reads.
/// Fixed SQL with two positional binds each, no ledger/receipt/outbox and no
/// lock/refund path; the caller runs them on its already-open read unit. The
/// separate `LOCK TABLE` lease stays on the caller's connection.
pub const PMS_RD_ATTACHMENT_PROPERTY_IMAGE: &str = "pms_rd_attachment_property_image";
pub const PMS_RD_ATTACHMENT_RECORD_IMAGE: &str = "pms_rd_attachment_record_image";
/// Native successor of the owner-local `pmsDocumentQueries.relatedPostings`
/// fixed SQL. Fixed SQL with two positional binds, no ledger/receipt/outbox and
/// no lock/refund path; the caller runs it on its already-open read unit.
pub const PMS_RD_DOCUMENT_RELATED_POSTINGS: &str = "pms_rd_document_related_postings";
/// Native successors of the owner-local `posGuestAccountInterface` hotel and
/// choice reads. Fixed SQL with three and four positional binds respectively,
/// no ledger/receipt/outbox and no lock/refund path; the caller runs them on
/// its already-open read unit. The `assertDay` `FOR SHARE` read stays in the
/// caller's TypeScript unit and is not one of these kinds.
pub const PMS_RD_POS_GUEST_HOTELS: &str = "pms_rd_pos_guest_hotels";
pub const PMS_RD_POS_GUEST_CHOICES: &str = "pms_rd_pos_guest_choices";
/// Native successors of the owner-local `posStripePmsFacts` byte-identical
/// point reads. Fixed SQL with four positional binds, no ledger/receipt/outbox
/// and no lock/refund path; the caller runs them on its already-open read unit.
/// Dedicated refund-state and FOR UPDATE reads stay in the caller's TypeScript unit.
pub const PMS_RD_POS_STRIPE_PAYMENT: &str = "pms_rd_pos_stripe_payment";
pub const PMS_RD_POS_BOOKING_PAYMENT: &str = "pms_rd_pos_booking_payment";
/// Native successors of the owner-local `pms-stripe.ts` byte-identical plain
/// payment point reads. Fixed SQL with one/two positional binds, no
/// ledger/receipt/outbox and no lock/refund path; the caller runs them on its
/// already-open read unit. The FOR UPDATE and refund-state reads stay in the
/// caller's TypeScript unit.
pub const PMS_RD_STRIPE_PAYMENT_BY_ID: &str = "pms_rd_stripe_payment_by_id";
pub const PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY: &str = "pms_rd_stripe_payment_by_id_company";
pub const PMS_RD_STRIPE_PAYMENT_PROPERTY: &str = "pms_rd_stripe_payment_property";
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

/// `SELECT 1 FROM pms_charge_corrections WHERE entry_id=? AND status='pending'`
const SQL_CHARGE_CORRECTION_PENDING: &str = "SELECT 1 FROM pms_charge_corrections WHERE entry_id=? AND status='pending'";
const READ_CHARGE_CORRECTION_PENDING: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_CHARGE_CORRECTION_PENDING,
    sql: SQL_CHARGE_CORRECTION_PENDING,
    bind_count: 1,
};
/// `SELECT count(*) n FROM pms_charge_corrections WHERE company_id=? AND property_id=? AND status='pending'`
const SQL_CHARGE_CORRECTION_PENDING_COUNT: &str = "SELECT count(*) n FROM pms_charge_corrections WHERE company_id=? AND property_id=? AND status='pending'";
const READ_CHARGE_CORRECTION_PENDING_COUNT: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_CHARGE_CORRECTION_PENDING_COUNT,
    sql: SQL_CHARGE_CORRECTION_PENDING_COUNT,
    bind_count: 2,
};

/// `SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='guest' AND id=?`
const SQL_GUEST_COMMAND_RECORD: &str = "SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='guest' AND id=?";
const READ_GUEST_COMMAND_RECORD: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_GUEST_COMMAND_RECORD,
    sql: SQL_GUEST_COMMAND_RECORD,
    bind_count: 2,
};

/// `SELECT * FROM pms_documents WHERE id=?`
const SQL_DOCUMENT_BY_ID: &str = "SELECT * FROM pms_documents WHERE id=?";
const READ_DOCUMENT_BY_ID: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_DOCUMENT_BY_ID,
    sql: SQL_DOCUMENT_BY_ID,
    bind_count: 1,
};
/// `SELECT property_id,snapshot_json FROM pms_documents WHERE id=?`
const SQL_DOCUMENT_SNAPSHOT: &str = "SELECT property_id,snapshot_json FROM pms_documents WHERE id=?";
const READ_DOCUMENT_SNAPSHOT: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_DOCUMENT_SNAPSHOT,
    sql: SQL_DOCUMENT_SNAPSHOT,
    bind_count: 1,
};
/// `SELECT config_json FROM pms_properties WHERE id=? AND company_id=?`
const SQL_PROPERTY_CONFIG: &str = "SELECT config_json FROM pms_properties WHERE id=? AND company_id=?";
const READ_PROPERTY_CONFIG: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_PROPERTY_CONFIG,
    sql: SQL_PROPERTY_CONFIG,
    bind_count: 2,
};
/// `SELECT count(*) n FROM pms_transfer_requests WHERE company_id=? AND property_id=? AND business_date=? AND state='pending'`
const SQL_PENDING_HOTEL_TRANSFERS: &str = "SELECT count(*) n FROM pms_transfer_requests WHERE company_id=? AND property_id=? AND business_date=? AND state='pending'";
const READ_PENDING_HOTEL_TRANSFERS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_PENDING_HOTEL_TRANSFERS,
    sql: SQL_PENDING_HOTEL_TRANSFERS,
    bind_count: 3,
};
/// Retired TypeScript literal (newline and four-space indentation preserved):
/// `SELECT count(*) n FROM pms_booking_payments\n    WHERE property_id=? AND company_id=? AND paid_at IS NOT NULL AND status IN ('payment_pending','expired')\n    AND to_char(to_timestamp(paid_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?`
const SQL_UNPOSTED_BOOKING_RECEIPTS: &str = "SELECT count(*) n FROM pms_booking_payments\n    WHERE property_id=? AND company_id=? AND paid_at IS NOT NULL AND status IN ('payment_pending','expired')\n    AND to_char(to_timestamp(paid_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?";
const READ_UNPOSTED_BOOKING_RECEIPTS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_UNPOSTED_BOOKING_RECEIPTS,
    sql: SQL_UNPOSTED_BOOKING_RECEIPTS,
    bind_count: 4,
};
/// Retired TypeScript literal (newline and four-space indentation preserved):
/// `SELECT count(*) n FROM pms_stripe_payments\n    WHERE property_id=? AND company_id=? AND received_at IS NOT NULL AND state IN ('requested','pending','cancelled')\n    AND to_char(to_timestamp(received_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?`
const SQL_UNPOSTED_COUNTER_RECEIPTS: &str = "SELECT count(*) n FROM pms_stripe_payments\n    WHERE property_id=? AND company_id=? AND received_at IS NOT NULL AND state IN ('requested','pending','cancelled')\n    AND to_char(to_timestamp(received_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?";
const READ_UNPOSTED_COUNTER_RECEIPTS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_UNPOSTED_COUNTER_RECEIPTS,
    sql: SQL_UNPOSTED_COUNTER_RECEIPTS,
    bind_count: 4,
};
/// Retired TypeScript literal:
/// `SELECT id,name,state,business_date FROM pms_properties WHERE company_id=? AND branch_id=? ORDER BY name`
const SQL_HOSPITALITY_SETUP_PROPERTIES: &str = "SELECT id,name,state,business_date FROM pms_properties WHERE company_id=? AND branch_id=? ORDER BY name";
const READ_HOSPITALITY_SETUP_PROPERTIES: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_HOSPITALITY_SETUP_PROPERTIES,
    sql: SQL_HOSPITALITY_SETUP_PROPERTIES,
    bind_count: 2,
};

/// `SELECT to_jsonb(p)::text image FROM pms_properties p WHERE p.id=? AND p.company_id=?`
const SQL_ATTACHMENT_PROPERTY_IMAGE: &str = "SELECT to_jsonb(p)::text image FROM pms_properties p WHERE p.id=? AND p.company_id=?";
const READ_ATTACHMENT_PROPERTY_IMAGE: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_ATTACHMENT_PROPERTY_IMAGE,
    sql: SQL_ATTACHMENT_PROPERTY_IMAGE,
    bind_count: 2,
};
/// `SELECT jsonb_build_object('property',to_jsonb(p),'record',to_jsonb(r))::text image FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=? AND p.company_id=?`
const SQL_ATTACHMENT_RECORD_IMAGE: &str = "SELECT jsonb_build_object('property',to_jsonb(p),'record',to_jsonb(r))::text image FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=? AND p.company_id=?";
const READ_ATTACHMENT_RECORD_IMAGE: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_ATTACHMENT_RECORD_IMAGE,
    sql: SQL_ATTACHMENT_RECORD_IMAGE,
    bind_count: 2,
};

/// Retired TypeScript literal (newline and four-space indentation preserved):
/// `SELECT DISTINCT pp.journal_id FROM pms_postings pp JOIN pms_records r ON r.id=pp.record_id AND r.property_id=pp.property_id\n    WHERE pp.property_id=? AND r.kind='entry' AND r.data_json::jsonb->>'guestAccountId'=(SELECT guest_account_id FROM pms_documents WHERE id=?)`
const SQL_DOCUMENT_RELATED_POSTINGS: &str = "SELECT DISTINCT pp.journal_id FROM pms_postings pp JOIN pms_records r ON r.id=pp.record_id AND r.property_id=pp.property_id\n    WHERE pp.property_id=? AND r.kind='entry' AND r.data_json::jsonb->>'guestAccountId'=(SELECT guest_account_id FROM pms_documents WHERE id=?)";
const READ_DOCUMENT_RELATED_POSTINGS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_DOCUMENT_RELATED_POSTINGS,
    sql: SQL_DOCUMENT_RELATED_POSTINGS,
    bind_count: 2,
};

/// Retired TypeScript literal:
/// `SELECT id,name,branch_id FROM pms_properties WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name,id LIMIT 100`
const SQL_POS_GUEST_HOTELS: &str = "SELECT id,name,branch_id FROM pms_properties WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name,id LIMIT 100";
const READ_POS_GUEST_HOTELS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_POS_GUEST_HOTELS,
    sql: SQL_POS_GUEST_HOTELS,
    bind_count: 3,
};
/// Retired TypeScript literal (newline and eight-space indentation preserved):
/// `SELECT p.id property_id,p.version property_version,s.id stay_id,\n        r.data_json::jsonb->>'number' room_number,g.data_json::jsonb->>'name' guest_name\n        FROM pms_properties p JOIN pms_records s ON s.property_id=p.id AND s.kind='stay'\n        JOIN pms_records a ON a.id=s.data_json::jsonb->>'guestAccountId' AND a.property_id=p.id AND a.kind='guest_account'\n        JOIN pms_records r ON r.id=s.data_json::jsonb->>'roomId' AND r.property_id=p.id AND r.kind='room'\n        JOIN pms_records g ON g.id=s.data_json::jsonb->>'guestId' AND g.property_id=p.id AND g.kind='guest'\n        WHERE p.company_id=? AND p.branch_id=? AND p.id=? AND p.state='open' AND p.config_json::jsonb->>'currency'=?\n        AND s.data_json::jsonb->>'state'='in_house' AND a.data_json::jsonb->>'state'='open'\n        AND COALESCE(a.data_json::jsonb->>'invoiceId','')='' ORDER BY r.data_json::jsonb->>'number',s.id LIMIT 100`
const SQL_POS_GUEST_CHOICES: &str = "SELECT p.id property_id,p.version property_version,s.id stay_id,\n        r.data_json::jsonb->>'number' room_number,g.data_json::jsonb->>'name' guest_name\n        FROM pms_properties p JOIN pms_records s ON s.property_id=p.id AND s.kind='stay'\n        JOIN pms_records a ON a.id=s.data_json::jsonb->>'guestAccountId' AND a.property_id=p.id AND a.kind='guest_account'\n        JOIN pms_records r ON r.id=s.data_json::jsonb->>'roomId' AND r.property_id=p.id AND r.kind='room'\n        JOIN pms_records g ON g.id=s.data_json::jsonb->>'guestId' AND g.property_id=p.id AND g.kind='guest'\n        WHERE p.company_id=? AND p.branch_id=? AND p.id=? AND p.state='open' AND p.config_json::jsonb->>'currency'=?\n        AND s.data_json::jsonb->>'state'='in_house' AND a.data_json::jsonb->>'state'='open'\n        AND COALESCE(a.data_json::jsonb->>'invoiceId','')='' ORDER BY r.data_json::jsonb->>'number',s.id LIMIT 100";
const READ_POS_GUEST_CHOICES: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_POS_GUEST_CHOICES,
    sql: SQL_POS_GUEST_CHOICES,
    bind_count: 4,
};

/// Retired TypeScript literal (byte-identical in readPmsStripePayments1/2):
/// `SELECT id FROM pms_stripe_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?`
const SQL_POS_STRIPE_PAYMENT: &str = "SELECT id FROM pms_stripe_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?";
const READ_POS_STRIPE_PAYMENT: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_POS_STRIPE_PAYMENT,
    sql: SQL_POS_STRIPE_PAYMENT,
    bind_count: 4,
};
/// Retired TypeScript literal (byte-identical in readPmsBookingPayments1/2):
/// `SELECT id FROM pms_booking_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?`
const SQL_POS_BOOKING_PAYMENT: &str = "SELECT id FROM pms_booking_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?";
const READ_POS_BOOKING_PAYMENT: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_POS_BOOKING_PAYMENT,
    sql: SQL_POS_BOOKING_PAYMENT,
    bind_count: 4,
};

/// Retired TypeScript literal (byte-identical in readPmsStripePayments2/4/5/13):
/// `SELECT * FROM pms_stripe_payments WHERE id=?`
const SQL_STRIPE_PAYMENT_BY_ID: &str = "SELECT * FROM pms_stripe_payments WHERE id=?";
const READ_STRIPE_PAYMENT_BY_ID: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_STRIPE_PAYMENT_BY_ID,
    sql: SQL_STRIPE_PAYMENT_BY_ID,
    bind_count: 1,
};
/// Retired TypeScript literal (byte-identical in readPmsStripePayments3/16):
/// `SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=?`
const SQL_STRIPE_PAYMENT_BY_ID_COMPANY: &str = "SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=?";
const READ_STRIPE_PAYMENT_BY_ID_COMPANY: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY,
    sql: SQL_STRIPE_PAYMENT_BY_ID_COMPANY,
    bind_count: 2,
};
/// Retired TypeScript literal (byte-identical in readPmsStripePayments9/11):
/// `SELECT property_id FROM pms_stripe_payments WHERE id=? AND company_id=?`
const SQL_STRIPE_PAYMENT_PROPERTY: &str = "SELECT property_id FROM pms_stripe_payments WHERE id=? AND company_id=?";
const READ_STRIPE_PAYMENT_PROPERTY: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_STRIPE_PAYMENT_PROPERTY,
    sql: SQL_STRIPE_PAYMENT_PROPERTY,
    bind_count: 2,
};

const READS: &[NativeReadStatement] = &[READ_INVENTORY_COMMAND, READ_BOOKING_CHANGE, READ_BOOKING_CHANGE_PENDING, READ_BOOKING_PAYMENT, READ_POSTING_CHARGE, READ_POSTING_BY_RECORD, READ_CHARGE_CORRECTION_PENDING, READ_CHARGE_CORRECTION_PENDING_COUNT, READ_GUEST_COMMAND_RECORD, READ_DOCUMENT_BY_ID, READ_DOCUMENT_SNAPSHOT, READ_PROPERTY_CONFIG, READ_PENDING_HOTEL_TRANSFERS, READ_UNPOSTED_BOOKING_RECEIPTS, READ_UNPOSTED_COUNTER_RECEIPTS, READ_HOSPITALITY_SETUP_PROPERTIES, READ_ATTACHMENT_PROPERTY_IMAGE, READ_ATTACHMENT_RECORD_IMAGE, READ_DOCUMENT_RELATED_POSTINGS, READ_POS_GUEST_HOTELS, READ_POS_GUEST_CHOICES, READ_POS_STRIPE_PAYMENT, READ_POS_BOOKING_PAYMENT, READ_STRIPE_PAYMENT_BY_ID, READ_STRIPE_PAYMENT_BY_ID_COMPANY, READ_STRIPE_PAYMENT_PROPERTY];

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

    #[test]
    fn charge_correction_pending_reads_are_fixed_ledger_free_selects() {
        let pending = read_statement(PMS_RD_CHARGE_CORRECTION_PENDING).expect("pending charge-correction read");
        let count = read_statement(PMS_RD_CHARGE_CORRECTION_PENDING_COUNT).expect("pending charge-correction count read");
        assert_eq!(pending.sql, "SELECT 1 FROM pms_charge_corrections WHERE entry_id=? AND status='pending'");
        assert_eq!(count.sql, "SELECT count(*) n FROM pms_charge_corrections WHERE company_id=? AND property_id=? AND status='pending'");
        for read in [pending, count] {
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
        assert_eq!(pending.bind_count, 1);
        assert_eq!(count.bind_count, 2);
    }

    #[test]
    fn guest_command_record_read_is_fixed_ledger_free_select() {
        let read = read_statement(PMS_RD_GUEST_COMMAND_RECORD).expect("guest command record read");
        assert_eq!(read.sql, "SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='guest' AND id=?");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"));
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE FROM"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("FOR SHARE"));
        assert_eq!(read.sql.matches('?').count(), read.bind_count);
        assert_eq!(read.bind_count, 2);
        assert!(is_read(read.kind));
    }

    #[test]
    fn document_point_reads_are_fixed_ledger_free_selects() {
        let full = read_statement(PMS_RD_DOCUMENT_BY_ID).expect("document by id read");
        assert_eq!(full.sql, "SELECT * FROM pms_documents WHERE id=?");
        let snapshot = read_statement(PMS_RD_DOCUMENT_SNAPSHOT).expect("document snapshot read");
        assert_eq!(snapshot.sql, "SELECT property_id,snapshot_json FROM pms_documents WHERE id=?");
        for read in [full, snapshot] {
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            assert_eq!(read.bind_count, 1);
            assert!(is_read(read.kind));
        }
    }

    #[test]
    fn property_config_read_is_fixed_ledger_free_select() {
        let read = read_statement(PMS_RD_PROPERTY_CONFIG).expect("property config read");
        assert_eq!(read.sql, "SELECT config_json FROM pms_properties WHERE id=? AND company_id=?");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"));
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE FROM"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("FOR SHARE"));
        assert!(!upper.contains("LOCK TABLE"));
        assert_eq!(read.sql.matches('?').count(), read.bind_count);
        assert_eq!(read.bind_count, 2);
        assert!(is_read(read.kind));
    }

    #[test]
    fn day_control_count_reads_match_retired_typescript_literals() {
        let transfers = read_statement(PMS_RD_PENDING_HOTEL_TRANSFERS).expect("pending transfers read");
        assert_eq!(transfers.sql, "SELECT count(*) n FROM pms_transfer_requests WHERE company_id=? AND property_id=? AND business_date=? AND state='pending'");
        let receipts = read_statement(PMS_RD_UNPOSTED_BOOKING_RECEIPTS).expect("unposted receipts read");
        assert_eq!(receipts.sql, "SELECT count(*) n FROM pms_booking_payments\n    WHERE property_id=? AND company_id=? AND paid_at IS NOT NULL AND status IN ('payment_pending','expired')\n    AND to_char(to_timestamp(paid_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?");
        for read in [transfers, receipts] {
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            assert!(is_read(read.kind));
        }
        assert_eq!(transfers.bind_count, 3);
        assert_eq!(receipts.bind_count, 4);
        assert!(!is_read("pms_rd_refund"));
    }

    #[test]
    fn unposted_counter_receipts_read_matches_retired_typescript_literal() {
        let read = read_statement(PMS_RD_UNPOSTED_COUNTER_RECEIPTS).expect("unposted counter receipts read");
        assert_eq!(read.sql, "SELECT count(*) n FROM pms_stripe_payments\n    WHERE property_id=? AND company_id=? AND received_at IS NOT NULL AND state IN ('requested','pending','cancelled')\n    AND to_char(to_timestamp(received_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"));
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE FROM"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("FOR SHARE"));
        assert!(!upper.contains("LOCK TABLE"));
        assert!(!upper.contains("REFUND"));
        assert_eq!(read.sql.matches('?').count(), read.bind_count);
        assert_eq!(read.bind_count, 4);
        assert!(is_read(read.kind));
    }

    #[test]
    fn hospitality_setup_properties_read_matches_retired_typescript_literal() {
        let read = read_statement(PMS_RD_HOSPITALITY_SETUP_PROPERTIES).expect("hospitality setup properties read");
        assert_eq!(read.sql, "SELECT id,name,state,business_date FROM pms_properties WHERE company_id=? AND branch_id=? ORDER BY name");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"));
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE FROM"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("FOR SHARE"));
        assert!(!upper.contains("LOCK TABLE"));
        assert!(!upper.contains("REFUND"));
        assert_eq!(read.sql.matches('?').count(), read.bind_count);
        assert_eq!(read.bind_count, 2);
        assert!(is_read(read.kind));
    }

    #[test]
    fn attachment_read_source_images_are_fixed_ledger_free_selects() {
        let property = read_statement(PMS_RD_ATTACHMENT_PROPERTY_IMAGE).expect("property image read");
        let record = read_statement(PMS_RD_ATTACHMENT_RECORD_IMAGE).expect("record image read");
        assert_eq!(property.sql, "SELECT to_jsonb(p)::text image FROM pms_properties p WHERE p.id=? AND p.company_id=?");
        assert_eq!(record.sql, "SELECT jsonb_build_object('property',to_jsonb(p),'record',to_jsonb(r))::text image FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=? AND p.company_id=?");
        for read in [property, record] {
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(!upper.contains("REFUND"));
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            assert_eq!(read.bind_count, 2);
            assert!(is_read(read.kind));
        }
    }

    #[test]
    fn document_related_postings_read_matches_retired_typescript_literal() {
        let read = read_statement(PMS_RD_DOCUMENT_RELATED_POSTINGS).expect("related postings read");
        assert_eq!(read.sql, "SELECT DISTINCT pp.journal_id FROM pms_postings pp JOIN pms_records r ON r.id=pp.record_id AND r.property_id=pp.property_id\n    WHERE pp.property_id=? AND r.kind='entry' AND r.data_json::jsonb->>'guestAccountId'=(SELECT guest_account_id FROM pms_documents WHERE id=?)");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"));
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE FROM"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("FOR SHARE"));
        assert!(!upper.contains("LOCK TABLE"));
        assert!(!upper.contains("REFUND"));
        assert_eq!(read.sql.matches('?').count(), read.bind_count);
        assert_eq!(read.bind_count, 2);
        assert!(is_read(read.kind));
    }

    #[test]
    fn pos_guest_account_reads_match_retired_typescript_literals() {
        let hotels = read_statement(PMS_RD_POS_GUEST_HOTELS).expect("pos guest hotels read");
        assert_eq!(hotels.sql, "SELECT id,name,branch_id FROM pms_properties WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name,id LIMIT 100");
        assert_eq!(hotels.bind_count, 3);

        let choices = read_statement(PMS_RD_POS_GUEST_CHOICES).expect("pos guest choices read");
        assert_eq!(choices.sql, "SELECT p.id property_id,p.version property_version,s.id stay_id,\n        r.data_json::jsonb->>'number' room_number,g.data_json::jsonb->>'name' guest_name\n        FROM pms_properties p JOIN pms_records s ON s.property_id=p.id AND s.kind='stay'\n        JOIN pms_records a ON a.id=s.data_json::jsonb->>'guestAccountId' AND a.property_id=p.id AND a.kind='guest_account'\n        JOIN pms_records r ON r.id=s.data_json::jsonb->>'roomId' AND r.property_id=p.id AND r.kind='room'\n        JOIN pms_records g ON g.id=s.data_json::jsonb->>'guestId' AND g.property_id=p.id AND g.kind='guest'\n        WHERE p.company_id=? AND p.branch_id=? AND p.id=? AND p.state='open' AND p.config_json::jsonb->>'currency'=?\n        AND s.data_json::jsonb->>'state'='in_house' AND a.data_json::jsonb->>'state'='open'\n        AND COALESCE(a.data_json::jsonb->>'invoiceId','')='' ORDER BY r.data_json::jsonb->>'number',s.id LIMIT 100");
        assert_eq!(choices.bind_count, 4);

        for read in [hotels, choices] {
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(!upper.contains("REFUND"));
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            assert!(is_read(read.kind));
        }
        // The hotel day assert stays in the caller's TypeScript unit.
        assert!(!READS.iter().any(|read| read.sql.contains("FOR SHARE")));
    }

    #[test]
    fn pos_stripe_fact_reads_match_retired_typescript_literals() {
        let stripe = read_statement(PMS_RD_POS_STRIPE_PAYMENT).expect("pos stripe payment read");
        assert_eq!(stripe.sql, "SELECT id FROM pms_stripe_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?");
        assert_eq!(stripe.bind_count, 4);

        let booking = read_statement(PMS_RD_POS_BOOKING_PAYMENT).expect("pos booking payment read");
        assert_eq!(booking.sql, "SELECT id FROM pms_booking_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?");
        assert_eq!(booking.bind_count, 4);

        for read in [stripe, booking] {
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(!upper.contains("REFUND"));
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            assert!(is_read(read.kind));
        }
    }

    #[test]
    fn stripe_plain_point_reads_match_retired_typescript_literals() {
        let by_id = read_statement(PMS_RD_STRIPE_PAYMENT_BY_ID).expect("stripe payment by id read");
        assert_eq!(by_id.sql, "SELECT * FROM pms_stripe_payments WHERE id=?");
        assert_eq!(by_id.bind_count, 1);

        let by_company = read_statement(PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY).expect("stripe payment by id/company read");
        assert_eq!(by_company.sql, "SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=?");
        assert_eq!(by_company.bind_count, 2);

        let property = read_statement(PMS_RD_STRIPE_PAYMENT_PROPERTY).expect("stripe payment property read");
        assert_eq!(property.sql, "SELECT property_id FROM pms_stripe_payments WHERE id=? AND company_id=?");
        assert_eq!(property.bind_count, 2);

        for read in [by_id, by_company, property] {
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(!upper.contains("REFUND"));
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            assert!(is_read(read.kind));
        }
    }
}
