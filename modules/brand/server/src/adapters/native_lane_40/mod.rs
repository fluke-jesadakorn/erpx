//! Native migration lane 40 for brand.
//!
//! The owner-executed Brand management authority guards: Rust owns the exact
//! statements, the Company scope, the row projection and the response shape.
//! TypeScript is a strict, fail-closed transport with no SQL, no plan request
//! and no fallback reader.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    read_active_branch, read_company_exists, BranchGuardQuery, CompanyGuardQuery, GuardFuture,
    ManagementGuardRead,
};
pub use domain::{
    project_branch, valid_id, validate_target, ActiveBranch, MAX_ID_LEN, MAX_NAME_BYTES,
};
pub use http::{router, router_for_holdings, ManagementGuardTarget};
pub use postgres::{PgManagementGuardRead, BRANCH_GUARD_SQL, COMPANY_GUARD_SQL};
