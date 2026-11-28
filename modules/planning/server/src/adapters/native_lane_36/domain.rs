//! Pure domain for the native Planning lane-36 scoped workspace reads.
//!
//! All six reads touch only Planning-owned relations (`planning_recommendations`,
//! `planning_scenarios`, `production_resources` and
//! `production_resource_calendar`). This module owns the closed read-kind set,
//! bounded scope/identifier validation and the exact response row shapes.
//! There is no SQL and no I/O here.

use erpx_platform_shared_server::public::BusinessError;
use serde::Serialize;
use serde_json::{Value, json};

pub const KIND_RECOMMENDATIONS: &str = "recommendations";
pub const KIND_SCENARIOS: &str = "scenarios";
pub const KIND_RESOURCES: &str = "resources";
pub const KIND_CALENDAR: &str = "calendar";
pub const KIND_SCENARIO: &str = "scenario";
pub const KIND_SCOPED_RECOMMENDATION: &str = "scoped-recommendation";
pub const KIND_COMPARISON: &str = "comparison";
pub const KIND_EXECUTIVE_SHORTAGE: &str = "executive-shortage";
pub const KIND_EXECUTIVE_RECOMMENDATION: &str = "executive-recommendation";

/// The closed discriminator set. The wire accepts no other value.
pub const KINDS: [&str; 9] = [
    KIND_RECOMMENDATIONS,
    KIND_SCENARIOS,
    KIND_RESOURCES,
    KIND_CALENDAR,
    KIND_SCENARIO,
    KIND_SCOPED_RECOMMENDATION,
    KIND_COMPARISON,
    KIND_EXECUTIVE_SHORTAGE,
    KIND_EXECUTIVE_RECOMMENDATION,
];

pub fn valid_kind(value: &str) -> bool {
    KINDS.contains(&value)
}

/// Row reads return one optional row; list reads return an array.
pub fn is_row_kind(kind: &str) -> bool {
    matches!(
        kind,
        KIND_SCENARIO
            | KIND_SCOPED_RECOMMENDATION
            | KIND_COMPARISON
            | KIND_EXECUTIVE_SHORTAGE
            | KIND_EXECUTIVE_RECOMMENDATION
    )
}

/// The recommendation projection has exactly two retained eligibility branches.
pub fn valid_eligibility(value: &str) -> bool {
    matches!(value, "proposed" | "capacity")
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

/// Validated transport facts for one scoped read. Both Company and a bounded
/// branch scope are mandatory; row reads additionally require their closed
/// identity fields. No caller-supplied SQL, relation, permission or projection
/// reaches the query port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneScope {
    pub kind: String,
    pub company_id: String,
    pub branch: Option<String>,
    pub id: Option<String>,
    pub comparison_scenario_id: Option<String>,
    pub specification_id: Option<String>,
    /// Executive evidence window, echoed by the owner as bound facts.
    pub from: Option<String>,
    pub to: Option<String>,
    /// Closed eligibility branch of the executive recommendation read.
    pub eligibility: Option<String>,
}

