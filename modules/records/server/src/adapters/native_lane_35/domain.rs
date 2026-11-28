//! Lane 35 domain for the Records work-report read family.
//!
//! Pure response types, input invariants and projection rules. No SQL, no HTTP
//! and no I/O live here. The owner SQL, the Company fence and the transport are
//! in `postgres.rs` and `http.rs` respectively.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Exact registered Holding target the native route accepts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

/// The one bounded read family this lane migrates. `company_id` is also carried
/// in a header and checked against the header by the HTTP adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LaneQuery {
    Candidates {
        #[serde(rename = "companyId")]
        company_id: String,
    },
    Events {
        #[serde(rename = "companyId")]
        company_id: String,
        ids: String,
    },
}

impl LaneQuery {
    pub fn company_id(&self) -> &str {
        match self {
            Self::Candidates { company_id } | Self::Events { company_id, .. } => company_id,
        }
    }
}

/// Strict response row for a work-report candidate. Field order and null
/// canonicalisation are part of the contract and are exercised by the oracle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkReportCandidate {
    pub approval_json: Option<String>,
    pub items_json: Option<String>,
    pub id: String,
    pub kind: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub title: String,
    pub status: String,
    pub requester: String,
    pub branch: Option<String>,
    pub department: Option<String>,
    pub currency: Option<String>,
    pub amount: Option<i64>,
    pub units: Option<i64>,
    pub submitted_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Strict response row for a work-report event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkReportEvent {
    pub record_id: String,
    pub action: String,
    pub actor: String,
    pub at: i64,
    pub reason: String,
}

/// Final owner response envelope. Exactly one key under `data`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaneResponse {
    Candidates(Vec<WorkReportCandidate>),
    Events(Vec<WorkReportEvent>),
}

impl LaneResponse {
    pub fn value(self) -> Value {
        match self {
            Self::Candidates(rows) => serde_json::json!({"data": {"candidates": rows}}),
            Self::Events(rows) => serde_json::json!({"data": {"events": rows}}),
        }
    }
}

/// Raw owner row returned by the candidate statement before Rust projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateRow {
    pub approval_json: Option<String>,
    pub items_json: Option<String>,
    pub id: String,
    pub kind: String,
    pub title: String,
    pub state: String,
    pub requester_user_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub currency: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Raw owner row returned by the submit-time statement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmitTime {
    pub request_id: String,
    pub submitted_at: i64,
}

/// Raw owner row returned by the event statement. `id` orders equal timestamps
/// byte-identically to the legacy `ORDER BY created_at,id` and is not returned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventRow {
    pub id: String,
    pub record_id: String,
    pub action: String,
    pub actor: String,
    pub at: i64,
    pub reason: String,
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_RECORDS_WORK_REPORT_UNAVAILABLE",
        "The native Records work-report read is unavailable.",
    )
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_RECORDS_WORK_REPORT_REQUEST_INVALID",
        "Use one bounded Records work-report read.",
    )
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_RECORDS_WORK_REPORT_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

/// Company scope is admitted here and again as a bind parameter in Postgres.
pub fn validate_company(company: &str) -> Result<(), BusinessError> {
    if valid_id(company) {
        Ok(())
    } else {
        Err(invalid())
    }
}

/// The legacy statement bound a JSON text array. Validate it strictly so an
/// arbitrary SQL string can never be expanded by `$2::jsonb`.
pub fn parse_ids(raw: &str) -> Result<Vec<String>, BusinessError> {
    if raw.is_empty() || raw.len() > 65_536 {
        return Err(invalid());
    }
    let values: Value = serde_json::from_str(raw).map_err(|_| invalid())?;
    let Some(items) = values.as_array() else {
        return Err(invalid());
    };
    if items.len() > 5_000 {
        return Err(invalid());
    }
    let mut ids = Vec::with_capacity(items.len());
    for item in items {
        let Some(id) = item.as_str() else {
            return Err(invalid());
        };
        if !valid_id(id) {
            return Err(invalid());
        }
        ids.push(id.to_owned());
    }
    Ok(ids)
}

/// Rust owns the kind predicate the legacy TS statement pushed into SQL.
pub fn candidate_kind_admitted(kind: &str) -> bool {
    matches!(kind, "purchase" | "service")
}

