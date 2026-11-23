//! Pure domain facts for the native Planning evidence SEARCH owner port.
//!
//! The port reads only Planning-owned scenario/recommendation rows and the
//! already-owned label projections for Specification, Product, Warehouse and
//! Organization. Company/branch scope, a bounded text query, a bounded cursor
//! and the page limit are fixed here. Purchase history and current IAM
//! admission are composition responsibilities supplied through
//! [`EvidenceEligibility`] and the actor; nothing here mints authority, writes
//! a row or accepts caller SQL.
use base64::Engine;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const PROVIDER: &str = "planning-scenarios-recommendations";

pub const SHORTAGE_SCENARIOS: &str = "planning-shortage-scenarios";
pub const PROPOSED_RECOMMENDATIONS: &str = "proposed-planning-recommendations";
pub const CAPACITY_FEASIBLE_RECOMMENDATIONS: &str = "capacity-feasible-recommendations";

pub const METRICS: [&str; 3] = [
    SHORTAGE_SCENARIOS,
    PROPOSED_RECOMMENDATIONS,
    CAPACITY_FEASIBLE_RECOMMENDATIONS,
];

/// Exact field identifiers retained for the Runtime evidence contract.
pub const SCENARIO_FIELDS: [&str; 23] = [
    "planning_scenarios.id",
    "planning_scenarios.company_id",
    "planning_scenarios.branch_id",
    "planning_scenarios.warehouse_id",
    "planning_scenarios.specification_id",
    "planning_scenarios.specification_version",
    "planning_scenarios.demand_quantity_milli",
    "planning_scenarios.due_date",
    "planning_scenarios.capacity_minutes",
    "planning_scenarios.shortages_json",
    "planning_scenarios.status",
    "planning_scenarios.version",
    "planning_scenarios.created_at",
    "planning_scenarios.updated_at",
    "manufacturing_specifications.name",
    "commerce_products.name",
    "commerce_warehouses.name",
    "organization_nodes.display_name",
    "planning_recommendations.id",
    "planning_recommendations.status",
    "planning_recommendations.required_minutes",
    "planning_recommendations.feasible_date",
    "planning_recommendations.version",
];

pub const RECOMMENDATION_FIELDS: [&str; 24] = [
    "planning_recommendations.id",
    "planning_recommendations.scenario_id",
    "planning_recommendations.status",
    "planning_recommendations.recommended_quantity_milli",
    "planning_recommendations.required_minutes",
    "planning_recommendations.feasible_date",
    "planning_recommendations.version",
    "planning_recommendations.scenario_version",
    "planning_recommendations.created_at",
    "planning_recommendations.updated_at",
    "planning_scenarios.id",
    "planning_scenarios.branch_id",
    "planning_scenarios.warehouse_id",
    "planning_scenarios.specification_id",
    "planning_scenarios.specification_version",
    "planning_scenarios.demand_quantity_milli",
    "planning_scenarios.due_date",
    "planning_scenarios.capacity_minutes",
    "planning_scenarios.shortages_json",
    "planning_scenarios.status",
    "planning_scenarios.version",
    "manufacturing_specifications.name",
    "commerce_products.name",
    "commerce_warehouses.name",
];

pub fn fields_for(metric: &str) -> &'static [&'static str] {
    if metric == SHORTAGE_SCENARIOS {
        &SCENARIO_FIELDS
    } else {
        &RECOMMENDATION_FIELDS
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "EVIDENCE_INPUT_INVALID",
        "Use one bounded Planning evidence search request.",
    )
}

pub fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "EVIDENCE_FORBIDDEN",
        "Planning evidence requires current module read access.",
    )
}

pub fn cursor_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "EVIDENCE_CURSOR_INVALID",
        "The Planning evidence cursor is invalid. Refresh the evidence list.",
    )
}

