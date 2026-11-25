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
/// Native successors of the owner-local scoped-records command readers. Each
/// expands the retired TypeScript template literal byte-for-byte (the scoped
/// projection and member-predicate fragments are inlined at generation time).
/// No ledger/receipt/outbox and no lock/refund path; the caller runs the exact
/// statement on its already-open read unit. Any FOR UPDATE/FOR SHARE/LOCK TABLE
/// read stays inline in the caller's TypeScript unit.
pub const PMS_RD_STAY_COMMAND: &str = "pms_rd_stay_command";
pub const PMS_RD_CASHIER_COMMAND: &str = "pms_rd_cashier_command";
pub const PMS_RD_POS_COMMAND: &str = "pms_rd_pos_command";
pub const PMS_RD_NIGHT_COMMAND: &str = "pms_rd_night_command";
/// Native successors of the owner-local `hotelSources` recursive-CTE read (eleven
/// positional binds) and the housekeeping command snapshot read (four positional
/// binds). Both are non-lock reads: the caller runs them on its already-open unit
/// with the same payload, so a caller holding the property row `FOR UPDATE`
/// observes its own uncommitted state. The property lock read itself stays in the
/// caller's TypeScript unit.
pub const PMS_RD_HOTEL_SOURCES: &str = "pms_rd_hotel_sources";
pub const PMS_RD_HOUSEKEEPING_COMMAND: &str = "pms_rd_housekeeping_command";
/// Native successor of the owner-local `sectionRelatedRecords` section-list read.
/// The 4281-byte retired TypeScript template is expanded byte-identically here with
/// 29 positional binds in the scout order. No lock, refund or ledger path; the caller
/// runs it on its already-open unit and decodes rows with `decodePmsRecord`.
pub const PMS_RD_SECTION_RELATED_RECORDS: &str = "pms_rd_section_related_records";
/// Native successor of the owner-local `pms/guest-document.ts` document point
/// read. Fixed SQL with three positional binds, no ledger/receipt/outbox and no
/// lock/refund path; the guest booking document transaction runs it on its
/// already-open unit with the same `[id, propertyId, reservationId]` payload.
pub const PMS_RD_GUEST_BOOKING_DOCUMENT: &str = "pms_rd_guest_booking_document";
/// Native successor of the owner-local home `hotelMetrics` property-scope read.
/// The exact retained statement (the same bytes as the owner read port in
/// `home_metrics_postgres.rs`, with `$1..$5` numbered by `?` placeholders) with
/// five positional binds; no ledger/receipt/outbox and no lock/refund path. The
/// caller runs it on its already-open unit with the retained Company/branch scope.
pub const PMS_RD_HOTEL_METRICS: &str = "pms_rd_hotel_metrics";
/// Native successors of the owner-local `pms-booking.ts` receipt point reads.
/// Fixed SQL with one/two positional binds, no ledger/receipt/outbox and no
/// lock/refund path; the caller runs them on its already-open booking unit.
/// The receipt idempotency writes and conflict locks stay in the caller.
pub const PMS_RD_BOOKING_RECEIPT_BY_KEY: &str = "pms_rd_booking_receipt_by_key";
pub const PMS_RD_BOOKING_RECEIPT_BY_ATTEMPT: &str = "pms_rd_booking_receipt_by_attempt";
/// Native successors of the owner-local booking companion reads used when a
/// management token opens a reservation: the guest-document list and the
/// pending change-payment point read. Fixed SQL with three/one positional
/// binds, no ledger/receipt/outbox and no lock/refund path; the caller runs
/// them on its already-open booking transaction.
pub const PMS_RD_BOOKING_DOCUMENTS: &str = "pms_rd_booking_documents";
pub const PMS_RD_CHANGE_PAYMENT_PENDING: &str = "pms_rd_change_payment_pending";
/// Native successors of the paired refund-allocation companion reads used
/// together when reviewing a refund request. Fixed SQL with one positional
/// bind each, no ledger/receipt/outbox and no lock/refund decision; the caller
/// runs them on its already-open refund transaction. Refund-state decisions
/// and FOR UPDATE locks stay in the caller.
pub const PMS_RD_BOOKING_ALLOCATIONS: &str = "pms_rd_booking_allocations";
pub const PMS_RD_REFUND_REQUEST_ALLOCATIONS: &str = "pms_rd_refund_request_allocations";
/// Native successors of the owner-local `pms.ts` transfer/document companion
/// reads. Fixed SQL with two positional binds each, no ledger/receipt/outbox
/// and no lock/refund path; the caller runs them on its already-open unit.
pub const PMS_RD_TRANSFER_REQUESTS: &str = "pms_rd_transfer_requests";
pub const PMS_RD_GUEST_DOCUMENTS: &str = "pms_rd_guest_documents";
/// Native successors of the owner-local `pms.ts` fixed allocation-existence
/// and pending-job list reads. Fixed SQL with two/four positional binds, no
/// ledger/receipt/outbox and no lock/refund path; the caller runs them on its
/// already-open unit. The property FOR UPDATE lock stays in the caller.
pub const PMS_RD_BOOKING_ALLOCATION_EXISTS: &str = "pms_rd_booking_allocation_exists";
pub const PMS_RD_PENDING_JOBS: &str = "pms_rd_pending_jobs";
/// Native successors of the owner-local `pms-booking.ts` property-scope and
/// records-list reads. Each has two closed variants (branch filter / date
/// search) with the exact retired SQL bytes and 2/3 and 1/3 positional binds.
/// No ledger/receipt/outbox and no lock/refund path; the caller runs them on
/// its already-open booking unit.
pub const PMS_RD_BOOKING_PROPERTY_SCOPE: &str = "pms_rd_booking_property_scope";
pub const PMS_RD_BOOKING_PROPERTY_SCOPE_BRANCH: &str = "pms_rd_booking_property_scope_branch";
pub const PMS_RD_BOOKING_RECORDS: &str = "pms_rd_booking_records";
pub const PMS_RD_BOOKING_RECORDS_SEARCH: &str = "pms_rd_booking_records_search";
/// Native successors of the owner-local `pms.ts` guest-account sum and
/// charge-correction reads. Fixed SQL with one/three and two positional binds;
/// the correction read has two closed variants (front-mode requestedBy filter
/// or pending state). No ledger/receipt/outbox and no lock/refund path; the
/// caller runs them on its already-open unit.
pub const PMS_RD_GUEST_ACCOUNT_SUMS: &str = "pms_rd_guest_account_sums";
pub const PMS_RD_CHARGE_CORRECTIONS: &str = "pms_rd_charge_corrections";
pub const PMS_RD_CHARGE_CORRECTIONS_FRONT: &str = "pms_rd_charge_corrections_front";
/// Native successors of the retired r46 two-column point reads. Three fixed
/// kinds, no ledger/receipt/outbox and no lock/refund path; the caller runs
/// them on its already-open unit. The FOR KEY SHARE sibling of the property
/// scope read stays in the caller.
pub const PMS_RD_BOOKING_TOKEN: &str = "pms_rd_booking_token";
pub const PMS_RD_STRIPE_PAYMENT_KEY: &str = "pms_rd_stripe_payment_key";
pub const PMS_RD_PROPERTY_SCOPE: &str = "pms_rd_property_scope";
/// Native successor of the owner-local hospitality day-control
/// `failedCounterRefunds` count. Fixed SQL with four positional binds, no
/// ledger/receipt/outbox and no lock path; the same read-only diagnostic count
/// the other day-control counter closure kinds already own.
pub const PMS_RD_FAILED_COUNTER_REFUNDS: &str = "pms_rd_failed_counter_refunds";
/// Native successors of the owner-local reservation-property probe and the
/// rollout setup-readiness probe. Fixed SQL with one positional bind each, no
/// ledger/receipt/outbox and no lock path; the caller runs them on its
/// already-open unit. The readiness head inlines the closed r22 room/rate
/// witnesses byte-for-byte.
pub const PMS_RD_RESERVATION_PROPERTY: &str = "pms_rd_reservation_property";
pub const PMS_RD_ROLLOUT_SETUP_READY: &str = "pms_rd_rollout_setup_ready";
/// Native successors of the paired attachment owner-fact reads used by the
/// Finance attachment boundary. Fixed SQL with two positional binds each, no
/// ledger/receipt/outbox and no lock path; the caller runs them on its unit.
pub const PMS_RD_ATTACHMENT_OWNER_FACT: &str = "pms_rd_attachment_owner_fact";
pub const PMS_RD_ATTACHMENT_PROPERTY_OWNER: &str = "pms_rd_attachment_property_owner";
/// Native successor of the owner-local `readPmsDashboard` records read. The
/// retired template is expanded byte-for-byte from the r24 projection head,
/// the r42 ordered-records tail and the closed stay predicate; four positional
/// binds. No ledger/receipt/outbox and no lock/refund path; the caller runs it
/// on its already-open unit.
pub const PMS_RD_DASHBOARD_RECORDS: &str = "pms_rd_dashboard_records";
/// Native successors of the owner-local record detail and choices reads. The
/// retired templates are expanded byte-for-byte from the r24/r26/r31/r35/r45
/// fragments; the choices read has two closed variants (block group filter or
/// default) with seven positional binds each. No ledger/receipt/outbox and no
/// lock path; the caller runs them on its already-open unit.
pub const PMS_RD_RECORD_DETAIL: &str = "pms_rd_record_detail";
pub const PMS_RD_PMS_CHOICES: &str = "pms_rd_pms_choices";
pub const PMS_RD_PMS_CHOICES_BLOCK: &str = "pms_rd_pms_choices_block";
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