impl LaneScope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: &str,
        company_id: &str,
        branch: Option<&str>,
        id: Option<&str>,
        comparison_scenario_id: Option<&str>,
        specification_id: Option<&str>,
    ) -> Result<Self, BusinessError> {
        if !valid_kind(kind) || !valid_id(company_id) {
            return Err(invalid());
        }
        let bounded = |value: &str| {
            if valid_scope(value) {
                Ok(value.to_owned())
            } else {
                Err(invalid())
            }
        };
        let branch = branch.map(bounded).transpose()?;
        let id = id.map(bounded).transpose()?;
        let comparison_scenario_id = comparison_scenario_id.map(bounded).transpose()?;
        let specification_id = specification_id.map(bounded).transpose()?;
        match kind {
            KIND_SCENARIO | KIND_SCOPED_RECOMMENDATION if id.is_none() => return Err(invalid()),
            KIND_COMPARISON if comparison_scenario_id.is_none() || specification_id.is_none() => {
                return Err(invalid());
            }
            _ => {}
        }
        Ok(Self {
            kind: kind.to_owned(),
            company_id: company_id.to_owned(),
            branch,
            id,
            comparison_scenario_id,
            specification_id,
            from: None,
            to: None,
            eligibility: None,
        })
    }

    /// Executive evidence reads carry the record id, the bounded window and,
    /// for recommendations, one of the two closed eligibility branches.
    #[allow(clippy::too_many_arguments)]
    pub fn new_executive(
        kind: &str,
        company_id: &str,
        branch: Option<&str>,
        id: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        eligibility: Option<&str>,
    ) -> Result<Self, BusinessError> {
        if kind != KIND_EXECUTIVE_SHORTAGE && kind != KIND_EXECUTIVE_RECOMMENDATION {
            return Err(invalid());
        }
        let bounded = |value: &str| {
            if valid_scope(value) {
                Ok(value.to_owned())
            } else {
                Err(invalid())
            }
        };
        let from = from.map(bounded).transpose()?.ok_or_else(invalid)?;
        let to = to.map(bounded).transpose()?.ok_or_else(invalid)?;
        let eligibility = match (kind, eligibility) {
            (KIND_EXECUTIVE_RECOMMENDATION, Some(value)) if valid_eligibility(value) => {
                Some(value.to_owned())
            }
            (KIND_EXECUTIVE_RECOMMENDATION, _) => return Err(invalid()),
            (_, None) => None,
            (_, _) => return Err(invalid()),
        };
        let mut scope = Self::new(kind, company_id, branch, id, None, None)?;
        scope.from = Some(from);
        scope.to = Some(to);
        scope.eligibility = eligibility;
        Ok(scope)
    }

    /// List reads carry only Company and optional branch scope.
    pub fn new_list(kind: &str, company_id: &str, branch: Option<&str>) -> Result<Self, BusinessError> {
        Self::new(kind, company_id, branch, None, None, None)
    }

    /// Stable, closed projection of the scope used for the SQL/result oracle.
    pub fn oracle_key(&self) -> Value {
        json!([
            self.kind.as_str(),
            self.company_id.as_str(),
            self.branch.as_deref(),
            self.id.as_deref(),
            self.comparison_scenario_id.as_deref(),
            self.specification_id.as_deref(),
            self.from.as_deref(),
            self.to.as_deref(),
            self.eligibility.as_deref(),
        ])
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

/// Exact row shape of the scenario scoped list. The owner statement projects
/// the scenario columns and the Manufacturing-owned specification name; the
/// cross-owner fragment stays inlined until Manufacturing exposes a subquery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScenarioListItem {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub specification_id: String,
    pub specification_version: i64,
    pub demand_quantity_milli: i64,
    pub due_date: String,
    pub capacity_minutes: i64,
    pub shortages_json: String,
    pub status: String,
    pub created_by: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub context_json: String,
    pub specification_name: String,
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

/// Exact row shape of the single scenario row read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScenarioRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub specification_id: String,
    pub specification_version: i64,
    pub demand_quantity_milli: i64,
    pub due_date: String,
    pub capacity_minutes: i64,
    pub shortages_json: String,
    pub status: String,
    pub created_by: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub context_json: String,
}

/// Exact row shape of the single scoped recommendation row read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScopedRecommendationRow {
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
    pub warehouse_id: String,
    pub specification_id: String,
    pub specification_version: i64,
    pub context_json: String,
    pub scenario_created_by: String,
}

/// Exact row shape of the single comparison row read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ComparisonRow {
    pub id: String,
    pub version: i64,
    pub shortages_json: String,
    pub required_minutes: i64,
    pub feasible_date: String,
}