/// A single source row returned by the native port. Numeric text columns are
/// strings because the retired owner SQL cast them with `::text`; revisions and
/// epoch-millis timestamps are bounded signed integers.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRow {
    pub id: String,
    pub scenario_id: String,
    pub recommendation_id: Option<String>,
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
    pub recommendation_status: Option<String>,
    pub recommended_quantity_milli: Option<String>,
    pub required_minutes: Option<String>,
    pub feasible_date: Option<String>,
    pub recommendation_version: Option<i64>,
    pub scenario_version_ref: Option<i64>,
    pub recommendation_created_at: Option<i64>,
    pub recommendation_updated_at: Option<i64>,
    pub shortage_count: String,
    pub shortage_items_json: String,
}

/// Bounded, validated search request. `cursor_id` is decoded from the caller's
/// opaque cursor; all other cursor binding (metric, filters, query, scope) is
/// performed by the caller before this request is formed.
#[derive(Clone, Debug)]
pub struct EvidenceQuery {
    pub metric: String,
    pub from: String,
    pub to: String,
    pub query: String,
    pub cursor_id: Option<String>,
    pub limit: u32,
}

/// Transport DTO: the exact closed JSON accepted by the native HTTP ingress.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceQueryInput {
    pub metric: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    25
}

impl EvidenceQueryInput {
    pub fn into_query(self) -> Result<EvidenceQuery, BusinessError> {
        let query = self.query;
        if !METRICS.contains(&self.metric.as_str())
            || !valid_date(&self.from)
            || !valid_date(&self.to)
            || !whole_year(&self.from, &self.to)
            || query.encode_utf16().count() > 100
            || self.limit < 1
            || self.limit > 50
        {
            return Err(invalid());
        }
        let cursor_id = match self.cursor.as_deref() {
            None => None,
            Some(value) => Some(cursor_id(value)?),
        };
        Ok(EvidenceQuery {
            metric: self.metric,
            from: self.from,
            to: self.to,
            query,
            cursor_id,
            limit: self.limit,
        })
    }
}

fn whole_year(from: &str, to: &str) -> bool {
    let left = date_ordinal(from);
    let right = date_ordinal(to);
    match (left, right) {
        (Some(left), Some(right)) => right >= left && right - left <= 366,
        _ => false,
    }
}

fn date_ordinal(value: &str) -> Option<i64> {
    if !valid_date(value) {
        return None;
    }
    let year: i64 = value[0..4].parse().ok()?;
    let month: i64 = value[5..7].parse().ok()?;
    let day: i64 = value[8..10].parse().ok()?;
    let days = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let mut total = year * 365 + days[(month - 1) as usize] + day;
    if leap && month > 2 {
        total += 1;
    }
    Some(total)
}

pub fn valid_date(value: &str) -> bool {
    if value.len() != 10
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || !value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let month = match value[5..7].parse::<u32>() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let day = match value[8..10].parse::<u32>() {
        Ok(value) => value,
        Err(_) => return false,
    };
    if !(1..=12).contains(&month) || day == 0 {
        return false;
    }
    let year = value[0..4].parse::<i64>().unwrap_or(0);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day <= max
}

/// Decode only the `id` member of the opaque cursor produced by the Runtime
/// application. Extra members (metric, filters, query, scope) are intentionally
/// ignored: the application rebinds them to the current request before the
/// native port is called.
pub fn cursor_id(value: &str) -> Result<String, BusinessError> {
    if value.is_empty() || value.len() > 2000 {
        return Err(cursor_invalid());
    }
    #[derive(Deserialize)]
    struct Cursor {
        id: String,
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| cursor_invalid())?;
    let cursor: Cursor = serde_json::from_slice(&decoded).map_err(|_| cursor_invalid())?;
    if cursor.id.is_empty() || cursor.id.len() > 200 {
        return Err(cursor_invalid());
    }
    Ok(cursor.id)
}

