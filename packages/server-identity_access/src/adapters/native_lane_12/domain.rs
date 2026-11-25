//! Pure response types and input validation for lane 12's owner-local
//! People-template attribute reads. No SQL, no I/O and no transport concerns
//! live here; the axum side only transports these bounded values.

use erpx_server_shared::public::{BusinessError, RecordId};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// An exact registered Holding target. Host and slug are matched as a pair so a
/// request cannot cross into another Holding's company scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

/// The three owner-local reads previously rendered inline by
/// `adapters/people-template-attributes.ts`. The kind is a closed union; a
/// caller never supplies SQL, a table or a predicate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttributeRead {
    Attributes { company_id: String },
    ActiveRole { company_id: String, role: String },
    GroupReferences { company_id: String, value: String },
}

impl AttributeRead {
    pub const KINDS: [&'static str; 3] = ["attributes", "activeRole", "groupReferences"];

    /// Validate one bounded read. Company is always taken from the trusted
    /// context header, never from a query parameter.
    pub fn parse(
        kind: &str,
        company: &str,
        role: Option<&str>,
        value: Option<&str>,
    ) -> Result<Self, BusinessError> {
        let invalid =
            || BusinessError::invalid("NATIVE_LANE_12_READ_INVALID", "Use one bounded People-template attribute read.");
        if !valid_id(company) {
            return Err(invalid());
        }
        let company_id = company.to_owned();
        match kind {
            "attributes" if role.is_none() && value.is_none() => {
                Ok(Self::Attributes { company_id })
            }
            "activeRole" => {
                let role = role.filter(|role| valid_id(role)).ok_or_else(invalid)?;
                Ok(Self::ActiveRole {
                    company_id,
                    role: role.to_owned(),
                })
            }
            "groupReferences" => {
                let value = value
                    .filter(|value| !value.is_empty() && value.len() <= 200)
                    .ok_or_else(invalid)?;
                Ok(Self::GroupReferences {
                    company_id,
                    value: value.to_owned(),
                })
            }
            _ => Err(invalid()),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Attributes { .. } => "attributes",
            Self::ActiveRole { .. } => "activeRole",
            Self::GroupReferences { .. } => "groupReferences",
        }
    }
}

pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.' || byte == b':')
}

/// Pinned response envelope. `rowCount` mirrors pg's `QueryResult.rowCount`,
/// which is the only field the people-template application reads for probes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadPage<T: Serialize> {
    pub rows: Vec<T>,
    pub row_count: i64,
}

impl<T: Serialize> ReadPage<T> {
    pub fn new(rows: Vec<T>) -> Self {
        let row_count = rows.len() as i64;
        Self { rows, row_count }
    }
}

/// The wire carries JSON rows only; `Value` is the native projection of a
/// JSONB `definition` and of the `SELECT 1` probe markers.
pub type AttributePage = ReadPage<Value>;

/// The owner-local workflow approval assignment reads retired from
/// `adapters/workflow-approvals-facts.ts`. Command guards stay in TypeScript;
/// only these three closed reads cross the wire.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApprovalRead {
    CanOverride {
        assignment_id: String,
        company_id: String,
        user_id: String,
        branch_id: Option<String>,
        department_id: Option<String>,
    },
    ActiveScope {
        assignment_id: String,
        user_id: String,
        company_id: String,
        role: String,
        now: i64,
        now2: i64,
        policy_step: bool,
        branch_id: Option<String>,
        department_id: Option<String>,
    },
    ExpenseEvidence {
        company_id: String,
        assignment_id: String,
        now: i64,
        now2: i64,
    },
}

impl ApprovalRead {
    pub const KINDS: [&'static str; 3] = [
        "canOverrideStaffStepRole",
        "approvalViewActiveScope",
        "expenseDecisionEvidence",
    ];

    /// Parse one bounded approval read from validated query parameters.
    pub fn parse(kind: &str, query: &BTreeMap<String, String>) -> Result<Self, BusinessError> {
        let invalid =
            || BusinessError::invalid("NATIVE_LANE_12_APPROVAL_INVALID", "Use one bounded workflow approval read.");
        match kind {
            "canOverrideStaffStepRole" => Ok(Self::CanOverride {
                assignment_id: required(query, "assignment", invalid)?,
                company_id: required(query, "company", invalid)?,
                user_id: required(query, "user", invalid)?,
                branch_id: optional(query, "branch", invalid)?,
                department_id: optional(query, "department", invalid)?,
            }),
            "approvalViewActiveScope" => Ok(Self::ActiveScope {
                assignment_id: required(query, "assignment", invalid)?,
                user_id: required(query, "user", invalid)?,
                company_id: required(query, "company", invalid)?,
                role: required(query, "role", invalid)?,
                now: integer(query, "now", invalid)?,
                now2: integer(query, "now2", invalid)?,
                policy_step: boolean(query, "policyStep", invalid)?,
                branch_id: optional(query, "branch", invalid)?,
                department_id: optional(query, "department", invalid)?,
            }),
            "expenseDecisionEvidence" => Ok(Self::ExpenseEvidence {
                company_id: required(query, "company", invalid)?,
                assignment_id: required(query, "assignment", invalid)?,
                now: integer(query, "now", invalid)?,
                now2: integer(query, "now2", invalid)?,
            }),
            _ => Err(invalid()),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::CanOverride { .. } => "canOverrideStaffStepRole",
            Self::ActiveScope { .. } => "approvalViewActiveScope",
            Self::ExpenseEvidence { .. } => "expenseDecisionEvidence",
        }
    }
}

fn required<E>(
    query: &BTreeMap<String, String>,
    key: &str,
    invalid: impl Fn() -> E,
) -> Result<String, E> {
    query
        .get(key)
        .filter(|value| valid_id(value))
        .cloned()
        .ok_or_else(invalid)
}