/// Exact row shape of the executive evidence projection. Left-joined owner
/// facts stay nullable so the runtime evidence presenter keeps its existing
/// fallback labels. The duplicate `scenario_version` projection in the
/// registered statement resolves to the scenario's version, as in TS.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ExecutiveRow {
    pub scenario_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub branch_name: Option<String>,
    pub warehouse_id: String,
    pub warehouse_name: Option<String>,
    pub specification_id: String,
    pub specification_name: Option<String>,
    pub output_product_id: Option<String>,
    pub output_product_name: Option<String>,
    pub specification_version: i64,
    pub demand_quantity_milli: String,
    pub due_date: String,
    pub capacity_minutes: String,
    pub scenario_status: String,
    pub scenario_version: i64,
    pub scenario_created_at: i64,
    pub scenario_updated_at: i64,
    pub recommendation_id: Option<String>,
    pub recommendation_status: Option<String>,
    pub recommended_quantity_milli: Option<String>,
    pub required_minutes: Option<String>,
    pub feasible_date: Option<String>,
    pub recommendation_version: Option<i64>,
    pub recommendation_created_at: Option<i64>,
    pub recommendation_updated_at: Option<i64>,
    pub shortage_count: String,
    pub shortage_items_json: String,
}

/// One closed response. List reads return an array; row reads return one row
/// or `null`. A row is never mixed with a list in the same envelope.
#[derive(Clone, Debug, PartialEq)]
pub enum LaneData {
    List(Vec<Value>),
    One(Option<Value>),
}

