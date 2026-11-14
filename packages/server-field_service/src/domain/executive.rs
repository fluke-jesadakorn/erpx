//! Field report policy. Query input never conveys source authority.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum FieldMetric {
    #[serde(rename = "open-service-orders")]
    Open,
    #[serde(rename = "service-billing-releases")]
    Billing,
    #[serde(rename = "field-visit-completion")]
    Completion,
}
impl FieldMetric {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open-service-orders",
            Self::Billing => "service-billing-releases",
            Self::Completion => "field-visit-completion",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FieldBreakdown {
    #[default]
    Default,
    Status,
    Branch,
    Department,
    Month,
    Technician,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldExecutiveQuery {
    #[serde(default)]
    pub access_only: bool,
    pub metric: FieldMetric,
    #[serde(default)]
    pub breakdown: FieldBreakdown,
    pub from_at: i64,
    pub to_at: i64,
    #[serde(default)]
    pub query: String,
    pub before_at: Option<i64>,
    pub before_id: Option<String>,
    /// Evidence paging is separate from complete aggregate calculation.
    pub evidence_limit: Option<usize>,
}
impl FieldExecutiveQuery {
    pub fn validate(&self) -> Result<(), BusinessError> {
        let days = |v: i64| {
            (-62_135_596_800_000..=253_402_214_400_000).contains(&v)
                && v.rem_euclid(86_400_000) == 0
        };
        let breakdown = match self.metric {
            FieldMetric::Open => matches!(
                self.breakdown,
                FieldBreakdown::Default
                    | FieldBreakdown::Status
                    | FieldBreakdown::Branch
                    | FieldBreakdown::Department
            ),
            FieldMetric::Billing => matches!(
                self.breakdown,
                FieldBreakdown::Default
                    | FieldBreakdown::Month
                    | FieldBreakdown::Branch
                    | FieldBreakdown::Department
            ),
            FieldMetric::Completion => true,
        };
        if self.access_only
            && (self.evidence_limit.is_some() || self.before_at.is_some() || !self.query.is_empty())
            || !days(self.from_at)
            || !days(self.to_at)
            || self.to_at < self.from_at
            || !breakdown
            || self.query.chars().count() > 200
            || self.before_at.is_some() != self.before_id.is_some()
            || self
                .before_at
                .is_some_and(|v| v.unsigned_abs() > 9_007_199_254_740_991)
            || self
                .before_id
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.encode_utf16().count() > 200)
            || self.evidence_limit.is_some_and(|v| !(1..=100).contains(&v))
            || self.metric != FieldMetric::Completion
                && (self.evidence_limit.is_some()
                    || self.before_at.is_some()
                    || !self.query.is_empty())
        {
            return Err(BusinessError::invalid(
                "INVALID_FIELD_REPORT_QUERY",
                "Use a registered Field report and date interval.",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldExecutiveCandidate {
    pub source_order: i64,
    pub id: String,
    pub version: i64,
    pub status: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub technician_id: String,
    pub scheduled_at: Option<i64>,
    pub visit_completed_at: Option<i64>,
    pub month: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FieldMetricRow {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numerator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denominator: Option<String>,
}
/// The composition supplies only rows with current owner witnesses and resolved
/// labels. Completion uses the recorded event as well as terminal visit state.
pub fn aggregate<'a>(
    metric: FieldMetric,
    rows: impl IntoIterator<Item = (&'a str, &'a FieldExecutiveCandidate)>,
) -> Vec<FieldMetricRow> {
    let mut groups: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for (label, row) in rows {
        let group = groups.entry(label.to_owned()).or_default();
        group.1 += 1;
        if matches!(row.status.as_str(), "completed" | "billed") && row.visit_completed_at.is_some()
        {
            group.0 += 1;
        }
    }
    groups
        .into_iter()
        .map(|(label, (completed, total))| FieldMetricRow {
            label,
            value: (metric != FieldMetric::Completion).then(|| total.to_string()),
            numerator: (metric == FieldMetric::Completion).then(|| completed.to_string()),
            denominator: (metric == FieldMetric::Completion).then(|| total.to_string()),
        })
        .collect()
}
