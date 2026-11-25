//! Pure response types and validation for the native Learning evidence-detail
//! read family. No SQL and no transport live here.
//!
//! The family is the two owner-local single-row reads formerly composed in
//! `adapters/learning.ts` (`nativeLearningExecutiveEvidence`). Both read only
//! Learning-owned tables (`learning_assignment_events`, `learning_assignments`,
//! `learning_courses`) with company/branch/department scope; no cross-owner
//! projection or lock enters this module.

use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceKind {
    Completion,
    Overdue,
}

impl EvidenceKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "learning-completions" | "completion" => Some(Self::Completion),
            "overdue-required-learning" | "overdue" => Some(Self::Overdue),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completion => "completion",
            Self::Overdue => "overdue",
        }
    }
}

/// Bounded transport request. Only scalar identities and an optional millisecond
/// range cross the wire; the caller never supplies SQL, predicates or binds.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceQuery {
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_exclusive: Option<i64>,
}

/// Pinned response shape: the `LearningEvidenceRow` projection. Optional
/// columns are omitted so the completion and overdue branches stay distinct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceRow {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    pub course_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_for: Option<String>,
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEARNING_EVIDENCE_READ_UNAVAILABLE",
        "The native Learning evidence read is unavailable.",
    )
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LEARNING_EVIDENCE_READ_REQUEST_INVALID",
        "Use one bounded Learning evidence read.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "LEARNING_EVIDENCE_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

use erpx_server_shared::public::FailureKind;

/// Validate one read before it reaches the owner reader. Completion requires a
/// closed, ordered millisecond range; overdue forbids one and relies on the
/// owner clock. Company scope is always required and bounded.
pub fn validate(query: &EvidenceQuery, company: &str) -> Result<EvidenceKind, BusinessError> {
    let kind = EvidenceKind::parse(&query.kind).ok_or_else(invalid)?;
    if company.is_empty() || company.len() > 200 {
        return Err(unavailable());
    }
    if query.id.is_empty() || query.id.len() > 200 {
        return Err(invalid());
    }
    match kind {
        EvidenceKind::Completion => match (query.from, query.to_exclusive) {
            (Some(from), Some(to)) if from < to => Ok(kind),
            _ => Err(invalid()),
        },
        EvidenceKind::Overdue => {
            if query.from.is_some() || query.to_exclusive.is_some() {
                return Err(invalid());
            }
            Ok(kind)
        }
    }
}

/// Opaque continuation values from the caller's already-parsed, validated
/// cursor. No SQL or SQL fragment crosses the wire.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCursor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    pub id: String,
}

/// Bounded search request for the owner-local evidence list family. The TS
/// caller supplies only scalar filters and a parsed cursor; the owner owns the
/// predicates, projection, ordering and company scope.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSearchQuery {
    pub kind: String,
    #[serde(default)]
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_exclusive: Option<i64>,
    pub limit: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<EvidenceCursor>,
}

fn valid_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

/// Validate one bounded search before it reaches the reader. The completion
/// branch needs a closed millisecond range and a `createdAt` cursor; the
/// overdue branch uses the owner clock and a `dueDate` cursor.
pub fn validate_search(
    query: &EvidenceSearchQuery,
    company: &str,
) -> Result<EvidenceKind, BusinessError> {
    let kind = EvidenceKind::parse(&query.kind).ok_or_else(invalid)?;
    if company.is_empty() || company.len() > 200 {
        return Err(unavailable());
    }
    if query.query.len() > 100 || !(1..=50).contains(&query.limit) {
        return Err(invalid());
    }
    match kind {
        EvidenceKind::Completion => {
            match (query.from, query.to_exclusive) {
                (Some(from), Some(to)) if from < to => {}
                _ => return Err(invalid()),
            }
            match &query.cursor {
                None => Ok(kind),
                Some(cursor)
                    if cursor.created_at.is_some()
                        && cursor.due_date.is_none()
                        && !cursor.id.is_empty()
                        && cursor.id.len() <= 200 =>
                {
                    Ok(kind)
                }
                _ => Err(invalid()),
            }
        }
        EvidenceKind::Overdue => {
            if query.from.is_some() || query.to_exclusive.is_some() {
                return Err(invalid());
            }
            match &query.cursor {
                None => Ok(kind),
                Some(cursor)
                    if cursor.due_date.as_deref().is_some_and(valid_date)
                        && cursor.created_at.is_none()
                        && !cursor.id.is_empty()
                        && cursor.id.len() <= 200 =>
                {
                    Ok(kind)
                }
                _ => Err(invalid()),
            }
        }
    }
}

