//! Native migration lane 39 for brand.
//!
//! The owner-executed workspace-choice publication read: Rust owns the SQL,
//! the Company scope, the projection and the response shape. TypeScript is a
//! strict, fail-closed transport with no SQL and no fallback reader.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{WorkspaceChoiceFuture, WorkspaceChoiceRead, read_workspace_choices};
pub use domain::{
    MAX_COMPANY_IDS, ScopeKind, WorkspaceChoice, valid_id, validate_company_ids,
};
pub use http::{LaneTarget, WorkspaceChoiceTarget, router, router_for_holdings};
pub use postgres::{
    PgWorkspaceChoiceRead, WORKSPACE_CHOICE_SQL_HEAD, WORKSPACE_CHOICE_SQL_TAIL,
    workspace_choice_sql,
};