fn optional<E>(
    query: &BTreeMap<String, String>,
    key: &str,
    invalid: impl Fn() -> E,
) -> Result<Option<String>, E> {
    match query.get(key) {
        None => Ok(None),
        Some(value) if value.is_empty() => Ok(None),
        Some(value) if valid_id(value) => Ok(Some(value.clone())),
        Some(_) => Err(invalid()),
    }
}

fn integer<E>(
    query: &BTreeMap<String, String>,
    key: &str,
    invalid: impl Fn() -> E,
) -> Result<i64, E> {
    query
        .get(key)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value >= 0)
        .ok_or_else(invalid)
}

fn boolean<E>(
    query: &BTreeMap<String, String>,
    key: &str,
    invalid: impl Fn() -> E,
) -> Result<bool, E> {
    match query.get(key).map(String::as_str) {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_lane_12_parses_each_bounded_kind() {
        let attributes = AttributeRead::parse("attributes", "acme-1", None, None).unwrap();
        assert_eq!(
            attributes,
            AttributeRead::Attributes {
                company_id: "acme-1".to_owned()
            }
        );
        assert_eq!(attributes.kind(), "attributes");

        let role = AttributeRead::parse("activeRole", "acme-1", Some("iam.roles.manage"), None).unwrap();
        assert_eq!(
            role,
            AttributeRead::ActiveRole {
                company_id: "acme-1".to_owned(),
                role: "iam.roles.manage".to_owned()
            }
        );

        let group = AttributeRead::parse("groupReferences", "acme-1", None, Some("custom:abc")).unwrap();
        assert_eq!(
            group,
            AttributeRead::GroupReferences {
                company_id: "acme-1".to_owned(),
                value: "custom:abc".to_owned()
            }
        );
        assert_eq!(AttributeRead::KINDS.len(), 3);
    }

    #[test]
    fn native_lane_12_rejects_unbounded_or_cross_scope_input() {
        assert!(AttributeRead::parse("attributes", "", None, None).is_err());
        assert!(AttributeRead::parse("attributes", "bad\nid", None, None).is_err());
        assert!(AttributeRead::parse("activeRole", "acme-1", None, None).is_err());
        assert!(AttributeRead::parse("activeRole", "acme-1", Some("bad value"), None).is_err());
        assert!(AttributeRead::parse("groupReferences", "acme-1", None, Some("")).is_err());
        assert!(AttributeRead::parse("SELECT 1", "acme-1", None, None).is_err());
        assert!(AttributeRead::parse("attributes", "acme-1", Some("x"), None).is_err());
    }

    #[test]
    fn native_lane_12_pins_response_envelope() {
        let page = AttributePage::new(vec![json!({"id": "a", "version": 1, "definition": {}})]);
        assert_eq!(serde_json::to_value(&page).unwrap(), json!({"rows": [{"id": "a", "version": 1, "definition": {}}], "rowCount": 1}));
        let empty: AttributePage = ReadPage::new(Vec::new());
        assert_eq!(serde_json::to_value(&empty).unwrap(), json!({"rows": [], "rowCount": 0}));
    }

    fn query(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn native_lane_12_parses_each_approval_read() {
        let override_read = ApprovalRead::parse(
            "canOverrideStaffStepRole",
            &query(&[
                ("assignment", "a1"),
                ("company", "c1"),
                ("user", "u1"),
                ("branch", "b1"),
                ("department", ""),
            ]),
        )
        .unwrap();
        assert_eq!(
            override_read,
            ApprovalRead::CanOverride {
                assignment_id: "a1".to_owned(),
                company_id: "c1".to_owned(),
                user_id: "u1".to_owned(),
                branch_id: Some("b1".to_owned()),
                department_id: None,
            }
        );

        let scope = ApprovalRead::parse(
            "approvalViewActiveScope",
            &query(&[
                ("assignment", "a1"),
                ("user", "u1"),
                ("company", "c1"),
                ("role", "workflow.approve"),
                ("now", "1700000000000"),
                ("now2", "1700000000001"),
                ("policyStep", "true"),
            ]),
        )
        .unwrap();
        assert!(matches!(scope, ApprovalRead::ActiveScope { policy_step: true, .. }));
        assert_eq!(scope.kind(), "approvalViewActiveScope");

        let evidence = ApprovalRead::parse(
            "expenseDecisionEvidence",
            &query(&[("company", "c1"), ("assignment", "a1"), ("now", "1"), ("now2", "2")]),
        )
        .unwrap();
        assert_eq!(evidence.kind(), "expenseDecisionEvidence");
        assert_eq!(ApprovalRead::KINDS.len(), 3);
    }

    #[test]
    fn native_lane_12_rejects_unbounded_approval_input() {
        assert!(ApprovalRead::parse("DROP TABLE", &query(&[])).is_err());
        assert!(ApprovalRead::parse("canOverrideStaffStepRole", &query(&[("assignment", "a1")])).is_err());
        assert!(ApprovalRead::parse(
            "canOverrideStaffStepRole",
            &query(&[("assignment", "a1"), ("company", "c1"), ("user", "bad value")]),
        )
        .is_err());
        assert!(ApprovalRead::parse(
            "approvalViewActiveScope",
            &query(&[("assignment", "a1"), ("user", "u1"), ("company", "c1"), ("role", "r"), ("now", "-1"), ("now2", "2"), ("policyStep", "false")]),
        )
        .is_err());
        assert!(ApprovalRead::parse(
            "approvalViewActiveScope",
            &query(&[("assignment", "a1"), ("user", "u1"), ("company", "c1"), ("role", "r"), ("now", "1"), ("now2", "2"), ("policyStep", "yes")]),
        )
        .is_err());
    }
}
