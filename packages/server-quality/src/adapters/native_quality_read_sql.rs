//! Native Quality read-plan registry for the mounted `/quality/read-plan` route.
//!
//! One closed read kind: the alert-to-hold point lookup used by the retained
//! purchase gates. It is deliberately unscoped (no Company or branch predicate)
//! and takes exactly one caller-supplied alert-id bind. The caller executes the
//! returned statement on its own already-open unit, so the uncommitted state of
//! the purchase transaction stays visible and no TypeScript SQL mirror remains.

/// Allow-listed native Quality read kind: alert id to owning hold id.
pub const RD_ALERT_HOLD_ID: &str = "quality_rd_alert_hold_id";

/// Exact statement retired from `purchases.ts`. Byte parity is pinned by the
/// owner Rust test and the TypeScript read-tail r28 witness.
const SQL_ALERT_HOLD_ID: &str = "SELECT hold_id FROM inventory_quality_alerts WHERE id=?";

pub struct QualityReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

const READ_ALERT_HOLD_ID: QualityReadStatement = QualityReadStatement {
    kind: RD_ALERT_HOLD_ID,
    sql: SQL_ALERT_HOLD_ID,
    bind_count: 1,
};

/// Resolve one allow-listed kind. No SQL is accepted from the wire.
pub fn read_statement(kind: &str) -> Option<&'static QualityReadStatement> {
    match kind {
        RD_ALERT_HOLD_ID => Some(&READ_ALERT_HOLD_ID),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_hold_id_read_plan_is_exact() {
        let plan = read_statement(RD_ALERT_HOLD_ID).expect("registered read kind");
        assert_eq!(plan.kind, RD_ALERT_HOLD_ID);
        assert_eq!(plan.sql, SQL_ALERT_HOLD_ID);
        assert_eq!(plan.bind_count, 1);
        assert_eq!(plan.sql.matches('?').count(), plan.bind_count);
        assert!(plan.sql.starts_with("SELECT "));
        assert!(!plan.sql.contains("FOR UPDATE"));
        assert!(!plan.sql.contains("LOCK "));
        assert!(read_statement("unknown_quality_read").is_none());
    }
}
