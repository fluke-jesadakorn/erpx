//! Native Platform usage slice (usage route family).
//!
//! Owner-local successor of the `recordsExecutiveUsageSql` projection in
//! `adapters/records-ai-usage-projection.ts`: one bounded
//! `executive_generation_jobs` count for a Company and UTC month. The axum
//! owner executes the SQL on its own pool; no TypeScript prepares, executes or
//! projects this operation.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    ExecutiveUsageQuery, ExecutiveUsageRead, SharedExecutiveUsageRead, UsageFuture,
    executive_usage, forbidden,
};
pub use domain::{
    MAX_COMPANY, MAX_MONTH, MAX_USAGE, ExecutiveUsage, request_invalid, source_invalid,
    valid_company, valid_month,
};
pub use http::{
    PlatformUsageService, PlatformUsageTarget, platform_usage_router,
    platform_usage_router_for_hosts,
};
pub use postgres::{
    EXECUTIVE_USAGE_SQL, PgExecutiveUsageRead, rendered_usage_sql, shared_reader,
};
