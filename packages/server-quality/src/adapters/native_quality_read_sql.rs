//! Native Quality read-plan registry for the mounted `/quality/read-plan` route.
//!
//! Six closed read kinds: the alert-to-hold point lookup and the hold/spec
//! existence probes used by the retained purchase gates (all unscoped, exactly
//! one caller-supplied bind), the two attachment owner CAS guards (three binds,
//! returned as an `EXISTS(...)` wrapper for the caller's command assertion),
//! and the Company-scoped Quality attachment descriptor (the exact statement
//! owned by the fenced `inventory_quality_holds` descriptor port). The caller
//! executes the returned statement on its own already-open unit, so the
//! uncommitted state of the caller's transaction stays visible and no
//! TypeScript SQL mirror remains.

/// Allow-listed native Quality read kind: alert id to owning hold id.
pub const RD_ALERT_HOLD_ID: &str = "quality_rd_alert_hold_id";

/// Allow-listed native Quality read kind: unscoped hold id existence probe.
pub const RD_HOLD_EXISTS: &str = "quality_rd_hold_exists";

/// Allow-listed native Quality read kind: unscoped spec id existence probe.
pub const RD_SPEC_EXISTS: &str = "quality_rd_spec_exists";

/// Allow-listed native Quality read kind: hold attachment version CAS guard.
pub const RD_ATTACH_HOLD_CAS: &str = "quality_rd_attach_hold_cas";

/// Allow-listed native Quality read kind: spec attachment version CAS guard.
pub const RD_ATTACH_SPEC_CAS: &str = "quality_rd_attach_spec_cas";

/// Allow-listed native Quality read kind: Company-scoped hold descriptor.
pub const RD_ATTACHMENT_DESCRIPTOR: &str = "quality_rd_attachment_descriptor";

/// Exact statement retired from `purchases.ts`. Byte parity is pinned by the
/// owner Rust test and the TypeScript read-tail r28 witness.
const SQL_ALERT_HOLD_ID: &str = "SELECT hold_id FROM inventory_quality_alerts WHERE id=?";

/// Exact unaliased hold probe retired from the shared TypeScript id-exists helper.
const SQL_HOLD_EXISTS: &str = "SELECT 1 FROM inventory_quality_holds WHERE id=?";

/// Exact unaliased spec probe retired from the shared TypeScript id-exists helper.
const SQL_SPEC_EXISTS: &str = "SELECT 1 FROM inventory_quality_specs WHERE id=?";

/// Exact three-bind CAS wrapper retired from the service attachment facts.
const SQL_ATTACH_HOLD_CAS: &str = "EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=?)";

/// Exact three-bind CAS wrapper retired from the operational attachment facts.
const SQL_ATTACH_SPEC_CAS: &str = "EXISTS(SELECT 1 FROM inventory_quality_specs WHERE id=? AND company_id=? AND version=?)";

/// Exact two-bind descriptor statement retired from `policyQualityFacts`. It is
/// the fenced owner port's capture statement with `$1/$2` rewritten to `?`;
/// a Rust test pins that byte identity against `DESCRIPTOR_CAPTURE_SQL`.
const SQL_ATTACHMENT_DESCRIPTOR: &str = "SELECT CASE WHEN octet_length(company_id)+coalesce(octet_length(branch_id),0)<=2097152 THEN company_id END company_id,CASE WHEN octet_length(company_id)+coalesce(octet_length(branch_id),0)<=2097152 THEN branch_id END branch_id,NULL::text department_id,version::bigint version FROM inventory_quality_holds WHERE id=? AND company_id=?";

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

const READ_HOLD_EXISTS: QualityReadStatement = QualityReadStatement {
    kind: RD_HOLD_EXISTS,
    sql: SQL_HOLD_EXISTS,
    bind_count: 1,
};

const READ_SPEC_EXISTS: QualityReadStatement = QualityReadStatement {
    kind: RD_SPEC_EXISTS,
    sql: SQL_SPEC_EXISTS,
    bind_count: 1,
};

const READ_ATTACH_HOLD_CAS: QualityReadStatement = QualityReadStatement {
    kind: RD_ATTACH_HOLD_CAS,
    sql: SQL_ATTACH_HOLD_CAS,
    bind_count: 3,
};

const READ_ATTACH_SPEC_CAS: QualityReadStatement = QualityReadStatement {
    kind: RD_ATTACH_SPEC_CAS,
    sql: SQL_ATTACH_SPEC_CAS,
    bind_count: 3,
};

