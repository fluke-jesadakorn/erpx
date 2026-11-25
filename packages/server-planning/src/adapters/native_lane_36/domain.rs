//! Pure domain for the native Planning lane-36 scoped workspace reads.
//!
//! The three reads in this family touch only Planning-owned relations
//! (`planning_recommendations`, `planning_scenarios`, `production_resources`
//! and `production_resource_calendar`). This module owns the closed read-kind
//! set, the bounded scope/identifier validation and the exact response row
//! shapes. There is no SQL and no I/O here.

use erpx_server_shared::public::BusinessError;
use serde::Serialize;
use serde_json::{Value, json};

pub const KIND_RECOMMENDATIONS: &str = "recommendations";
pub const KIND_RESOURCES: &str = "resources";
pub const KIND_CALENDAR: &str = "calendar";

/// The closed discriminator set. The wire accepts no other value.
pub const KINDS: [&str; 3] = [KIND_RECOMMENDATIONS, KIND_RESOURCES, KIND_CALENDAR];

pub fn valid_kind(value: &str) -> bool {
    KINDS.contains(&value)
}

/// Bounded record identifier. Postgres ids are text; only the same safe
/// character set used by the other native owner readers is admitted.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// Company/branch scope is either absent (`NULL` bind) or one bounded id.
pub fn valid_scope(value: &str) -> bool {
    valid_id(value)
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_LANE36_QUERY_INVALID",
        "Use one bounded Planning scoped read.",
    )
}

/// Validated transport facts for one scoped list read. Commissioning both a
/// Company and an optional branch is mandatory; no caller-supplied SQL,
/// relation, permission or projection reaches the query port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneScope {
    pub kind: String,
    pub company_id: String,
    pub branch: Option<String>,
}

impl LaneScope {
    pub fn new(kind: &str, company_id: &str, branch: Option<&str>) -> Result<Self, BusinessError> {
        if !valid_kind(kind) || !valid_id(company_id) {
            return Err(invalid());
        }
        let branch = branch
            .map(|value| {
                if valid_scope(value) {
                    Ok(value.to_owned())
                } else {
                    Err(invalid())
                }
            })
            .transpose()?;
        Ok(Self {
            kind: kind.to_owned(),
            company_id: company_id.to_owned(),
            branch,
        })
    }

    /// Stable, closed projection of the scope used for the SQL/result oracle.
    pub fn oracle_key(&self) -> Value {
        json!([self.kind.as_str(), self.company_id.as_str(), self.branch.as_deref()])
    }
}

/// Exact row shape of the recommendations scoped list. `company_id` and
/// `branch_id` are projected from the parent scenario, never from the
/// recommendation, so the response can be company/branch asserted by callers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecommendationListItem {
    pub id: String,
    pub scenario_id: String,
    pub scenario_version: i64,
    pub recommended_quantity_milli: i64,
    pub required_minutes: i64,
    pub feasible_date: String,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub company_id: String,
    pub branch_id: String,
}

/// Exact row shape of the production-resources scoped list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceListItem {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub code: String,
    pub name: String,
    pub daily_minutes: i64,
    pub work_weekdays_json: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Exact row shape of the resource-calendar scoped list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CalendarListItem {
    pub resource_id: String,
    pub work_date: String,
    pub available_minutes: i64,
    pub reason: String,
    pub version: i64,
    pub updated_by: String,
    pub updated_at: i64,
}

/// One closed response. List reads always return an array; a row is never
/// mixed with a list in the same envelope.
#[derive(Clone, Debug, PartialEq)]
pub enum LaneData {
    List(Vec<Value>),
}

impl LaneData {
    pub fn value(&self) -> Value {
        match self {
            LaneData::List(rows) => json!(rows),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            LaneData::List(rows) => rows.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_kind_set() {
        assert!(KINDS.iter().all(|kind| valid_kind(kind)));
        assert!(!valid_kind("scenarios"));
        assert!(!valid_kind("executive-shortage"));
        assert!(!valid_kind(""));
    }

    #[test]
    fn bounded_identifiers() {
        assert!(valid_id("plan-recommendation-1"));
        assert!(valid_id("a:b.c_d-9"));
        assert!(!valid_id(""));
        assert!(!valid_id("x/y"));
        assert!(!valid_id(&"x".repeat(201)));
    }

    #[test]
    fn scope_requires_company_and_bounded_branch() {
        let scope = LaneScope::new(KIND_RESOURCES, "company-1", Some("branch-1")).unwrap();
        assert_eq!(scope.company_id, "company-1");
        assert_eq!(scope.branch.as_deref(), Some("branch-1"));
        assert_eq!(
            scope.oracle_key(),
            json!(["resources", "company-1", "branch-1"])
        );
        assert!(LaneScope::new(KIND_RESOURCES, "", None).is_err());
        assert!(LaneScope::new("other", "company-1", None).is_err());
        assert!(LaneScope::new(KIND_CALENDAR, "company-1", Some("x/y")).is_err());
        assert_eq!(
            LaneScope::new(KIND_CALENDAR, "company-1", None)
                .unwrap()
                .branch,
            None
        );
    }

    #[test]
    fn response_shapes_are_closed() {
        let recommendation = RecommendationListItem {
            id: "r1".into(),
            scenario_id: "s1".into(),
            scenario_version: 1,
            recommended_quantity_milli: 2000,
            required_minutes: 60,
            feasible_date: "2026-10-04".into(),
            status: "proposed".into(),
            created_by: "planner".into(),
            reviewed_by: None,
            version: 1,
            created_at: 100,
            updated_at: 100,
            company_id: "company-1".into(),
            branch_id: "branch-1".into(),
        };
        let value = serde_json::to_value(&recommendation).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 14);
        assert_eq!(value["company_id"], "company-1");
        assert!(value["reviewed_by"].is_null());
        assert!(value.get("permission").is_none());

        let list = LaneData::List(vec![value]);
        assert_eq!(list.len(), 1);
        assert!(list.value().is_array());
    }
}
