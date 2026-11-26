//! Pure domain for the native Talent executive-evidence point read.
//!
//! This module owns the wire bounds, the metric discriminator and the exact
//! owner-authored Postgres projections. It emits no connection, no sqlx call
//! and performs no IO: every statement here is executed only by the Postgres
//! adapter. The two projections are the byte-equivalent successors of the
//! retired TypeScript `nativeTalentExecutiveEvidence` plan reads
//! (`talent_rd_exec_requisition` and `talent_rd_exec_application`), with the
//! `?` placeholders replaced by Postgres positional binds (`$1`..`$6`).
//!
//! The retired TypeScript application returned the raw owner row (or `null`)
//! without deriving any field. This slice preserves that shape exactly: the
//! adapter returns the row as the `data` member and never adds cross-owner
//! presentation fields.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;
use serde_json::Value;

pub const MAX_IDENTIFIER: usize = 200;
pub const METRIC_OPEN_REQUISITIONS: &str = "open-requisitions";
pub const METRIC_APPLICATION_STAGE_COUNTS: &str = "application-stage-counts";

/// The only two Talent metrics with an owner-evidence point read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceMetric {
    OpenRequisitions,
    ApplicationStageCounts,
}

impl EvidenceMetric {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            METRIC_OPEN_REQUISITIONS => Ok(Self::OpenRequisitions),
            METRIC_APPLICATION_STAGE_COUNTS => Ok(Self::ApplicationStageCounts),
            _ => Err(invalid()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenRequisitions => METRIC_OPEN_REQUISITIONS,
            Self::ApplicationStageCounts => METRIC_APPLICATION_STAGE_COUNTS,
        }
    }

    /// The application-stage metric is the only one bound to an update window.
    pub fn requires_range(self) -> bool {
        matches!(self, Self::ApplicationStageCounts)
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "TALENT_EVIDENCE_INVALID",
        "Use a bounded Talent executive-evidence read.",
    )
}

/// Parity with `createTalentExecutiveEvidence`: the application metric has no
/// meaning without an update range.
pub fn range_required() -> BusinessError {
    BusinessError::invalid(
        "EVIDENCE_FILTER_REQUIRED",
        "An application update date range is required.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_TALENT_EVIDENCE_UNAVAILABLE",
        "The native Talent evidence read is unavailable.",
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

/// Native successor of the TypeScript `talent_rd_exec_requisition` plan read.
/// The caller binds `[id, company_id, branch_id, department_id]` on its unit.
/// `to_jsonb(x)` supplies every projected column under its original snake_case
/// key, so the wire payload is identical to the former row literal.
pub const EVIDENCE_REQUISITION_SQL: &str = concat!(
    "SELECT COALESCE((",
    "SELECT to_jsonb(x) FROM (",
    "SELECT r.id,r.job_title,r.target_start_date,r.status,r.version,r.updated_at,r.branch_id,r.department_id ",
    "FROM talent_requisitions r WHERE r.id=$1 AND r.company_id=$2 ",
    "AND ($3::text IS NULL OR r.branch_id=$3) AND ($4::text IS NULL OR r.department_id=$4) ",
    "AND r.status='open' LIMIT 1",
    ") x),'null'::jsonb)::text AS row_json"
);

/// Native successor of the TypeScript `talent_rd_exec_application` plan read.
/// The caller binds `[id, company_id, branch_id, department_id, from,
/// to_exclusive]` on its unit. The update window stays an owner-owned
/// predicate; no caller SQL is accepted.
pub const EVIDENCE_APPLICATION_SQL: &str = concat!(
    "SELECT COALESCE((",
    "SELECT to_jsonb(x) FROM (",
    "SELECT a.id,a.requisition_id,a.status,a.version,a.updated_at,r.job_title,r.branch_id,r.department_id ",
    "FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id ",
    "WHERE a.id=$1 AND r.company_id=$2 AND ($3::text IS NULL OR r.branch_id=$3) ",
    "AND ($4::text IS NULL OR r.department_id=$4) AND a.updated_at>=$5 AND a.updated_at<$6 LIMIT 1",
    ") x),'null'::jsonb)::text AS row_json"
);

/// Complete owner-local payload returned by the retired TypeScript
/// `talentExecutiveEvidenceRow` operation. `data` is the raw row (snake_case
/// SQL column keys) or `null`; no field is renamed or derived.
#[derive(Clone, Debug, Serialize)]
pub struct TalentEvidence {
    pub data: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_metrics_and_scope_are_bounded_before_sql() {
        assert!(validate_id("talent-requisition-1").is_ok());
        assert!(validate_id("bad id!").is_err());
        assert!(validate_id("").is_err());
        assert!(validate_optional_scope(None).is_ok());
        assert!(validate_optional_scope(Some("branch-a")).is_ok());
        assert!(validate_optional_scope(Some("bad branch!")).is_err());
        assert_eq!(
            EvidenceMetric::parse(METRIC_OPEN_REQUISITIONS).unwrap(),
            EvidenceMetric::OpenRequisitions
        );
        assert_eq!(
            EvidenceMetric::parse(METRIC_APPLICATION_STAGE_COUNTS).unwrap(),
            EvidenceMetric::ApplicationStageCounts
        );
        assert!(EvidenceMetric::parse("other").is_err());
        assert!(!EvidenceMetric::OpenRequisitions.requires_range());
        assert!(EvidenceMetric::ApplicationStageCounts.requires_range());
    }

    #[test]
    fn every_projection_is_owner_local_select_only_with_postgres_binds() {
        for sql in [EVIDENCE_REQUISITION_SQL, EVIDENCE_APPLICATION_SQL] {
            assert!(sql.starts_with("SELECT "), "evidence read must be SELECT-only: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!sql.contains(forbidden), "evidence read must not mutate: {forbidden}");
            }
            assert!(sql.contains("talent_"), "evidence read must stay on owner tables: {sql}");
            assert!(!sql.contains('?'), "bind markers must be Postgres positional: {sql}");
            assert!(sql.contains("to_jsonb(x)"), "row must keep every projected column: {sql}");
        }
        assert_eq!(EVIDENCE_REQUISITION_SQL.matches("$1").count(), 1);
        assert_eq!(EVIDENCE_REQUISITION_SQL.matches("$4").count(), 2);
        assert!(!EVIDENCE_REQUISITION_SQL.contains("$5"));
        assert_eq!(EVIDENCE_APPLICATION_SQL.matches("$1").count(), 1);
        assert_eq!(EVIDENCE_APPLICATION_SQL.matches("$4").count(), 2);
        assert_eq!(EVIDENCE_APPLICATION_SQL.matches("$5").count(), 1);
        assert_eq!(EVIDENCE_APPLICATION_SQL.matches("$6").count(), 1);
        assert!(EVIDENCE_REQUISITION_SQL.contains("r.status='open'"));
        assert!(EVIDENCE_APPLICATION_SQL.contains("a.updated_at>=$5 AND a.updated_at<$6"));
    }
}