impl LaneData {
    pub fn value(&self) -> Value {
        match self {
            LaneData::List(rows) => json!(rows),
            LaneData::One(row) => json!(row),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            LaneData::List(rows) => rows.len(),
            LaneData::One(row) => usize::from(row.is_some()),
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
        assert!(is_row_kind(KIND_SCENARIO));
        assert!(!is_row_kind(KIND_SCENARIOS));
        assert!(!is_row_kind(KIND_RESOURCES));
        assert!(valid_kind(KIND_SCENARIOS));
        assert!(valid_kind(KIND_EXECUTIVE_SHORTAGE));
        assert!(valid_kind(KIND_EXECUTIVE_RECOMMENDATION));
        assert!(is_row_kind(KIND_EXECUTIVE_SHORTAGE));
        assert!(valid_eligibility("proposed"));
        assert!(valid_eligibility("capacity"));
        assert!(!valid_eligibility("approved"));
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
    fn list_scope_requires_company_and_bounded_branch() {
        let scope = LaneScope::new_list(KIND_RESOURCES, "company-1", Some("branch-1")).unwrap();
        assert_eq!(scope.company_id, "company-1");
        assert_eq!(scope.branch.as_deref(), Some("branch-1"));
        assert!(LaneScope::new_list(KIND_RESOURCES, "", None).is_err());
        assert!(LaneScope::new_list("other", "company-1", None).is_err());
        assert!(LaneScope::new_list(KIND_CALENDAR, "company-1", Some("x/y")).is_err());
        assert!(LaneScope::new_list(KIND_SCENARIOS, "company-1", None).is_ok());
    }

    #[test]
    fn row_scope_requires_closed_identity_fields() {
        let scenario =
            LaneScope::new(KIND_SCENARIO, "company-1", Some("b1"), Some("s1"), None, None).unwrap();
        assert_eq!(scenario.id.as_deref(), Some("s1"));
        assert!(LaneScope::new(KIND_SCENARIO, "company-1", None, None, None, None).is_err());
        assert!(
            LaneScope::new(
                KIND_COMPARISON,
                "company-1",
                Some("b1"),
                None,
                Some("other"),
                Some("spec"),
            )
            .is_ok()
        );
        assert!(LaneScope::new(KIND_COMPARISON, "company-1", None, None, Some("other"), None).is_err());
        assert!(LaneScope::new(KIND_SCOPED_RECOMMENDATION, "company-1", None, Some("x/y"), None, None).is_err());
    }

    #[test]
    fn executive_scope_requires_id_window_and_closed_eligibility() {
        let shortage = LaneScope::new_executive(
            KIND_EXECUTIVE_SHORTAGE,
            "company-1",
            Some("branch-1"),
            Some("scenario-1"),
            Some("2026-01-01"),
            Some("2026-12-31"),
            None,
        )
        .unwrap();
        assert_eq!(shortage.from.as_deref(), Some("2026-01-01"));
        assert_eq!(shortage.oracle_key()[6], json!("2026-01-01"));
        assert!(LaneScope::new_executive(
            KIND_EXECUTIVE_SHORTAGE,
            "company-1",
            None,
            Some("scenario-1"),
            None,
            Some("2026-12-31"),
            None,
        )
        .is_err());
        let recommended = LaneScope::new_executive(
            KIND_EXECUTIVE_RECOMMENDATION,
            "company-1",
            None,
            Some("recommendation-1"),
            Some("2026-01-01"),
            Some("2026-12-31"),
            Some("capacity"),
        )
        .unwrap();
        assert_eq!(recommended.eligibility.as_deref(), Some("capacity"));
        assert!(LaneScope::new_executive(
            KIND_EXECUTIVE_RECOMMENDATION,
            "company-1",
            None,
            Some("recommendation-1"),
            Some("2026-01-01"),
            Some("2026-12-31"),
            Some("approved"),
        )
        .is_err());
        assert!(LaneScope::new_executive(
            KIND_SCENARIO,
            "company-1",
            None,
            Some("scenario-1"),
            Some("2026-01-01"),
            Some("2026-12-31"),
            None,
        )
        .is_err());
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

        let scenario = ScenarioListItem {
            id: "s1".into(),
            company_id: "company-1".into(),
            branch_id: "branch-1".into(),
            warehouse_id: "warehouse-1".into(),
            specification_id: "spec-1".into(),
            specification_version: 1,
            demand_quantity_milli: 2000,
            due_date: "2026-10-04".into(),
            capacity_minutes: 120,
            shortages_json: "[]".into(),
            status: "calculated".into(),
            created_by: "planner".into(),
            version: 1,
            created_at: 10,
            updated_at: 20,
            context_json: "{}".into(),
            specification_name: "Assembly".into(),
        };
        let scenario_value = serde_json::to_value(&scenario).unwrap();
        assert_eq!(scenario_value.as_object().unwrap().len(), 17);
        assert_eq!(scenario_value["specification_name"], "Assembly");

        let executive = ExecutiveRow {
            scenario_id: "s1".into(),
            company_id: "company-1".into(),
            branch_id: "branch-1".into(),
            branch_name: Some("North".into()),
            warehouse_id: "warehouse-1".into(),
            warehouse_name: None,
            specification_id: "spec-1".into(),
            specification_name: Some("Assembly".into()),
            output_product_id: Some("product-1".into()),
            output_product_name: None,
            specification_version: 1,
            demand_quantity_milli: "2000".into(),
            due_date: "2026-10-04".into(),
            capacity_minutes: "120".into(),
            scenario_status: "calculated".into(),
            scenario_version: 2,
            scenario_created_at: 10,
            scenario_updated_at: 20,
            recommendation_id: Some("r1".into()),
            recommendation_status: Some("proposed".into()),
            recommended_quantity_milli: Some("2000".into()),
            required_minutes: Some("60".into()),
            feasible_date: Some("2026-10-04".into()),
            recommendation_version: Some(1),
            recommendation_created_at: Some(30),
            recommendation_updated_at: Some(40),
            shortage_count: "1".into(),
            shortage_items_json: "[]".into(),
        };
        let executive_value = serde_json::to_value(&executive).unwrap();
        assert_eq!(executive_value.as_object().unwrap().len(), 28);
        assert!(executive_value["warehouse_name"].is_null());
        assert_eq!(executive_value["scenario_version"], 2);

        let list = LaneData::List(vec![value]);
        assert_eq!(list.len(), 1);
        assert!(list.value().is_array());
        let missing = LaneData::One(None);
        assert!(missing.is_empty());
        assert!(missing.value().is_null());
    }
}