/// Composition-owned purchase/assignment admission for scenario visibility.
/// `All` is the exact non-enforced source image (`SELECT id FROM
/// planning_scenarios`); `Scenarios` is the bounded eligible set resolved by
/// the owner service on the same unit before this read.
#[derive(Clone, Debug, Default)]
pub enum EvidenceEligibility {
    #[default]
    All,
    Scenarios(Vec<String>),
}

/// Current validated actor facts forwarded by composition. The owner rechecks
/// module/permission admission before any read.
#[derive(Clone, Debug)]
pub struct EvidenceActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub assignment_id: String,
    pub department_id: Option<String>,
    pub role: String,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
}

impl EvidenceActor {
    pub fn require_read(&self) -> Result<(), BusinessError> {
        if self.user_id.is_empty()
            || self.company_id.is_empty()
            || self.assignment_id.is_empty()
            || self.role.is_empty()
            || !self.modules.iter().any(|value| value == "planning")
            || !self.permissions.iter().any(|value| value == "records.read")
        {
            return Err(forbidden());
        }
        Ok(())
    }
}

/// Validate a bounded page of owner rows before it leaves the port.
pub fn validate_rows(metric: &str, rows: &[EvidenceRow]) -> Result<(), BusinessError> {
    if rows.len() > 51 {
        return Err(invalid());
    }
    let scenario = metric == SHORTAGE_SCENARIOS;
    for row in rows {
        let version = if scenario {
            row.scenario_version
        } else {
            row.recommendation_version.unwrap_or_default()
        };
        if row.id.is_empty()
            || row.scenario_id.is_empty()
            || version < 1
            || row.scenario_version < 1
            || !valid_date(&row.due_date)
            || row.shortage_count.is_empty()
            || !row.shortage_count.bytes().all(|byte| byte.is_ascii_digit())
            || row.shortage_items_json.len() > 1_048_576
            || row.specification_version < 1
        {
            return Err(invalid());
        }
        if !scenario && row.recommendation_id.is_none() {
            return Err(invalid());
        }
        if let Some(date) = row.feasible_date.as_deref() {
            if !valid_date(date) {
                return Err(invalid());
            }
        }
    }
    if rows.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_and_fields_are_closed() {
        assert_eq!(METRICS.len(), 3);
        assert_eq!(fields_for(SHORTAGE_SCENARIOS).len(), 23);
        assert_eq!(fields_for(PROPOSED_RECOMMENDATIONS).len(), 24);
    }

    #[test]
    fn dates_and_year_bounds_are_checked() {
        assert!(valid_date("2026-01-01"));
        assert!(!valid_date("2026-1-01"));
        assert!(!valid_date("2026-13-01"));
        assert!(whole_year("2026-01-01", "2026-12-31"));
        assert!(!whole_year("2026-01-01", "2027-01-03"));
    }

    #[test]
    fn cursor_decodes_only_the_id() {
        let cursor = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            r#"{"metric":"planning-shortage-scenarios","filters":{"from":"2026-01-01","to":"2026-12-31","currency":"THB"},"query":"x","scope":["c","a","b","d"],"id":"rec-9"}"#,
        );
        assert_eq!(cursor_id(&cursor).unwrap(), "rec-9");
        assert!(cursor_id("not-base64!").is_err());
        assert!(cursor_id("").is_err());
    }

    #[test]
    fn input_bounds_are_enforced() {
        let base = |limit: u32, query: &str| EvidenceQueryInput {
            metric: SHORTAGE_SCENARIOS.into(),
            from: "2026-01-01".into(),
            to: "2026-12-31".into(),
            query: query.into(),
            cursor: None,
            limit,
        };
        assert!(base(25, "cell").into_query().is_ok());
        assert!(base(0, "").into_query().is_err());
        assert!(base(51, "").into_query().is_err());
        assert!(base(25, &"x".repeat(101)).into_query().is_err());
        let mut bad = base(25, "");
        bad.metric = "bogus".into();
        assert!(bad.into_query().is_err());
    }
}