/// Retired TypeScript literal (`readPmsStayCommand`) with every shared fragment
/// expanded byte-identically; six positional binds.
const SQL_STAY_COMMAND: &str = "WITH scope AS (\n          SELECT ?::text property_id, ?::text target_id, ?::text target_kind,\n            ?::text guest_id, ?::text account_id, ?::text room_id\n        ), target AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n          WHERE r.property_id=q.property_id AND r.kind=q.target_kind AND r.id=q.target_id\n        ), booking_ids AS (\n          SELECT CASE WHEN kind='reservation' THEN id ELSE data_json::jsonb->>'reservationId' END id FROM target\n        ), siblings AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n          WHERE r.property_id=q.property_id AND r.kind='stay'\n            AND r.data_json::jsonb->>'reservationId' IN (SELECT id FROM booking_ids)\n        ), account_candidates AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n          WHERE r.property_id=q.property_id AND r.kind='guest_account'\n            AND r.data_json::jsonb->>'reservationId' IN (SELECT id FROM booking_ids)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest_account'\n              AND r.id IN (SELECT data_json::jsonb->>'guestAccountId' FROM siblings)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id=q.account_id\n        ), accounts AS MATERIALIZED (\n          SELECT * FROM account_candidates\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.data_json::jsonb->>'state'='credit'\n              AND r.data_json::jsonb->>'partyId' IN (SELECT data_json::jsonb->>'partyId' FROM account_candidates WHERE data_json::jsonb->>'state'='credit')\n        ), affected_rooms AS (\n          SELECT data_json::jsonb->>'roomId' id FROM target WHERE kind='stay'\n          UNION SELECT room_id FROM scope\n        ), selected AS (\n          SELECT * FROM target\n          UNION SELECT * FROM siblings\n          UNION SELECT * FROM accounts\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('room_type','room','rate','block','item','credit')\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='stay' AND r.data_json::jsonb->>'state' IN ('reserved','in_house')\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='reservation' AND r.id IN (SELECT id FROM booking_ids)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund') AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM accounts)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest' AND (r.id=q.guest_id\n              OR r.id IN (SELECT data_json::jsonb->>'guestId' FROM siblings)\n              OR r.id IN (SELECT data_json::jsonb->>'guestId' FROM accounts)\n              OR r.id IN (SELECT data_json::jsonb->>'guestId' FROM pms_records WHERE property_id=q.property_id AND kind='reservation' AND id IN (SELECT id FROM booking_ids)))\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='task' AND r.data_json::jsonb->>'roomId' IN (SELECT id FROM affected_rooms)\n        ) SELECT * FROM selected ORDER BY updated_at,id";
const READ_STAY_COMMAND: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_STAY_COMMAND,
    sql: SQL_STAY_COMMAND,
    bind_count: 6,
};

/// Retired TypeScript literal (`readPmsCashierCommand`) with every shared fragment
/// expanded byte-identically; five positional binds.
const SQL_CASHIER_COMMAND: &str = "WITH scope AS (\n          SELECT ?::text property_id, ?::text target_id, ?::text target_kind, ?::text shift_id, ?::text user_id\n        ), target AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n          WHERE r.property_id=q.property_id AND r.kind=CASE WHEN q.target_kind='account_close' THEN 'guest_account' ELSE q.target_kind END AND r.id=q.target_id\n        ), account_ids AS (\n          SELECT CASE WHEN kind='refund' THEN data_json::jsonb->>'guestAccountId' ELSE id END id\n          FROM target WHERE kind IN ('guest_account','refund')\n        ), accounts AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n          WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id IN (SELECT id FROM account_ids)\n        ), selected AS (\n          SELECT * FROM target\n          UNION SELECT * FROM accounts\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('rate','credit','item')\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund')\n              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest'\n              AND r.id IN (SELECT data_json::jsonb->>'guestId' FROM accounts)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND q.target_kind='account_close' AND r.kind='stay'\n              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='shift' AND (r.id=q.shift_id OR\n              (q.target_kind='new_shift' AND r.data_json::jsonb->>'userId'=q.user_id AND r.data_json::jsonb->>'state'='open'))\n        ) SELECT * FROM selected ORDER BY updated_at,id";
const READ_CASHIER_COMMAND: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_CASHIER_COMMAND,
    sql: SQL_CASHIER_COMMAND,
    bind_count: 5,
};

/// Retired TypeScript literal (`readPmsPosCommand`) with every shared fragment
/// expanded byte-identically; three positional binds.
const SQL_POS_COMMAND: &str = "WITH scope AS (\n          SELECT ?::text property_id,?::text stay_id,?::text source_key\n        ), source AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='entry' AND r.data_json::jsonb->>'sourceKey'=q.source_key\n        ), stays AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='stay' AND r.id=q.stay_id\n        ), account_ids AS (\n          SELECT data_json::jsonb->>'guestAccountId' id FROM source\n          UNION SELECT data_json::jsonb->>'guestAccountId' id FROM stays\n        ), accounts AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id IN (SELECT id FROM account_ids)\n        ), selected AS (\n          SELECT * FROM source UNION SELECT * FROM stays UNION SELECT * FROM accounts\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund')\n              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='entry'\n              AND r.data_json::jsonb->>'originalId' IN (SELECT id FROM source)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest'\n              AND r.id IN (SELECT data_json::jsonb->>'guestId' FROM accounts UNION SELECT data_json::jsonb->>'guestId' FROM stays)\n        ) SELECT * FROM selected ORDER BY updated_at,id";
const READ_POS_COMMAND: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_POS_COMMAND,
    sql: SQL_POS_COMMAND,
    bind_count: 3,
};

/// Retired TypeScript literal (`readPmsNightCommand`) with every shared fragment
/// expanded byte-identically; three positional binds.
const SQL_NIGHT_COMMAND: &str = "WITH scope AS (\n          SELECT ?::text property_id,?::text business_date,?::text target_id\n        ), stays AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='stay' AND r.data_json::jsonb->>'state' IN ('reserved','in_house')\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='stay'\n              AND r.data_json::jsonb->>'departure'>q.business_date AND r.data_json::jsonb->>'arrival'<=q.business_date\n        ), accounts AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest_account'\n              AND r.id IN (SELECT data_json::jsonb->>'guestAccountId' FROM stays)\n        ), selected AS (\n          SELECT * FROM stays UNION SELECT * FROM accounts\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('room_type','room','rate','block','item','credit')\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund')\n              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM accounts)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest' AND r.id IN (\n              SELECT data_json::jsonb->>'guestId' FROM stays UNION SELECT data_json::jsonb->>'guestId' FROM accounts)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='night' AND (r.id=q.target_id OR r.data_json::jsonb->>'date'=q.business_date)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='shift' AND (\n              r.data_json::jsonb->>'state'='open' OR (r.data_json::jsonb->>'state'='closed' AND r.data_json::jsonb->'variance' IS DISTINCT FROM '0'::jsonb))\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='task' AND r.data_json::jsonb->>'type'='stayover'\n              AND r.data_json::jsonb->>'date'=(q.business_date::date+1)::text\n              AND r.data_json::jsonb->>'roomId' IN (SELECT data_json::jsonb->>'roomId' FROM stays)\n        ) SELECT * FROM selected ORDER BY updated_at,id";
const READ_NIGHT_COMMAND: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_NIGHT_COMMAND,
    sql: SQL_NIGHT_COMMAND,
    bind_count: 3,
};

