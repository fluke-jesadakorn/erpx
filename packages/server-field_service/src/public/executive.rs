//! Native Field report contract. Rows still require retained source authority.
pub const CONTRACT_VERSION: &str = "field-executive.v1";
pub use crate::adapters::executive::{
    CurrentFieldExecutiveCandidates, order_field_metric_rows, read_field_executive_candidates, read_field_evidence_candidates,
};
pub use crate::domain::executive::{
    FieldBreakdown, FieldExecutiveCandidate, FieldExecutiveQuery, FieldMetric, FieldMetricRow,
    aggregate,
};
