//! Native migration for the Ecommerce executive metric read family.
//!
//! Rust owns the three aggregate statements and the branch/department
//! organisation labels, executes them on the owner pool and returns scalar
//! rows plus the trusted statement for the retained source drill-down.
//! TypeScript is a strict, fail-closed multi-holding transport with no SQL.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{ExecMetricFuture, ExecMetricRead, read_exec_metric};
pub use domain::{
    BindValue, Breakdown, Metric, MetricRow, QuerySpec, Scope, build_query, branch_name_label,
    department_name_label, node_name_label, validate_range, validate_scope,
};
pub use http::{ExecMetricTarget, MetricTarget, router, router_for_holdings};
pub use postgres::PgExecMetricRead;