/// Retired TypeScript literal (`pmsHotelSources`) with the recursive CTE kept
/// byte-identically, including its single-space line-leading indentation; eleven
/// positional binds.
const SQL_HOTEL_SOURCES: &str = "WITH RECURSIVE roots AS (\n SELECT r.id FROM pms_records r WHERE r.property_id=? AND r.kind='entry' AND r.data_json::jsonb->>'sourceKey' LIKE 'night:%'\n UNION ALL SELECT r.id FROM pms_records r JOIN roots e ON r.data_json::jsonb->>'originalId'=e.id WHERE r.property_id=? AND r.kind='entry' AND r.id<>e.id\n ), sources AS (\n SELECT r.* FROM pms_records r WHERE r.property_id=? AND (\n (? IN ('occupancy','revpar') AND r.kind='room' AND r.data_json::jsonb->>'active'='true' AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=r.property_id AND b.kind='block' AND b.data_json::jsonb->>'state'='active' AND b.data_json::jsonb->>'kind'='maintenance' AND b.data_json::jsonb->>'from'<=? AND b.data_json::jsonb->>'roomId'=r.id))\n OR (? IN ('occupancy','adr') AND r.kind='stay' AND r.data_json::jsonb->>'state' IN ('in_house','checked_out') AND r.data_json::jsonb->>'arrival'<=? AND r.data_json::jsonb->>'departure'>?)\n OR (?<>'occupancy' AND r.kind='entry' AND r.id IN (SELECT id FROM roots) AND r.data_json::jsonb->>'date'=? AND r.data_json::jsonb->>'kind' IN ('charge','adjustment'))\n )) SELECT id,kind,version,data_json FROM sources ORDER BY kind,id LIMIT 31 OFFSET ?";
const READ_HOTEL_SOURCES: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_HOTEL_SOURCES,
    sql: SQL_HOTEL_SOURCES,
    bind_count: 11,
};

/// Retired TypeScript literal (`readPmsHousekeepingCommand`) with its ten-space
/// and eight-space indentation preserved byte-identically; four positional
/// binds. No lock clause: the property `FOR UPDATE` read remains on the caller.
const SQL_HOUSEKEEPING_COMMAND: &str = "WITH target AS (\n          SELECT CASE WHEN kind='task' THEN data_json::jsonb->>'roomId' ELSE id END room_id FROM pms_records WHERE property_id=? AND kind=? AND id=?\n        ) SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r JOIN target t ON\n          (r.kind='room' AND r.id=t.room_id) OR (r.kind='task' AND r.data_json::jsonb->>'roomId'=t.room_id)\n          OR (r.kind='stay' AND r.data_json::jsonb->>'roomId'=t.room_id AND r.data_json::jsonb->>'state'='in_house')\n          WHERE r.property_id=? ORDER BY r.id";
const READ_HOUSEKEEPING_COMMAND: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_HOUSEKEEPING_COMMAND,
    sql: SQL_HOUSEKEEPING_COMMAND,
    bind_count: 4,
};
/// Retired TypeScript literal (`sectionRelatedRecords`) expanded byte-for-byte from
/// fragments r24/r25/r26/r31/r35/r38/r42/r44/r45; 29 positional binds. No lock clause.
const SQL_SECTION_RELATED_RECORDS: &str = "WITH chosen AS (\n   SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id IN (SELECT jsonb_array_elements_text(?::jsonb))\n ), guest_history AS (\n   SELECT id,data_json::jsonb data,row_number() OVER(PARTITION BY data_json::jsonb->>'guestId' ORDER BY data_json::jsonb->>'arrival' DESC,id) position\n   FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'guestId' IN (SELECT id FROM chosen WHERE kind='guest')\n ), stays AS (\n   SELECT id,data FROM chosen WHERE kind='stay'\n   UNION SELECT st.id,st.data FROM chosen account JOIN LATERAL (\n     SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay'\n       AND data_json::jsonb->>'guestAccountId'=account.id OFFSET 0\n   ) st ON true WHERE account.kind='guest_account'\n   UNION SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay' AND (\n     (data_json::jsonb->>'state'='in_house' AND ((? AND data_json::jsonb->>'roomId' IN (SELECT id FROM chosen WHERE kind='room')) OR ?))\n     OR (? AND ((data_json::jsonb->>'state'='reserved' AND data_json::jsonb->>'arrival'<=?) OR (data_json::jsonb->>'state'='in_house')))\n   ) UNION SELECT id,data FROM guest_history WHERE position<=20\n ), guestAccounts AS (SELECT id FROM chosen WHERE kind='guest_account' UNION SELECT data->>'guestAccountId' FROM stays UNION SELECT data->>'guestAccountId' FROM chosen),\n room_ids AS (SELECT id FROM chosen WHERE kind='room' UNION SELECT data->>'roomId' FROM stays UNION SELECT data->>'roomId' FROM chosen),\n recent_tasks AS (\n   SELECT id FROM pms_records WHERE property_id=? AND ? AND kind='task'\n     AND data_json::jsonb->>'roomId' IN (SELECT id FROM room_ids) AND data_json::jsonb->>'state' IN ('done','superseded')\n     ORDER BY updated_at DESC,id LIMIT 50\n ),\n inventory_recent AS (\n   SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND ? AND (\n     (kind='lost' AND (data_json::jsonb->>'state'='stored' OR data_json::jsonb->>'foundDate' BETWEEN ? AND ?))\n     OR (kind='movement' AND data_json::jsonb->>'date' BETWEEN ? AND ?)\n   ) ORDER BY updated_at DESC,id LIMIT 50\n ), linked_ids AS (\n   SELECT id FROM chosen UNION SELECT id FROM stays UNION SELECT id FROM guestAccounts\n   UNION SELECT data->>'reservationId' FROM stays\n   UNION SELECT data->>'guestId' FROM stays UNION SELECT data->>'guestId' FROM chosen\n ), selected_records AS (\n SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=?\n AND kind IN ('room','guest','room_type','rate','entry','refund','correction','task','block','shift','item','lost','movement') AND (\n   id IN (SELECT id FROM chosen) OR id IN (SELECT id FROM stays)\n   OR (kind='reservation' AND id IN (SELECT data->>'reservationId' FROM stays))\n   OR (kind='guest' AND id IN (SELECT data->>'guestId' FROM stays UNION SELECT data->>'guestId' FROM chosen))\n   OR (kind='room' AND (? OR id IN (SELECT id FROM room_ids)))\n   OR (kind='guest' AND ? AND id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='guest' ORDER BY updated_at DESC,id LIMIT 50))\n   OR (kind IN ('room_type','rate') AND (? OR ? OR id IN (SELECT data->>'typeId' FROM stays UNION SELECT data->>'rateId' FROM stays)))\n   OR (kind='guest_account' AND id IN (SELECT id FROM guestAccounts))\n   OR (kind IN ('entry','refund','correction') AND data_json::jsonb->>'guestAccountId' IN (SELECT id FROM guestAccounts))\n   OR (kind='task' AND ? AND data_json::jsonb->>'roomId' IN (SELECT id FROM room_ids) AND (data_json::jsonb->>'state' NOT IN ('done','superseded') OR id IN (SELECT id FROM recent_tasks)))\n   OR (kind='block' AND ? AND data_json::jsonb->>'roomId' IN (SELECT id FROM room_ids) AND data_json::jsonb->>'state'='active')\n   OR (kind='shift' AND (? OR ?) AND (data_json::jsonb->>'state'='open' OR (? AND data_json::jsonb->>'state'='closed' AND (data_json::jsonb->>'variance')::bigint<>0)))\n   OR (kind='item' AND id IN (SELECT data->>'itemId' FROM inventory_recent WHERE kind='movement'))\n   OR id IN (SELECT id FROM inventory_recent)\n ) UNION SELECT related.* FROM linked_ids linked JOIN LATERAL (\n SELECT id,kind,data_json,version,updated_at FROM pms_records\n WHERE property_id=? AND id=linked.id OFFSET 0\n ) related ON true\n ) SELECT * FROM selected_records ORDER BY updated_at,id";
const READ_SECTION_RELATED_RECORDS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_SECTION_RELATED_RECORDS,
    sql: SQL_SECTION_RELATED_RECORDS,
    bind_count: 29,
};
/// Retired TypeScript literal from `pms/guest-document.ts`, byte-for-byte, with
/// the same three positional binds and no lock clause.
const SQL_GUEST_BOOKING_DOCUMENT: &str = "SELECT d.kind,d.snapshot_json,d.created_at FROM pms_documents d JOIN pms_records f ON f.id=d.guest_account_id AND f.property_id=d.property_id AND f.kind='guest_account'\nWHERE d.id=? AND d.property_id=? AND f.data_json::jsonb->>'reservationId'=? AND d.kind IN ('invoice','receipt','refund','credit_note')";
const READ_GUEST_BOOKING_DOCUMENT: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_GUEST_BOOKING_DOCUMENT,
    sql: SQL_GUEST_BOOKING_DOCUMENT,
    bind_count: 3,
};
/// Retired owner `hotelMetrics` statement, expanded byte-for-byte from the
/// retained property-scope projection with `?` placeholders. Five positional
/// binds in the source order: company, branch twice, property twice.
const SQL_HOTEL_METRICS: &str = r#"SELECT p.id,p.name,p.branch_id,p.business_date,p.state,p.config_json::jsonb->>'currency' currency,
    (SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true') room_count,
    (SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true'
      AND r.data_json::jsonb->>'cleanliness'='inspected') inspected,
    (SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true'
      AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=p.id AND b.kind='block' AND b.data_json::jsonb->>'state'='active'
        AND b.data_json::jsonb->>'kind'='maintenance' AND b.data_json::jsonb->>'from'<=p.business_date AND b.data_json::jsonb->>'roomId'=r.id)) capacity,
    (SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state' IN ('in_house','checked_out')
      AND s.data_json::jsonb->>'arrival'<=p.business_date AND s.data_json::jsonb->>'departure'>p.business_date) sold,
    (SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state'='reserved' AND s.data_json::jsonb->>'arrival'<=p.business_date) arrivals,
    (SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state'='in_house' AND s.data_json::jsonb->>'departure'<=p.business_date) departures
    FROM pms_properties p WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)
    ORDER BY p.name,p.id"#;
const READ_HOTEL_METRICS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_HOTEL_METRICS,
    sql: SQL_HOTEL_METRICS,
    bind_count: 5,
};
/// Retired TypeScript literals from `pms-booking.ts`, byte-for-byte.
const SQL_BOOKING_RECEIPT_BY_KEY: &str = "SELECT * FROM pms_booking_receipts WHERE id=?";
const READ_BOOKING_RECEIPT_BY_KEY: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_RECEIPT_BY_KEY,
    sql: SQL_BOOKING_RECEIPT_BY_KEY,
    bind_count: 1,
};
const SQL_BOOKING_RECEIPT_BY_ATTEMPT: &str = "SELECT response_json FROM pms_booking_receipts WHERE reservation_id=? AND response_json::jsonb->>'attemptId'=? LIMIT 1";
const READ_BOOKING_RECEIPT_BY_ATTEMPT: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_RECEIPT_BY_ATTEMPT,
    sql: SQL_BOOKING_RECEIPT_BY_ATTEMPT,
    bind_count: 2,
};
/// Retired TypeScript literals from `pms-booking.ts`, byte-for-byte.
const SQL_BOOKING_DOCUMENTS: &str = "SELECT id,kind,created_at FROM pms_documents WHERE property_id=? AND guest_account_id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='guest_account' AND data_json::jsonb->>'reservationId'=?) ORDER BY created_at DESC LIMIT 100";
const READ_BOOKING_DOCUMENTS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_DOCUMENTS,
    sql: SQL_BOOKING_DOCUMENTS,
    bind_count: 3,
};
const SQL_CHANGE_PAYMENT_PENDING: &str = "SELECT id,expires_at FROM pms_booking_payments WHERE reservation_id=? AND purpose='change' AND status='payment_pending' ORDER BY created_at DESC LIMIT 1";
const READ_CHANGE_PAYMENT_PENDING: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_CHANGE_PAYMENT_PENDING,
    sql: SQL_CHANGE_PAYMENT_PENDING,
    bind_count: 1,
};
/// Retired TypeScript literals from `pms-booking-refunds.ts`, byte-for-byte.
/// The second is the rendered r46 `refundRequestAllocations` head.
const SQL_BOOKING_ALLOCATIONS: &str = "SELECT stay_id,guest_account_id,entry_id,amount_minor FROM pms_booking_allocations WHERE payment_id=? ORDER BY stay_id";
const READ_BOOKING_ALLOCATIONS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_ALLOCATIONS,
    sql: SQL_BOOKING_ALLOCATIONS,
    bind_count: 1,
};
const SQL_REFUND_REQUEST_ALLOCATIONS: &str = "SELECT amount_minor,allocations_json FROM pms_booking_refund_requests WHERE payment_id=?";
const READ_REFUND_REQUEST_ALLOCATIONS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_REFUND_REQUEST_ALLOCATIONS,
    sql: SQL_REFUND_REQUEST_ALLOCATIONS,
    bind_count: 1,
};
/// Retired TypeScript literals from `pms.ts`, byte-for-byte.
const SQL_TRANSFER_REQUESTS: &str = "SELECT * FROM pms_transfer_requests WHERE company_id=? AND property_id=? ORDER BY created_at DESC LIMIT 100";
const READ_TRANSFER_REQUESTS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_TRANSFER_REQUESTS,
    sql: SQL_TRANSFER_REQUESTS,
    bind_count: 2,
};
const SQL_GUEST_DOCUMENTS: &str = "SELECT COALESCE(finance_id,id) id,snapshot_json FROM pms_documents WHERE property_id=? AND guest_account_id=? AND (kind='receipt' OR (kind='opening' AND finance_id IS NOT NULL)) ORDER BY created_at,id";
const READ_GUEST_DOCUMENTS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_GUEST_DOCUMENTS,
    sql: SQL_GUEST_DOCUMENTS,
    bind_count: 2,
};
/// Retired TypeScript literals from `pms.ts`, byte-for-byte.
const SQL_BOOKING_ALLOCATION_EXISTS: &str = "SELECT id FROM pms_booking_allocations WHERE guest_account_id IN (?,?) LIMIT 1";
const READ_BOOKING_ALLOCATION_EXISTS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_ALLOCATION_EXISTS,
    sql: SQL_BOOKING_ALLOCATION_EXISTS,
    bind_count: 2,
};
const SQL_PENDING_JOBS: &str = "SELECT id,property_id,actor_json FROM pms_jobs WHERE state IN ('pending','running') AND (?::text IS NULL OR property_id=?) AND (?::text IS NULL OR id=?) ORDER BY updated_at LIMIT 4";
const READ_PENDING_JOBS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_PENDING_JOBS,
    sql: SQL_PENDING_JOBS,
    bind_count: 4,
};
/// Retired TypeScript literals from `pms-booking.ts`, byte-for-byte. The two
/// records variants preserve the original bind order (property, departure,
/// arrival).
const SQL_BOOKING_PROPERTY_SCOPE: &str = "SELECT id FROM pms_properties WHERE id=? AND company_id=?";
const READ_BOOKING_PROPERTY_SCOPE: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_PROPERTY_SCOPE,
    sql: SQL_BOOKING_PROPERTY_SCOPE,
    bind_count: 2,
};
const SQL_BOOKING_PROPERTY_SCOPE_BRANCH: &str = "SELECT id FROM pms_properties WHERE id=? AND company_id=? AND branch_id=?";
const READ_BOOKING_PROPERTY_SCOPE_BRANCH: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_PROPERTY_SCOPE_BRANCH,
    sql: SQL_BOOKING_PROPERTY_SCOPE_BRANCH,
    bind_count: 3,
};
const SQL_BOOKING_RECORDS: &str = "SELECT * FROM pms_records WHERE property_id=? ORDER BY id";
const READ_BOOKING_RECORDS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_RECORDS,
    sql: SQL_BOOKING_RECORDS,
    bind_count: 1,
};
const SQL_BOOKING_RECORDS_SEARCH: &str = "SELECT * FROM pms_records WHERE property_id=? AND (kind IN ('room_type','room','rate') OR (kind='block' AND data_json::jsonb->>'state'='active') OR (kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house') AND data_json::jsonb->>'arrival' < ? AND data_json::jsonb->>'departure' > ?)) ORDER BY id";
const READ_BOOKING_RECORDS_SEARCH: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_RECORDS_SEARCH,
    sql: SQL_BOOKING_RECORDS_SEARCH,
    bind_count: 3,
};
/// Retired TypeScript literals from `pms.ts`, byte-for-byte, with the closed
/// correction variant inlined at generation time.
const SQL_GUEST_ACCOUNT_SUMS: &str = r#"SELECT COALESCE(SUM((data_json::jsonb->>'due')::bigint),0)::double precision due,
      COALESCE(SUM((data_json::jsonb->>'advance')::bigint),0)::double precision advance,
      COALESCE(SUM((data_json::jsonb->>'deposit')::bigint),0)::double precision deposit
      FROM pms_records WHERE property_id=? AND kind='guest_account'"#;