/// Project raw owner rows into the pinned candidate shape. The submitted-time
/// calculation and the literal/derived fields are owner logic, not TS logic.
pub fn project_candidates(
    rows: Vec<CandidateRow>,
    submits: Vec<SubmitTime>,
) -> Vec<WorkReportCandidate> {
    let mut submitted: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for row in submits {
        submitted.insert(row.request_id, row.submitted_at);
    }
    rows.into_iter()
        .filter(|row| candidate_kind_admitted(&row.kind))
        .map(|row| WorkReportCandidate {
            approval_json: row.approval_json,
            items_json: row.items_json,
            id: row.id.clone(),
            kind: "request".to_owned(),
            record_type: row.kind,
            title: row.title,
            status: row.state,
            requester: row.requester_user_id,
            branch: row.branch_id,
            department: row.department_id,
            currency: row.currency,
            amount: None,
            units: None,
            submitted_at: submitted.get(&row.id).copied(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
        .collect()
}

/// Rust owns the legacy `ORDER BY created_at,id`. Sorting on the raw event id
/// keeps equal-timestamp ordering stable and the id is dropped from the reply.
pub fn project_events(mut rows: Vec<EventRow>) -> Vec<WorkReportEvent> {
    rows.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));
    rows.into_iter()
        .map(|row| WorkReportEvent {
            record_id: row.record_id,
            action: row.action,
            actor: row.actor,
            at: row.at,
            reason: row.reason,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, kind: &str, created: i64) -> CandidateRow {
        CandidateRow {
            approval_json: None,
            items_json: None,
            id: id.to_owned(),
            kind: kind.to_owned(),
            title: "t".to_owned(),
            state: "queued".to_owned(),
            requester_user_id: "u".to_owned(),
            branch_id: None,
            department_id: None,
            currency: None,
            created_at: created,
            updated_at: created + 1,
        }
    }

    #[test]
    fn candidate_projection_filters_kind_and_adds_submitted_at() {
        let rows = vec![
            row("r1", "purchase", 90),
            row("r2", "equipment", 80),
            row("r3", "service", 70),
        ];
        let submits = vec![SubmitTime {
            request_id: "r1".to_owned(),
            submitted_at: 100,
        }];
        let projected = project_candidates(rows, submits);
        assert_eq!(projected.len(), 2);
        assert_eq!(projected[0].id, "r1");
        assert_eq!(projected[0].kind, "request");
        assert_eq!(projected[0].record_type, "purchase");
        assert_eq!(projected[0].submitted_at, Some(100));
        assert_eq!(projected[1].id, "r3");
        assert_eq!(projected[1].submitted_at, None);
    }

    #[test]
    fn event_projection_sorts_by_at_then_id_and_drops_id() {
        let event = |id: &str, at: i64| EventRow {
            id: id.to_owned(),
            record_id: "r1".to_owned(),
            action: "submit".to_owned(),
            actor: "u1".to_owned(),
            at,
            reason: "go".to_owned(),
        };
        let projected = project_events(vec![event("b", 10), event("a", 10), event("c", 5)]);
        assert_eq!(projected.len(), 3);
        assert_eq!(projected[0].at, 5);
        assert_eq!(projected[1].action, "submit");
        assert_eq!(serde_json::to_value(&projected[0]).unwrap().as_object().unwrap().len(), 5);
    }

    #[test]
    fn golden_candidate_result_shape_is_pinned() {
        let rows = vec![
            CandidateRow {
                approval_json: Some("{\"steps\":[]}".to_owned()),
                items_json: Some("[]".to_owned()),
                id: "r1".to_owned(),
                kind: "purchase".to_owned(),
                title: "Desk".to_owned(),
                state: "queued".to_owned(),
                requester_user_id: "u1".to_owned(),
                branch_id: None,
                department_id: None,
                currency: Some("USD".to_owned()),
                created_at: 90,
                updated_at: 110,
            },
            CandidateRow {
                approval_json: None,
                items_json: None,
                id: "r2".to_owned(),
                kind: "service".to_owned(),
                title: "Cleaning".to_owned(),
                state: "draft".to_owned(),
                requester_user_id: "u2".to_owned(),
                branch_id: Some("b1".to_owned()),
                department_id: None,
                currency: None,
                created_at: 200,
                updated_at: 210,
            },
        ];
        let submits = vec![SubmitTime {
            request_id: "r1".to_owned(),
            submitted_at: 100,
        }];
        let value = serde_json::to_value(project_candidates(rows, submits)).unwrap();
        assert_eq!(
            value,
            serde_json::json!([
                {
                    "approval_json": "{\"steps\":[]}", "items_json": "[]", "id": "r1",
                    "kind": "request", "type": "purchase", "title": "Desk", "status": "queued",
                    "requester": "u1", "branch": null, "department": null, "currency": "USD",
                    "amount": null, "units": null, "submitted_at": 100,
                    "created_at": 90, "updated_at": 110
                },
                {
                    "approval_json": null, "items_json": null, "id": "r2",
                    "kind": "request", "type": "service", "title": "Cleaning", "status": "draft",
                    "requester": "u2", "branch": "b1", "department": null, "currency": null,
                    "amount": null, "units": null, "submitted_at": null,
                    "created_at": 200, "updated_at": 210
                }
            ])
        );
    }

    #[test]
    fn ids_are_a_strict_json_array() {
        assert!(parse_ids("[\"r1\",\"r2\"]").is_ok());
        assert!(parse_ids("r1").is_err());
        assert!(parse_ids("[\"bad id\"]").is_err());
        assert!(parse_ids("[1]").is_err());
    }

    #[test]
    fn company_scope_is_bounded() {
        assert!(validate_company("acme-co_1").is_ok());
        assert!(validate_company("").is_err());
        assert!(validate_company("bad id").is_err());
    }
}