/// A registered Holding target: exact request Host and Holding path slug. The
/// selected Company/branch/department arrive as single-valued headers and are
/// never inferred from the body.
#[derive(Clone, Debug)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(kind: &str) -> EvidenceQuery {
        EvidenceQuery {
            kind: kind.to_owned(),
            id: "event-1".to_owned(),
            from: None,
            to_exclusive: None,
        }
    }

    #[test]
    fn validates_each_branch() {
        let mut completion = query("learning-completions");
        assert_eq!(validate(&completion, "co-1"), Err(invalid()));
        completion.from = Some(10);
        completion.to_exclusive = Some(20);
        assert_eq!(validate(&completion, "co-1"), Ok(EvidenceKind::Completion));
        completion.to_exclusive = Some(10);
        assert_eq!(validate(&completion, "co-1"), Err(invalid()));

        let overdue = query("overdue-required-learning");
        assert_eq!(validate(&overdue, "co-1"), Ok(EvidenceKind::Overdue));

        assert_eq!(validate(&query("unknown"), "co-1"), Err(invalid()));
        assert_eq!(validate(&overdue, ""), Err(unavailable()));
    }

    fn search(kind: &str) -> EvidenceSearchQuery {
        EvidenceSearchQuery {
            kind: kind.to_owned(),
            query: String::new(),
            from: None,
            to_exclusive: None,
            limit: 25,
            cursor: None,
        }
    }

    #[test]
    fn validates_search_families() {
        let mut completion = search("learning-completions");
        completion.from = Some(10);
        completion.to_exclusive = Some(20);
        assert_eq!(
            validate_search(&completion, "co-1"),
            Ok(EvidenceKind::Completion)
        );
        completion.cursor = Some(EvidenceCursor {
            created_at: Some(15),
            due_date: None,
            id: "event-1".into(),
        });
        assert_eq!(
            validate_search(&completion, "co-1"),
            Ok(EvidenceKind::Completion)
        );
        completion.cursor = Some(EvidenceCursor {
            created_at: None,
            due_date: Some("2026-11-25".into()),
            id: "event-1".into(),
        });
        assert_eq!(validate_search(&completion, "co-1"), Err(invalid()));

        let overdue = search("overdue-required-learning");
        assert_eq!(validate_search(&overdue, "co-1"), Ok(EvidenceKind::Overdue));
        let mut overdue_page = overdue;
        overdue_page.cursor = Some(EvidenceCursor {
            created_at: None,
            due_date: Some("2026-11-25".into()),
            id: "assignment-1".into(),
        });
        assert_eq!(
            validate_search(&overdue_page, "co-1"),
            Ok(EvidenceKind::Overdue)
        );
        overdue_page.cursor = Some(EvidenceCursor {
            created_at: None,
            due_date: Some("not-a-date".into()),
            id: "assignment-1".into(),
        });
        assert_eq!(validate_search(&overdue_page, "co-1"), Err(invalid()));

        let mut too_many = search("learning-completions");
        too_many.limit = 51;
        assert_eq!(validate_search(&too_many, "co-1"), Err(invalid()));
    }

    #[test]
    fn pins_response_shape() {
        let row = EvidenceRow {
            id: "e1".into(),
            assignment_id: Some("a1".into()),
            event_version: Some(2),
            version: None,
            status: "completed".into(),
            created_at: Some(1_700_000_000_000),
            due_date: None,
            updated_at: None,
            course_title: "Safety".into(),
            required_for: None,
        };
        let value = serde_json::to_value(&row).unwrap();
        assert_eq!(value["id"], "e1");
        assert_eq!(value["assignmentId"], serde_json::Value::Null);
        assert_eq!(value["assignment_id"], "a1");
        assert_eq!(value["event_version"], 2);
        assert!(value.get("version").is_none());
        assert_eq!(value["course_title"], "Safety");
    }
}