const READ_GUEST_ACCOUNT_SUMS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_GUEST_ACCOUNT_SUMS,
    sql: SQL_GUEST_ACCOUNT_SUMS,
    bind_count: 1,
};
const SQL_CHARGE_CORRECTIONS: &str = r#"SELECT c.id,c.entry_id,c.guest_account_id,c.status,c.version,c.reason,c.requested_by,c.reviewed_by,c.review_reason,c.reversal_entry_id,c.created_at,
        e.data_json::jsonb->>'kind' entry_kind,e.data_json::jsonb->>'description' entry_description,(e.data_json::jsonb->>'total')::bigint entry_total
        FROM pms_charge_corrections c JOIN pms_records e ON e.id=c.entry_id AND e.property_id=c.property_id
        WHERE c.company_id=? AND c.property_id=? AND c.status='pending' ORDER BY c.created_at DESC LIMIT 100"#;
const SQL_CHARGE_CORRECTIONS_FRONT: &str = r#"SELECT c.id,c.entry_id,c.guest_account_id,c.status,c.version,c.reason,c.requested_by,c.reviewed_by,c.review_reason,c.reversal_entry_id,c.created_at,
        e.data_json::jsonb->>'kind' entry_kind,e.data_json::jsonb->>'description' entry_description,(e.data_json::jsonb->>'total')::bigint entry_total
        FROM pms_charge_corrections c JOIN pms_records e ON e.id=c.entry_id AND e.property_id=c.property_id
        WHERE c.company_id=? AND c.property_id=? AND c.requested_by=? ORDER BY c.created_at DESC LIMIT 100"#;
