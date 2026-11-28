//! Native POS day-control read kinds. These closed, `SELECT`-only owner
//! statements are the runtime authority for the TypeScript day-control bridge
//! (`pms-day-control-facts.ts`). The bridge sends a typed kind and positional
//! values; this module selects the SQL. There is no TypeScript SQL, mirror or
//! fallback for this family. The integration oracle imports these statements so
//! the runtime registry and the parity test cannot drift apart.

pub const OPEN_SHIFTS: &str = "pos_daycontrol_open_shifts";
pub const UNREVIEWED_VARIANCES: &str = "pos_daycontrol_unreviewed_variances";
pub const OPEN_BILLS: &str = "pos_daycontrol_open_bills";
pub const PENDING_PAYMENTS: &str = "pos_daycontrol_pending_payments";
pub const UNRESOLVED_REFUNDS: &str = "pos_daycontrol_unresolved_refunds";
pub const FAILED_JOBS: &str = "pos_daycontrol_failed_jobs";
pub const MISSING_ROOM_CHARGES: &str = "pos_daycontrol_missing_room_charges";
pub const TOTALS: &str = "pos_daycontrol_totals";
pub const STOCK_MOVES: &str = "pos_daycontrol_stock_moves";
pub const PENDING_REVERSALS: &str = "pos_daycontrol_pending_reversals";