const READ_ATTACHMENT_DESCRIPTOR: QualityReadStatement = QualityReadStatement {
    kind: RD_ATTACHMENT_DESCRIPTOR,
    sql: SQL_ATTACHMENT_DESCRIPTOR,
    bind_count: 2,
};

/// Resolve one allow-listed kind. No SQL is accepted from the wire.
pub fn read_statement(kind: &str) -> Option<&'static QualityReadStatement> {
    match kind {
        RD_ALERT_HOLD_ID => Some(&READ_ALERT_HOLD_ID),
        RD_HOLD_EXISTS => Some(&READ_HOLD_EXISTS),
        RD_SPEC_EXISTS => Some(&READ_SPEC_EXISTS),
        RD_ATTACH_HOLD_CAS => Some(&READ_ATTACH_HOLD_CAS),
        RD_ATTACH_SPEC_CAS => Some(&READ_ATTACH_SPEC_CAS),
        RD_ATTACHMENT_DESCRIPTOR => Some(&READ_ATTACHMENT_DESCRIPTOR),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planned(kind: &str, sql: &str, binds: usize) {
        let plan = read_statement(kind).expect("registered read kind");
        assert_eq!(plan.kind, kind);
        assert_eq!(plan.sql, sql);
        assert_eq!(plan.bind_count, binds);
        assert_eq!(plan.sql.matches('?').count(), plan.bind_count);
        assert!(plan.sql.contains("FROM inventory_quality_"));
        assert!(!plan.sql.contains("FOR UPDATE"));
        assert!(!plan.sql.contains("LOCK "));
    }

    #[test]
    fn alert_hold_id_read_plan_is_exact() {
        planned(RD_ALERT_HOLD_ID, SQL_ALERT_HOLD_ID, 1);
        assert!(SQL_ALERT_HOLD_ID.starts_with("SELECT "));
        assert!(!SQL_ALERT_HOLD_ID.contains("company_id"));
        assert!(!SQL_ALERT_HOLD_ID.contains("branch_id"));
    }

    #[test]
    fn hold_and_spec_existence_read_plans_are_exact() {
        planned(RD_HOLD_EXISTS, SQL_HOLD_EXISTS, 1);
        planned(RD_SPEC_EXISTS, SQL_SPEC_EXISTS, 1);
        for sql in [SQL_HOLD_EXISTS, SQL_SPEC_EXISTS] {
            assert!(sql.starts_with("SELECT 1 FROM "));
            assert!(!sql.contains("company_id"));
            assert!(!sql.contains("branch_id"));
            assert!(!sql.contains(" IN ("));
        }
    }

    #[test]
    fn attachment_cas_guard_read_plans_are_exact() {
        planned(RD_ATTACH_HOLD_CAS, SQL_ATTACH_HOLD_CAS, 3);
        planned(RD_ATTACH_SPEC_CAS, SQL_ATTACH_SPEC_CAS, 3);
        for sql in [SQL_ATTACH_HOLD_CAS, SQL_ATTACH_SPEC_CAS] {
            assert!(sql.starts_with("EXISTS(SELECT 1 FROM "));
            assert!(sql.ends_with(')'));
            assert!(sql.contains(" WHERE id=? AND company_id=? AND version=?)"));
        }
    }

    #[test]
    fn attachment_descriptor_read_plan_is_exact() {
        planned(RD_ATTACHMENT_DESCRIPTOR, SQL_ATTACHMENT_DESCRIPTOR, 2);
        assert!(SQL_ATTACHMENT_DESCRIPTOR.starts_with("SELECT CASE WHEN octet_length(company_id)"));
        assert!(SQL_ATTACHMENT_DESCRIPTOR.contains("NULL::text department_id,version::bigint version"));
        assert!(SQL_ATTACHMENT_DESCRIPTOR.ends_with("FROM inventory_quality_holds WHERE id=? AND company_id=?"));
        assert_eq!(
            SQL_ATTACHMENT_DESCRIPTOR,
            crate::adapters::attachment_descriptor::DESCRIPTOR_CAPTURE_SQL
                .replace("$1", "?")
                .replace("$2", "?")
        );
    }

    #[test]
    fn unknown_kinds_fail_closed() {
        for kind in [
            "unknown_quality_read",
            "quality_rd_hold_exists ",
            "QUALITY_RD_SPEC_EXISTS",
            "",
        ] {
            assert!(read_statement(kind).is_none());
        }
    }
}
