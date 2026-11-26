//! Pure domain for the native Talent application-detail read.
//!
//! This module owns the exact response shape, the wire bounds and the
//! owner-authored Postgres projections. It emits no connection, no sqlx call
//! and performs no IO: every statement here is executed only by the Postgres
//! adapter. The projections are the byte-equivalent successors of the retired
//! TypeScript `nativeTalentDetail` plan reads, with Postgres bind markers
//! (`$1`..`$4`) in place of the TypeScript `?` placeholders.
//!
//! The four owner-local row families (application, history, proposal, offer,
//! hire handoff) are the complete payload produced by the owner application
//! `createTalentDetail`. Cross-owner presentation fields (owners, purchases,
//! offer-document editability, canWrite) stay with their own owners and are
//! added by composition, never by a TypeScript fallback in this owner.

use erpx_server_shared::public::BusinessError;
use serde::Serialize;
use serde_json::Value;

pub const MAX_IDENTIFIER: usize = 200;

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "TALENT_DETAIL_INVALID",
        "Use a bounded Talent application detail read.",
    )
}

pub fn application_not_found() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::NotFound,
        "TALENT_APPLICATION_NOT_FOUND",
        "Application not found.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "NATIVE_TALENT_DETAIL_UNAVAILABLE",
        "The native Talent detail read is unavailable.",
    )
}

/// Validate a wire-supplied record identifier before it reaches SQL.
pub fn validate_id(value: &str) -> Result<(), BusinessError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER
        || value.contains('\u{0000}')
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
        })
    {
        return Err(invalid());
    }
    Ok(())
}

/// Validate the live caller's resolved Company/scope facts before SQL.
pub fn validate_optional_scope(value: Option<&str>) -> Result<(), BusinessError> {
    match value {
        None => Ok(()),
        Some(value) => {
            if value.is_empty()
                || value.len() > MAX_IDENTIFIER
                || value.contains('\u{0000}')
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            {
                Err(invalid())
            } else {
                Ok(())
            }
        }
    }
}

/// Native successor of the TypeScript `nativeTalentDetail` application row
/// read. `to_jsonb(a)` supplies every `talent_applications` column (the former
/// `a.*`), and `jsonb_build_object` supplies the exact seven computed aliases
/// the retired literal projected from the requisition, candidate and IAM
/// assignment witness.
pub const DETAIL_APPLICATION_SQL: &str = concat!(
    "SELECT COALESCE((",
    "SELECT to_jsonb(a) || jsonb_build_object(",
    "'job_title',r.job_title,",
    "'branch_id',r.branch_id,",
    "'department_id',r.department_id,",
    "'candidate_name',c.legal_name,",
    "'candidate_email',c.email,",
    "'retention_until',c.retention_until,",
    "'candidate_version',c.version,",
    "'owner_eligible',CASE WHEN EXISTS(SELECT 1 FROM (SELECT id,user_id,role,company_id,scope_kind,scope_node_id,revoked_at FROM assignments) x ",
    "WHERE x.company_id=a.company_id AND x.user_id=a.owner_id ",
    "AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ",
    "AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ",
    "AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=r.branch_id) ",
    "OR (x.scope_kind='department' AND x.scope_node_id=r.department_id))) THEN 1 ELSE 0 END",
    ") FROM talent_applications a ",
    "JOIN talent_requisitions r ON r.id=a.requisition_id ",
    "JOIN talent_candidates c ON c.id=a.candidate_id ",
    "WHERE a.id=$1 AND a.company_id=$2 ",
    "AND ($3::text IS NULL OR r.branch_id=$3) ",
    "AND ($4::text IS NULL OR r.department_id=$4)",
    "),'null'::jsonb)::text AS row_json"
);

/// Native successor of the `talent_rd_application_events` history read.
pub const DETAIL_HISTORY_SQL: &str = concat!(
    "SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY e.version),'[]'::jsonb)::text AS rows_json ",
    "FROM talent_application_events e WHERE e.application_id=$1"
);

/// Native successor of the `talent_rd_proposals` read. `application_id` is
/// unique, so the subquery yields at most one row.
pub const DETAIL_PROPOSAL_SQL: &str = concat!(
    "SELECT COALESCE((SELECT to_jsonb(p) FROM talent_proposals p ",
    "WHERE p.application_id=$1),'null'::jsonb)::text AS row_json"
);

/// Native successor of the `talent_rd_offers` read. `application_id` is
/// unique, so the subquery yields at most one row.
pub const DETAIL_OFFER_SQL: &str = concat!(
    "SELECT COALESCE((SELECT to_jsonb(o) FROM talent_offers o ",
    "WHERE o.application_id=$1),'null'::jsonb)::text AS row_json"
);

/// Native successor of the `talent_rd_hire_handoffs` read. `offer_id` is
/// unique, so the subquery yields at most one row.
pub const DETAIL_HANDOFF_SQL: &str = concat!(
    "SELECT COALESCE((SELECT to_jsonb(h) FROM talent_hire_handoffs h ",
    "WHERE h.offer_id=$1),'null'::jsonb)::text AS row_json"
);

/// Complete owner-local payload returned by the retired TypeScript
/// `talentDetail` application operation. Field names are intentionally
/// camelCase to match the wire payload exactly; row maps keep the raw
/// snake_case SQL column keys, as the TypeScript operation returned them.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TalentDetail {
    pub application: Value,
    pub history: Vec<Value>,
    pub proposal: Value,
    pub offer: Value,
    pub handoff: Value,
    pub actor_id: String,
    pub can_review: bool,
    pub can_manage_documents: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_and_scope_are_bounded_before_sql() {
        assert!(validate_id("talent-application-1").is_ok());
        assert!(validate_id("bad id!").is_err());
        assert!(validate_id("").is_err());
        assert!(validate_optional_scope(None).is_ok());
        assert!(validate_optional_scope(Some("branch-a")).is_ok());
        assert!(validate_optional_scope(Some("bad branch!")).is_err());
    }

    #[test]
    fn every_projection_is_owner_local_select_only_with_postgres_binds() {
        for sql in [
            DETAIL_APPLICATION_SQL,
            DETAIL_HISTORY_SQL,
            DETAIL_PROPOSAL_SQL,
            DETAIL_OFFER_SQL,
            DETAIL_HANDOFF_SQL,
        ] {
            assert!(sql.starts_with("SELECT "), "detail read must be SELECT-only: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
                assert!(!sql.contains(forbidden), "detail read must not mutate: {forbidden}");
            }
            assert!(sql.contains("talent_"), "detail read must stay on owner tables: {sql}");
            assert!(!sql.contains('?'), "bind markers must be Postgres positional: {sql}");
        }
        assert_eq!(DETAIL_APPLICATION_SQL.matches("$1").count(), 1);
        assert_eq!(DETAIL_APPLICATION_SQL.matches("$2").count(), 1);
        assert_eq!(DETAIL_APPLICATION_SQL.matches("$3").count(), 2);
        assert_eq!(DETAIL_APPLICATION_SQL.matches("$4").count(), 2);
        assert!(DETAIL_APPLICATION_SQL.contains("'owner_eligible'"));
        assert!(DETAIL_APPLICATION_SQL.contains("iam_assignment_active"));
        assert!(DETAIL_HISTORY_SQL.contains("jsonb_agg"));
    }
}