const READ_CHARGE_CORRECTIONS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_CHARGE_CORRECTIONS,
    sql: SQL_CHARGE_CORRECTIONS,
    bind_count: 2,
};
const READ_CHARGE_CORRECTIONS_FRONT: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_CHARGE_CORRECTIONS_FRONT,
    sql: SQL_CHARGE_CORRECTIONS_FRONT,
    bind_count: 3,
};
/// Retired r46 two-column point reads, byte-for-byte.
const SQL_BOOKING_TOKEN: &str = "SELECT property_id,reservation_id FROM pms_booking_tokens WHERE token_hash=? AND expires_at>?";
const READ_BOOKING_TOKEN: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_BOOKING_TOKEN,
    sql: SQL_BOOKING_TOKEN,
    bind_count: 2,
};
const SQL_STRIPE_PAYMENT_KEY: &str = "SELECT id,request_hash FROM pms_stripe_payments WHERE id=?";
const READ_STRIPE_PAYMENT_KEY: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_STRIPE_PAYMENT_KEY,
    sql: SQL_STRIPE_PAYMENT_KEY,
    bind_count: 1,
};
const SQL_PROPERTY_SCOPE: &str = "SELECT company_id,branch_id FROM pms_properties WHERE id=?";
const READ_PROPERTY_SCOPE: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_PROPERTY_SCOPE,
    sql: SQL_PROPERTY_SCOPE,
    bind_count: 1,
};
/// Retired TypeScript literal from `hospitality-day-controls.ts`, byte-for-byte.
const SQL_FAILED_COUNTER_REFUNDS: &str = r#"SELECT count(*) n FROM pms_stripe_payments
    WHERE property_id=? AND company_id=? AND (refund_state='failed' OR (refund_state IN ('approved','processing','requires_action') AND last_error IS NOT NULL))
    AND to_char(to_timestamp(COALESCE(received_at,created_at)/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?"#;
const READ_FAILED_COUNTER_REFUNDS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_FAILED_COUNTER_REFUNDS,
    sql: SQL_FAILED_COUNTER_REFUNDS,
    bind_count: 4,
};
/// Retired TypeScript literals, byte-for-byte with the closed witnesses inlined.
const SQL_RESERVATION_PROPERTY: &str = "SELECT property_id FROM pms_records WHERE id=? AND kind='reservation'";
const READ_RESERVATION_PROPERTY: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_RESERVATION_PROPERTY,
    sql: SQL_RESERVATION_PROPERTY,
    bind_count: 1,
};
const SQL_ROLLOUT_SETUP_READY: &str = r#"SELECT 1 FROM pms_properties p WHERE p.company_id=? AND p.state='open'
      AND p.config_json::jsonb->>'currency'='THB'
      AND COALESCE(p.config_json::jsonb->>'approvedBy','')<>''
      AND EXISTS(SELECT 1 FROM pms_records r WHERE r.property_id=p.id AND r.kind='room')
      AND EXISTS(SELECT 1 FROM pms_records r WHERE r.property_id=p.id AND r.kind='rate') LIMIT 1"#;
const READ_ROLLOUT_SETUP_READY: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_ROLLOUT_SETUP_READY,
    sql: SQL_ROLLOUT_SETUP_READY,
    bind_count: 1,
};
/// Retired TypeScript literals, byte-for-byte (the first inlines the closed
/// r36 ownerFact head).
const SQL_ATTACHMENT_OWNER_FACT: &str = r#"SELECT p.company_id,p.branch_id,NULL::text department_id,r.version,r.kind
       FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
       WHERE r.id=? AND p.company_id=?"#;
const READ_ATTACHMENT_OWNER_FACT: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_ATTACHMENT_OWNER_FACT,
    sql: SQL_ATTACHMENT_OWNER_FACT,
    bind_count: 2,
};
const SQL_ATTACHMENT_PROPERTY_OWNER: &str = "SELECT company_id,branch_id,NULL::text department_id,version FROM pms_properties WHERE id=? AND company_id=?";
const READ_ATTACHMENT_PROPERTY_OWNER: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_ATTACHMENT_PROPERTY_OWNER,
    sql: SQL_ATTACHMENT_PROPERTY_OWNER,
    bind_count: 2,
};
/// Retired TypeScript template `pmsDashboardRecordsSql`, byte-for-byte rendered.
const SQL_DASHBOARD_RECORDS: &str = r#"WITH selected_stays AS MATERIALIZED (
 SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='stay' AND (data_json::jsonb->>'state' IN ('reserved','in_house') OR
    data_json::jsonb->>'arrival'=? OR data_json::jsonb->>'departure'=?)
), scope AS (SELECT ?::text property_id), selected_records AS (
 SELECT * FROM selected_stays
 UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records
 WHERE property_id=(SELECT property_id FROM scope) AND kind IN ('room_type','room','rate')
 UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records
 WHERE property_id=(SELECT property_id FROM scope) AND kind='task' AND data_json::jsonb->>'state' NOT IN ('done','superseded')
 UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records
 WHERE property_id=(SELECT property_id FROM scope) AND kind='guest_account' AND data_json::jsonb->>'state'!='closed' AND (data_json::jsonb->>'due')::bigint>0
 UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records
 WHERE property_id=(SELECT property_id FROM scope) AND kind='guest' AND id IN (SELECT data_json::jsonb->>'guestId' FROM selected_stays)
) SELECT * FROM selected_records ORDER BY updated_at,id"#;
const READ_DASHBOARD_RECORDS: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_DASHBOARD_RECORDS,
    sql: SQL_DASHBOARD_RECORDS,
    bind_count: 4,
};
/// Retired TypeScript templates, byte-for-byte rendered.
const SQL_RECORD_DETAIL: &str = r#"WITH selected AS (
      SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id=?
    ), related AS (
      SELECT id FROM selected UNION
      SELECT value FROM selected CROSS JOIN LATERAL jsonb_each_text(data)
        WHERE key IN ('guestId','reservationId','guestAccountId','roomId','typeId','rateId')
    ) SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=?
      AND (id IN (SELECT id FROM related) OR (kind='entry' AND data_json::jsonb->>'guestAccountId' IN
        (SELECT id FROM selected WHERE kind='guest_account' UNION SELECT data->>'guestAccountId' FROM selected)))
    ORDER BY updated_at,id"#;
const READ_RECORD_DETAIL: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_RECORD_DETAIL,
    sql: SQL_RECORD_DETAIL,
    bind_count: 3,
};
const SQL_PMS_CHOICES: &str = r#"WITH matched AS (
  SELECT id FROM pms_records WHERE property_id=? AND kind=? 
  AND (?='' OR position(lower(?) in lower(concat_ws(' ',data_json::jsonb->>'name',data_json::jsonb->>'email',id)))>0)
  ORDER BY data_json::jsonb->>'name',id LIMIT 50
 ) SELECT id,kind,version,updated_at,
 CASE WHEN kind='guest' THEN jsonb_build_object('name',data_json::jsonb->>'name','email',COALESCE(data_json::jsonb->>'email',''),'phone','')
 ELSE jsonb_build_object('name',data_json::jsonb->>'name','kind','group','state','active') END::text data_json
 FROM pms_records WHERE property_id=? AND kind=? 
 AND (id IN (SELECT id FROM matched) OR id=?) ORDER BY data_json::jsonb->>'name',id"#;
const READ_PMS_CHOICES: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_PMS_CHOICES,
    sql: SQL_PMS_CHOICES,
    bind_count: 7,
};
const SQL_PMS_CHOICES_BLOCK: &str = r#"WITH matched AS (
  SELECT id FROM pms_records WHERE property_id=? AND kind=? AND data_json::jsonb->>'kind'='group' AND data_json::jsonb->>'state'='active' AND COALESCE(data_json::jsonb->>'bookingChangeId','')=''
  AND (?='' OR position(lower(?) in lower(concat_ws(' ',data_json::jsonb->>'name',data_json::jsonb->>'email',id)))>0)
  ORDER BY data_json::jsonb->>'name',id LIMIT 50
 ) SELECT id,kind,version,updated_at,
 CASE WHEN kind='guest' THEN jsonb_build_object('name',data_json::jsonb->>'name','email',COALESCE(data_json::jsonb->>'email',''),'phone','')
 ELSE jsonb_build_object('name',data_json::jsonb->>'name','kind','group','state','active') END::text data_json
 FROM pms_records WHERE property_id=? AND kind=? AND data_json::jsonb->>'kind'='group' AND data_json::jsonb->>'state'='active' AND COALESCE(data_json::jsonb->>'bookingChangeId','')=''
 AND (id IN (SELECT id FROM matched) OR id=?) ORDER BY data_json::jsonb->>'name',id"#;