pub const KINDS: &[&str] = &[
    OPEN_SHIFTS,
    UNREVIEWED_VARIANCES,
    OPEN_BILLS,
    PENDING_PAYMENTS,
    UNRESOLVED_REFUNDS,
    FAILED_JOBS,
    MISSING_ROOM_CHARGES,
    TOTALS,
    STOCK_MOVES,
    PENDING_REVERSALS,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

const PMS_SOURCES: &str = "SELECT \"property_id\",\"source_key\",\"record_id\" FROM pms_sources";
const PMS_POSTINGS: &str =
    "SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings";
const PMS_RECORDS: &str =
    "SELECT \"id\",\"property_id\",\"kind\",\"data_json\",\"version\",\"updated_at\" FROM pms_records";

/// Authoritative SQL for the ten day-control kinds, byte-identical to the
/// retired TypeScript literals and the integration oracle. Cross-owner
/// PMS/Commerce fragments are inlined closed; the caller supplies only
/// positional Company/branch/property/date values on its serializable unit.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        OPEN_SHIFTS => "SELECT count(*) n FROM pos_shifts WHERE company_id=? AND branch_id=? AND register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND state='open'",
        UNREVIEWED_VARIANCES => "SELECT count(*) n FROM pos_shifts WHERE company_id=? AND branch_id=? AND register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND state='closed' AND COALESCE(variance_minor,0)<>0",
        OPEN_BILLS => "SELECT count(*) n FROM pos_sales WHERE company_id=? AND branch_id=? AND business_date=? AND register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND state IN ('open','held','payment_pending')",
        PENDING_PAYMENTS => "SELECT count(*) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND p.state IN ('requested','pending','exception')",
        UNRESOLVED_REFUNDS => "SELECT count(*) n FROM pos_refunds WHERE company_id=? AND branch_id=? AND accounting_date=? AND sale_id IN (SELECT s.id FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE r.property_id=?) AND state IN ('requested','approved','processing','exception')",
        FAILED_JOBS => "SELECT count(*) n FROM pos_jobs WHERE company_id=? AND EXISTS(SELECT 1 FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE r.property_id=? AND (s.id=pos_jobs.record_id OR EXISTS(SELECT 1 FROM pos_payments pay WHERE pay.sale_id=s.id AND pay.id=pos_jobs.record_id) OR EXISTS(SELECT 1 FROM pos_refunds refund WHERE refund.sale_id=s.id AND refund.id=pos_jobs.record_id) OR EXISTS(SELECT 1 FROM pos_refund_payments rp JOIN pos_refunds rf ON rf.id=rp.refund_id WHERE rp.id=pos_jobs.record_id AND rf.sale_id=s.id))) AND state='exception' AND kind NOT IN ('connect','connect_refresh')",
        MISSING_ROOM_CHARGES => {
            // The retired literal contains embedded newlines; keep them byte-for-byte.
            const SQL: &str = concat!(
                "SELECT count(*) n FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND s.state='completed' AND s.invoice_id IS NULL\n",
                "      AND NOT EXISTS(SELECT 1 FROM pos_room_charge_reversals rr WHERE rr.sale_id=s.id)\n",
                "      AND NOT EXISTS(SELECT 1 FROM (SELECT \"property_id\",\"source_key\",\"record_id\" FROM pms_sources) ps JOIN (SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings) pp ON pp.property_id=ps.property_id AND pp.record_id=ps.record_id\n",
                "        JOIN (SELECT \"id\",\"property_id\",\"kind\",\"data_json\",\"version\",\"updated_at\" FROM pms_records) e ON e.id=ps.record_id AND e.property_id=ps.property_id AND e.kind='entry'\n",
                "        WHERE ps.property_id=? AND ps.source_key='pos:'||s.id||':'||s.id AND (e.data_json::jsonb->>'total')::bigint=s.total_minor)",
            );
            let _ = (PMS_SOURCES, PMS_POSTINGS, PMS_RECORDS);
            SQL
        }
        TOTALS => concat!(
            "SELECT\n",
            "    COALESCE(SUM(CASE WHEN s.invoice_id IS NOT NULL THEN s.total_minor ELSE 0 END),0) direct_total,\n",
            "    COALESCE(SUM(CASE WHEN s.invoice_id IS NULL THEN s.total_minor ELSE 0 END),0) room_total,\n",
            "    COALESCE(SUM((SELECT SUM(p.amount_minor) FROM pos_payments p WHERE p.sale_id=s.id AND p.method='cash' AND p.state='succeeded')),0) cash_received,\n",
            "    COALESCE(SUM((SELECT SUM(p.amount_minor) FROM pos_payments p WHERE p.sale_id=s.id AND p.method='transfer' AND p.state='succeeded')),0) transfer_verified\n",
            "    FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND s.state='completed'\n",
            "      AND NOT EXISTS(SELECT 1 FROM pos_room_charge_reversals rr WHERE rr.sale_id=s.id)",
        ),
        STOCK_MOVES => {
            const SQL: &str = concat!(
                "SELECT COALESCE(SUM(m.value_minor),0) n FROM pos_stock_moves m JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses) w ON w.id=m.warehouse_id\n",
                "    WHERE m.company_id=? AND w.branch_id=? AND m.movement_date=? AND EXISTS(SELECT 1 FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE r.property_id=? AND (s.id=m.source_id OR EXISTS(SELECT 1 FROM pos_lines l WHERE l.sale_id=s.id AND l.fulfillment_id=m.fulfillment_id)))",
            );
            SQL
        }
        PENDING_REVERSALS => "SELECT count(*) n FROM pos_room_charge_reversal_requests WHERE company_id=? AND property_id=? AND state IN ('requested','pos_approved')",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_control_kinds_are_select_only_and_parameterized() {
        assert_eq!(KINDS.len(), 10);
        for kind in KINDS {
            assert!(supported(kind), "kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT"), "kind must be SELECT-only: {kind}");
            assert!(stmt.contains('?'), "kind must be parameterized: {kind}");
            for forbidden in ["INSERT", "DELETE", "RETURNING", ";"] {
                assert!(!stmt.contains(forbidden), "kind must not mutate: {kind} contains {forbidden}");
            }
            assert!(!stmt.contains("UPDATE "), "kind must not mutate: {kind}");
        }
    }

    #[test]
    fn unknown_day_control_kind_fails_closed() {
        assert!(!supported("pos_daycontrol_unlisted"));
        assert!(sql("pos_daycontrol_unlisted").is_none());
    }
}