const READ_PMS_CHOICES_BLOCK: NativeReadStatement = NativeReadStatement {
    kind: PMS_RD_PMS_CHOICES_BLOCK,
    sql: SQL_PMS_CHOICES_BLOCK,
    bind_count: 7,
};

const READS: &[NativeReadStatement] = &[READ_INVENTORY_COMMAND, READ_BOOKING_CHANGE, READ_BOOKING_CHANGE_PENDING, READ_BOOKING_PAYMENT, READ_POSTING_CHARGE, READ_POSTING_BY_RECORD, READ_CHARGE_CORRECTION_PENDING, READ_CHARGE_CORRECTION_PENDING_COUNT, READ_GUEST_COMMAND_RECORD, READ_DOCUMENT_BY_ID, READ_DOCUMENT_SNAPSHOT, READ_PROPERTY_CONFIG, READ_PENDING_HOTEL_TRANSFERS, READ_UNPOSTED_BOOKING_RECEIPTS, READ_UNPOSTED_COUNTER_RECEIPTS, READ_HOSPITALITY_SETUP_PROPERTIES, READ_ATTACHMENT_PROPERTY_IMAGE, READ_ATTACHMENT_RECORD_IMAGE, READ_DOCUMENT_RELATED_POSTINGS, READ_POS_GUEST_HOTELS, READ_POS_GUEST_CHOICES, READ_POS_STRIPE_PAYMENT, READ_POS_BOOKING_PAYMENT, READ_STRIPE_PAYMENT_BY_ID, READ_STRIPE_PAYMENT_BY_ID_COMPANY, READ_STRIPE_PAYMENT_PROPERTY, READ_STAY_COMMAND, READ_CASHIER_COMMAND, READ_POS_COMMAND, READ_NIGHT_COMMAND, READ_HOTEL_SOURCES, READ_HOUSEKEEPING_COMMAND, READ_SECTION_RELATED_RECORDS, READ_GUEST_BOOKING_DOCUMENT, READ_HOTEL_METRICS, READ_BOOKING_RECEIPT_BY_KEY, READ_BOOKING_RECEIPT_BY_ATTEMPT, READ_BOOKING_DOCUMENTS, READ_CHANGE_PAYMENT_PENDING, READ_BOOKING_ALLOCATIONS, READ_REFUND_REQUEST_ALLOCATIONS, READ_TRANSFER_REQUESTS, READ_GUEST_DOCUMENTS, READ_BOOKING_ALLOCATION_EXISTS, READ_PENDING_JOBS, READ_BOOKING_PROPERTY_SCOPE, READ_BOOKING_PROPERTY_SCOPE_BRANCH, READ_BOOKING_RECORDS, READ_BOOKING_RECORDS_SEARCH, READ_GUEST_ACCOUNT_SUMS, READ_CHARGE_CORRECTIONS, READ_CHARGE_CORRECTIONS_FRONT, READ_BOOKING_TOKEN, READ_STRIPE_PAYMENT_KEY, READ_PROPERTY_SCOPE, READ_FAILED_COUNTER_REFUNDS, READ_RESERVATION_PROPERTY, READ_ROLLOUT_SETUP_READY, READ_ATTACHMENT_OWNER_FACT, READ_ATTACHMENT_PROPERTY_OWNER, READ_DASHBOARD_RECORDS, READ_RECORD_DETAIL, READ_PMS_CHOICES, READ_PMS_CHOICES_BLOCK];

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

    #[test]
    fn scoped_records_reads_match_retired_typescript_literals() {
        let cases = [
            (PMS_RD_STAY_COMMAND, 6usize),
            (PMS_RD_CASHIER_COMMAND, 5),
            (PMS_RD_POS_COMMAND, 3),
            (PMS_RD_NIGHT_COMMAND, 3),
        ];
        for (kind, binds) in cases {
            let read = read_statement(kind).expect("scoped records read kind");
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("WITH SCOPE AS ("));
            assert!(upper.ends_with(") SELECT * FROM SELECTED ORDER BY UPDATED_AT,ID"));
            assert!(upper.contains("FROM PMS_RECORDS R,SCOPE Q"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(!upper.contains("PG_ADVISORY"));
            assert_eq!(read.sql.matches('?').count(), binds);
            assert_eq!(read.bind_count, binds);
            assert!(is_read(kind));
        }
        assert!(!is_read("pms_rd_stay_command_v2"));
    }

    #[test]
    fn hotel_sources_and_housekeeping_reads_are_fixed_ledger_free_selects() {
        let hotel = read_statement(PMS_RD_HOTEL_SOURCES).expect("hotel sources read");
        assert_eq!(hotel.bind_count, 11);
        assert_eq!(hotel.sql.matches('?').count(), 11);
        assert_eq!(hotel.sql.split("SELECT ").count() - 1, 6);
        assert!(hotel.sql.starts_with("WITH RECURSIVE roots AS (\n SELECT r.id FROM pms_records r"));
        assert!(hotel.sql.contains("UNION ALL SELECT r.id FROM pms_records r JOIN roots e"));
        assert!(hotel.sql.contains("NOT EXISTS(SELECT 1 FROM pms_records b"));
        assert!(hotel.sql.contains("r.id IN (SELECT id FROM roots)"));
        assert!(hotel.sql.ends_with("ORDER BY kind,id LIMIT 31 OFFSET ?"));

        let housekeeping = read_statement(PMS_RD_HOUSEKEEPING_COMMAND).expect("housekeeping read");
        assert_eq!(housekeeping.bind_count, 4);
        assert_eq!(housekeeping.sql.matches('?').count(), 4);
        assert_eq!(housekeeping.sql.split("SELECT ").count() - 1, 2);
        assert!(housekeeping.sql.starts_with("WITH target AS (\n          SELECT CASE WHEN kind='task'"));
        assert!(housekeeping.sql.contains("JOIN target t ON"));
        assert!(housekeeping.sql.ends_with("WHERE r.property_id=? ORDER BY r.id"));

        for read in [hotel, housekeeping] {
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("WITH "));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(!upper.contains("PG_ADVISORY"));
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_hotel_sources_v2"));
        assert!(!is_read("pms_rd_housekeeping_command_v2"));
    }

    #[test]
    fn section_related_records_read_matches_retired_typescript_literal() {
        let read = read_statement(PMS_RD_SECTION_RELATED_RECORDS).expect("section related records read");
        assert_eq!(read.kind, "pms_rd_section_related_records");
        assert_eq!(read.sql.len(), 4281);
        assert_eq!(read.sql.matches('?').count(), 29);
        assert_eq!(read.bind_count, 29);
        assert_eq!(read.sql.split("SELECT ").count() - 1, 45);
        assert!(read.sql.starts_with("WITH chosen AS (\n   SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id IN (SELECT jsonb_array_elements_text(?::jsonb))"));
        assert!(read.sql.ends_with(") SELECT * FROM selected_records ORDER BY updated_at,id"));
        assert!(read.sql.contains("JOIN LATERAL"));
        let upper = read.sql.to_ascii_uppercase();
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("DELETE FROM"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("FOR SHARE"));
        assert!(!upper.contains("LOCK TABLE"));
        assert!(!upper.contains("PG_ADVISORY"));
        assert!(is_read(read.kind));
        assert!(!is_read("pms_rd_section_related_records_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn guest_booking_document_read_matches_retired_typescript_literal() {
        let read = read_statement(PMS_RD_GUEST_BOOKING_DOCUMENT).expect("guest booking document read");
        assert_eq!(read.kind, "pms_rd_guest_booking_document");
        assert_eq!(
            read.sql,
            "SELECT d.kind,d.snapshot_json,d.created_at FROM pms_documents d JOIN pms_records f ON f.id=d.guest_account_id AND f.property_id=d.property_id AND f.kind='guest_account'\nWHERE d.id=? AND d.property_id=? AND f.data_json::jsonb->>'reservationId'=? AND d.kind IN ('invoice','receipt','refund','credit_note')"
        );
        assert_eq!(read.sql.matches('?').count(), 3);
        assert_eq!(read.bind_count, 3);
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"));
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE FROM"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("FOR SHARE"));
        assert!(!upper.contains("LOCK TABLE"));
        assert!(!upper.contains("PG_ADVISORY"));
        assert!(is_read(read.kind));
        assert!(!is_read("pms_rd_guest_booking_document_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn hotel_metrics_read_matches_the_retained_owner_statement() {
        let read = read_statement(PMS_RD_HOTEL_METRICS).expect("hotel metrics read");
        assert_eq!(read.kind, "pms_rd_hotel_metrics");
        assert_eq!(read.bind_count, 5);
        assert_eq!(read.sql.matches('?').count(), 5);
        assert!(read.sql.starts_with("SELECT p.id,p.name,p.branch_id,p.business_date,p.state,p.config_json::jsonb->>'currency' currency,"));
        assert!(read.sql.ends_with("ORDER BY p.name,p.id"));
        assert!(read.sql.contains("'maintenance' AND b.data_json::jsonb->>'from'<=p.business_date AND b.data_json::jsonb->>'roomId'=r.id"));
        assert!(read.sql.contains("WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)"));
        let upper = read.sql.to_ascii_uppercase();
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE FROM"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("FOR SHARE"));
        assert!(!upper.contains("LOCK TABLE"));
        assert!(!upper.contains("PG_ADVISORY"));
        assert!(is_read(read.kind));
        assert!(!is_read("pms_rd_hotel_metrics_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn booking_receipt_point_reads_match_retired_typescript_literals() {
        let by_key = read_statement(PMS_RD_BOOKING_RECEIPT_BY_KEY).expect("receipt by key read");
        let by_attempt = read_statement(PMS_RD_BOOKING_RECEIPT_BY_ATTEMPT).expect("receipt by attempt read");
        assert_eq!(by_key.kind, "pms_rd_booking_receipt_by_key");
        assert_eq!(by_key.sql, "SELECT * FROM pms_booking_receipts WHERE id=?");
        assert_eq!(by_key.bind_count, 1);
        assert_eq!(by_attempt.kind, "pms_rd_booking_receipt_by_attempt");
        assert_eq!(by_attempt.sql, "SELECT response_json FROM pms_booking_receipts WHERE reservation_id=? AND response_json::jsonb->>'attemptId'=? LIMIT 1");
        assert_eq!(by_attempt.bind_count, 2);
        for read in [by_key, by_attempt] {
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_booking_receipt_by_key_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn booking_companion_reads_match_retired_typescript_literals() {
        let documents = read_statement(PMS_RD_BOOKING_DOCUMENTS).expect("booking documents read");
        let pending = read_statement(PMS_RD_CHANGE_PAYMENT_PENDING).expect("change payment pending read");
        assert_eq!(documents.kind, "pms_rd_booking_documents");
        assert_eq!(documents.sql, "SELECT id,kind,created_at FROM pms_documents WHERE property_id=? AND guest_account_id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='guest_account' AND data_json::jsonb->>'reservationId'=?) ORDER BY created_at DESC LIMIT 100");
        assert_eq!(documents.bind_count, 3);
        assert_eq!(pending.kind, "pms_rd_change_payment_pending");
        assert_eq!(pending.sql, "SELECT id,expires_at FROM pms_booking_payments WHERE reservation_id=? AND purpose='change' AND status='payment_pending' ORDER BY created_at DESC LIMIT 1");
        assert_eq!(pending.bind_count, 1);
        for read in [documents, pending] {
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_booking_documents_v2"));
        assert!(!is_read("pms_rd_change_payment_pending_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn refund_allocation_reads_match_retired_typescript_literals() {
        let allocations = read_statement(PMS_RD_BOOKING_ALLOCATIONS).expect("booking allocations read");
        let refund = read_statement(PMS_RD_REFUND_REQUEST_ALLOCATIONS).expect("refund request allocations read");
        assert_eq!(allocations.kind, "pms_rd_booking_allocations");
        assert_eq!(allocations.sql, "SELECT stay_id,guest_account_id,entry_id,amount_minor FROM pms_booking_allocations WHERE payment_id=? ORDER BY stay_id");
        assert_eq!(allocations.bind_count, 1);
        assert_eq!(refund.kind, "pms_rd_refund_request_allocations");
        assert_eq!(refund.sql, "SELECT amount_minor,allocations_json FROM pms_booking_refund_requests WHERE payment_id=?");
        assert_eq!(refund.bind_count, 1);
        for read in [allocations, refund] {
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_booking_allocations_v2"));
        assert!(!is_read("pms_rd_refund_request_allocations_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn transfer_and_guest_document_reads_match_retired_typescript_literals() {
        let transfers = read_statement(PMS_RD_TRANSFER_REQUESTS).expect("transfer requests read");
        let documents = read_statement(PMS_RD_GUEST_DOCUMENTS).expect("guest documents read");
        assert_eq!(transfers.kind, "pms_rd_transfer_requests");
        assert_eq!(transfers.sql, "SELECT * FROM pms_transfer_requests WHERE company_id=? AND property_id=? ORDER BY created_at DESC LIMIT 100");
        assert_eq!(transfers.bind_count, 2);
        assert_eq!(documents.kind, "pms_rd_guest_documents");
        assert_eq!(documents.sql, "SELECT COALESCE(finance_id,id) id,snapshot_json FROM pms_documents WHERE property_id=? AND guest_account_id=? AND (kind='receipt' OR (kind='opening' AND finance_id IS NOT NULL)) ORDER BY created_at,id");
        assert_eq!(documents.bind_count, 2);
        for read in [transfers, documents] {
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_transfer_requests_v2"));
        assert!(!is_read("pms_rd_guest_documents_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn allocation_and_pending_job_reads_match_retired_typescript_literals() {
        let allocation = read_statement(PMS_RD_BOOKING_ALLOCATION_EXISTS).expect("allocation existence read");
        let jobs = read_statement(PMS_RD_PENDING_JOBS).expect("pending jobs read");
        assert_eq!(allocation.kind, "pms_rd_booking_allocation_exists");
        assert_eq!(allocation.sql, "SELECT id FROM pms_booking_allocations WHERE guest_account_id IN (?,?) LIMIT 1");
        assert_eq!(allocation.bind_count, 2);
        assert_eq!(jobs.kind, "pms_rd_pending_jobs");
        assert_eq!(jobs.sql, "SELECT id,property_id,actor_json FROM pms_jobs WHERE state IN ('pending','running') AND (?::text IS NULL OR property_id=?) AND (?::text IS NULL OR id=?) ORDER BY updated_at LIMIT 4");
        assert_eq!(jobs.bind_count, 4);
        for read in [allocation, jobs] {
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_booking_allocation_exists_v2"));
        assert!(!is_read("pms_rd_pending_jobs_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn booking_property_and_records_variants_match_retired_typescript_literals() {
        let scope = read_statement(PMS_RD_BOOKING_PROPERTY_SCOPE).expect("property scope read");
        let branch = read_statement(PMS_RD_BOOKING_PROPERTY_SCOPE_BRANCH).expect("branch property scope read");
        let records = read_statement(PMS_RD_BOOKING_RECORDS).expect("booking records read");
        let search = read_statement(PMS_RD_BOOKING_RECORDS_SEARCH).expect("booking records search read");
        assert_eq!(scope.sql, "SELECT id FROM pms_properties WHERE id=? AND company_id=?");
        assert_eq!(scope.bind_count, 2);
        assert_eq!(branch.sql, "SELECT id FROM pms_properties WHERE id=? AND company_id=? AND branch_id=?");
        assert_eq!(branch.bind_count, 3);
        assert_eq!(records.sql, "SELECT * FROM pms_records WHERE property_id=? ORDER BY id");
        assert_eq!(records.bind_count, 1);
        assert_eq!(search.sql, "SELECT * FROM pms_records WHERE property_id=? AND (kind IN ('room_type','room','rate') OR (kind='block' AND data_json::jsonb->>'state'='active') OR (kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house') AND data_json::jsonb->>'arrival' < ? AND data_json::jsonb->>'departure' > ?)) ORDER BY id");
        assert_eq!(search.bind_count, 3);
        for read in [scope, branch, records, search] {
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_booking_records_search_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn guest_account_and_correction_reads_match_retired_typescript_literals() {
        let sums = read_statement(PMS_RD_GUEST_ACCOUNT_SUMS).expect("guest account sums read");
        let pending = read_statement(PMS_RD_CHARGE_CORRECTIONS).expect("pending corrections read");
        let front = read_statement(PMS_RD_CHARGE_CORRECTIONS_FRONT).expect("front corrections read");
        assert_eq!(sums.kind, "pms_rd_guest_account_sums");
        assert_eq!(sums.sql, "SELECT COALESCE(SUM((data_json::jsonb->>'due')::bigint),0)::double precision due,\n      COALESCE(SUM((data_json::jsonb->>'advance')::bigint),0)::double precision advance,\n      COALESCE(SUM((data_json::jsonb->>'deposit')::bigint),0)::double precision deposit\n      FROM pms_records WHERE property_id=? AND kind='guest_account'");
        assert_eq!(sums.bind_count, 1);
        assert_eq!(pending.kind, "pms_rd_charge_corrections");
        assert_eq!(pending.sql, "SELECT c.id,c.entry_id,c.guest_account_id,c.status,c.version,c.reason,c.requested_by,c.reviewed_by,c.review_reason,c.reversal_entry_id,c.created_at,\n        e.data_json::jsonb->>'kind' entry_kind,e.data_json::jsonb->>'description' entry_description,(e.data_json::jsonb->>'total')::bigint entry_total\n        FROM pms_charge_corrections c JOIN pms_records e ON e.id=c.entry_id AND e.property_id=c.property_id\n        WHERE c.company_id=? AND c.property_id=? AND c.status='pending' ORDER BY c.created_at DESC LIMIT 100");
        assert_eq!(pending.bind_count, 2);
        assert_eq!(front.kind, "pms_rd_charge_corrections_front");
        assert_eq!(front.sql, "SELECT c.id,c.entry_id,c.guest_account_id,c.status,c.version,c.reason,c.requested_by,c.reviewed_by,c.review_reason,c.reversal_entry_id,c.created_at,\n        e.data_json::jsonb->>'kind' entry_kind,e.data_json::jsonb->>'description' entry_description,(e.data_json::jsonb->>'total')::bigint entry_total\n        FROM pms_charge_corrections c JOIN pms_records e ON e.id=c.entry_id AND e.property_id=c.property_id\n        WHERE c.company_id=? AND c.property_id=? AND c.requested_by=? ORDER BY c.created_at DESC LIMIT 100");
        assert_eq!(front.bind_count, 3);
        for read in [sums, pending, front] {
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_charge_corrections_front_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn two_column_point_reads_match_retired_r46_literals() {
        let token = read_statement(PMS_RD_BOOKING_TOKEN).expect("booking token read");
        let stripe = read_statement(PMS_RD_STRIPE_PAYMENT_KEY).expect("stripe payment key read");
        let property = read_statement(PMS_RD_PROPERTY_SCOPE).expect("property scope read");
        assert_eq!(token.kind, "pms_rd_booking_token");
        assert_eq!(token.sql, "SELECT property_id,reservation_id FROM pms_booking_tokens WHERE token_hash=? AND expires_at>?");
        assert_eq!(token.bind_count, 2);
        assert_eq!(stripe.kind, "pms_rd_stripe_payment_key");
        assert_eq!(stripe.sql, "SELECT id,request_hash FROM pms_stripe_payments WHERE id=?");
        assert_eq!(stripe.bind_count, 1);
        assert_eq!(property.kind, "pms_rd_property_scope");
        assert_eq!(property.sql, "SELECT company_id,branch_id FROM pms_properties WHERE id=?");
        assert_eq!(property.bind_count, 1);
        for read in [token, stripe, property] {
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_booking_token_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn failed_counter_refunds_read_matches_retired_typescript_literal() {
        let read = read_statement(PMS_RD_FAILED_COUNTER_REFUNDS).expect("failed counter refunds read");
        assert_eq!(read.kind, "pms_rd_failed_counter_refunds");
        assert_eq!(read.sql, "SELECT count(*) n FROM pms_stripe_payments\n    WHERE property_id=? AND company_id=? AND (refund_state='failed' OR (refund_state IN ('approved','processing','requires_action') AND last_error IS NOT NULL))\n    AND to_char(to_timestamp(COALESCE(received_at,created_at)/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?");
        assert_eq!(read.bind_count, 4);
        assert_eq!(read.sql.matches('?').count(), 4);
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"));
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE FROM"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("FOR SHARE"));
        assert!(!upper.contains("LOCK TABLE"));
        assert!(is_read(read.kind));
        assert!(!is_read("pms_rd_failed_counter_refunds_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn reservation_property_and_rollout_probe_reads_match_retired_literals() {
        let reservation = read_statement(PMS_RD_RESERVATION_PROPERTY).expect("reservation property read");
        let rollout = read_statement(PMS_RD_ROLLOUT_SETUP_READY).expect("rollout readiness read");
        assert_eq!(reservation.kind, "pms_rd_reservation_property");
        assert_eq!(reservation.sql, "SELECT property_id FROM pms_records WHERE id=? AND kind='reservation'");
        assert_eq!(reservation.bind_count, 1);
        assert_eq!(rollout.kind, "pms_rd_rollout_setup_ready");
        assert_eq!(rollout.sql, "SELECT 1 FROM pms_properties p WHERE p.company_id=? AND p.state='open'\n      AND p.config_json::jsonb->>'currency'='THB'\n      AND COALESCE(p.config_json::jsonb->>'approvedBy','')<>''\n      AND EXISTS(SELECT 1 FROM pms_records r WHERE r.property_id=p.id AND r.kind='room')\n      AND EXISTS(SELECT 1 FROM pms_records r WHERE r.property_id=p.id AND r.kind='rate') LIMIT 1");
        assert_eq!(rollout.bind_count, 1);
        for read in [reservation, rollout] {
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_rollout_setup_ready_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn attachment_owner_reads_match_retired_typescript_literals() {
        let fact = read_statement(PMS_RD_ATTACHMENT_OWNER_FACT).expect("attachment owner fact read");
        let property = read_statement(PMS_RD_ATTACHMENT_PROPERTY_OWNER).expect("attachment property owner read");
        assert_eq!(fact.kind, "pms_rd_attachment_owner_fact");
        assert_eq!(fact.sql, "SELECT p.company_id,p.branch_id,NULL::text department_id,r.version,r.kind\n       FROM pms_records r JOIN pms_properties p ON p.id=r.property_id\n       WHERE r.id=? AND p.company_id=?");
        assert_eq!(fact.bind_count, 2);
        assert_eq!(property.kind, "pms_rd_attachment_property_owner");
        assert_eq!(property.sql, "SELECT company_id,branch_id,NULL::text department_id,version FROM pms_properties WHERE id=? AND company_id=?");
        assert_eq!(property.bind_count, 2);
        for read in [fact, property] {
            assert_eq!(read.sql.matches('?').count(), read.bind_count);
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_attachment_owner_fact_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn dashboard_records_read_matches_retired_typescript_template() {
        let read = read_statement(PMS_RD_DASHBOARD_RECORDS).expect("dashboard records read");
        assert_eq!(read.kind, "pms_rd_dashboard_records");
        assert_eq!(read.bind_count, 4);
        assert_eq!(read.sql.matches('?').count(), 4);
        assert!(read.sql.starts_with("WITH selected_stays AS MATERIALIZED (\n SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='stay' AND (data_json::jsonb->>'state' IN ('reserved','in_house') OR"));
        assert!(read.sql.contains("), scope AS (SELECT ?::text property_id), selected_records AS ("));
        assert!(read.sql.contains(" id IN (SELECT data_json::jsonb->>'guestId' FROM selected_stays)"));
        assert!(read.sql.ends_with(") SELECT * FROM selected_records ORDER BY updated_at,id"));
        let upper = read.sql.to_ascii_uppercase();
        assert!(!upper.contains("INSERT INTO"));
        assert!(!upper.contains("UPDATE "));
        assert!(!upper.contains("DELETE FROM"));
        assert!(!upper.contains("FOR UPDATE"));
        assert!(!upper.contains("FOR SHARE"));
        assert!(!upper.contains("LOCK TABLE"));
        assert!(is_read(read.kind));
        assert!(!is_read("pms_rd_dashboard_records_v2"));
        assert_eq!(READS.len(), 64);
    }

    #[test]
    fn record_detail_and_choices_reads_match_retired_typescript_templates() {
        let detail = read_statement(PMS_RD_RECORD_DETAIL).expect("record detail read");
        let choices = read_statement(PMS_RD_PMS_CHOICES).expect("choices read");
        let block = read_statement(PMS_RD_PMS_CHOICES_BLOCK).expect("block choices read");
        assert_eq!(detail.kind, "pms_rd_record_detail");
        assert_eq!(detail.bind_count, 3);
        assert_eq!(detail.sql.matches('?').count(), 3);
        assert!(detail.sql.starts_with("WITH selected AS (\n      SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id=?"));
        assert!(detail.sql.contains("SELECT id FROM selected WHERE kind='guest_account' UNION SELECT data->>'guestAccountId' FROM selected"));
        assert_eq!(choices.kind, "pms_rd_pms_choices");
        assert_eq!(choices.bind_count, 7);
        assert_eq!(choices.sql.matches('?').count(), 7);
        assert!(choices.sql.starts_with("WITH matched AS (\n  SELECT id FROM pms_records WHERE property_id=? AND kind=? \n"));
        assert_eq!(block.kind, "pms_rd_pms_choices_block");
        assert_eq!(block.bind_count, 7);
        assert_eq!(block.sql.matches('?').count(), 7);
        assert!(block.sql.contains("AND data_json::jsonb->>'kind'='group' AND data_json::jsonb->>'state'='active' AND COALESCE(data_json::jsonb->>'bookingChangeId','')=''"));
        for read in [detail, choices, block] {
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("WITH "));
            assert!(!upper.contains("INSERT INTO"));
            assert!(!upper.contains("UPDATE "));
            assert!(!upper.contains("DELETE FROM"));
            assert!(!upper.contains("FOR UPDATE"));
            assert!(!upper.contains("FOR SHARE"));
            assert!(!upper.contains("LOCK TABLE"));
            assert!(is_read(read.kind));
        }
        assert!(!is_read("pms_rd_pms_choices_v2"));
        assert_eq!(READS.len(), 64);
    }
}
